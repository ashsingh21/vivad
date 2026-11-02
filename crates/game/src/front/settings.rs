//! What a player sets once and keeps: volume and mouse speed, on the pause menu, saved to a small
//! file in the home folder. Nothing here goes to the sim or the other peers. In the browser it lasts
//! the session: there is no file to write.

use std::path::PathBuf;

use bevy::audio::{GlobalVolume, Volume};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Slider, SliderRange, SliderStep, SliderThumb, SliderValue, TrackClick, slider_self_update};
use serde::{Deserialize, Serialize};

use super::Screen;
use super::theme::{CREAM, Fonts, INK, MARIGOLD, MUSTARD, line, u};
use crate::input::Bindings;

const VOLUME: &str = "Awaaz";
const MOUSE: &str = "Mouse ki speed";

/// What the file holds. A file from an older game or a hand edit fills in what it lacks.
#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// 0 to 1.
    pub volume: f32,
    /// Times the usual mouse speed.
    pub mouse: f32,
    /// The best place today: (the day, the place).
    pub best: Option<(u64, u32)>,
}

impl Default for Saved {
    fn default() -> Self {
        Self { volume: 1.0, mouse: 1.0, best: None }
    }
}

/// Today, as a count of days. The day turns at midnight in Delhi.
pub fn today() -> u64 {
    const IST: u64 = 19_800;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    (now + IST) / 86_400
}

/// VIVAD_SAVE=<file>, else `.panga.ron` in the home folder.
fn path() -> Option<PathBuf> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    let home = || std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| PathBuf::from(h).join(".panga.ron"));
    std::env::var_os("VIVAD_SAVE").map(PathBuf::from).or_else(home)
}

impl Saved {
    fn read(text: &str) -> Self {
        let mut s: Saved = ron::from_str(text).unwrap_or_default();
        s.volume = s.volume.clamp(0.0, 1.0);
        s.mouse = s.mouse.clamp(MOUSE_RANGE.0, MOUSE_RANGE.1);
        s
    }

    fn load() -> Self {
        path().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(Saved::default, |t| Saved::read(&t))
    }

    /// A match's place, against today's best so far: that best if there was one.
    pub fn record(&mut self, day: u64, place: u32) -> Option<u32> {
        let before = self.best.filter(|b| b.0 == day).map(|b| b.1);
        if before.is_none_or(|b| place < b) {
            self.best = Some((day, place));
        }
        before
    }

    pub fn save(&self) {
        let (Some(p), Ok(text)) = (path(), ron::ser::to_string_pretty(self, default())) else { return };
        if let Err(e) = std::fs::write(&p, text) {
            warn!("couldn't save {}: {e}", p.display());
        }
    }
}

const MOUSE_RANGE: (f32, f32) = (0.3, 3.0);

/// Which setting a slider, its thumb, its fill or its number is.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Setting {
    Volume,
    Mouse,
}

#[derive(Component)]
struct Fill;

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(Saved::load())
        .add_systems(Startup, apply)
        .add_systems(Update, follow.run_if(in_state(Screen::Pause)))
        .add_systems(OnExit(Screen::Pause), keep);
}

fn set_volume(volume: &mut GlobalVolume, v: f32) {
    // A test recipe's silence stays.
    if std::env::var("VIVAD_MUTE").is_err() {
        *volume = GlobalVolume::new(Volume::Linear(v));
    }
}

fn set_mouse(bindings: &mut Bindings, times: f32) {
    bindings.mouse_sensitivity = Bindings::default().mouse_sensitivity * times;
}

fn apply(saved: Res<Saved>, mut volume: ResMut<GlobalVolume>, mut bindings: ResMut<Bindings>) {
    set_volume(&mut volume, saved.volume);
    set_mouse(&mut bindings, saved.mouse);
}

/// Follows the sliders: the thumbs and numbers on screen, and what they set, as they move.
fn follow(
    sliders: Query<(&Setting, &SliderValue, &SliderRange), Changed<SliderValue>>,
    mut thumbs: Query<(&Setting, &mut Node), (With<SliderThumb>, Without<Fill>)>,
    mut fills: Query<(&Setting, &mut Node), (With<Fill>, Without<SliderThumb>)>,
    mut numbers: Query<(&Setting, &mut Text)>,
    mut saved: ResMut<Saved>,
    mut volume: ResMut<GlobalVolume>,
    mut bindings: ResMut<Bindings>,
) {
    for (what, value, range) in &sliders {
        let along = range.thumb_position(value.0);
        for (s, mut n) in &mut thumbs {
            if s == what {
                n.left = percent(along * 100.0);
            }
        }
        for (s, mut n) in &mut fills {
            if s == what {
                n.width = percent(along * 100.0);
            }
        }
        for (s, mut t) in &mut numbers {
            if s == what {
                t.0 = shown(*what, value.0);
            }
        }
        match what {
            Setting::Volume if saved.volume != value.0 => {
                saved.volume = value.0;
                set_volume(&mut volume, value.0);
            }
            Setting::Mouse if saved.mouse != value.0 => {
                saved.mouse = value.0;
                set_mouse(&mut bindings, value.0);
            }
            _ => {}
        }
    }
}

fn keep(saved: Res<Saved>, mut wrote: Local<Option<Saved>>) {
    if wrote.as_ref() != Some(&*saved) && (wrote.is_some() || *saved != Saved::load()) {
        saved.save();
    }
    *wrote = Some(saved.clone());
}

fn shown(what: Setting, v: f32) -> String {
    match what {
        Setting::Volume => format!("{:.0}%", v * 100.0),
        Setting::Mouse => format!("{v:.1}x"),
    }
}

/// The two sliders, for the pause menu.
pub(super) fn sliders(p: &mut ChildSpawnerCommands, fonts: &Fonts, saved: &Saved) {
    slider(p, fonts, VOLUME, Setting::Volume, saved.volume, (0.0, 1.0), 0.05);
    slider(p, fonts, MOUSE, Setting::Mouse, saved.mouse, MOUSE_RANGE, 0.1);
}

fn slider(p: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, what: Setting, value: f32, range: (f32, f32), step: f32) {
    const THUMB: f32 = 24.0;
    let along = SliderRange::new(range.0, range.1).thumb_position(value) * 100.0;
    p.spawn((Node { align_items: AlignItems::Center, column_gap: u(16.0), ..default() }, Pickable::IGNORE)).with_children(|row| {
        row.spawn((Node { width: u(190.0), ..default() }, Pickable::IGNORE)).with_child(line(label, &fonts.body, 23.0, CREAM));
        row.spawn((
            Slider { track_click: TrackClick::Snap, ..default() },
            SliderValue(value),
            SliderRange::new(range.0, range.1),
            SliderStep(step),
            what,
            Hovered::default(),
            Node { flex_grow: 1.0, height: u(30.0), border: UiRect::all(u(3.0)), ..default() },
            BackgroundColor(INK),
            BorderColor::all(CREAM),
        ))
        .observe(slider_self_update)
        .with_children(|s| {
            // Short of the slider by the thumb, so the thumb can go by simple shares.
            s.spawn((Node { position_type: PositionType::Absolute, left: Val::ZERO, right: u(THUMB), top: Val::ZERO, bottom: Val::ZERO, ..default() }, Pickable::IGNORE)).with_children(|t| {
                t.spawn((Fill, what, Node { position_type: PositionType::Absolute, left: Val::ZERO, top: Val::ZERO, bottom: Val::ZERO, width: percent(along), ..default() }, BackgroundColor(MARIGOLD), Pickable::IGNORE));
                t.spawn((
                    SliderThumb,
                    what,
                    Node { position_type: PositionType::Absolute, left: percent(along), top: Val::ZERO, bottom: Val::ZERO, width: u(THUMB), border: UiRect::horizontal(u(3.0)), ..default() },
                    BackgroundColor(MUSTARD),
                    BorderColor::all(INK),
                ));
            });
        });
        row.spawn((Node { width: u(74.0), justify_content: JustifyContent::End, ..default() }, Pickable::IGNORE)).with_child((what, line(shown(what, value), &fonts.number, 28.0, MUSTARD)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_file_comes_back_the_same() {
        let s = Saved { volume: 0.35, mouse: 1.7, best: Some((20_000, 7)) };
        assert_eq!(Saved::read(&ron::ser::to_string_pretty(&s, default()).unwrap()), s);
    }

    #[test]
    fn a_bad_or_old_file_does_no_harm() {
        assert_eq!(Saved::read("not a settings file"), Saved::default());
        assert_eq!(Saved::read("(volume: 0.5)"), Saved { volume: 0.5, ..default() });
        assert_eq!(Saved::read("(volume: 9.0, mouse: 0.0)"), Saved { volume: 1.0, mouse: MOUSE_RANGE.0, best: None });
    }

    #[test]
    fn best_today_keeps_the_best_and_starts_over_tomorrow() {
        let mut s = Saved::default();
        assert_eq!(s.record(10, 20), None);
        assert_eq!(s.record(10, 7), Some(20));
        assert_eq!(s.record(10, 12), Some(7));
        assert_eq!(s.best, Some((10, 7)));
        assert_eq!(s.record(11, 30), None, "a new day");
        assert_eq!(s.best, Some((11, 30)));
    }
}
