//! What gets said in a match: the kill feed, built by each peer from who fell and how, the big
//! lines for the player's own kills, and what warriors shout (bots on their own, players from the
//! wheel). All text comes from `crate::lines`; nothing here touches the match's random streams.

use bevy::prelude::*;

use super::bots::BotBrain;
use super::combat::Projectile;
use super::flow::{Match, MatchState};
use super::net::{NetId, NetMsg, Outbox, Remote};
use super::zone::Zone;
use super::{DamageCause, Eliminated, FeedMsg, HitMsg, LocalPlayer, SimSet, Warrior};
use crate::items::{ItemDb, ItemKind};
use crate::lines::{Lines, Person, about, roll};

/// A warrior fell, on any peer. The feed, the tests and the HUD read this rather than feed text.
#[derive(Message, Clone, Debug)]
pub struct FellMsg {
    pub victim: Entity,
    pub by: Option<Entity>,
    pub cause: DamageCause,
    pub placement: u32,
    pub headshot: bool,
    pub at: Vec3,
}

/// A big line for the player alone: their own kill, called out.
#[derive(Message, Clone, Debug)]
pub struct CalloutMsg(pub String);

/// `who` said line `line` (its id in `Lines`). `wheel`: a player chose it, so it goes in the feed.
#[derive(Message, Clone, Debug)]
pub struct SayMsg {
    pub who: Entity,
    pub line: String,
    pub wheel: bool,
}

/// A kill from this far is a long one, metres.
const LONG_KILL: f32 = 60.0;
/// Kills this close together count as a run, seconds.
const KILL_RUN: f32 = 6.0;
/// A killer who falls this soon after their kill traded it, seconds.
const TRADE: f32 = 2.0;
/// Quiet time per bot between shouts, and between any two bots' shouts, seconds.
const BARK_GAP: f32 = 7.0;
const BARK_ANY_GAP: f32 = 1.2;
/// Above this many warriors the second gap shrinks: over a 4 km map with 64 in it nearly every
/// shout is out of a player's earshot, and one gap for all would leave the street near them silent.
const BARK_CROWD: f32 = 16.0;

pub struct TalkPlugin;

impl Plugin for TalkPlugin {
    fn build(&self, app: &mut App) {
        // Not in a SimSet: those stop for the melee hit-stop, and the news would age out meanwhile.
        app.add_message::<FellMsg>()
            .add_message::<CalloutMsg>()
            .add_message::<SayMsg>()
            .add_systems(FixedUpdate, (start_line, announce_falls, echo_wheel, bot_barks).chain().after(SimSet::Damage));
    }
}

fn start_line(m: Res<Match>, lines: Res<Lines>, layout: Res<crate::map::MapLayout>, mut feed: MessageWriter<FeedMsg>, mut shown: Local<u32>) {
    if m.generation != *shown && m.state == MatchState::InProgress {
        *shown = m.generation;
        let n = m.total.to_string();
        feed.write(FeedMsg(about(lines.pick("match.start", m.seed, 0), None, None, &[("n", &n), ("map", &layout.name)])));
    }
}

/// This match's kills so far, for its moments.
#[derive(Default)]
struct Tally {
    generation: u32,
    any_kill: bool,
    /// Each killer's run of quick kills: (last kill's time, kills in the run).
    runs: std::collections::BTreeMap<u32, (f32, u32)>,
    /// (killer, victim, time) of the latest kills.
    recent: Vec<(u32, u32, f32)>,
}

#[allow(clippy::too_many_arguments)]
fn announce_falls(
    mut fell: MessageReader<FellMsg>,
    mut feed: MessageWriter<FeedMsg>,
    mut callout: MessageWriter<CalloutMsg>,
    lines: Res<Lines>,
    db: Res<ItemDb>,
    m: Res<Match>,
    people: Query<(&NetId, &Warrior, Has<LocalPlayer>, &Transform)>,
    mut tally: Local<Tally>,
) {
    if tally.generation != m.generation {
        *tally = Tally { generation: m.generation, ..default() };
    }
    let now = m.elapsed;
    for f in fell.read() {
        let Ok((vid, vw, v_you, _)) = people.get(f.victim) else { continue };
        let victim = Person { name: &vw.name, you: v_you };
        let killer = f.by.and_then(|e| people.get(e).ok());
        let k_person = killer.map(|(_, w, you, _)| Person { name: &w.name, you });
        let pair = (vid.0 as u64) << 32 | killer.map_or(0xffff_ffff, |k| k.0.0 as u64);
        let say = |at: &str, key: u64, k, v| about(lines.pick(at, m.seed, key), k, v, &[]);
        let item = match &f.cause {
            DamageCause::Weapon(id) => Some(id.as_str()),
            _ => None,
        };
        let w = item.map_or("", |id| db.get(id).name.as_str());
        let suicide = f.by == Some(f.victim);
        let at = match (&f.cause, killer) {
            (DamageCause::Pralaya, _) => "down.aandhi",
            (DamageCause::Fall, _) => "down.fall",
            (DamageCause::Weapon(id), _) if suicide => match (id.as_str(), &db.get(id).kind) {
                ("katta", _) => "down.self.katta",
                (_, ItemKind::Astra { .. }) => "down.self.bomb",
                _ => "down.self",
            },
            _ => "",
        };
        let at = if at.is_empty() { kill_moment(&lines, &db, item.unwrap_or_default(), f, killer.map(|k| k.3.translation), m.seed, pair) } else { at.to_string() };
        feed.write(FeedMsg(about(lines.pick(&at, m.seed, pair), k_person, Some(victim), &[("w", w)])));

        // The moments that follow a kill.
        let last2 = f.placement == 3;
        if let Some((kid, kw, k_you, ktf)) = killer.filter(|_| !suicide && item.is_some()) {
            if !tally.any_kill {
                tally.any_kill = true;
                feed.write(FeedMsg(say("moment.first", pair, k_person, Some(victim))));
            }
            let run = tally.runs.entry(kid.0).or_insert((now, 0));
            run.1 = if now - run.0 < KILL_RUN { run.1 + 1 } else { 1 };
            run.0 = now;
            match run.1 {
                2 => { feed.write(FeedMsg(say("moment.double", pair, k_person, Some(victim)))); }
                3 => { feed.write(FeedMsg(say("moment.triple", pair, k_person, Some(victim)))); }
                _ => {}
            }
            let top = people.iter().filter(|(i, ..)| i.0 != vid.0).map(|(_, w, ..)| w.kills).max().unwrap_or(0);
            if vw.kills >= 3 && vw.kills > top {
                feed.write(FeedMsg(say("moment.leader", pair, k_person, Some(victim))));
            }
            // Each fell to the other, within a breath: nobody won that one.
            if tally.recent.iter().any(|(k, v, t)| *k == vid.0 && *v == kid.0 && now - t < TRADE) {
                feed.write(FeedMsg(say("moment.trade", pair, None, None)));
            }
            tally.recent.retain(|(.., t)| now - t < TRADE);
            tally.recent.push((kid.0, vid.0, now));
            if k_you {
                let far = ktf.translation.distance(f.at) >= LONG_KILL;
                let at = if f.headshot { "you.head" } else if far { "you.long" } else { "you.kill" };
                callout.write(CalloutMsg(say(at, pair ^ kw.kills as u64, None, None)));
            }
        }
        if last2 {
            feed.write(FeedMsg(lines.pick("moment.last2", m.seed, 0).to_string()));
        }
    }
}

/// The moment for a kill, most specific first: a long shot, then the weapon's own lines two times
/// in three, then any kill.
fn kill_moment(lines: &Lines, db: &ItemDb, item: &str, f: &FellMsg, from: Option<Vec3>, seed: u64, pair: u64) -> String {
    let own = format!("kill.{item}");
    let gun = matches!(db.items.get(item).map(|d| &d.kind), Some(ItemKind::Bow { .. }));
    if gun && from.is_some_and(|p| p.distance(f.at) >= LONG_KILL) {
        return "kill.long".into();
    }
    let any = roll(seed, pair, "kill.any") % 3 == 0;
    if lines.has(&own) && !any { own } else { "kill".into() }
}

/// A line a player chose goes in the feed too, with their name.
fn echo_wheel(mut said: MessageReader<SayMsg>, lines: Res<Lines>, people: Query<(&Warrior, Has<LocalPlayer>)>, mut feed: MessageWriter<FeedMsg>) {
    for s in said.read().filter(|s| s.wheel) {
        let (Ok((w, you)), Some(line)) = (people.get(s.who), lines.get(&s.line)) else { continue };
        let name = if you { lines.text("ui.you") } else { w.name.as_str() };
        feed.write(FeedMsg(format!("{name}: {}", line.text)));
    }
}

/// The player's own choice of line (or a bot's): shown here and sent to the other peers. Only
/// lines from the file go out, so nothing typed ever reaches anyone.
pub fn say(who: Entity, id: NetId, line: String, wheel: bool, said: &mut MessageWriter<SayMsg>, outbox: &mut Outbox) {
    outbox.0.push(NetMsg::Say { id, line: line.clone(), wheel });
    said.write(SayMsg { who, line, wheel });
}

/// True for a line a peer may say: one from a wheel page or a bark moment.
pub fn sayable(lines: &Lines, id: &str) -> bool {
    lines.get(id).is_some_and(|l| l.plays() && l.at.iter().any(|a| a.starts_with("wheel.") || a.starts_with("bark.")))
}

/// When each of our bots last shouted, and what it was after.
#[derive(Default)]
struct Barks {
    generation: u32,
    last: std::collections::BTreeMap<u32, (f32, bool)>,
    last_any: f32,
    count: u64,
}

/// Bots say what a Delhi person says: on spotting someone, on being hit, after a kill, and running
/// from the Aandhi. Each bot's owner decides and tells the others, as it does the bot's shots.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn bot_barks(
    mut hits: MessageReader<HitMsg>,
    mut fell: MessageReader<FellMsg>,
    bots: Query<(Entity, &NetId, &BotBrain, &Transform), (Without<Eliminated>, Without<Remote>, Without<Projectile>)>,
    lines: Res<Lines>,
    m: Res<Match>,
    zone: Res<Zone>,
    mut said: MessageWriter<SayMsg>,
    mut outbox: ResMut<Outbox>,
    mut state: Local<Barks>,
) {
    if state.generation != m.generation {
        *state = Barks { generation: m.generation, ..default() };
    }
    let now = m.elapsed;
    let any_gap = BARK_ANY_GAP * (BARK_CROWD / m.total.max(1) as f32).min(1.0);
    let mut events: Vec<(Entity, &'static str, u64)> = vec![];
    for h in hits.read().filter(|h| h.target != h.attacker) {
        events.push((h.target, "bark.hit", 3));
    }
    for f in fell.read().filter(|f| f.by != Some(f.victim)) {
        if let Some(k) = f.by {
            events.push((k, "bark.win", 2));
        }
    }
    for (e, id, brain, tf) in &bots {
        let (_, hunting) = state.last.get(&id.0).copied().unwrap_or((-BARK_GAP, false));
        let hunts = brain.target.is_some();
        if hunts && !hunting {
            events.push((e, "bark.spot", 2));
        } else if brain.urgent && zone.outside(tf.translation) {
            events.push((e, "bark.run", 40));
        }
        let entry = state.last.entry(id.0).or_insert((-BARK_GAP, false));
        entry.1 = hunts;
    }
    for (who, at, one_in) in events {
        let Ok((_, id, ..)) = bots.get(who) else { continue };
        let (last, _) = state.last.get(&id.0).copied().unwrap_or((-BARK_GAP, false));
        if now - last < BARK_GAP || now - state.last_any < any_gap {
            continue;
        }
        state.count += 1;
        let key = (id.0 as u64) << 32 | state.count;
        if !roll(m.seed, key, at).is_multiple_of(one_in) {
            continue;
        }
        if let Some(line) = lines.line(at, m.seed, key) {
            state.last_any = now;
            state.last.entry(id.0).or_default().0 = now;
            say(who, *id, line.id.clone(), false, &mut said, &mut outbox);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_preset_lines_can_be_said() {
        let lines = Lines::builtin();
        assert!(sayable(&lines, "chat.16"));
        assert!(sayable(&lines, "spot.1"));
        // Not a line, not a line for speaking, and one a reader hasn't cleared.
        assert!(!sayable(&lines, "Phod diya, bhai!"));
        assert!(!sayable(&lines, "hud.1"));
        assert!(!sayable(&lines, "chat.15"));
    }
}
