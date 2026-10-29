//! Authoritative game simulation. Runs in `FixedUpdate`, reads `PlayerIntent`, never touches
//! rendering. A future server runs exactly this plugin; clients only add presentation.

pub mod bots;
pub mod combat;
pub mod flow;
pub mod loot;
pub mod movement;
pub mod zone;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::items::{ItemDb, ItemStack};
use crate::map::{Block, MapLayout};

pub const TICK_HZ: f64 = 60.0;

// Character geometry: capsule centred on the Transform.
pub const CAPSULE_RADIUS: f32 = 0.38;
pub const CAPSULE_LENGTH: f32 = 1.0;
pub const HALF_HEIGHT: f32 = CAPSULE_RADIUS + CAPSULE_LENGTH / 2.0;
/// Eye / camera pivot height above the capsule centre.
pub const EYE_OFFSET: f32 = 0.72;
/// Over-the-shoulder camera, shared so server-side aim matches the client crosshair.
pub const CAMERA_DISTANCE: f32 = 3.4;
pub const CAMERA_SHOULDER: f32 = 0.65;
pub const HEADSHOT_HEIGHT: f32 = 0.5;

#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub enum Layer {
    #[default]
    World,
    Character,
    Door,
    Pickup,
}

/// The world for line-of-sight and movement: static geometry plus doors.
pub fn solid_mask() -> LayerMask {
    LayerMask::from([Layer::World, Layer::Door])
}

/// What projectiles and melee can hit.
pub fn hit_mask() -> LayerMask {
    LayerMask::from([Layer::World, Layer::Door, Layer::Character])
}

// ------------------------------------------------------------------ intent (the only input)

pub mod btn {
    pub const JUMP: u16 = 1 << 0;
    pub const FIRE: u16 = 1 << 1;
    pub const AIM: u16 = 1 << 2;
    pub const RELOAD: u16 = 1 << 3;
    pub const INTERACT: u16 = 1 << 4;
    pub const SPRINT: u16 = 1 << 5;
    pub const CROUCH: u16 = 1 << 6;
}

/// Everything a controller (human, bot or network peer) can ask a warrior to do this tick.
#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct PlayerIntent {
    /// x = strafe right, y = forward; length <= 1.
    pub move_axis: Vec2,
    pub yaw: f32,
    pub pitch: f32,
    /// Buttons currently held.
    pub held: u16,
    /// Buttons pressed since the last simulation tick (latched by the controller, cleared by the sim).
    pub pressed: u16,
    /// Released since the last tick.
    pub released: u16,
    /// Requested hotbar slot (0 = melee, 1..=5 = inventory).
    pub select: Option<u8>,
}

impl PlayerIntent {
    pub fn held(&self, b: u16) -> bool {
        self.held & b != 0
    }
    pub fn pressed(&self, b: u16) -> bool {
        self.pressed & b != 0
    }
    pub fn released(&self, b: u16) -> bool {
        self.released & b != 0
    }
    pub fn aim_rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }
    pub fn aim_dir(&self) -> Vec3 {
        self.aim_rotation() * Vec3::NEG_Z
    }
    pub fn body_rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }
}

// ------------------------------------------------------------------ warrior state

#[derive(Component, Clone, Debug, Reflect)]
pub struct Warrior {
    pub name: String,
    pub is_bot: bool,
    /// Index used by clients to pick a colour / outfit.
    pub look: u32,
    pub kills: u32,
}

/// Marks the warrior controlled from this machine (set by the client, ignored by the sim).
#[derive(Component)]
pub struct LocalPlayer;

#[derive(Component, Clone, Debug, Reflect)]
pub struct Health {
    pub hp: f32,
    pub max_hp: f32,
    pub shield: f32,
    pub max_shield: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self { hp: 100.0, max_hp: 100.0, shield: 0.0, max_shield: 100.0 }
    }
}

#[derive(Component, Clone, Debug, Reflect)]
pub struct Eliminated {
    pub placement: u32,
    pub by: Option<Entity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect, Default)]
pub enum MoveMode {
    /// Riding the Pushpaka Vimana.
    #[default]
    Vimana,
    Freefall,
    Gliding,
    Ground,
}

#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct Kinematic {
    pub velocity: Vec3,
    pub grounded: bool,
    /// Surface normal under the feet while grounded.
    pub ground_normal: Vec3,
    pub mode: MoveMode,
    /// Highest point since last on the ground on foot (fall damage is by height dropped).
    /// None while airborne untracked: thrown by a wind astra, which lands harmlessly.
    pub fall_from: Option<f32>,
}

/// Previous and current fixed-tick positions so clients can interpolate between ticks.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
pub struct PosHistory {
    pub prev: Vec3,
    pub curr: Vec3,
}

pub const INVENTORY_SLOTS: usize = 5;

#[derive(Component, Clone, Debug, Reflect)]
pub struct Inventory {
    pub melee: ItemStack,
    pub slots: [Option<ItemStack>; INVENTORY_SLOTS],
    /// 0 = melee, 1..=5 = slots.
    pub selected: usize,
    pub arrows: u32,
}

impl Inventory {
    pub fn new(melee: &str) -> Self {
        Self {
            melee: ItemStack { id: melee.into(), rarity: crate::items::Rarity::Common, count: 1 },
            slots: Default::default(),
            selected: 0,
            arrows: 0,
        }
    }
    pub fn current(&self) -> Option<&ItemStack> {
        if self.selected == 0 { Some(&self.melee) } else { self.slots[self.selected - 1].as_ref() }
    }
    pub fn current_mut(&mut self) -> Option<&mut ItemStack> {
        if self.selected == 0 { Some(&mut self.melee) } else { self.slots[self.selected - 1].as_mut() }
    }
}

/// Transient weapon/use timers.
#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct Action {
    pub cooldown: f32,
    pub reload_left: f32,
    pub charge: f32,
    /// Melee swing: time until the hit lands.
    pub swing_hit_in: Option<f32>,
    /// Consumable in use: (slot, time left).
    pub using: Option<(usize, f32)>,
    /// Seconds since the last swing or shot (for client animation).
    pub since_attack: f32,
}

// ------------------------------------------------------------------ world entities

#[derive(Component, Clone, Debug)]
pub struct MapBlock(pub Block);

/// The heightfield ground; clients build its mesh from `MapLayout::terrain`.
#[derive(Component, Clone, Copy, Debug)]
pub struct MapTerrain;

/// Index into `MapLayout::props`; clients attach the art.
#[derive(Component, Clone, Copy, Debug)]
pub struct PropIndex(pub usize);

#[derive(Component, Clone, Debug)]
pub struct Door {
    pub hp: f32,
}

// ------------------------------------------------------------------ messages

#[derive(Message, Clone, Debug)]
pub struct DamageMsg {
    pub target: Entity,
    pub amount: f32,
    pub source: Option<Entity>,
    pub headshot: bool,
    pub cause: DamageCause,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DamageCause {
    Weapon(String),
    Pralaya,
    Fall,
}

/// A feed of eliminations and notable events, consumed by clients.
#[derive(Message, Clone, Debug)]
pub struct FeedMsg(pub String);

/// Hit confirmations for the attacker's HUD (damage numbers, hit markers).
#[derive(Message, Clone, Debug)]
pub struct HitMsg {
    pub attacker: Entity,
    pub target: Entity,
    pub amount: f32,
    pub headshot: bool,
    pub shield: bool,
    pub at: Vec3,
}

#[derive(Message, Clone, Debug, Default)]
pub struct RestartMatch;

// ------------------------------------------------------------------ plugin

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet {
    Brains,
    Flow,
    Movement,
    Combat,
    Damage,
    Loot,
    Zone,
    Cleanup,
}

#[derive(Resource, Clone, Copy)]
pub struct MatchConfig {
    pub map_seed: u64,
    pub bots: u32,
    /// Let a bot brain drive the local warrior (automated play-testing).
    pub autopilot: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self { map_seed: map_seed(), bots: 15, autopilot: false }
    }
}

/// The map seed: VIVAD_MAP_SEED=<number>, or =random for a fresh map each launch (printed so it
/// can be replayed); 7 when unset. The seed varies the hills, groves, village layouts and loot;
/// the landmarks keep their places.
pub fn map_seed() -> u64 {
    match std::env::var("VIVAD_MAP_SEED").as_deref() {
        Ok("random") => {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let seed = (t as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 44;
            println!("map seed {seed} (VIVAD_MAP_SEED={seed} to play it again)");
            seed
        }
        Ok(s) => s.trim().parse().unwrap_or_else(|_| panic!("VIVAD_MAP_SEED: expected a number or `random`, got {s:?}")),
        Err(_) => 7,
    }
}

pub struct SimPlugin {
    pub items_ron: String,
}

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        let db = ItemDb::load_from_str(&self.items_ron).expect("items.ron");
        let config = app.world().get_resource::<MatchConfig>().copied().unwrap_or_default();
        let layout = crate::map::india::generate(config.map_seed);
        app.insert_resource(db)
            .insert_resource(config)
            .insert_resource(layout)
            .insert_resource(Time::<Fixed>::from_hz(TICK_HZ))
            .insert_resource(Gravity(Vec3::ZERO))
            .add_message::<DamageMsg>()
            .add_message::<FeedMsg>()
            .add_message::<HitMsg>()
            .add_message::<RestartMatch>()
            .configure_sets(
                FixedUpdate,
                (
                    SimSet::Brains,
                    SimSet::Flow,
                    SimSet::Movement,
                    SimSet::Combat,
                    SimSet::Damage,
                    SimSet::Loot,
                    SimSet::Zone,
                    SimSet::Cleanup,
                )
                    .chain(),
            )
            .add_systems(Startup, spawn_map)
            .add_systems(FixedFirst, record_prev_positions)
            .add_systems(FixedUpdate, clear_latched_input.in_set(SimSet::Cleanup))
            .add_plugins((
                flow::FlowPlugin,
                movement::MovementPlugin,
                combat::CombatPlugin,
                loot::LootPlugin,
                zone::ZonePlugin,
                bots::BotPlugin,
            ));
    }
}

fn spawn_map(mut commands: Commands, layout: Res<MapLayout>) {
    let t = &layout.terrain;
    commands.spawn((
        MapTerrain,
        Transform::default(),
        RigidBody::Static,
        Collider::heightfield(t.rows(), Vec3::new(t.size, 1.0, t.size)),
        CollisionLayers::new(Layer::World, LayerMask::ALL),
    ));
    for block in &layout.blocks {
        let mut e = commands.spawn((
            MapBlock(block.clone()),
            Transform::from_translation(block.pos).with_rotation(block.rot),
        ));
        if block.collide {
            e.insert((RigidBody::Static, collider_for(block), CollisionLayers::new(Layer::World, LayerMask::ALL)));
        }
    }
    for door in &layout.doors {
        commands.spawn((
            MapBlock(door.clone()),
            Door { hp: 60.0 },
            Transform::from_translation(door.pos).with_rotation(door.rot),
            RigidBody::Static,
            collider_for(door),
            CollisionLayers::new(Layer::Door, LayerMask::ALL),
        ));
    }
    for (i, prop) in layout.props.iter().enumerate() {
        let mut e = commands.spawn((
            PropIndex(i),
            Transform::from_translation(prop.pos).with_rotation(Quat::from_rotation_y(prop.yaw)).with_scale(Vec3::splat(prop.scale)),
        ));
        if let Some((r, h)) = prop.trunk {
            // Collider as a child so the prop's scale doesn't double-apply.
            e.with_children(|c| {
                c.spawn((
                    Transform::from_translation(Vec3::Y * h / prop.scale / 2.0).with_scale(Vec3::splat(1.0 / prop.scale)),
                    RigidBody::Static,
                    Collider::cylinder(r, h),
                    CollisionLayers::new(Layer::World, LayerMask::ALL),
                ));
            });
        }
    }
    info!(
        "map {}: {} blocks, {} props, {} doors, {} chests, {} floor loot",
        layout.name,
        layout.blocks.len(),
        layout.props.len(),
        layout.doors.len(),
        layout.chests.len(),
        layout.floor_loot.len()
    );
}

pub fn collider_for(block: &Block) -> Collider {
    match block.shape {
        crate::map::Shape::Box(s) => Collider::cuboid(s.x, s.y, s.z),
        crate::map::Shape::Cylinder { radius, height } => Collider::cylinder(radius, height),
        crate::map::Shape::Sphere(r) => Collider::sphere(r),
    }
}

fn record_prev_positions(mut q: Query<(&Transform, &mut PosHistory)>) {
    for (t, mut h) in &mut q {
        h.prev = t.translation;
    }
}

fn clear_latched_input(mut q: Query<(&Transform, &mut PlayerIntent, Option<&mut PosHistory>)>) {
    for (t, mut i, h) in &mut q {
        i.pressed = 0;
        i.released = 0;
        i.select = None;
        if let Some(mut h) = h {
            h.curr = t.translation;
        }
    }
}

/// Camera-equivalent aim ray, shared by player shots and the client crosshair.
pub fn aim_ray(center: Vec3, intent: &PlayerIntent) -> (Vec3, Vec3) {
    let rot = intent.aim_rotation();
    let pivot = center + Vec3::Y * EYE_OFFSET;
    let origin = pivot + rot * Vec3::new(CAMERA_SHOULDER, 0.0, CAMERA_DISTANCE);
    (origin, rot * Vec3::NEG_Z)
}
