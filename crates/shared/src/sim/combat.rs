//! Weapons, projectiles, astras, consumables, damage and eliminations.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::{
    Action, DamageCause, DamageMsg, Door, Eliminated, FeedMsg, HEADSHOT_HEIGHT, Health, HitMsg, Inventory,
    Kinematic, MoveMode, PlayerIntent, SimSet, Warrior, aim_ray, btn, hit_mask,
};
use crate::items::{AstraEffect, ItemDb, ItemKind, ItemStack};
use crate::sim::net::{NetId, NetMsg, Outbox, Remote, Shot, drop_loot};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<KillMsg>().add_message::<MisfireMsg>().add_systems(
            FixedUpdate,
            (
                (select_slot, use_items).chain().in_set(SimSet::Combat),
                (step_projectiles, tick_fire_zones).in_set(SimSet::Combat).after(use_items),
                (apply_damage, count_kills).chain().in_set(SimSet::Damage),
            ),
        );
    }
}

/// A flying arrow or thrown astra.
#[derive(Component, Clone, Debug)]
pub struct Projectile {
    pub owner: Entity,
    pub velocity: Vec3,
    pub gravity: f32,
    pub damage: f32,
    pub headshot_mult: f32,
    pub ttl: f32,
    pub item: String,
    pub astra: Option<AstraEffect>,
    /// Stuck in a wall: no longer moves, fades out.
    pub stuck: bool,
}

/// Burning ground left by an Agneyastra.
#[derive(Component, Clone, Debug)]
pub struct FireZone {
    pub owner: Entity,
    pub radius: f32,
    pub dps: f32,
    pub ttl: f32,
}

/// One-shot visual cue for clients (wind bursts etc.).
#[derive(Component, Clone, Debug)]
pub struct Burst {
    pub radius: f32,
    pub ttl: f32,
}

fn select_slot(mut q: Query<(&PlayerIntent, &mut Inventory, &mut Action), (Without<Eliminated>, Without<Remote>)>) {
    for (intent, mut inv, mut act) in &mut q {
        if let Some(s) = intent.select {
            let s = s as usize;
            if s != inv.selected && (s == 0 || inv.slots.get(s - 1).is_some_and(|x| x.is_some())) {
                inv.selected = s;
                act.reload_left = 0.0;
                act.charge = 0.0;
                act.using = None;
                act.swing_hit_in = None;
                act.cooldown = act.cooldown.max(0.25);
            }
        }
        if inv.selected > 0 && inv.slots[inv.selected - 1].is_none() {
            inv.selected = 0;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn use_items(
    mut commands: Commands,
    mut q: Query<
        (Entity, &Transform, &PlayerIntent, &mut Inventory, &mut Action, &mut Health, &Kinematic, Option<&NetId>),
        (Without<Eliminated>, Without<Remote>),
    >,
    db: Res<ItemDb>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
    mut misfires: MessageWriter<MisfireMsg>,
    mut outbox: ResMut<Outbox>,
) {
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    for (entity, tf, intent, mut inv, mut act, mut health, kin, net_id) in &mut q {
        // Arrows and astras are announced so the other peers fly them too.
        let mut announce = |p: &Projectile, from: Vec3| {
            if let Some(&owner) = net_id {
                outbox.0.push(NetMsg::Shot(Shot {
                    owner,
                    from,
                    velocity: p.velocity,
                    gravity: p.gravity,
                    damage: p.damage,
                    headshot_mult: p.headshot_mult,
                    item: p.item.clone(),
                    astra: p.astra.clone(),
                }));
            }
        };
        act.cooldown = (act.cooldown - dt).max(0.0);
        act.since_attack += dt;
        if kin.mode != MoveMode::Ground {
            act.charge = 0.0;
            continue;
        }
        let Some(stack) = inv.current().cloned() else { continue };
        let def = db.get(&stack.id);
        let scale = stack.rarity.damage_scale();
        let (eye_origin, aim) = aim_ray(tf.translation, intent);
        let muzzle = tf.translation + Vec3::Y * 0.5 + intent.body_rotation() * Vec3::new(0.3, 0.0, -0.4);

        match &def.kind {
            ItemKind::Melee { damage: dmg, interval, reach, hit_delay } => {
                if let Some(t) = act.swing_hit_in.as_mut() {
                    *t -= dt;
                    if *t <= 0.0 {
                        act.swing_hit_in = None;
                        // Swept sphere along the swing direction (flattened aim).
                        let dir = Dir3::new(Vec3::new(aim.x, aim.y.clamp(-0.5, 0.5), aim.z)).unwrap_or(Dir3::NEG_Z);
                        let start = tf.translation + Vec3::Y * 0.35;
                        let filter = SpatialQueryFilter::from_mask(hit_mask()).with_excluded_entities([entity]);
                        if let Some(hit) = spatial.cast_shape(
                            &Collider::sphere(0.45),
                            start,
                            Quat::IDENTITY,
                            dir,
                            &ShapeCastConfig::from_max_distance(*reach),
                            &filter,
                        ) {
                            damage.write(DamageMsg {
                                target: hit.entity,
                                amount: dmg * scale,
                                source: Some(entity),
                                headshot: false,
                                cause: DamageCause::Weapon(stack.id.clone()),
                            });
                        }
                    }
                } else if intent.held(btn::FIRE) && act.cooldown <= 0.0 {
                    act.swing_hit_in = Some(*hit_delay);
                    act.cooldown = *interval;
                    act.since_attack = 0.0;
                }
            }
            ItemKind::Bow { damage: dmg, interval, speed, gravity, pellets, spread_deg, magazine, reload, charge, headshot, misfire, backfire } => {
                let slot = inv.selected;
                // Reloading.
                if act.reload_left > 0.0 {
                    act.reload_left -= dt;
                    if act.reload_left <= 0.0 {
                        act.reload_left = 0.0;
                        let have = inv.slots[slot - 1].as_ref().map(|s| s.count).unwrap_or(0);
                        let take = (magazine - have).min(inv.arrows);
                        inv.arrows -= take;
                        if let Some(s) = inv.slots[slot - 1].as_mut() {
                            s.count += take;
                        }
                    }
                    continue;
                }
                let loaded = stack.count;
                let wants_reload = intent.pressed(btn::RELOAD) && loaded < *magazine;
                if (loaded == 0 || wants_reload) && inv.arrows > 0 {
                    act.reload_left = *reload;
                    act.charge = 0.0;
                    continue;
                }
                if loaded == 0 {
                    continue;
                }
                let release = if *charge > 0.0 {
                    if intent.held(btn::FIRE) && act.cooldown <= 0.0 {
                        act.charge = (act.charge + dt).min(*charge);
                        None
                    } else if act.charge > 0.0 && !intent.held(btn::FIRE) {
                        let frac = (act.charge / charge).clamp(0.3, 1.0);
                        act.charge = 0.0;
                        Some(frac)
                    } else {
                        None
                    }
                } else if intent.held(btn::FIRE) && act.cooldown <= 0.0 {
                    Some(1.0)
                } else {
                    None
                };
                let Some(power) = release else { continue };
                // A katta lets you down now and then: a click and a spent round, or worse.
                let roll = rng.random_range(0.0f32..1.0);
                if roll < misfire + backfire {
                    let blew = roll < *backfire;
                    if blew {
                        damage.write(DamageMsg { target: entity, amount: BACKFIRE_DAMAGE, source: Some(entity), headshot: false, cause: DamageCause::Weapon(stack.id.clone()) });
                    }
                    misfires.write(MisfireMsg { who: entity, at: muzzle, backfire: blew });
                    if let Some(s) = inv.slots[slot - 1].as_mut() {
                        s.count -= 1;
                    }
                    act.cooldown = *interval;
                    act.since_attack = 0.0;
                    continue;
                }
                // Aim at whatever the crosshair ray hits, fire from the bow.
                let filter = SpatialQueryFilter::from_mask(hit_mask()).with_excluded_entities([entity]);
                let target = spatial
                    .cast_ray(eye_origin, Dir3::new(aim).unwrap_or(Dir3::NEG_Z), 600.0, true, &filter)
                    .map(|h| eye_origin + aim * h.distance)
                    .unwrap_or(eye_origin + aim * 600.0);
                let base = (target - muzzle).normalize_or(aim);
                let ads = intent.held(btn::AIM);
                let spread = spread_deg.to_radians() * if ads { 0.6 } else { 1.0 };
                for _ in 0..*pellets {
                    let yaw_off = rng.random_range(-1.0f32..1.0) * spread;
                    let pitch_off = rng.random_range(-1.0f32..1.0) * spread * 0.6;
                    let rot = Quat::from_axis_angle(Vec3::Y, yaw_off)
                        * Quat::from_axis_angle(base.cross(Vec3::Y).normalize_or(Vec3::X), pitch_off);
                    let dir = rot * base;
                    let arrow = Projectile {
                        owner: entity,
                        velocity: dir * speed * (0.55 + 0.45 * power),
                        gravity: *gravity,
                        damage: dmg * scale * power,
                        headshot_mult: *headshot,
                        ttl: 4.0,
                        item: stack.id.clone(),
                        astra: None,
                        stuck: false,
                    };
                    announce(&arrow, muzzle);
                    commands.spawn((arrow, Transform::from_translation(muzzle).looking_to(dir, Vec3::Y)));
                }
                if let Some(s) = inv.slots[slot - 1].as_mut() {
                    s.count -= 1;
                }
                act.cooldown = *interval;
                act.since_attack = 0.0;
            }
            ItemKind::Astra { effect, speed, .. } => {
                if intent.pressed(btn::FIRE) && act.cooldown <= 0.0 {
                    let dir = (aim + Vec3::Y * 0.18).normalize();
                    let astra = Projectile {
                        owner: entity,
                        velocity: dir * speed,
                        gravity: 16.0,
                        damage: 0.0,
                        headshot_mult: 1.0,
                        ttl: 6.0,
                        item: stack.id.clone(),
                        astra: Some(effect.clone()),
                        stuck: false,
                    };
                    announce(&astra, muzzle);
                    commands.spawn((astra, Transform::from_translation(muzzle)));
                    let slot = inv.selected;
                    if let Some(s) = inv.slots[slot - 1].as_mut() {
                        s.count -= 1;
                        if s.count == 0 {
                            inv.slots[slot - 1] = None;
                        }
                    }
                    act.cooldown = 0.8;
                    act.since_attack = 0.0;
                }
            }
            ItemKind::Consumable { heal, heal_cap, shield, use_time, .. } => {
                let slot = inv.selected;
                match act.using {
                    Some((s, t)) if s == slot => {
                        let t = t - dt;
                        if t <= 0.0 {
                            act.using = None;
                            health.hp = (health.hp + heal).min(heal_cap.max(health.hp)).min(health.max_hp);
                            health.shield = (health.shield + shield).min(health.max_shield);
                            if let Some(st) = inv.slots[slot - 1].as_mut() {
                                st.count -= 1;
                                if st.count == 0 {
                                    inv.slots[slot - 1] = None;
                                }
                            }
                        } else {
                            act.using = Some((s, t));
                        }
                    }
                    _ => {
                        let useful = (*heal > 0.0 && health.hp < heal_cap.min(health.max_hp))
                            || (*shield > 0.0 && health.shield < health.max_shield);
                        if intent.pressed(btn::FIRE) && useful {
                            act.using = Some((slot, *use_time));
                        }
                    }
                }
            }
            ItemKind::Arrows { .. } => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn step_projectiles(
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut Projectile)>,
    warriors: Query<&Transform, (With<Warrior>, Without<Projectile>)>,
    mut kin: Query<(&Transform, &mut Kinematic), (With<Warrior>, Without<Eliminated>, Without<Projectile>, Without<Remote>)>,
    remote: Query<(), With<Remote>>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut p) in &mut q {
        // An echo of another peer's arrow: its shooter decides what it hit.
        let echo = remote.contains(p.owner);
        p.ttl -= dt;
        if p.ttl <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        if p.stuck {
            continue;
        }
        p.velocity.y -= p.gravity * dt;
        let step = p.velocity * dt;
        let Ok((dir, dist)) = Dir3::new_and_length(step) else { continue };
        let filter = SpatialQueryFilter::from_mask(hit_mask()).with_excluded_entities([p.owner]);
        if let Some(hit) = spatial.cast_ray(tf.translation, dir, dist, true, &filter) {
            let point = tf.translation + dir * hit.distance;
            if let Some(effect) = p.astra.clone() {
                astra_burst(&mut commands, &mut kin, p.owner, point, &effect);
                commands.entity(e).despawn();
                continue;
            }
            let headshot = warriors
                .get(hit.entity)
                .map(|t| point.y > t.translation.y + HEADSHOT_HEIGHT)
                .unwrap_or(false);
            if !echo {
                damage.write(DamageMsg {
                    target: hit.entity,
                    amount: if headshot { p.damage * p.headshot_mult } else { p.damage },
                    source: Some(p.owner),
                    headshot,
                    cause: DamageCause::Weapon(p.item.clone()),
                });
            }
            if warriors.contains(hit.entity) {
                commands.entity(e).despawn();
            } else {
                tf.translation = point;
                p.stuck = true;
                p.ttl = p.ttl.min(6.0);
            }
            continue;
        }
        tf.translation += step;
        tf.look_to(dir, Vec3::Y);
    }
}

fn astra_burst(
    commands: &mut Commands,
    kin: &mut Query<(&Transform, &mut Kinematic), (With<Warrior>, Without<Eliminated>, Without<Projectile>, Without<Remote>)>,
    owner: Entity,
    at: Vec3,
    effect: &AstraEffect,
) {
    match effect {
        AstraEffect::Fire { radius, dps, duration } => {
            commands.spawn((
                FireZone { owner, radius: *radius, dps: *dps, ttl: *duration },
                Transform::from_translation(at),
            ));
        }
        AstraEffect::Wind { radius, impulse } => {
            commands.spawn((Burst { radius: *radius, ttl: 0.6 }, Transform::from_translation(at)));
            for (tf, mut k) in kin.iter_mut() {
                let d = tf.translation - at;
                if d.length() < *radius {
                    let out = d.with_y(0.0).normalize_or(Vec3::X);
                    k.velocity = out * impulse * 0.45 + Vec3::Y * impulse;
                    k.grounded = false;
                    // Lands as a normal jump, without fall damage.
                    k.fall_from = None;
                }
            }
        }
    }
}

fn tick_fire_zones(
    mut commands: Commands,
    mut zones: Query<(Entity, &Transform, &mut FireZone)>,
    warriors: Query<(Entity, &Transform), (With<Warrior>, Without<Eliminated>, Without<Remote>)>,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let dt = time.delta_secs();
    for (e, tf, mut z) in &mut zones {
        z.ttl -= dt;
        if z.ttl <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        for (we, wt) in &warriors {
            let d = wt.translation - tf.translation;
            if d.with_y(0.0).length() < z.radius && d.y > -1.0 && d.y < 3.0 {
                damage.write(DamageMsg {
                    target: we,
                    amount: z.dps * dt,
                    source: Some(z.owner),
                    headshot: false,
                    cause: DamageCause::Weapon("agneyastra".into()),
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn apply_damage(
    mut commands: Commands,
    mut msgs: MessageReader<DamageMsg>,
    // Each peer applies damage only to its own warriors (hits on others go to their owners).
    mut targets: Query<(&mut Health, &Transform, &mut Inventory, &Warrior), (Without<Eliminated>, Without<Remote>)>,
    remote_targets: Query<(&NetId, &Health, &Transform), (With<Remote>, Without<Eliminated>)>,
    net_ids: Query<&NetId>,
    mut outbox: ResMut<Outbox>,
    mut doors: Query<&mut Door>,
    alive: Query<(), (With<Warrior>, Without<Eliminated>)>,
    db: Res<ItemDb>,
    mut feed: MessageWriter<FeedMsg>,
    mut hits: MessageWriter<HitMsg>,
    names: Query<&Warrior>,
    mut kill_msgs: MessageWriter<KillMsg>,
) {
    let mut alive_count = alive.iter().count() as u32;
    for msg in msgs.read() {
        if let Ok(mut door) = doors.get_mut(msg.target) {
            door.hp -= msg.amount;
            outbox.0.push(NetMsg::DoorHit { index: door.index, amount: msg.amount });
            if door.hp <= 0.0 {
                commands.entity(msg.target).despawn();
            }
            continue;
        }
        // Another peer's warrior: tell its owner, who applies it.
        if let Ok((target, h, tf)) = remote_targets.get(msg.target) {
            let (Some(src), DamageCause::Weapon(item)) = (msg.source, &msg.cause) else { continue };
            let Ok(&by) = net_ids.get(src) else { continue };
            outbox.0.push(NetMsg::Hit { target: *target, by, amount: msg.amount, headshot: msg.headshot, item: item.clone() });
            hits.write(HitMsg {
                attacker: src,
                target: msg.target,
                amount: msg.amount,
                headshot: msg.headshot,
                shield: h.shield > 0.0,
                at: tf.translation + Vec3::Y * 1.0,
            });
            continue;
        }
        let Ok((mut h, tf, mut inv, w)) = targets.get_mut(msg.target) else { continue };
        if h.hp <= 0.0 {
            continue;
        }
        let shield_hit = h.shield > 0.0;
        let absorbed = msg.amount.min(h.shield);
        h.shield -= absorbed;
        h.hp -= msg.amount - absorbed;
        if let Some(src) = msg.source {
            hits.write(HitMsg {
                attacker: src,
                target: msg.target,
                amount: msg.amount,
                headshot: msg.headshot,
                shield: shield_hit,
                at: tf.translation + Vec3::Y * 1.0,
            });
        }
        if h.hp <= 0.0 {
            h.hp = 0.0;
            let placement = alive_count;
            alive_count -= 1;
            commands.entity(msg.target).insert(Eliminated { placement, by: msg.source })
                .remove::<Collider>();
            // Drop everything as loot.
            let mut drops: Vec<ItemStack> = inv.slots.iter_mut().filter_map(|s| s.take()).collect();
            if inv.arrows > 0 {
                drops.push(ItemStack { id: "arrows".into(), rarity: crate::items::Rarity::Common, count: inv.arrows });
                inv.arrows = 0;
            }
            let id = net_ids.get(msg.target).copied().unwrap_or(NetId(u32::MAX));
            let by = msg.source.and_then(|s| net_ids.get(s).ok()).copied();
            outbox.0.push(NetMsg::Down { id, by, placement, at: tf.translation, drops: drops.clone(), cause: msg.cause.clone() });
            drop_loot(&mut commands, id, tf.translation, drops);
            let killer = msg.source.and_then(|s| names.get(s).ok()).map(|k| k.name.as_str());
            feed.write(FeedMsg(feed_line(&w.name, &msg.cause, killer, &db)));
            if let Some(src) = msg.source {
                if src != msg.target {
                    kill_msgs.write(KillMsg { killer: src });
                }
            }
        }
    }
}

/// "Pappu ne Bunty ko tapka diya (Dunali)", in this peer's names. Hinglish built so it never
/// needs the victim's gender.
/// The local player is "You": "Tumne ...", "... tumhe ...", "Tumhara ...".
pub fn feed_line(victim: &str, cause: &DamageCause, killer: Option<&str>, db: &ItemDb) -> String {
    let whom = if victim == "You" { "tumhe".to_string() } else { format!("{victim} ko") };
    match cause {
        DamageCause::Pralaya => format!("Aandhi ne {whom} uda diya"),
        DamageCause::Fall if victim == "You" => "Tumhara panga zameen se ho gaya".to_string(),
        DamageCause::Fall => format!("{victim} ka panga zameen se ho gaya"),
        // A katta that blew back on its owner.
        DamageCause::Weapon(id) if killer == Some(victim) && victim == "You" => format!("{} ne tumhe hi tapka diya!", db.get(id).name),
        DamageCause::Weapon(id) if killer == Some(victim) => format!("{} ne {victim} ko hi tapka diya!", db.get(id).name),
        DamageCause::Weapon(id) => {
            let who = match killer.unwrap_or_default() {
                "You" => "Tumne".to_string(),
                k => format!("{k} ne"),
            };
            format!("{who} {whom} tapka diya ({})", db.get(id).name)
        }
    }
}

/// What a backfiring katta does to the hand that fired it.
pub const BACKFIRE_DAMAGE: f32 = 15.0;

/// A gun that let its owner down: it only clicked, or (`backfire`) blew back at them.
#[derive(Message, Clone, Debug)]
pub struct MisfireMsg {
    pub who: Entity,
    pub at: Vec3,
    pub backfire: bool,
}

#[derive(Message, Clone, Debug)]
pub struct KillMsg {
    pub killer: Entity,
}

fn count_kills(mut msgs: MessageReader<KillMsg>, mut warriors: Query<&mut Warrior>) {
    for m in msgs.read() {
        if let Ok(mut w) = warriors.get_mut(m.killer) {
            w.kills += 1;
        }
    }
}
