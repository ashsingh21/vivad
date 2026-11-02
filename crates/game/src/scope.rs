//! Looking through a rifle's fixed scope. Holding aim with a scoped gun puts the camera at the eye
//! (crate::camera), narrows the view, hides the warrior's own body (crate::visuals) and lays a ring
//! and reticle over the 3D view, under the HUD (assets/textures/panga/scope_*.png, made by
//! tools/textures/scope.py). Draw distances stretch with the zoom, so a far crate that stops a bullet
//! can be seen through the glass; a big map's mid tier shows such cover already.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use shared::items::ItemDb;
use shared::map::MapLayout;
use shared::sim::{Action, Eliminated, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, btn};

use crate::buildings::tiers::big_map;
use crate::characters::LodRange;
use crate::quality::LodScale;

/// The magnification the local player is looking through; 0 when not at a scope.
#[derive(Resource, Default, PartialEq)]
pub struct Scope(pub f32);

impl Scope {
    pub fn active(&self) -> bool {
        self.0 > 1.0
    }

    /// The camera's vertical field of view through it, in degrees: the normal 70, magnified.
    pub fn fov(&self) -> f32 {
        2.0 * (35f32.to_radians().tan() / self.0).atan().to_degrees()
    }
}

pub struct ScopePlugin;

impl Plugin for ScopePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scope>().add_systems(Startup, build_ring).add_systems(Update, (watch, show_ring, stretch_lods).chain());
    }
}

#[derive(Component)]
struct RingRoot;

/// The ring and reticle image for each magnification.
#[derive(Component)]
struct Ring([(f32, Handle<Image>); 3]);

fn build_ring(mut commands: Commands, assets: Res<AssetServer>) {
    let image = |zoom: u32| (zoom as f32, assets.load(format!("textures/panga/scope_{zoom}x.png")));
    let rings = [image(2), image(4), image(6)];
    let first = rings[0].1.clone();
    let side = || (Node { flex_grow: 1.0, height: percent(100.0), ..default() }, BackgroundColor(Color::BLACK));
    commands
        .spawn((
            RingRoot,
            Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() },
            // Over the 3D view (i32::MIN), under the HUD.
            GlobalZIndex(-1),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn(side());
            p.spawn((Ring(rings), ImageNode::new(first), Node { height: percent(100.0), aspect_ratio: Some(1.0), ..default() }));
            p.spawn(side());
        });
}

fn watch(mut scope: ResMut<Scope>, local: Query<(&Inventory, &PlayerIntent, &Kinematic, &Action, Has<Eliminated>), With<LocalPlayer>>, db: Res<ItemDb>) {
    let zoom = local
        .single()
        .ok()
        .filter(|(_, intent, kin, act, dead)| !dead && kin.mode == MoveMode::Ground && intent.held(btn::AIM) && act.reload_left <= 0.0)
        .and_then(|(inv, ..)| inv.current().and_then(|s| db.get(&s.id).kind.scope()))
        .unwrap_or(0.0);
    scope.set_if_neq(Scope(zoom));
}

fn show_ring(scope: Res<Scope>, mut root: Query<&mut Visibility, With<RingRoot>>, mut ring: Query<(&Ring, &mut ImageNode)>) {
    if !scope.is_changed() {
        return;
    }
    for mut v in &mut root {
        *v = if scope.active() { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (r, mut node) in &mut ring {
        if let Some((_, image)) = r.0.iter().find(|(zoom, _)| *zoom == scope.0) {
            node.image = image.clone();
        }
    }
}

/// A prop seen 4x larger shows the detail of one 4x nearer, and must not vanish at its usual range.
/// Not on a big map: its near things exist only inside the near tier's reach (crate::batch::near),
/// so a stretched near tier would hide the mid tier's cover and trees with nothing in their place,
/// and draw the whole city at full detail.
fn stretch_lods(scope: Res<Scope>, lod: Res<LodScale>, layout: Res<MapLayout>, mut q: Query<(&LodRange, &mut VisibilityRange)>) {
    if scope.is_changed() && !big_map(&layout) {
        for (base, mut range) in &mut q {
            *range = base.scaled(lod.0 * scope.0.max(1.0));
        }
    }
}
