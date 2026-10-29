//! Weapons, projectiles, astras, consumables, damage and eliminations.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::{
    Action, DamageCause, DamageMsg, Door, Eliminated, FeedMsg, HEADSHOT_HEIGHT, Health, HitMsg, Inventory,
    Kinematic, MoveMode, PlayerIntent, SimSet, Warrior, aim_ray, btn, hit_mask,
};
use crate::items::{AstraEffect, ItemDb, ItemKind, ItemStack};
use crate::sim::loot::spawn_pickup;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<KillMsg>().add_systems(
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

fn select_slot(mut q: Query<(&PlayerIntent, &mut Inventory, &mut Action), Without<Eliminated>>) {
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
        (Entity, &Transform, &PlayerIntent, &mut Inventory, &mut Action, &mut Health, &Kinematic),
        Without<Eliminated>,
    >,
    db: Res<ItemDb>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    for (entity, tf, intent, mut inv, mut act, mut health, kin) in &mut q {
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
            ItemKind::Bow { damage: dmg, interval, speed, gravity, pellets, spread_deg, magazine, reload, charge, headshot } => {
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
                    commands.spawn((
                        Projectile {
                            owner: entity,
                            velocity: dir * speed * (0.55 + 0.45 * power),
                            gravity: *gravity,
                            damage: dmg * scale * power,
                            headshot_mult: *headshot,
                            ttl: 4.0,
                            item: stack.id.clone(),
                            astra: None,
                            stuck: false,
                        },
                        Transform::from_translation(muzzle).looking_to(dir, Vec3::Y),
                    ));
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
                    commands.spawn((
                        Projectile {
                            owner: entity,
                            velocity: dir * speed,
                            gravity: 16.0,
                            damage: 0.0,
                            headshot_mult: 1.0,
                            ttl: 6.0,
                            item: stack.id.clone(),
                            astra: Some(effect.clone()),
                            stuck: false,
                        },
                        Transform::from_translation(muzzle),
                    ));
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
    mut kin: Query<(&Transform, &mut Kinematic), (With<Warrior>, Without<Eliminated>, Without<Projectile>)>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut p) in &mut q {
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
            damage.write(DamageMsg {
                target: hit.entity,
                amount: if headshot { p.damage * p.headshot_mult } else { p.damage },
                source: Some(p.owner),
                headshot,
                cause: DamageCause::Weapon(p.item.clone()),
            });
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
    kin: &mut Query<(&Transform, &mut Kinematic), (With<Warrior>, Without<Eliminated>, Without<Projectile>)>,
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
    warriors: Query<(Entity, &Transform), (With<Warrior>, Without<Eliminated>)>,
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
    mut targets: Query<(&mut Health, &Transform, &mut Inventory, &Warrior), Without<Eliminated>>,
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
            if door.hp <= 0.0 {
                commands.entity(msg.target).despawn();
            }
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
            for (i, st) in drops.into_iter().enumerate() {
                let a = i as f32 * 1.3;
                spawn_pickup(&mut commands, tf.translation - Vec3::Y * 0.8 + Vec3::new(a.cos(), 0.0, a.sin()) * 1.2, st);
            }
            let how = match &msg.cause {
                DamageCause::Pralaya => "was consumed by the Pralaya".to_string(),
                DamageCause::Fall => "fell to their death".to_string(),
                DamageCause::Weapon(id) => {
                    let killer = msg.source.and_then(|s| names.get(s).ok()).map(|k| k.name.clone()).unwrap_or_default();
                    format!("was felled by {killer} ({})", db.get(id).name)
                }
            };
            feed.write(FeedMsg(format!("{} {}", w.name, how)));
            if let Some(src) = msg.source {
                if src != msg.target {
                    kill_msgs.write(KillMsg { killer: src });
                }
            }
        }
    }
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
