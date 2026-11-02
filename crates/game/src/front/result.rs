//! The result card: where you came, who got you, your numbers, and one press to go again.
//! The layout follows Free Fire's: PLAY AGAIN beside the way back to the lobby.

use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use shared::items::ItemDb;
use shared::lines::{Lines, with};
use shared::sim::flow::{self, Match, MatchState};
use shared::sim::net::{NetId, Peers};
use shared::sim::{DamageCause, DamageMsg, Eliminated, HitMsg, LocalPlayer, MatchConfig, Warrior};

use super::settings::{Saved, today};
use super::theme::{self, CREAM, Fonts, GHOST, INK, MARIGOLD, MUSTARD, RED, Shortcut, TEAL, u};
use super::{Press, Screen};

const AGAIN: &str = "PHIR SE PANGA";
const HOME: &str = "Ghar";
const OUT: &str = "TAPAK GAYE";
const KILLS: &str = "KILLS";
const DAMAGE: &str = "DAMAGE";
const TIME: &str = "TIME";

fn with_weapon(weapon: &str) -> String {
    format!("{weapon} se")
}
fn own_weapon(weapon: &str) -> String {
    format!("{weapon} ne tumhe hi tapka diya!")
}
fn left_standing(n: u32) -> String {
    format!("Abhi {n} bache hain")
}

/// A beat to see it happen, before the card comes up.
const BEAT: f32 = 0.6;
/// How long the card's buttons wait, so a shot still being fired can't press one.
const LOCK: f32 = 1.0;

/// This match's numbers for the local player.
#[derive(Resource, Default)]
pub(super) struct Tally {
    generation: u32,
    damage: f32,
    /// What hurt us last.
    last_hit: Option<DamageCause>,
    /// Match time when we fell or won.
    out_at: Option<f32>,
}

/// When the card came up.
#[derive(Resource, Default)]
struct Opened(f32);

/// How little a place missed the top 5 or the top 10 by: (that top, the places short).
fn near_miss(place: u32, total: u32) -> Option<(u32, u32)> {
    const CLOSE: u32 = 5;
    [5, 10].into_iter().filter(|top| total >= top * 3).find_map(|top| (place > top && place - top <= CLOSE).then(|| (top, place - top)))
}

/// Today's best, for the card: this match's place goes into it, and into the file unless a test
/// recipe is playing.
fn best_line(lines: &Lines, saved: &mut Saved, place: u32, autopilot: bool) -> String {
    if autopilot {
        return String::new();
    }
    let before = saved.record(today(), place);
    if !super::skip_home() || std::env::var_os("VIVAD_SAVE").is_some() {
        saved.save();
    }
    match before {
        Some(best) if place < best => with(lines.text("best.new"), &[("p", &place.to_string())]),
        Some(best) => with(lines.text("best.today"), &[("p", &best.to_string())]),
        None => with(lines.text("best.today"), &[("p", &place.to_string())]),
    }
}

/// The live line on the card: who is left, and who won.
#[derive(Component)]
struct Status;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Tally>()
        .init_resource::<Opened>()
        .add_systems(Update, tally)
        .add_systems(Update, out_of_the_match.after(tally).run_if(in_state(Screen::Match)))
        .add_systems(OnEnter(Screen::Result), spawn)
        .add_systems(Update, (unlock, status).run_if(in_state(Screen::Result)));
}

fn tally(
    mut t: ResMut<Tally>,
    m: Res<Match>,
    local: Query<(Entity, Has<Eliminated>), With<LocalPlayer>>,
    mut hits: MessageReader<HitMsg>,
    mut hurts: MessageReader<DamageMsg>,
) {
    if t.generation != m.generation {
        *t = Tally { generation: m.generation, ..default() };
    }
    let Ok((me, dead)) = local.single() else { return };
    t.damage += hits.read().filter(|h| h.attacker == me && h.target != me).map(|h| h.amount).sum::<f32>();
    if t.out_at.is_some() {
        return;
    }
    if let Some(hurt) = hurts.read().filter(|d| d.target == me).last() {
        t.last_hit = Some(hurt.cause.clone());
    }
    if dead || m.winner == Some(me) {
        t.out_at = Some(m.elapsed);
    }
}

/// Out of the match, by falling or by winning: after a beat, the card.
fn out_of_the_match(m: Res<Match>, local: Query<(Entity, Has<Eliminated>), With<LocalPlayer>>, time: Res<Time>, mut next: ResMut<NextState<Screen>>, mut since: Local<Option<f32>>) {
    let now = time.elapsed_secs();
    let out = local.single().is_ok_and(|(me, dead)| dead || m.winner == Some(me));
    if !out {
        *since = None;
    } else if now - *since.get_or_insert(now) >= BEAT {
        next.set(Screen::Result);
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    mut commands: Commands,
    fonts: Res<Fonts>,
    lines: Res<Lines>,
    m: Res<Match>,
    tally: Res<Tally>,
    db: Res<ItemDb>,
    local: Query<(Entity, &Warrior, &NetId, Option<&Eliminated>), With<LocalPlayer>>,
    names: Query<&Warrior>,
    time: Res<Time>,
    mut opened: ResMut<Opened>,
    mut saved: ResMut<Saved>,
    config: Res<MatchConfig>,
) {
    opened.0 = time.elapsed_secs();
    let Ok((me, w, id, fell)) = local.single() else { return };
    let key = id.0 as u64;
    let pick = |at: &str| lines.pick(at, m.seed, key).to_string();
    let seconds = tally.out_at.unwrap_or(m.elapsed) as u32;
    let numbers = [(w.kills.to_string(), KILLS), ((tally.damage.round() as u32).to_string(), DAMAGE), (format!("{}:{:02}", seconds / 60, seconds % 60), TIME)];
    let won = m.winner == Some(me);
    let place = if won { 1 } else { fell.map_or(m.alive, |e| e.placement) };
    // VIVAD_PLACE=<n>: the card as a match ending there would show it, for pictures of it.
    let place = std::env::var("VIVAD_PLACE").ok().and_then(|s| s.parse().ok()).unwrap_or(place);
    let best = best_line(&lines, &mut saved, place, config.autopilot);
    commands.spawn((theme::layer(5), theme::scrim_left(), DespawnOnExit(Screen::Result))).with_children(|p| {
        p.spawn((
            Node { position_type: PositionType::Absolute, left: u(72.0), top: u(60.0), width: u(640.0), flex_direction: FlexDirection::Column, row_gap: u(16.0), ..default() },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            if won {
                c.spawn(theme::painted(pick("win.title"), &fonts.title, 100.0, MUSTARD));
                c.spawn(theme::line(pick("win.sub"), &fonts.body, 27.0, CREAM));
                c.spawn((Node { column_gap: u(52.0), margin: UiRect::top(u(12.0)), ..default() }, Pickable::IGNORE)).with_children(|row| {
                    stats(row, &fonts, &numbers, MUSTARD, CREAM);
                });
                c.spawn(theme::line(best.clone(), &fonts.body, 27.0, CREAM));
            } else {
                c.spawn(theme::line(with(lines.text("hud.place"), &[("p", &place.to_string()), ("n", &m.total.to_string())]), &fonts.number, 104.0, MUSTARD));
                let (title, detail) = who_got_you(me, fell, tally.last_hit.as_ref(), &names, &db, &pick);
                // Near the end, or out among the first quarter to fall: a word on it.
                let finish = match place {
                    2 => pick("elim.place.2"),
                    3..=5 => pick("elim.place.top5"),
                    p if p * 4 >= m.total * 3 => pick("elim.place.low"),
                    _ => String::new(),
                };
                c.spawn(theme::sign(CREAM, MARIGOLD, UiRect::axes(u(36.0), u(26.0)))).with_children(|s| {
                    s.spawn(theme::line(OUT, &fonts.body, 17.0, INK));
                    s.spawn(theme::line(title, &fonts.title, 42.0, RED));
                    s.spawn(theme::line(detail, &fonts.body, 23.0, INK));
                    if !finish.is_empty() {
                        s.spawn(theme::line(finish, &fonts.body, 23.0, INK));
                    }
                    if let Some((top, short)) = near_miss(place, m.total) {
                        s.spawn(theme::line(with(lines.text("elim.near"), &[("top", &top.to_string()), ("n", &short.to_string())]), &fonts.body, 23.0, RED));
                    }
                    s.spawn((Node { justify_content: JustifyContent::SpaceBetween, margin: UiRect::top(u(14.0)), ..default() }, Pickable::IGNORE)).with_children(|row| {
                        stats(row, &fonts, &numbers, INK, INK);
                    });
                });
                c.spawn(theme::sign(TEAL, MUSTARD, UiRect::axes(u(30.0), u(16.0)))).with_children(|s| {
                    s.spawn((Status, theme::line("", &fonts.body, 25.0, CREAM)));
                    if !best.is_empty() {
                        s.spawn(theme::line(best.clone(), &fonts.body, 25.0, MUSTARD));
                    }
                });
            }
        });
        p.spawn(theme::corner()).with_children(|c| {
            c.spawn((Node { column_gap: u(14.0), ..default() }, Pickable::IGNORE)).with_children(|row| {
                let home = theme::small_button(row, &fonts, HOME, Press::Ghar, GHOST, Some(Shortcut::Esc));
                row.commands().entity(home).insert(InteractionDisabled);
            });
            let again = theme::big_button(c, &fonts, AGAIN, Press::Play, Shortcut::Enter);
            c.commands().entity(again).insert(InteractionDisabled);
        });
    });
}

fn stats(row: &mut ChildSpawnerCommands, fonts: &Fonts, numbers: &[(String, &str)], number: Color, label: Color) {
    for (n, what) in numbers {
        row.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, Pickable::IGNORE)).with_children(|c| {
            c.spawn(theme::line(n.clone(), &fonts.number, 64.0, number));
            c.spawn(theme::line(*what, &fonts.body, 17.0, label));
        });
    }
}

/// The card's title and the line under it.
fn who_got_you(me: Entity, fell: Option<&Eliminated>, last_hit: Option<&DamageCause>, names: &Query<&Warrior>, db: &ItemDb, pick: &dyn Fn(&str) -> String) -> (String, String) {
    let killer = fell.and_then(|e| e.by).filter(|by| *by != me).and_then(|by| names.get(by).ok()).map(|w| w.name.as_str());
    // Some of the killer's lines leave the name out; the card still says who it was.
    let got_you = |k: &str, how: String| {
        let title = with(&pick("elim.killer"), &[("k", k)]);
        let detail = match (title.contains(k), how.is_empty()) {
            (true, _) => how,
            (false, true) => k.to_string(),
            (false, false) => format!("{k}: {how}"),
        };
        (title, detail)
    };
    match (last_hit, killer) {
        (Some(DamageCause::Pralaya), _) => (pick("elim.aandhi"), pick("alert.thanks")),
        (Some(DamageCause::Fall), _) => (pick("elim.fall"), String::new()),
        (Some(DamageCause::Weapon(id)), Some(k)) => got_you(k, with_weapon(&db.get(id).name)),
        (Some(DamageCause::Weapon(id)), None) => (own_weapon(&db.get(id).name), pick("elim.self")),
        (None, Some(k)) => got_you(k, String::new()),
        (None, None) => (OUT.into(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::near_miss;

    #[test]
    fn a_near_miss_is_named_only_when_it_is_near() {
        assert_eq!(near_miss(7, 64), Some((5, 2)));
        assert_eq!(near_miss(6, 64), Some((5, 1)));
        assert_eq!(near_miss(13, 64), Some((10, 3)));
        assert_eq!(near_miss(5, 64), None, "in the top 5");
        assert_eq!(near_miss(40, 64), None);
        assert_eq!(near_miss(13, 16), None, "no top 10 to speak of among 16");
    }
}

/// Online the next match starts for everyone at once, so its button waits until it may.
fn unlock(
    mut commands: Commands,
    time: Res<Time>,
    opened: Res<Opened>,
    m: Res<Match>,
    config: Res<MatchConfig>,
    peers: Res<Peers>,
    standing: Query<&Warrior, Without<Eliminated>>,
    buttons: Query<(Entity, &Press), With<InteractionDisabled>>,
) {
    if time.elapsed_secs() - opened.0 < LOCK {
        return;
    }
    let online = config.online || peers.all.len() > 1;
    let open = !online || flow::next_match_open(&m, standing.iter());
    for (e, press) in &buttons {
        if *press != Press::Play || open {
            commands.entity(e).remove::<InteractionDisabled>();
        }
    }
}

fn status(
    m: Res<Match>,
    lines: Res<Lines>,
    config: Res<MatchConfig>,
    peers: Res<Peers>,
    votes: Res<flow::MapVotes>,
    names: Query<&Warrior>,
    mut line: Query<&mut Text, With<Status>>,
) {
    let online = config.online || peers.all.len() > 1;
    let mut s = match m.winner.and_then(|e| names.get(e).ok()) {
        Some(w) if m.state == MatchState::Ended => with(lines.pick("win.feed", m.seed, w.look as u64), &[("k", &w.name)]),
        _ if online => format!("{}\n{}", lines.pick("next.wait", m.seed, 0), left_standing(m.alive)),
        _ => left_standing(m.alive),
    };
    // The vote for the next map (the number keys, crate::input), and where it stands. With one map
    // in the turn there is nothing to vote on, and the vote stays off the card.
    let pool = &flow::ROTATION;
    if config.rotate && pool.len() > 1 {
        let next = flow::next_map(config.map, &votes);
        s.push('\n');
        s.push_str(lines.text("hud.vote"));
        for (k, map) in pool.iter().enumerate() {
            let n = votes.count(*map);
            let count = if n > 0 { format!(" ({n})") } else { String::new() };
            let (l, r) = if *map == next { ("[ ", " ]") } else { ("", "") };
            s.push_str(&format!("\n{l}{}  {}{count}{r}", k + 1, map.title()));
        }
    }
    for mut t in &mut line {
        if t.as_str() != s {
            t.0 = s.clone();
        }
    }
}
