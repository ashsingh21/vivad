//! Every line the game puts on screen, from `assets/data/lines.ron`. Built in, like the item data
//! in the client, so the browser needn't fetch it and the headless tools have it too.
//!
//! A moment is a pool of lines; `Lines::pick` chooses one by a hash of the match seed and the
//! players' ids, so every peer shows the same line and the match's random streams stay untouched.
//! The most specific moment wins (`Lines::first_of`), as in Left 4 Dead's rule tables.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;

use crate::items::Rarity;

/// What a Delhi reader still has to do with a line.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Check {
    /// Plain Hindi: a quick read.
    Plain,
    /// A native must check it.
    Read,
    /// A native must decide if we can use it at all: out of play until `ok`.
    Decide,
    /// As `Decide`, but the only text for its place, so it plays meanwhile.
    Working,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Line {
    pub id: String,
    /// The moments it plays in.
    pub at: Vec<String>,
    /// A Delhi native has read it.
    pub ok: bool,
    pub check: Check,
    pub text: String,
    pub mean: String,
    #[serde(default)]
    pub note: String,
    /// The same in Devanagari, for painted lines.
    #[serde(default)]
    pub deva: String,
}

impl Line {
    pub fn plays(&self) -> bool {
        self.ok || self.check != Check::Decide
    }
}

#[derive(Deserialize)]
struct LinesFile {
    lines: Vec<Line>,
}

#[derive(Resource, Clone, Debug)]
pub struct Lines {
    all: Vec<Line>,
    /// Each moment's lines in play, in file order.
    pools: BTreeMap<String, Vec<usize>>,
}

impl Lines {
    pub fn builtin() -> Self {
        Self::load_from_str(include_str!("../../../assets/data/lines.ron")).expect("lines.ron")
    }

    pub fn load_from_str(src: &str) -> Result<Self, ron::error::SpannedError> {
        let LinesFile { lines: all } = ron::from_str(src)?;
        let mut pools: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, l) in all.iter().enumerate().filter(|(_, l)| l.plays()) {
            for at in &l.at {
                pools.entry(at.clone()).or_default().push(i);
            }
        }
        Ok(Self { all, pools })
    }

    pub fn all(&self) -> &[Line] {
        &self.all
    }

    pub fn get(&self, id: &str) -> Option<&Line> {
        self.all.iter().find(|l| l.id == id)
    }

    /// A moment's lines in play.
    pub fn pool(&self, at: &str) -> impl Iterator<Item = &Line> {
        self.pools.get(at).into_iter().flatten().map(|i| &self.all[*i])
    }

    pub fn has(&self, at: &str) -> bool {
        self.pools.contains_key(at)
    }

    /// A moment's only line, for labels ("" for a moment with none, which a test rules out).
    pub fn text(&self, at: &str) -> &str {
        self.pool(at).next().map_or("", |l| l.text.as_str())
    }

    /// The moment's line for this seed and key (who it is about, or which time it is).
    pub fn line(&self, at: &str, seed: u64, key: u64) -> Option<&Line> {
        let pool = self.pools.get(at)?;
        Some(&self.all[pool[(roll(seed, key, at) % pool.len() as u64) as usize]])
    }

    pub fn pick(&self, at: &str, seed: u64, key: u64) -> &str {
        self.line(at, seed, key).map_or("", |l| l.text.as_str())
    }

    /// The first of these moments that has a line: list the most specific first.
    pub fn first_of<'a>(&self, moments: &[&'a str]) -> &'a str {
        moments.iter().copied().find(|m| self.has(m)).unwrap_or("")
    }

    pub fn rarity(&self, r: Rarity) -> &str {
        self.text(match r {
            Rarity::Common => "rarity.common",
            Rarity::Rare => "rarity.rare",
            Rarity::Epic => "rarity.epic",
            Rarity::Legendary => "rarity.legendary",
            Rarity::Divya => "rarity.divya",
        })
    }
}

/// A steady hash of the seed, the key and the moment (FNV-1a): the same on every peer and build.
pub fn roll(seed: u64, key: u64, at: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in seed.to_le_bytes().into_iter().chain(key.to_le_bytes()).chain(at.bytes()) {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    // FNV's low bits are weak on short keys, and the pools are small.
    h ^ (h >> 29)
}

/// Replaces each `{token}` once, left to right: what goes in is never read as a token, so a player
/// named "{k}" stays that. A token with no value is left as it is.
pub fn fill(template: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        out.push_str(&rest[..open]);
        let token = &rest[open + 1..open + close];
        match value(token) {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// `fill` from a list of values.
pub fn with(template: &str, values: &[(&str, &str)]) -> String {
    fill(template, |t| values.iter().find(|(k, _)| *k == t).map(|(_, v)| v.to_string()))
}

/// Someone a line is about. `you`: this machine's player, who reads as "tum".
#[derive(Clone, Copy, Debug)]
pub struct Person<'a> {
    pub name: &'a str,
    pub you: bool,
}

impl Person<'_> {
    /// `form` is what follows the token's underscore: "ne", "ko", "ka", "ki", "par", or "" for the
    /// bare name. Built so a line never needs the person's gender.
    fn form(&self, form: &str) -> Option<String> {
        let n = self.name;
        Some(match (self.you, form) {
            (false, "") => n.to_string(),
            (false, "ne" | "ko" | "ka" | "ki" | "par" | "se") => format!("{n} {form}"),
            (true, "") => "tum".into(),
            (true, "ne") => "tumne".into(),
            (true, "ko") => "tumhe".into(),
            (true, "ka") => "tumhara".into(),
            (true, "ki") => "tumhari".into(),
            (true, "par") => "tum par".into(),
            (true, "se") => "tumse".into(),
            _ => return None,
        })
    }
}

/// A line about a killer and a victim, plus any other values. Each sentence starts with a capital,
/// whoever "tum" turned out to be.
pub fn about(template: &str, k: Option<Person>, v: Option<Person>, values: &[(&str, &str)]) -> String {
    let filled = fill(template, |t| {
        let (who, form) = match t.split_once('_').unwrap_or((t, "")) {
            ("k", f) => (k?, f),
            ("v", f) => (v?, f),
            _ => return values.iter().find(|(key, _)| *key == t).map(|(_, v)| v.to_string()),
        };
        who.form(form)
    });
    let mut out = String::with_capacity(filled.len());
    let mut start = true;
    for c in filled.chars() {
        out.push(if start { c.to_ascii_uppercase() } else { c });
        start = match c {
            '.' | '!' | '?' | '(' => true,
            ' ' => start,
            _ => false,
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every moment the game asks for, with the tokens its lines may use. A line with any other
    /// token would show its braces on screen.
    fn moments() -> Vec<(&'static str, &'static [&'static str])> {
        const KILL: &[&str] = &["k", "k_ne", "k_ka", "k_ki", "v", "v_ne", "v_ko", "v_ka", "v_ki", "v_par", "w"];
        const VICTIM: &[&str] = &["v", "v_ne", "v_ko", "v_ka", "v_ki", "v_par", "w"];
        let mut all: Vec<(&'static str, &'static [&'static str])> = vec![
            ("kill", KILL), ("kill.long", KILL), ("kill.bat", KILL), ("kill.katta", KILL), ("kill.tamancha", KILL), ("kill.dunali", KILL),
            ("kill.gulel", KILL), ("kill.vajra9", KILL), ("kill.trishul_carbine", KILL), ("kill.agneyastra", KILL), ("kill.vayavyastra", KILL),
            ("kill.brahmastra_mk2", KILL), ("kill.chakra", KILL), ("kill.chappal", KILL), ("kill.brahmastra", KILL),
            ("down.aandhi", VICTIM), ("down.fall", VICTIM), ("down.self", VICTIM), ("down.self.katta", VICTIM), ("down.self.bomb", VICTIM),
            ("left", VICTIM), ("moment.first", KILL), ("moment.double", KILL), ("moment.triple", KILL), ("moment.last2", &[]), ("moment.leader", KILL),
            ("moment.trade", &[]), ("you.head", &[]), ("you.long", &[]), ("you.kill", &[]),
            ("elim.killer", &["k"]), ("elim.aandhi", &[]), ("elim.fall", &[]), ("elim.self", &[]), ("elim.place.2", &[]), ("elim.place.top5", &[]),
            ("elim.place.low", &[]), ("next.again", &[]), ("next.wait", &[]), ("win.title", &[]), ("win.sub", &[]), ("win.feed.you", &[]), ("win.feed", &["k"]),
            ("drop.far", &["map"]), ("drop.jump", &[]), ("drop.end", &[]), ("drop.ride", &[]), ("drop.open", &[]), ("drop.ground", &[]), ("match.start", &["n", "map"]),
            ("alert.next", &["place"]), ("alert.soon", &["t"]), ("alert.doors", &[]), ("alert.moving", &["place"]), ("alert.outside", &[]), ("alert.first", &[]),
            ("alert.last", &[]), ("alert.tiny", &[]), ("alert.loot", &[]), ("alert.thanks", &[]),
            ("hud.vitals", &["hp", "sh"]), ("hud.ammo", &["n"]), ("hud.reload", &[]), ("hud.draw", &[]), ("hud.counts", &["alive", "kills"]), ("hud.wait", &["t"]),
            ("hud.move", &["t"]), ("hud.idle", &[]), ("hud.safe.far", &["d"]), ("hud.safe.in", &[]), ("hud.vote", &[]), ("hud.place", &["p", "n"]), ("hud.broke", &["item"]),
            ("prompt.peti", &[]), ("prompt.ghada", &[]), ("prompt.pickup", &["item", "rarity"]), ("prompt.drink", &["item"]), ("prompt.backfire", &[]),
            ("prompt.misfire", &[]), ("prompt.reload", &[]), ("prompt.empty", &[]),
            ("rarity.common", &[]), ("rarity.rare", &[]), ("rarity.epic", &[]), ("rarity.legendary", &[]), ("rarity.divya", &[]),
            ("lobby.error", &[]), ("lobby.loading", &[]), ("lobby.host", &[]), ("lobby.share", &[]), ("lobby.going", &[]), ("lobby.players", &["n", "names"]),
            ("lobby.start", &[]), ("lobby.join", &[]), ("lobby.reaching", &[]), ("lobby.waiting", &[]), ("lobby.nohost", &[]), ("lobby.hostleft", &[]), ("lobby.player", &["n"]),
            ("bark.spot", &[]), ("bark.hit", &[]), ("bark.win", &[]), ("bark.run", &[]), ("ui.wheel.hint", &[]), ("ui.you", &[]),
        ];
        for p in ["wheel.dekho", "wheel.panga", "wheel.jeet", "wheel.haar", "wheel.masti"] {
            all.push((p, &[]));
        }
        for p in ["ui.wheel.dekho", "ui.wheel.panga", "ui.wheel.jeet", "ui.wheel.haar", "ui.wheel.masti"] {
            all.push((p, &[]));
        }
        all
    }

    /// Read here, used elsewhere or later: their tokens aren't the game's to fill.
    const NOT_PLAYED: [&str; 5] = ["paint", "signs", "items", "web", "later"];

    fn tokens(text: &str) -> Vec<String> {
        text.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(t, _)| t.to_string())).collect()
    }

    #[test]
    fn every_moment_has_a_line_and_every_line_a_moment() {
        let lines = Lines::builtin();
        let want = moments();
        for (m, _) in &want {
            assert!(lines.has(m), "no line in play for moment {m}");
        }
        for l in lines.all() {
            assert!(!l.at.is_empty(), "{} has no moment", l.id);
            for at in &l.at {
                assert!(NOT_PLAYED.contains(&at.as_str()) || want.iter().any(|(m, _)| m == at), "{} is at {at}, which the game never asks for", l.id);
            }
        }
    }

    #[test]
    fn ids_are_unique() {
        let lines = Lines::builtin();
        let mut ids: Vec<&str> = lines.all().iter().map(|l| l.id.as_str()).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(n, ids.len());
    }

    #[test]
    fn lines_use_only_their_moments_tokens() {
        let lines = Lines::builtin();
        let want = moments();
        for l in lines.all() {
            for (m, allowed) in want.iter().filter(|(m, _)| l.at.iter().any(|a| a == m)) {
                for t in tokens(&l.text) {
                    assert!(allowed.contains(&t.as_str()), "{}: {{{t}}} can't be filled at {m}", l.id);
                }
            }
        }
    }

    /// The wheel draws eight lines to a page.
    #[test]
    fn wheel_pages_fit() {
        let lines = Lines::builtin();
        for p in ["wheel.dekho", "wheel.panga", "wheel.jeet", "wheel.haar", "wheel.masti"] {
            assert!(lines.pool(p).count() <= 8, "{p} has more than eight lines in play");
        }
    }

    /// The ban list: anything a reader or a filter list has ruled out. Whole words, in any case.
    const BANNED: [&str; 12] = ["chhapri", "chakka", "patakha", "saala", "saale", "sala", "bandar", "bhangi", "gandu", "chutiya", "madarchod", "behenchod"];
    const BANNED_PHRASES: [&str; 2] = ["lag gayi", "kapde utaar"];
    /// Keep-out names: gods, sacred weapons and pots.
    const HOLY: [&str; 12] = ["vajra", "trishul", "brahmastra", "sudarshan", "vishnu", "shiv", "garuda", "kalash", "pushpaka", "kali", "astra", "divya"];

    fn words(text: &str) -> Vec<String> {
        text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
    }

    #[test]
    fn no_banned_words() {
        let lines = Lines::builtin();
        for l in lines.all() {
            let lower = l.text.to_lowercase();
            for p in BANNED_PHRASES {
                assert!(!lower.contains(p), "{}: {p}", l.id);
            }
            for w in words(&l.text) {
                assert!(!BANNED.contains(&w.as_str()), "{}: {w}", l.id);
                assert!(!HOLY.contains(&w.as_str()), "{}: {w}", l.id);
            }
        }
    }

    #[test]
    fn item_names_keep_out_of_the_holy_words() {
        let db = crate::items::ItemDb::load_from_str(include_str!("../../../assets/data/items.ron")).unwrap();
        for (id, item) in &db.items {
            for w in words(&item.name).into_iter().chain(words(&item.lore)) {
                assert!(!HOLY.contains(&w.as_str()), "{id}: {w}");
            }
        }
    }

    #[test]
    fn lines_a_reader_must_rule_on_stay_out_of_play() {
        let lines = Lines::builtin();
        let held: Vec<&Line> = lines.all().iter().filter(|l| l.check == Check::Decide && !l.ok).collect();
        assert!(!held.is_empty());
        for l in held {
            for at in &l.at {
                assert!(lines.pool(at).all(|p| p.id != l.id), "{} plays at {at}", l.id);
            }
        }
    }

    /// Lists what no Delhi native has read yet: cargo test -p shared lines -- --nocapture
    #[test]
    fn unread_lines() {
        let lines = Lines::builtin();
        let unread: Vec<&Line> = lines.all().iter().filter(|l| !l.ok).collect();
        println!("{} of {} lines unread by a Delhi native", unread.len(), lines.all().len());
        for l in unread.iter().filter(|l| l.check != Check::Plain) {
            println!("  {:?} {}: {}", l.check, l.id, l.text);
        }
    }

    #[test]
    fn a_pick_is_the_same_each_time_and_varies_by_seed() {
        let lines = Lines::builtin();
        assert_eq!(lines.pick("kill", 7, 9), lines.pick("kill", 7, 9));
        let picks: std::collections::BTreeSet<&str> = (0..40).map(|s| lines.pick("kill.bat", s, 3)).collect();
        assert!(picks.len() >= 4, "{picks:?}");
    }

    #[test]
    fn lines_read_right_for_the_player_and_for_others() {
        let bunty = Person { name: "Bunty", you: false };
        let you = Person { name: "You", you: true };
        let line = "{k_ne} {v_ko} chop kiya ({w})";
        assert_eq!(about(line, Some(bunty), Some(you), &[("w", "Katta")]), "Bunty ne tumhe chop kiya (Katta)");
        assert_eq!(about(line, Some(you), Some(bunty), &[("w", "Katta")]), "Tumne Bunty ko chop kiya (Katta)");
        assert_eq!(about("{v_ka} panga zameen se ho gaya", None, Some(you), &[]), "Tumhara panga zameen se ho gaya");
        assert_eq!(about("Wo kata! {k_ne} {v_ki} patang kaat di", Some(you), Some(bunty), &[]), "Wo kata! Tumne Bunty ki patang kaat di");
        assert_eq!(about("Ek goli: {v} saaf ({k})", Some(bunty), Some(you), &[]), "Ek goli: tum saaf (Bunty)");
        assert_eq!(about("Ek pe ek free! ({k})", Some(you), Some(bunty), &[]), "Ek pe ek free! (Tum)");
        // A player who types "You", or a token, is just a name.
        let you_named = Person { name: "You", you: false };
        assert_eq!(about(line, Some(you_named), Some(you), &[("w", "Katta")]), "You ne tumhe chop kiya (Katta)");
        let tricky = Person { name: "{k}", you: false };
        assert_eq!(about("{v_ko} {k}", Some(bunty), Some(tricky), &[]), "{k} ko Bunty");
    }
}
