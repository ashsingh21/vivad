//! The iroh transport for host-less play (see `shared::sim::net` for the rules it carries).
//!
//! Every peer connects directly to every other on our own protocol, `vivad/0`. Each side of a
//! connection opens one ordered stream for the messages that must arrive (shots, hits, deaths,
//! loot) and sends warrior state as unreliable datagrams, since a lost one is replaced a tick
//! later. A match is gathered by a lobby: one player shares a join code and the rest dial it.
//! That player only introduces everyone and picks the seed; once the match starts it is a peer
//! like the others.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use iroh::endpoint::{Connection, QuicTransportConfig, RecvStream, presets};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey, TransportAddr, Watcher};
use serde::{Deserialize, Serialize};
use shared::sim::flow::{Match, start_match};
use shared::map::MapId;
use shared::sim::{Hold, MatchConfig};
use shared::sim::net::{Departures, Inbox, NetMsg, Outbox, PeerId, Peers, WarriorState};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub const ALPN: &[u8] = b"vivad/0";

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// How long a joining peer waits to reach everyone in the match once it starts.
const MESH_TIMEOUT: Duration = Duration::from_secs(20);
/// A peer silent this long has gone (crashed, or its network has). Keep-alives come from the
/// network thread, not the game loop, so a long frame never trips it.
const IDLE_TIMEOUT: Duration = Duration::from_secs(3);
const KEEP_ALIVE: Duration = Duration::from_millis(500);

pub fn peer_id(id: &EndpointId) -> PeerId {
    u64::from_le_bytes(id.as_bytes()[..8].try_into().unwrap())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Member {
    addr: EndpointAddr,
    name: String,
}

impl Member {
    fn peer(&self) -> PeerId {
        peer_id(&self.addr.id)
    }
}

/// A frame on a connection's ordered stream.
#[derive(Serialize, Deserialize, Debug)]
enum Wire {
    /// The first frame each side sends.
    Hello(Member),
    /// The lobby so far (from the player who shared the code).
    Members(Vec<Member>),
    /// The match is full: everything every peer needs to build the same one.
    Start { seed: u64, map_seed: u64, bots: u32, members: Vec<Member> },
    /// A game message, tagged with the match (`Match::generation`) it belongs to.
    Game(u32, NetMsg),
    /// The sender has loaded the match and is waiting for the rest.
    Ready,
    /// Everyone has loaded: begin (from the present peer with the lowest key).
    Go,
    /// Not sent: the link to this peer is gone.
    Left,
}

/// What the connections share: who we are, who we're connected to, and where their frames go.
struct Mesh {
    me: Member,
    peers: Mutex<BTreeMap<PeerId, Link>>,
    events: UnboundedSender<(PeerId, Wire)>,
    sent: AtomicU64,
    received: AtomicU64,
}

struct Link {
    conn: Connection,
    frames: UnboundedSender<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct Handler(Arc<Mesh>);

impl ProtocolHandler for Handler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        if let Err(e) = run_link(self.0.clone(), conn).await {
            warn!("peer link ended: {e}");
        }
        Ok(())
    }
}

impl std::fmt::Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesh").field("me", &self.me.name).finish()
    }
}

fn frame(wire: &Wire) -> Vec<u8> {
    let body = postcard::to_stdvec(wire).expect("encode");
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend(body);
    out
}

async fn read_frame(recv: &mut RecvStream) -> Result<(Wire, usize), Error> {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await?;
    let len = u32::from_le_bytes(len) as usize;
    if len > 1 << 20 {
        return Err("oversized frame".into());
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body).await?;
    Ok((postcard::from_bytes(&body)?, len + 4))
}

/// One connection, dialled or accepted: swap hellos, then carry frames and datagrams both ways
/// until it closes.
async fn run_link(mesh: Arc<Mesh>, conn: Connection) -> Result<(), Error> {
    let them = peer_id(&conn.remote_id());
    let mut send = conn.open_uni().await?;
    send.write_all(&frame(&Wire::Hello(mesh.me.clone()))).await?;
    let mut recv = conn.accept_uni().await?;
    let (Wire::Hello(member), _) = read_frame(&mut recv).await? else { return Err("expected a hello".into()) };
    if member.peer() != them {
        return Err("hello from a different key".into());
    }
    let (frames, mut outgoing) = unbounded_channel::<Vec<u8>>();
    mesh.peers.lock().unwrap().insert(them, Link { conn: conn.clone(), frames });
    info!("linked with {} ({})", member.name, conn.remote_id().fmt_short());
    let _ = mesh.events.send((them, Wire::Hello(member)));

    let writer = tokio::spawn(async move {
        while let Some(bytes) = outgoing.recv().await {
            if send.write_all(&bytes).await.is_err() {
                break;
            }
        }
    });
    let datagrams = {
        let (mesh, conn) = (mesh.clone(), conn.clone());
        tokio::spawn(async move {
            while let Ok(bytes) = conn.read_datagram().await {
                mesh.received.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                let Some((generation, mut rest)) = bytes.split_first_chunk::<4>().map(|(g, r)| (u32::from_le_bytes(*g), r)) else { continue };
                while !rest.is_empty() {
                    let Ok((state, tail)) = postcard::take_from_bytes::<WarriorState>(rest) else { break };
                    let _ = mesh.events.send((them, Wire::Game(generation, NetMsg::State(state))));
                    rest = tail;
                }
            }
        })
    };
    let result = loop {
        match read_frame(&mut recv).await {
            Ok((wire, len)) => {
                mesh.received.fetch_add(len as u64, Ordering::Relaxed);
                let _ = mesh.events.send((them, wire));
            }
            Err(e) => break e,
        }
    };
    writer.abort();
    datagrams.abort();
    let mut peers = mesh.peers.lock().unwrap();
    if peers.get(&them).is_some_and(|l| l.conn.stable_id() == conn.stable_id()) {
        peers.remove(&them);
        let _ = mesh.events.send((them, Wire::Left));
    }
    Err(result)
}

/// This player's endpoint, before it has joined a match.
pub struct Node {
    relay: bool,
    runtime: Arc<Runtime>,
    router: Router,
    mesh: Arc<Mesh>,
    events: UnboundedReceiver<(PeerId, Wire)>,
    /// The room code we were bound for, if any.
    room: Option<String>,
}

/// Words for room codes: desi food, birds and beasts, games, festivals and places (128 of them).
const WORDS: [&str; 128] = [
    "chai", "samosa", "jalebi", "kulfi", "lassi", "pakora", "ladoo", "barfi", "rasgulla", "paneer", "dosa", "idli",
    "vada", "poha", "bhel", "chaat", "kachori", "paratha", "naan", "roti", "biryani", "korma", "tikka", "halwa",
    "kheer", "imarti", "peda", "gulab", "mango", "guava", "papaya", "litchi", "chikoo", "jamun", "amla", "nimbu",
    "adrak", "elaichi", "jeera", "haldi", "mirchi", "pudina", "tulsi", "neem", "peepal", "banyan", "gulmohar", "champa",
    "mogra", "kamal", "tota", "mor", "koel", "maina", "cheetah", "sher", "hathi", "ghoda", "oont", "bandar",
    "bhalu", "hiran", "kachua", "magar", "chidiya", "titli", "gilehri", "khargosh", "machli", "rickshaw", "tanga", "patang",
    "lattu", "kanche", "gilli", "danda", "kabaddi", "carrom", "ludo", "sixer", "googly", "yorker", "chakka", "dhoom",
    "masti", "jugaad", "bindaas", "jhakaas", "dhamaal", "filmy", "tadka", "garam", "thanda", "mithai", "namkeen", "achar",
    "papad", "raita", "dhaba", "mela", "holi", "diwali", "lohri", "onam", "pongal", "chowk", "gali", "mohalla",
    "bazaar", "haveli", "ghat", "qila", "mahal", "chhatri", "dilli", "mumbai", "kolkata", "chennai", "jaipur", "lucknow",
    "patna", "indore", "bhopal", "surat", "pune", "goa", "shimla", "manali",
];

/// A fresh room code to read out or type: three words and a number, "chai-tiger-kulfi-42".
pub fn new_room() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let w = |rng: &mut rand::rngs::ThreadRng| WORDS[rng.random_range(0..WORDS.len())];
    format!("{}-{}-{}-{}", w(&mut rng), w(&mut rng), w(&mut rng), rng.random_range(10..100))
}

/// A room code as typed (any case, spaces or dashes), or None if it isn't one.
fn room(code: &str) -> Option<String> {
    let parts: Vec<String> = code.trim().to_lowercase().split(['-', ' ', '_']).filter(|p| !p.is_empty()).map(str::to_string).collect();
    let ok = parts.len() == 4 && parts[..3].iter().all(|p| WORDS.contains(&p.as_str())) && parts[3].parse::<u8>().is_ok();
    ok.then(|| parts.join("-"))
}

/// The host's key, from its room code: whoever has the code knows whom to dial, and iroh's
/// address lookup (by key, over DNS) finds where the host is. No server holds the rooms.
fn room_key(room: &str) -> SecretKey {
    SecretKey::from_bytes(&blake3::derive_key("panga 2026-10 room key", room.as_bytes()))
}

/// Which of Panga's maps a room plays: its code says, so the host draws codes until one lands
/// on the map it chose (`new_room_for`).
pub fn room_map(room: &str) -> MapId {
    let h = blake3::derive_key("panga 2026-10 room map", room.as_bytes());
    MapId::DELHI[h[0] as usize % MapId::DELHI.len()]
}

/// A fresh room code for a match on `map` (one of Panga's Delhi maps; any other gets a room on
/// whichever the code lands).
pub fn new_room_for(map: MapId) -> String {
    loop {
        let r = new_room();
        if room_map(&r) == map || !MapId::DELHI.contains(&map) {
            return r;
        }
    }
}

/// The map seed every peer in the room plays.
pub fn room_map_seed(room: &str) -> u64 {
    let h = blake3::derive_key("panga 2026-10 room map seed", room.as_bytes());
    u64::from_le_bytes(h[..8].try_into().unwrap()) % 100_000
}

impl Node {
    /// `relay`: reachable across the internet through iroh's relays (otherwise direct addresses
    /// only, which is enough on one machine or a LAN).
    pub fn bind(name: &str, relay: bool) -> Result<Node, Error> {
        Node::bind_as(name, relay, None)
    }

    /// Bound with the key a room code gives, so the code is all a joiner needs (with relays).
    pub fn bind_room(name: &str, room: &str) -> Result<Node, Error> {
        let mut node = Node::bind_as(name, true, Some(room_key(room)))?;
        node.room = Some(room.to_string());
        Ok(node)
    }

    fn bind_as(name: &str, relay: bool, key: Option<SecretKey>) -> Result<Node, Error> {
        let runtime = Arc::new(tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?);
        let (events_tx, events) = unbounded_channel();
        let (router, mesh) = runtime.block_on(async {
            let quic = QuicTransportConfig::builder().keep_alive_interval(KEEP_ALIVE).max_idle_timeout(Some(IDLE_TIMEOUT.try_into()?)).build();
            let builder = if relay { Endpoint::builder(presets::N0) } else { Endpoint::builder(presets::N0DisableRelay) };
            let builder = match key {
                Some(k) => builder.secret_key(k),
                None => builder,
            };
            let endpoint = builder.transport_config(quic).bind().await?;
            if relay {
                let _ = tokio::time::timeout(Duration::from_secs(10), endpoint.online()).await;
            } else {
                let mut watch = endpoint.watch_addr();
                let _ = tokio::time::timeout(Duration::from_secs(5), async {
                    while watch.get().addrs.is_empty() {
                        if watch.updated().await.is_err() {
                            break;
                        }
                    }
                })
                .await;
            }
            let me = Member { addr: endpoint.addr(), name: name.to_string() };
            let mesh = Arc::new(Mesh { me, peers: default(), events: events_tx, sent: default(), received: default() });
            let router = Router::builder(endpoint).accept(ALPN, Handler(mesh.clone())).spawn();
            Ok::<_, Error>((router, mesh))
        })?;
        Ok(Node { relay, runtime, router, mesh, events, room: None })
    }

    pub fn peer(&self) -> PeerId {
        self.mesh.me.peer()
    }

    /// What other players type or paste to join this one's match on this map: the room code,
    /// or our key (plus our addresses when there are no relays to find us by key).
    pub fn code(&self, map: MapId, map_seed: u64) -> String {
        if let Some(r) = &self.room {
            return r.clone();
        }
        let mut addr = self.router.endpoint().addr();
        // With relays the key is enough to find us; without, a LAN's IPv4 addresses are.
        addr.addrs.retain(|a| !self.relay && matches!(a, TransportAddr::Ip(ip) if ip.is_ipv4()));
        base32(&postcard::to_stdvec(&Ticket { addr, map_seed, map: map.index() }).expect("encode"))
    }

    fn dial(&self, addr: EndpointAddr) {
        let (endpoint, mesh) = (self.router.endpoint().clone(), self.mesh.clone());
        self.runtime.spawn(async move {
            match endpoint.connect(addr, ALPN).await {
                Ok(conn) => {
                    if let Err(e) = run_link(mesh, conn).await {
                        warn!("peer link ended: {e}");
                    }
                }
                Err(e) => warn!("could not reach a peer: {e}"),
            }
        });
    }

    fn broadcast(&self, wire: &Wire) {
        let bytes = frame(wire);
        for link in self.mesh.peers.lock().unwrap().values() {
            let _ = link.frames.send(bytes.clone());
        }
    }

    /// The next lobby event, if one comes within `wait`. Peers that finish loading before we
    /// have even started are remembered.
    fn next(&mut self, wait: Duration, ready: &mut BTreeSet<PeerId>) -> Result<Option<(PeerId, Wire)>, Error> {
        match self.runtime.block_on(async { tokio::time::timeout(wait, self.events.recv()).await }) {
            Err(_) => Ok(None),
            Ok(None) => Err("network stopped".into()),
            Ok(Some((from, Wire::Ready))) => {
                ready.insert(from);
                Ok(None)
            }
            Ok(Some(event)) => Ok(Some(event)),
        }
    }

    /// Wait for `players` (us included), then start a match with `bots` bots on this map.
    pub fn host(self, players: usize, bots: u32, map_seed: u64) -> Result<Session, Error> {
        self.host_until(bots, map_seed, &mut |_| {}, &mut |n| n >= players)
    }

    /// Gather players, telling `report` who is in, until `go` says how many is enough.
    pub fn host_until(mut self, bots: u32, map_seed: u64, report: &mut dyn FnMut(Vec<String>), go: &mut dyn FnMut(usize) -> bool) -> Result<Session, Error> {
        let mut members = vec![self.mesh.me.clone()];
        let mut ready = BTreeSet::new();
        report(names(&members));
        while !go(members.len()) {
            let Some((from, wire)) = self.next(Duration::from_millis(100), &mut ready)? else { continue };
            match wire {
                Wire::Hello(m) if !members.iter().any(|x| x.peer() == m.peer()) => {
                    info!("{} joined ({} players)", m.name, members.len() + 1);
                    members.push(m);
                }
                Wire::Left => members.retain(|m| m.peer() != from),
                _ => continue,
            }
            self.broadcast(&Wire::Members(members.clone()));
            report(names(&members));
        }
        let seed = rand::random();
        self.broadcast(&Wire::Start { seed, map_seed, bots, members: members.clone() });
        Ok(self.session(seed, map_seed, bots, &members, ready))
    }

    /// Join the match whose code another player shared, and wait for it to start.
    pub fn join(self, code: &str) -> Result<Session, Error> {
        self.join_reporting(code, &mut |_| {}, &mut || true)
    }

    /// Join, telling `report` who is in, for as long as `stay` says so (leaving closes our links,
    /// and the host drops us from the lobby).
    pub fn join_reporting(mut self, code: &str, report: &mut dyn FnMut(Vec<String>), stay: &mut dyn FnMut() -> bool) -> Result<Session, Error> {
        let (addr, _, _) = read_code(code)?;
        let host = peer_id(&addr.id);
        self.dial(addr);
        let me = self.peer();
        let mut dialled = vec![host];
        let mut ready = BTreeSet::new();
        let mut start: Option<(u64, u64, u32, Vec<Member>)> = None;
        let mut deadline = std::time::Instant::now() + MESH_TIMEOUT;
        // Players who left after the match was set (or never answered): it starts without them.
        let mut gone = vec![];
        loop {
            if !stay() {
                self.close();
                return Err("left the lobby".into());
            }
            let linked = self.mesh.peers.lock().unwrap().keys().copied().collect::<Vec<_>>();
            if let Some((seed, map_seed, bots, members)) = &start {
                let missing: Vec<PeerId> = members.iter().map(Member::peer).filter(|p| *p != me && !linked.contains(p) && !gone.contains(p)).collect();
                if !missing.is_empty() && std::time::Instant::now() > deadline {
                    warn!("starting without {} player(s) we could not reach", missing.len());
                    gone.extend(missing);
                    continue;
                }
                if missing.is_empty() {
                    let mut session = self.session(*seed, *map_seed, *bots, &members.clone(), ready);
                    session.gone = gone;
                    return Ok(session);
                }
            } else if !linked.contains(&host) && std::time::Instant::now() > deadline {
                // Reaching the host has a deadline; the lobby filling up does not.
                return Err("could not reach the host".into());
            }
            let Some((from, wire)) = self.next(Duration::from_millis(200), &mut ready)? else { continue };
            match wire {
                // Of each pair, the lower key dials, so there is one connection per pair.
                Wire::Members(members) if from == host => {
                    report(names(&members));
                    for m in members {
                        if me < m.peer() && !dialled.contains(&m.peer()) {
                            dialled.push(m.peer());
                            self.dial(m.addr);
                        }
                    }
                }
                Wire::Start { seed, map_seed, bots, members } if from == host => {
                    info!("match starting with {} players", members.len());
                    start = Some((seed, map_seed, bots, members));
                    deadline = std::time::Instant::now() + MESH_TIMEOUT;
                }
                Wire::Left if from == host && start.is_none() => return Err("the host left".into()),
                Wire::Left if start.is_some() => gone.push(from),
                _ => {}
            }
        }
    }

    /// Close our links, so the others see us go at once.
    fn close(self) {
        let router = self.router.clone();
        let _ = self.runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), router.shutdown()).await });
    }

    fn session(self, seed: u64, map_seed: u64, bots: u32, members: &[Member], ready: BTreeSet<PeerId>) -> Session {
        let mut peers = Peers::new(self.peer(), members.iter().map(Member::peer).collect());
        peers.names = members.iter().map(|m| (m.peer(), m.name.clone())).collect();
        let transport = Transport {
            net: Some((self.runtime, self.router)),
            mesh: self.mesh,
            events: Mutex::new(self.events),
            held: default(),
            ready: Mutex::new(ready),
            steady: 0,
            said_ready: false,
            go: default(),
            heard: default(),
            waited: 0.0,
            ready_for: 0.0,
            conditions: Conditions::from_env(),
        };
        Session { peers, seed, map_seed, bots, transport, gone: vec![] }
    }
}

fn names(members: &[Member]) -> Vec<String> {
    members.iter().map(|m| m.name.clone()).collect()
}

/// What a join code holds.
#[derive(Serialize, Deserialize)]
struct Ticket {
    addr: EndpointAddr,
    map_seed: u64,
    map: u8,
}

/// The host's address and the map seed in a join code: a room code ("chai-tiger-kulfi-42") or
/// a long one (with the host's addresses, for LAN play without relays).
pub fn read_code(code: &str) -> Result<(EndpointAddr, MapId, u64), Error> {
    if let Some(r) = room(code) {
        return Ok((EndpointAddr::new(room_key(&r).public()), room_map(&r), room_map_seed(&r)));
    }
    let ticket: Ticket = postcard::from_bytes(&unbase32(code.trim()).ok_or("not a join code")?)?;
    Ok((ticket.addr, MapId::from_index(ticket.map), ticket.map_seed))
}

/// A match that has started: who is in it and what it was seeded with, and the live transport.
pub struct Session {
    pub peers: Peers,
    pub seed: u64,
    pub map_seed: u64,
    pub bots: u32,
    pub transport: Transport,
    /// Players in the match who left before it began here.
    pub gone: Vec<PeerId>,
}

/// The running network, as a Bevy resource: drains the `Outbox` onto the wire and fills the
/// `Inbox` from it.
#[derive(Resource)]
pub struct Transport {
    /// The network thread and our endpoint (taken by `crash`).
    net: Option<(Arc<Runtime>, Router)>,
    mesh: Arc<Mesh>,
    events: Mutex<UnboundedReceiver<(PeerId, Wire)>>,
    /// Messages for a match we haven't reached yet.
    held: Mutex<Vec<(PeerId, u32, NetMsg)>>,
    /// Peers that have loaded the match; it begins once all of them have.
    ready: Mutex<BTreeSet<PeerId>>,
    /// Smooth frames in a row since the match was built (loading makes long ones).
    steady: u32,
    said_ready: bool,
    go: std::sync::atomic::AtomicBool,
    /// Match traffic has come in for the match we're waiting to begin: whoever sent it has
    /// begun, so we may too (a lost Go, or a leader that lost sight of us, can't strand us).
    heard: std::sync::atomic::AtomicBool,
    /// Seconds spent waiting to begin, and since we said we were ready.
    waited: f32,
    ready_for: f32,
    /// Simulated bad network on what arrives (VIVAD_NET_SIM), for testing.
    conditions: Option<Conditions>,
}

/// A bad network, imitated on arrival: `VIVAD_NET_SIM=latency,jitter,loss` (milliseconds,
/// milliseconds, percent of position updates lost). The ordered stream stays in order.
pub struct Conditions {
    latency: f32,
    jitter: f32,
    loss: f32,
    queue: Mutex<Vec<(std::time::Instant, PeerId, Wire)>>,
    /// When each peer's last ordered message is let through (nothing overtakes it).
    ordered: Mutex<BTreeMap<PeerId, std::time::Instant>>,
}

impl Conditions {
    pub fn from_env() -> Option<Conditions> {
        let v: Vec<f32> = std::env::var("VIVAD_NET_SIM").ok().filter(|v| !v.is_empty())?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        let at = |i: usize| v.get(i).copied().unwrap_or(0.0);
        info!("simulating a bad network: {} ms latency, {} ms jitter, {}% loss", at(0), at(1), at(2));
        Some(Conditions { latency: at(0) / 1000.0, jitter: at(1) / 1000.0, loss: at(2) / 100.0, queue: default(), ordered: default() })
    }

    fn pass(&self, arrived: Vec<(PeerId, Wire)>) -> Vec<(PeerId, Wire)> {
        use rand::RngExt;
        let now = std::time::Instant::now();
        let mut rng = rand::rng();
        let mut queue = self.queue.lock().unwrap();
        let mut ordered = self.ordered.lock().unwrap();
        for (from, wire) in arrived {
            let state = matches!(wire, Wire::Game(_, NetMsg::State(_)));
            if state && rng.random::<f32>() < self.loss {
                continue;
            }
            let delay = (self.latency + rng.random_range(-1.0..=1.0) * self.jitter).max(0.0);
            let mut at = now + Duration::from_secs_f32(delay);
            if !state && !matches!(wire, Wire::Left) {
                let last = ordered.entry(from).or_insert(at);
                at = at.max(*last);
                *last = at;
            }
            queue.push((at, from, wire));
        }
        let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut *queue).into_iter().partition(|(at, ..)| *at <= now);
        *queue = wait;
        due.into_iter().map(|(_, from, wire)| (from, wire)).collect()
    }
}

impl Drop for Transport {
    /// Close our connections, so the others see us leave at once rather than after a timeout.
    fn drop(&mut self) {
        if let Some((runtime, router)) = self.net.take() {
            let _ = runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), router.shutdown()).await });
        }
    }
}

impl Transport {
    /// Stop dead, as a crashed game would: no goodbye, so the others only find out when we go
    /// silent. For testing.
    pub fn crash(mut self) {
        let Some((runtime, router)) = self.net.take() else { return };
        std::mem::forget(router);
        if let Ok(runtime) = Arc::try_unwrap(runtime) {
            runtime.shutdown_background();
        }
    }
}

impl Transport {
    /// Bytes this peer has sent and received in datagrams and frames.
    pub fn traffic(&self) -> (u64, u64) {
        (self.mesh.sent.load(Ordering::Relaxed), self.mesh.received.load(Ordering::Relaxed))
    }

    /// Peers currently connected.
    pub fn linked(&self) -> usize {
        self.mesh.peers.lock().unwrap().len()
    }
}

/// Carries the match over the network once a `Transport` is in (see `begin`), and runs a
/// `Lobby` if there is one.
pub struct TransportPlugin;

impl Plugin for TransportPlugin {
    fn build(&self, app: &mut App) {
        // A long frame (loading, a stall) is caught up rather than lost, so this peer's match
        // clock keeps step with the others'.
        if let Some(mut time) = app.world_mut().get_resource_mut::<Time<Virtual>>() {
            time.set_max_delta(Duration::from_secs(3));
        }
        app.add_systems(FixedPreUpdate, receive.run_if(resource_exists::<Transport>))
            .add_systems(FixedPostUpdate, send.run_if(resource_exists::<Transport>))
            .add_systems(Update, (poll_lobby.run_if(resource_exists::<Lobby>), wait_for_everyone.run_if(resource_exists::<Transport>)).chain());
    }
}

/// Build the session's match and hold it until every peer has loaded it.
pub fn begin(world: &mut World, session: Session) {
    let mut config = *world.resource::<MatchConfig>();
    assert_eq!(config.map_seed, session.map_seed, "the join code names the map");
    config.match_seed = Some(session.seed);
    config.bots = session.bots;
    world.insert_resource(config);
    world.insert_resource(session.peers);
    world.insert_resource(session.transport);
    world.resource_mut::<Hold>().0 = true;
    start_match(world);
    world.resource_mut::<Departures>().0.extend(session.gone);
}

/// Once our frames run smoothly (the match is loaded), say so; once everyone linked has, go.
/// Seconds of loading after which we say we're ready even if frames haven't settled, and seconds
/// after saying so that we begin even without the word to (it was lost, or the leader no longer
/// sees us: it will once the match's traffic reaches it).
const READY_ANYWAY: f32 = 15.0;
const BEGIN_ANYWAY: f32 = 30.0;

fn wait_for_everyone(mut transport: ResMut<Transport>, mut hold: ResMut<Hold>, time: Res<Time<Real>>, peers: Res<Peers>) {
    if !hold.0 {
        return;
    }
    let t = &mut *transport;
    t.steady = if time.delta_secs() < 0.05 { t.steady + 1 } else { 0 };
    t.waited += time.delta_secs();
    if t.said_ready {
        t.ready_for += time.delta_secs();
    }
    // A machine whose frames never settle under 50 ms (a slow one) still says it's ready, late.
    if (t.steady >= 20 || t.waited > READY_ANYWAY) && !t.said_ready {
        t.said_ready = true;
        let bytes = frame(&Wire::Ready);
        for link in t.mesh.peers.lock().unwrap().values() {
            let _ = link.frames.send(bytes.clone());
        }
    }
    // One peer gives the word, so all begin within a message's flight of each other (each
    // judging for itself would differ by however late it saw the last of a missing peer).
    let linked: Vec<PeerId> = t.mesh.peers.lock().unwrap().keys().copied().collect();
    let present = || peers.all.iter().copied().filter(|p| *p == peers.local || linked.contains(p));
    let leads = present().min() == Some(peers.local);
    let ready = t.ready.lock().unwrap();
    let go = if leads {
        t.said_ready && present().all(|p| p == peers.local || ready.contains(&p))
    } else {
        t.said_ready && (t.go.load(Ordering::Relaxed) || t.heard.load(Ordering::Relaxed) || t.ready_for > BEGIN_ANYWAY)
    };
    if go {
        if leads {
            let bytes = frame(&Wire::Go);
            for link in t.mesh.peers.lock().unwrap().values() {
                let _ = link.frames.send(bytes.clone());
            }
        }
        info!("everyone has loaded: the match begins");
        hold.0 = false;
    }
}

enum News {
    Code(String),
    Players(Vec<String>),
    Started(Box<Session>),
    Failed(String),
}

/// Gathering a match in the background while the game runs: what the lobby screen shows.
#[derive(Resource)]
pub struct Lobby {
    pub hosting: bool,
    /// The code to share (hosting only).
    pub code: Option<String>,
    pub players: Vec<String>,
    pub error: Option<String>,
    /// The match has been built (it may still be waiting for everyone to load).
    pub started: bool,
    go: Arc<std::sync::atomic::AtomicBool>,
    news: Mutex<std::sync::mpsc::Receiver<News>>,
}

impl Lobby {
    /// Host a match on this map; it starts at `players` (us included), or when `start_now` is called.
    /// With relays the match gets a room code (`room`, from `new_room`; the map seed must be
    /// `room_map_seed` of it); without, a long code carrying our addresses.
    pub fn host(name: String, relay: bool, room: Option<String>, players: Option<usize>, bots: u32, map: MapId, map_seed: u64) -> Lobby {
        let (lobby, news) = Lobby::new(true);
        let go = lobby.go.clone();
        std::thread::spawn(move || {
            let node = match &room {
                Some(r) if relay => Node::bind_room(&name, r),
                _ => Node::bind(&name, relay),
            };
            let result = node.and_then(|node| {
                let _ = news.send(News::Code(node.code(map, map_seed)));
                let tell = news.clone();
                node.host_until(bots, map_seed, &mut |p| drop(tell.send(News::Players(p))), &mut |n| {
                    players.is_some_and(|p| n >= p) || go.load(Ordering::Relaxed)
                })
            });
            let _ = news.send(match result {
                Ok(session) => News::Started(Box::new(session)),
                Err(e) => News::Failed(e.to_string()),
            });
        });
        lobby
    }

    pub fn join(name: String, relay: bool, code: String) -> Lobby {
        let (lobby, news) = Lobby::new(false);
        // For a joiner, `go` means leave.
        let go = lobby.go.clone();
        std::thread::spawn(move || {
            let tell = news.clone();
            let result = Node::bind(&name, relay)
                .and_then(|node| node.join_reporting(&code, &mut |p| drop(tell.send(News::Players(p))), &mut || !go.load(Ordering::Relaxed)));
            let _ = news.send(match result {
                Ok(session) => News::Started(Box::new(session)),
                Err(e) => News::Failed(e.to_string()),
            });
        });
        lobby
    }

    /// Start with whoever has joined (hosting).
    pub fn start_now(&self) {
        self.go.store(true, Ordering::Relaxed);
    }

    fn new(hosting: bool) -> (Lobby, std::sync::mpsc::Sender<News>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Lobby { hosting, code: None, players: vec![], error: None, started: false, go: default(), news: Mutex::new(rx) }, tx)
    }
}

impl Drop for Lobby {
    /// A joiner closing the game before the start leaves the lobby (the host's is gone with it).
    fn drop(&mut self) {
        if !self.hosting {
            self.go.store(true, Ordering::Relaxed);
        }
    }
}

fn poll_lobby(world: &mut World) {
    let news: Vec<News> = world.resource::<Lobby>().news.lock().unwrap().try_iter().collect();
    for n in news {
        let mut lobby = world.resource_mut::<Lobby>();
        match n {
            News::Code(code) => {
                println!("join code: {code}");
                lobby.code = Some(code);
            }
            News::Players(p) => lobby.players = p,
            News::Failed(e) => {
                warn!("online: {e}");
                lobby.error = Some(e);
            }
            News::Started(session) => {
                lobby.started = true;
                lobby.players = session.peers.all.iter().map(|p| session.peers.names.get(p).cloned().unwrap_or_default()).collect();
                begin(world, *session);
            }
        }
    }
}

/// Hand the sim what arrived for the match it is in. Everything after a restart waits a tick, for
/// the sim to start the next match first; messages from a match already over are dropped.
fn receive(transport: Res<Transport>, m: Res<Match>, mut inbox: ResMut<Inbox>, mut departures: ResMut<Departures>) {
    let mut held = transport.held.lock().unwrap();
    let mut queue = std::mem::take(&mut *held);
    let mut events = transport.events.lock().unwrap();
    let arrived: Vec<(PeerId, Wire)> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let arrived = match &transport.conditions {
        Some(c) => c.pass(arrived),
        None => arrived,
    };
    for (from, wire) in arrived {
        match wire {
            Wire::Game(generation, msg) => {
                // A warrior's state only goes out from a match that's running (its owner began);
                // the rest (inventories, at spawn) can come while everyone still waits.
                if generation == m.generation && matches!(msg, NetMsg::State(_)) {
                    transport.heard.store(true, Ordering::Relaxed);
                }
                queue.push((from, generation, msg))
            }
            Wire::Left => {
                info!("peer {from:016x} left");
                departures.0.push(from);
            }
            Wire::Ready => {
                transport.ready.lock().unwrap().insert(from);
            }
            Wire::Go => transport.go.store(true, Ordering::Relaxed),
            _ => {}
        }
    }
    let mut wait = false;
    for (from, generation, msg) in queue {
        if wait {
            held.push((from, generation, msg));
        } else if let NetMsg::Restart { .. } = msg {
            // Tagged with the sender's next match; the sim checks the one it names.
            inbox.0.push((from, msg));
            wait = true;
        } else if generation > m.generation {
            held.push((from, generation, msg));
        } else if generation == m.generation {
            inbox.0.push((from, msg));
        }
    }
}

fn send(transport: Res<Transport>, m: Res<Match>, mut outbox: ResMut<Outbox>) {
    if outbox.0.is_empty() {
        return;
    }
    let mut states = vec![];
    let mut frames = vec![];
    for msg in outbox.0.drain(..) {
        match msg {
            NetMsg::State(s) => states.push((postcard::to_stdvec(&s).expect("encode"), s)),
            msg => frames.extend(frame(&Wire::Game(m.generation, msg))),
        }
    }
    let mesh = &transport.mesh;
    for link in mesh.peers.lock().unwrap().values() {
        if !frames.is_empty() && link.frames.send(frames.clone()).is_ok() {
            mesh.sent.fetch_add(frames.len() as u64, Ordering::Relaxed);
        }
        // Pack the states into as few datagrams as fit; a peer that takes none gets them as frames.
        let Some(max) = link.conn.max_datagram_size() else {
            for (_, s) in &states {
                let _ = link.frames.send(frame(&Wire::Game(m.generation, NetMsg::State(s.clone()))));
            }
            continue;
        };
        let tag = m.generation.to_le_bytes();
        let mut gram: Vec<u8> = tag.to_vec();
        for (s, _) in &states {
            if gram.len() > 4 && gram.len() + s.len() > max {
                mesh.sent.fetch_add(gram.len() as u64, Ordering::Relaxed);
                let _ = link.conn.send_datagram(std::mem::replace(&mut gram, tag.to_vec()).into());
            }
            gram.extend(s);
        }
        if gram.len() > 4 {
            mesh.sent.fetch_add(gram.len() as u64, Ordering::Relaxed);
            let _ = link.conn.send_datagram(gram.into());
        }
    }
}

const BASE32: &[u8; 32] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// Lower-case base 32 without the look-alikes (l, o, 0, 1), for codes read off a screen.
fn base32(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0);
    for &b in bytes {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn unbase32(s: &str) -> Option<Vec<u8>> {
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        acc = (acc << 5) | BASE32.iter().position(|&x| x == c.to_ascii_lowercase())? as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}
