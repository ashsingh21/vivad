//! The iroh transport for host-less play (see `shared::sim::net` for the rules it carries).
//!
//! Every peer connects directly to every other on our own protocol, `vivad/0`. Each side of a
//! connection opens one ordered stream for the messages that must arrive (shots, hits, deaths,
//! loot) and sends warrior state as unreliable datagrams, since a lost one is replaced a tick
//! later. A match is gathered by a lobby: one player shares a join code and the rest dial it.
//! That player only introduces everyone and picks the seed; once the match starts it is a peer
//! like the others.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use iroh::endpoint::{Connection, QuicTransportConfig, RecvStream, presets};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, EndpointId, Watcher};
use serde::{Deserialize, Serialize};
use shared::sim::flow::Match;
use shared::sim::net::{Departures, Inbox, NetMsg, Outbox, PeerId, Peers, WarriorState};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub const ALPN: &[u8] = b"vivad/0";

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// How long a joining peer waits to reach everyone in the match once it starts.
const MESH_TIMEOUT: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(6);

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
    runtime: Arc<Runtime>,
    router: Router,
    mesh: Arc<Mesh>,
    events: UnboundedReceiver<(PeerId, Wire)>,
}

impl Node {
    /// `relay`: reachable across the internet through iroh's relays (otherwise direct addresses
    /// only, which is enough on one machine or a LAN).
    pub fn bind(name: &str, relay: bool) -> Result<Node, Error> {
        let runtime = Arc::new(tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?);
        let (events_tx, events) = unbounded_channel();
        let (router, mesh) = runtime.block_on(async {
            // A peer silent this long has left (keep-alives go every few seconds).
            let quic = QuicTransportConfig::builder().max_idle_timeout(Some(IDLE_TIMEOUT.try_into()?)).build();
            let endpoint = if relay {
                Endpoint::builder(presets::N0).transport_config(quic).bind().await?
            } else {
                Endpoint::builder(presets::N0DisableRelay).transport_config(quic).bind().await?
            };
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
        Ok(Node { runtime, router, mesh, events })
    }

    pub fn peer(&self) -> PeerId {
        self.mesh.me.peer()
    }

    /// What other players paste to join this one's match.
    pub fn code(&self) -> String {
        hex(&postcard::to_stdvec(&self.router.endpoint().addr()).expect("encode"))
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

    /// Wait for `players` (us included), then start a match with `bots` bots on this map.
    pub fn host(mut self, players: usize, bots: u32, map_seed: u64) -> Result<Session, Error> {
        let mut members = vec![self.mesh.me.clone()];
        while members.len() < players {
            let Some((_, wire)) = self.events.blocking_recv() else { return Err("network stopped".into()) };
            if let Wire::Hello(m) = wire
                && !members.iter().any(|x| x.peer() == m.peer())
            {
                info!("{} joined ({}/{players})", m.name, members.len() + 1);
                members.push(m);
                self.broadcast(&Wire::Members(members.clone()));
            }
        }
        let seed = rand::random();
        self.broadcast(&Wire::Start { seed, map_seed, bots, members: members.clone() });
        Ok(self.session(seed, map_seed, bots, &members))
    }

    /// Join the match whose code another player shared, and wait for it to start.
    pub fn join(mut self, code: &str) -> Result<Session, Error> {
        let addr: EndpointAddr = postcard::from_bytes(&unhex(code.trim()).ok_or("not a join code")?)?;
        let host = peer_id(&addr.id);
        self.dial(addr);
        let me = self.peer();
        let mut dialled = vec![host];
        let mut start: Option<(u64, u64, u32, Vec<Member>)> = None;
        let mut deadline = None;
        loop {
            if let Some((seed, map_seed, bots, members)) = &start {
                let linked = self.mesh.peers.lock().unwrap().keys().copied().collect::<Vec<_>>();
                if members.iter().all(|m| m.peer() == me || linked.contains(&m.peer())) {
                    return Ok(self.session(*seed, *map_seed, *bots, &members.clone()));
                }
            }
            let wait = deadline.map(|d: std::time::Instant| d.saturating_duration_since(std::time::Instant::now())).unwrap_or(Duration::from_secs(3600));
            let event = self.runtime.block_on(async { tokio::time::timeout(wait.min(Duration::from_millis(200)), self.events.recv()).await });
            if deadline.is_some_and(|d| std::time::Instant::now() > d) {
                return Err("could not reach every player in the match".into());
            }
            let Ok(event) = event else { continue };
            let Some((from, wire)) = event else { return Err("network stopped".into()) };
            match wire {
                // Of each pair, the lower key dials, so there is one connection per pair.
                Wire::Members(members) if from == host => {
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
                    deadline = Some(std::time::Instant::now() + MESH_TIMEOUT);
                }
                _ => {}
            }
        }
    }

    fn session(self, seed: u64, map_seed: u64, bots: u32, members: &[Member]) -> Session {
        let mut peers = Peers::new(self.peer(), members.iter().map(Member::peer).collect());
        peers.names = members.iter().map(|m| (m.peer(), m.name.clone())).collect();
        Session { peers, seed, map_seed, bots, transport: Transport { _runtime: self.runtime, _router: self.router, mesh: self.mesh, events: Mutex::new(self.events), held: default() } }
    }
}

/// A match that has started: who is in it and what it was seeded with, and the live transport.
pub struct Session {
    pub peers: Peers,
    pub seed: u64,
    pub map_seed: u64,
    pub bots: u32,
    pub transport: Transport,
}

/// The running network, as a Bevy resource: drains the `Outbox` onto the wire and fills the
/// `Inbox` from it.
#[derive(Resource)]
pub struct Transport {
    _runtime: Arc<Runtime>,
    _router: Router,
    mesh: Arc<Mesh>,
    events: Mutex<UnboundedReceiver<(PeerId, Wire)>>,
    /// Messages for a match we haven't reached yet.
    held: Mutex<Vec<(PeerId, u32, NetMsg)>>,
}

impl Drop for Transport {
    /// Close our connections, so the others see us leave at once rather than after a timeout.
    fn drop(&mut self) {
        let router = self._router.clone();
        let _ = self._runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), router.shutdown()).await });
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

pub struct TransportPlugin;

impl Plugin for TransportPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedPreUpdate, receive).add_systems(FixedPostUpdate, send);
    }
}

/// Hand the sim what arrived for the match it is in. Everything after a restart waits a tick, for
/// the sim to start the next match first; messages from a match already over are dropped.
fn receive(transport: Res<Transport>, m: Res<Match>, mut inbox: ResMut<Inbox>, mut departures: ResMut<Departures>) {
    let mut held = transport.held.lock().unwrap();
    let mut queue = std::mem::take(&mut *held);
    let mut events = transport.events.lock().unwrap();
    while let Ok((from, wire)) = events.try_recv() {
        match wire {
            Wire::Game(generation, msg) => queue.push((from, generation, msg)),
            Wire::Left => {
                info!("peer {from:016x} left");
                departures.0.push(from);
            }
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect()
}
