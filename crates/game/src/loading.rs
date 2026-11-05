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
pub struct Screen;

/// Shows the loading screen for `map`, then does `then` (the slow part) behind it.
pub fn load(world: &mut World, map: MapId, then: impl FnOnce(&mut World) + Send + Sync + 'static) {
    let since = world.resource::<Time<Real>>().elapsed_secs();
    world.insert_resource(Loading { wait: SETTLE, since, then: Some(Box::new(then)) });
    let font = world.get_resource::<crate::ui::UiFonts>().cloned();
    let k = world.query_filtered::<&Window, With<bevy::window::PrimaryWindow>>().iter(world).next().map(crate::ui::scale).unwrap_or(1.0);
    let face = |bold: bool| font.as_ref().map(|f| if bold { f.bold.clone() } else { f.semi.clone() }).unwrap_or_default();
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
            BackgroundColor(Color::srgb(0.03, 0.035, 0.06)),
            GlobalZIndex(30),
        ))
        .with_children(|p| {
            p.spawn((crate::ui::text(&face(true), "PANGA", 96.0, k, crate::ui::GOLD), Node { margin: UiRect::bottom(px(8.0 * k)), ..default() }));
            p.spawn(crate::ui::text(&face(false), format!("{} load ho raha hai...", map.title()), 22.0, k, crate::ui::MUTED));
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
