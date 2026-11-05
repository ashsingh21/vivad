//! The loading screen. Building a map's art holds up one frame for seconds (at launch and when a
//! menu choice changes the map), and nothing new is drawn meanwhile: so the screen is drawn
//! first, the work waits until it is on the window, and the screen comes off once that frame is
//! over.

use bevy::prelude::*;
use shared::map::MapId;

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, step.run_if(resource_exists::<Loading>));
    }
}

/// Frames drawn before the work starts (rendering trails the game by a frame), and after it
/// before the screen comes off.
const SETTLE: u32 = 3;
/// And seconds before it starts: at launch the screen's own shaders are still compiling.
const SHOWN: f32 = 0.6;

#[derive(Resource)]
pub struct Loading {
    wait: u32,
    since: f32,
    then: Option<Box<dyn FnOnce(&mut World) + Send + Sync>>,
}

/// Run condition for the other slow builders (the street kit, whose files load meanwhile): not
/// while the loading screen is still waiting to be seen.
pub fn settled(loading: Option<Res<Loading>>) -> bool {
    loading.is_none_or(|l| l.then.is_none())
}

#[derive(Component)]
struct Screen;

/// Shows the loading screen for `map`, then does `then` (the slow part) behind it.
pub fn load(world: &mut World, map: MapId, then: impl FnOnce(&mut World) + Send + Sync + 'static) {
    let since = world.resource::<Time<Real>>().elapsed_secs();
    world.insert_resource(Loading { wait: SETTLE, since, then: Some(Box::new(then)) });
    let text = |s: String, px: f32, c: Color| (Text::new(s), TextFont { font_size: FontSize::Px(px), ..default() }, TextColor(c));
    world
        .spawn((
            Screen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.06, 0.09)),
            GlobalZIndex(30),
        ))
        .with_children(|p| {
            p.spawn((text("PANGA".into(), 64.0, Color::srgb(1.0, 0.82, 0.42)), Node { margin: UiRect::bottom(px(16.0)), ..default() }));
            p.spawn(text(format!("Loading {}...", map.title()), 24.0, Color::srgb(0.75, 0.78, 0.85)));
        });
}

fn step(world: &mut World) {
    let now = world.resource::<Time<Real>>().elapsed_secs();
    let mut loading = world.resource_mut::<Loading>();
    if loading.then.is_some() && now - loading.since < SHOWN {
        return;
    }
    if loading.wait > 0 {
        loading.wait -= 1;
        return;
    }
    if let Some(then) = loading.then.take() {
        loading.wait = SETTLE;
        then(world);
        return;
    }
    world.remove_resource::<Loading>();
    for e in world.query_filtered::<Entity, With<Screen>>().iter(world).collect::<Vec<_>>() {
        world.despawn(e);
    }
}
