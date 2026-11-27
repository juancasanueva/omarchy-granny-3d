//! World-space combat feedback: tracers, muzzle flashes, impact shards, blood,
//! rings, and the glowing pickups. Presentation only; nothing feeds back into the sim.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};

use super::AppState;
use super::actors::{GrannyActor, Muzzle, Rig};
use super::assets::Ground;
use super::run::{Aim, FrameEvents, Sim, SimAdvance, StepClock};
use crate::sim::data::{Gun, PICKUP_TTL_TICKS, STEP_S, Weapon};
use crate::sim::rng::Rng;
use crate::sim::world::{DamageSource, PickupKind, SimEvent};

/// Height of Granny's guns above the ground.
pub const MUZZLE_HEIGHT: f32 = 1.05;

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_fx_assets)
            .add_systems(
                Update,
                (
                    spawn_event_fx,
                    sync_pickups,
                    tick_fx,
                    cap_blood,
                    aim_reticle,
                    cursor_icon,
                )
                    .chain()
                    .after(SimAdvance),
            )
            // Shots read the muzzle after this frame's pose has been animated and propagated.
            .add_systems(
                PostUpdate,
                spawn_shot_fx
                    .after(TransformSystems::Propagate)
                    // Spawn before bounds and visibility run, so effects show on their first frame.
                    .before(VisibilitySystems::CalculateBounds)
                    .before(VisibilitySystems::VisibilityPropagate),
            )
            .add_systems(OnEnter(AppState::Title), clear_fx);
    }
}

#[derive(Resource)]
struct FxAssets {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    shard: Handle<Mesh>,
    coin: Handle<Mesh>,
    ring: Handle<Mesh>,
    disc: Handle<Mesh>,
    tracer: Handle<StandardMaterial>,
    tracer_shotgun: Handle<StandardMaterial>,
    muzzle: Handle<StandardMaterial>,
    shard_cream: Handle<StandardMaterial>,
    shard_amber: Handle<StandardMaterial>,
    blood_drop: Handle<StandardMaterial>,
    blood_pool: [Handle<StandardMaterial>; 2],
    pickup: HashMap<&'static str, Handle<StandardMaterial>>,
    glow: HashMap<&'static str, Handle<StandardMaterial>>,
    rng: Rng,
}

#[derive(Component)]
struct Fx {
    life: f32,
    max: f32,
    kind: FxKind,
}

enum FxKind {
    /// Shrinks the listed axes to zero over its life.
    Shrink { base: Vec3, axes: Vec3 },
    /// Flies ballistically, spinning and shrinking.
    Shard { vel: Vec3, spin: Vec3, size: f32 },
    /// Grows from `from` to `to` radius while fading its own material.
    Ring {
        from: f32,
        to: f32,
        color: LinearRgba,
        material: Handle<StandardMaterial>,
    },
    /// A flash of light that decays to zero.
    Light { peak: f32 },
    /// A blood droplet falling ballistically; it may leave a splat where it lands.
    Drop { vel: Vec3 },
    /// A blood splat on the ground: holds its size, then shrinks away at the end of its life.
    Splat { base: Vec3 },
}

/// Marks the ground splats so their number can be capped.
#[derive(Component)]
struct BloodSplat;

/// Most blood splats on the ground at once; the oldest go first.
const MAX_BLOOD_SPLATS: usize = 400;
/// How long a blood splat stays on the ground, in seconds.
const SPLAT_LIFE: f32 = 14.0;
/// Fraction of a splat's life spent shrinking away.
const SPLAT_FADE: f32 = 0.2;
/// Chance that a landing droplet leaves a splat.
const DROP_SPLAT_CHANCE: f32 = 0.45;
/// Height of the hit wound above the ground.
const WOUND_HEIGHT: f32 = 1.0;

#[derive(Component)]
struct PickupView {
    id: u64,
    kind: PickupKind,
    seed: f32,
}

#[derive(Component)]
struct PickupSpin;

fn key(kind: PickupKind) -> &'static str {
    match kind {
        PickupKind::Ammo(Gun::Revolver) => "revolver",
        PickupKind::Ammo(Gun::Shotgun) => "shotgun",
        PickupKind::Health => "health",
        PickupKind::Cash => "cash",
    }
}

pub fn pickup_color(kind: PickupKind) -> Color {
    match kind {
        PickupKind::Ammo(Gun::Revolver) => Color::srgb_u8(0xff, 0xd3, 0x6a),
        PickupKind::Ammo(Gun::Shotgun) => Color::srgb_u8(0x62, 0xdf, 0xdb),
        PickupKind::Health => Color::srgb_u8(0xff, 0x63, 0x7a),
        PickupKind::Cash => Color::srgb_u8(0x83, 0xdf, 0x65),
    }
}

fn unlit(color: LinearRgba, alpha: AlphaMode) -> StandardMaterial {
    StandardMaterial {
        base_color: color.into(),
        unlit: true,
        alpha_mode: alpha,
        cull_mode: None,
        ..default()
    }
}

fn build_fx_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let mut pickup = HashMap::new();
    let mut glow = HashMap::new();
    for kind in [
        PickupKind::Ammo(Gun::Revolver),
        PickupKind::Ammo(Gun::Shotgun),
        PickupKind::Health,
        PickupKind::Cash,
    ] {
        let c = pickup_color(kind).to_linear();
        pickup.insert(
            key(kind),
            mats.add(StandardMaterial {
                base_color: pickup_color(kind),
                emissive: c * 2.2,
                perceptual_roughness: 0.35,
                metallic: 0.2,
                ..default()
            }),
        );
        glow.insert(key(kind), mats.add(unlit(c * 0.9, AlphaMode::Add)));
    }
    commands.insert_resource(FxAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(2).expect("ico sphere")),
        shard: meshes.add(Tetrahedron::default()),
        coin: meshes.add(Cylinder::new(0.18, 0.05)),
        ring: meshes.add(Annulus::new(0.85, 1.0).mesh().resolution(48)),
        disc: meshes.add(Circle::new(1.0).mesh().resolution(32)),
        tracer: mats.add(unlit(LinearRgba::rgb(9.0, 6.5, 2.8), AlphaMode::Opaque)),
        tracer_shotgun: mats.add(unlit(LinearRgba::rgb(3.0, 8.0, 7.5), AlphaMode::Opaque)),
        muzzle: mats.add(unlit(LinearRgba::rgb(14.0, 9.0, 3.5), AlphaMode::Add)),
        shard_cream: mats.add(StandardMaterial {
            base_color: Color::srgb_u8(0xff, 0xf1, 0xcd),
            emissive: LinearRgba::rgb(1.2, 1.0, 0.7),
            ..default()
        }),
        shard_amber: mats.add(StandardMaterial {
            base_color: Color::srgb_u8(0xff, 0xb3, 0x47),
            emissive: LinearRgba::rgb(1.6, 0.8, 0.2),
            ..default()
        }),
        blood_drop: mats.add(StandardMaterial {
            base_color: Color::srgb_u8(0xb4, 0x08, 0x14),
            emissive: LinearRgba::rgb(0.45, 0.0, 0.03),
            perceptual_roughness: 0.3,
            ..default()
        }),
        blood_pool: [
            Color::srgb_u8(0x7a, 0x04, 0x0e),
            Color::srgb_u8(0x5a, 0x02, 0x08),
        ]
        .map(|c| {
            mats.add(StandardMaterial {
                base_color: c,
                emissive: LinearRgba::rgb(0.12, 0.0, 0.01),
                perceptual_roughness: 0.18,
                reflectance: 0.7,
                ..default()
            })
        }),
        pickup,
        glow,
        rng: Rng::new(0xC0FFEE),
    });
}

fn ground_point(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x, y, p.y)
}

fn ring(
    commands: &mut Commands,
    fx: &FxAssets,
    mats: &mut Assets<StandardMaterial>,
    at: Vec3,
    color: LinearRgba,
    from: f32,
    to: f32,
    life: f32,
) {
    let material = mats.add(unlit(color, AlphaMode::Add));
    commands.spawn((
        Mesh3d(fx.ring.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(at)
            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
            .with_scale(Vec3::splat(from)),
        Fx {
            life,
            max: life,
            kind: FxKind::Ring {
                from,
                to,
                color,
                material,
            },
        },
    ));
}

/// How many blood droplets a hit sprays: melee splashes more, and bigger hits bleed more.
fn blood_drops(source: DamageSource, amount: i32) -> usize {
    let base = if source == DamageSource::Melee {
        30
    } else {
        22
    };
    base + (amount.max(0) as usize / 4).min(30)
}

/// A flat blood splat on the ground at `at`, `size` metres in radius, of random shape and shade.
fn blood_splat(commands: &mut Commands, fx: &mut FxAssets, ground: &Ground, at: Vec2, size: f32) {
    let stretch = 0.7 + fx.rng.next_f32() * 0.6;
    let base = Vec3::new(size * stretch, size / stretch, 1.0);
    // Stagger heights so overlapping splats don't flicker against each other.
    let y = ground.height(at) + 0.02 + fx.rng.next_f32() * 0.025;
    let shade = (fx.rng.next_f32() * 2.0) as usize % 2;
    commands.spawn((
        BloodSplat,
        Mesh3d(fx.disc.clone()),
        MeshMaterial3d(fx.blood_pool[shade].clone()),
        Transform::from_translation(ground_point(at, y))
            .with_rotation(
                Quat::from_rotation_y(fx.rng.next_f32() * std::f32::consts::TAU)
                    * Quat::from_rotation_x(-FRAC_PI_2),
            )
            .with_scale(base),
        Fx {
            life: SPLAT_LIFE,
            max: SPLAT_LIFE,
            kind: FxKind::Splat { base },
        },
    ));
}

/// Sprays droplets from a wound, mostly away from Granny, and splashes the ground below.
fn blood_spray(
    commands: &mut Commands,
    fx: &mut FxAssets,
    ground: &Ground,
    at: Vec2,
    away: Vec2,
    drops: usize,
) {
    let center = ground_point(at, ground.height(at) + WOUND_HEIGHT);
    for _ in 0..drops {
        let a = fx.rng.next_f32() * std::f32::consts::TAU;
        let spread = fx.rng.next_f32() * 2.4;
        let push = 1.2 + fx.rng.next_f32() * 3.4;
        let h = away * push + Vec2::new(a.cos(), a.sin()) * spread;
        let vel = Vec3::new(h.x, 0.8 + fx.rng.next_f32() * 3.6, h.y);
        let size = 0.035 + fx.rng.next_f32() * 0.06;
        commands.spawn((
            Mesh3d(fx.sphere.clone()),
            MeshMaterial3d(fx.blood_drop.clone()),
            Transform::from_translation(center).with_scale(Vec3::splat(size)),
            Fx {
                life: 1.6,
                max: 1.6,
                kind: FxKind::Drop { vel },
            },
        ));
    }
    let splats = 2 + (fx.rng.next_f32() * 3.0) as usize;
    for _ in 0..splats {
        let off = Vec2::new(fx.rng.signed(), fx.rng.signed()) * 0.5 + away * 0.4;
        let size = 0.11 + fx.rng.next_f32() * 0.2;
        blood_splat(commands, fx, ground, at + off, size);
    }
}

/// Direction from Granny to a hit zombie, so blood sprays out of the far side.
fn away_from(player: Option<Vec2>, at: Vec2) -> Vec2 {
    player
        .map(|p| (at - p).normalize_or_zero())
        .unwrap_or(Vec2::ZERO)
}

fn spawn_event_fx(
    mut commands: Commands,
    events: Res<FrameEvents>,
    sim: Option<Res<Sim>>,
    ground: Option<Res<Ground>>,
    fx: Option<ResMut<FxAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(ground), Some(mut fx)) = (ground, fx) else {
        return;
    };
    let player = sim.map(|s| s.world.player.pos);
    for e in &events.0 {
        match e {
            SimEvent::ZombieHit {
                at, amount, source, ..
            } => {
                let away = away_from(player, *at);
                blood_spray(
                    &mut commands,
                    &mut fx,
                    &ground,
                    *at,
                    away,
                    blood_drops(*source, *amount),
                );
                let center = ground_point(*at, ground.height(*at) + WOUND_HEIGHT);
                let count = if *source == DamageSource::Melee {
                    10
                } else {
                    6
                };
                for i in 0..count {
                    let a = fx.rng.next_f32() * std::f32::consts::TAU;
                    let up = 1.8 + fx.rng.next_f32() * 2.5;
                    let out = 1.5 + fx.rng.next_f32() * 2.5;
                    let vel = Vec3::new(a.cos() * out, up, a.sin() * out);
                    let spin = Vec3::new(fx.rng.signed(), fx.rng.signed(), fx.rng.signed()) * 12.0;
                    let size = 0.07 + fx.rng.next_f32() * 0.06;
                    commands.spawn((
                        Mesh3d(fx.shard.clone()),
                        MeshMaterial3d(if i % 2 == 0 {
                            fx.shard_cream.clone()
                        } else {
                            fx.shard_amber.clone()
                        }),
                        Transform::from_translation(center).with_scale(Vec3::splat(size)),
                        Fx {
                            life: 0.5,
                            max: 0.5,
                            kind: FxKind::Shard { vel, spin, size },
                        },
                    ));
                }
            }
            SimEvent::ZombieDeath { at, .. } => {
                let away = away_from(player, *at);
                let size = 0.5 + fx.rng.next_f32() * 0.25;
                blood_splat(&mut commands, &mut fx, &ground, *at + away * 0.3, size);
                for _ in 0..4 {
                    let off = Vec2::new(fx.rng.signed(), fx.rng.signed()) * 0.7;
                    let size = 0.15 + fx.rng.next_f32() * 0.2;
                    blood_splat(&mut commands, &mut fx, &ground, *at + off, size);
                }
                let p = ground_point(*at, ground.height(*at) + 0.06);
                ring(
                    &mut commands,
                    &fx,
                    &mut mats,
                    p,
                    LinearRgba::rgb(1.4, 1.2, 0.8),
                    0.3,
                    1.6,
                    0.35,
                );
            }
            SimEvent::PlayerHurt { at, .. } => {
                let p = ground_point(*at, ground.height(*at) + 0.06);
                ring(
                    &mut commands,
                    &fx,
                    &mut mats,
                    p,
                    LinearRgba::rgb(3.0, 0.3, 0.45),
                    0.4,
                    1.4,
                    0.3,
                );
            }
            SimEvent::PickupCollected { kind, at, .. } => {
                let p = ground_point(*at, ground.height(*at) + 0.08);
                ring(
                    &mut commands,
                    &fx,
                    &mut mats,
                    p,
                    pickup_color(*kind).to_linear() * 2.0,
                    0.2,
                    1.1,
                    0.3,
                );
            }
            SimEvent::MeleeHit { at, .. } => {
                let p = ground_point(*at, ground.height(*at) + 1.0);
                commands.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.9, 0.7),
                        intensity: 50_000.0,
                        range: 6.0,
                        ..default()
                    },
                    Transform::from_translation(p),
                    Fx {
                        life: 0.1,
                        max: 0.1,
                        kind: FxKind::Light { peak: 50_000.0 },
                    },
                ));
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_pickups(
    mut commands: Commands,
    time: Res<Time>,
    sim: Option<Res<Sim>>,
    clock: Res<StepClock>,
    ground: Option<Res<Ground>>,
    fx: Option<Res<FxAssets>>,
    mut views: Query<(Entity, &PickupView, &mut Transform, &mut Visibility)>,
    mut spinners: Query<&mut Transform, (With<PickupSpin>, Without<PickupView>)>,
    children: Query<&Children>,
    mut last_run: Local<u32>,
) {
    let (Some(sim), Some(ground), Some(fx)) = (sim, ground, fx) else {
        return;
    };
    // Pickup ids restart with every run: drop the previous run's views first.
    if sim.run_id != *last_run {
        *last_run = sim.run_id;
        for (e, ..) in &views {
            commands.entity(e).despawn();
        }
        return;
    }
    let world = &sim.world;
    let mut live: HashMap<u64, usize> = world
        .pickups
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id, i))
        .collect();
    if sim.showcase {
        live.clear();
    }
    let t = time.elapsed_secs();
    for (e, view, mut tf, mut vis) in &mut views {
        let Some(&i) = live.get(&view.id) else {
            commands.entity(e).despawn();
            continue;
        };
        live.remove(&view.id);
        let p = &world.pickups[i];
        let pos = p.prev_pos.lerp(p.pos, clock.alpha);
        tf.translation = ground_point(pos, ground.height(pos));
        let remaining = PICKUP_TTL_TICKS.saturating_sub(p.age_ticks) as f32 * STEP_S;
        let blink = remaining < 4.0 && (t * if remaining < 1.5 { 14.0 } else { 7.0 }).sin() < -0.2;
        *vis = if blink {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        for child in children.iter_descendants(e) {
            if let Ok(mut s) = spinners.get_mut(child) {
                s.translation.y = 0.45 + (t * 3.0 + view.seed).sin() * 0.08;
                s.rotation = Quat::from_rotation_y(t * 2.2 + view.seed)
                    * if view.kind == PickupKind::Cash {
                        Quat::from_rotation_x(FRAC_PI_2)
                    } else {
                        Quat::IDENTITY
                    };
            }
        }
    }
    for (_, i) in live {
        let p = &world.pickups[i];
        let k = key(p.kind);
        let mat = fx.pickup[k].clone();
        commands
            .spawn((
                PickupView {
                    id: p.id,
                    kind: p.kind,
                    seed: (p.id % 97) as f32 * 0.61,
                },
                Transform::from_translation(ground_point(p.pos, ground.height(p.pos))),
                Visibility::Inherited,
            ))
            .with_children(|root| {
                root.spawn((
                    Mesh3d(fx.disc.clone()),
                    MeshMaterial3d(fx.glow[k].clone()),
                    Transform::from_xyz(0.0, 0.04, 0.0)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                        .with_scale(Vec3::splat(0.42)),
                ));
                root.spawn((
                    PickupSpin,
                    Transform::from_xyz(0.0, 0.45, 0.0),
                    Visibility::Inherited,
                ))
                .with_children(|s| {
                    let part = |size: Vec3, at: Vec3| {
                        (
                            Mesh3d(fx.cube.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_translation(at).with_scale(size),
                        )
                    };
                    match p.kind {
                        PickupKind::Health => {
                            s.spawn(part(Vec3::new(0.34, 0.11, 0.11), Vec3::ZERO));
                            s.spawn(part(Vec3::new(0.11, 0.34, 0.11), Vec3::ZERO));
                        }
                        PickupKind::Cash => {
                            s.spawn((
                                Mesh3d(fx.coin.clone()),
                                MeshMaterial3d(mat.clone()),
                                Transform::default(),
                            ));
                        }
                        PickupKind::Ammo(Gun::Revolver) => {
                            s.spawn(part(Vec3::new(0.3, 0.16, 0.2), Vec3::ZERO));
                        }
                        PickupKind::Ammo(Gun::Shotgun) => {
                            for x in [-0.09, 0.0, 0.09] {
                                s.spawn(part(Vec3::new(0.07, 0.26, 0.07), Vec3::new(x, 0.0, 0.0)));
                            }
                        }
                    }
                });
            });
    }
}

fn tick_fx(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<AppState>>,
    ground: Option<Res<Ground>>,
    mut assets: Option<ResMut<FxAssets>>,
    mut q: Query<(Entity, &mut Fx, &mut Transform, Option<&mut PointLight>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if *state.get() == AppState::Paused {
        return;
    }
    let dt = time.delta_secs();
    let mut landed = Vec::new();
    for (e, mut fx, mut tf, light) in &mut q {
        fx.life -= dt;
        if fx.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let k = fx.life / fx.max;
        match &mut fx.kind {
            FxKind::Shrink { base, axes } => {
                tf.scale = *base * (Vec3::ONE - *axes * (1.0 - k));
            }
            FxKind::Shard { vel, spin, size } => {
                vel.y -= 14.0 * dt;
                tf.translation += *vel * dt;
                tf.translation.y = tf.translation.y.max(0.05);
                tf.rotate(Quat::from_scaled_axis(*spin * dt));
                tf.scale = Vec3::splat(*size * k.sqrt());
            }
            FxKind::Ring {
                from,
                to,
                color,
                material,
            } => {
                let r = *from + (*to - *from) * (1.0 - k * k);
                tf.scale = Vec3::splat(r);
                if let Some(mut m) = mats.get_mut(material) {
                    m.base_color = (*color * k).into();
                }
            }
            FxKind::Light { peak } => {
                if let Some(mut l) = light {
                    l.intensity = *peak * k;
                }
            }
            FxKind::Drop { vel } => {
                vel.y -= 14.0 * dt;
                tf.translation += *vel * dt;
                let at = tf.translation.xz();
                let floor = ground.as_ref().map_or(0.0, |g| g.height(at));
                if vel.y < 0.0 && tf.translation.y <= floor + 0.03 {
                    landed.push((at, tf.scale.x));
                    commands.entity(e).despawn();
                }
            }
            FxKind::Splat { base } => {
                tf.scale = *base * (k / SPLAT_FADE).min(1.0);
            }
        }
    }
    if let (Some(ground), Some(fx)) = (ground, assets.as_mut()) {
        for (at, drop) in landed {
            if fx.rng.next_f32() < DROP_SPLAT_CHANCE {
                let size = drop * (1.0 + fx.rng.next_f32() * 1.5);
                blood_splat(&mut commands, fx, &ground, at, size);
            }
        }
    }
}

/// The `n` entries with the least life left, where `n` is how far `items` exceeds `cap`.
fn oldest_beyond<T: Copy>(items: &mut [(T, f32)], cap: usize) -> Vec<T> {
    let excess = items.len().saturating_sub(cap);
    if excess == 0 {
        return Vec::new();
    }
    items.sort_by(|a, b| a.1.total_cmp(&b.1));
    items[..excess].iter().map(|(t, _)| *t).collect()
}

/// Keeps the ground from drowning in splats: the oldest go once there are too many.
fn cap_blood(mut commands: Commands, splats: Query<(Entity, &Fx), With<BloodSplat>>) {
    if splats.iter().len() <= MAX_BLOOD_SPLATS {
        return;
    }
    let mut items: Vec<_> = splats.iter().map(|(e, fx)| (e, fx.life)).collect();
    for e in oldest_beyond(&mut items, MAX_BLOOD_SPLATS) {
        commands.entity(e).despawn();
    }
}

fn clear_fx(mut commands: Commands, fx: Query<Entity, Or<(With<Fx>, With<PickupView>)>>) {
    for e in &fx {
        commands.entity(e).despawn();
    }
}

/// Where a tracer ends: the hit point at muzzle height, or `None` when the target is
/// closer to Granny than the muzzle (the tracer would point backwards).
pub fn tracer_stop(from: Vec2, start: Vec3, end: Vec2) -> Option<Vec3> {
    let stop = ground_point(end, start.y);
    ((end - from).dot(stop.xz() - start.xz()) > 0.0).then_some(stop)
}

/// The muzzle of `gun` in Granny's hand, in this frame's world space.
fn granny_muzzle(
    gun: Gun,
    granny: &Query<&Rig, With<GrannyActor>>,
    children: &Query<&Children>,
    muzzles: &Query<&GlobalTransform, With<Muzzle>>,
) -> Option<Vec3> {
    let rig = granny.single().ok()?;
    let (_, model) = rig.guns.iter().find(|(w, _)| *w == Weapon::from(gun))?;
    children
        .iter_descendants(*model)
        .find_map(|e| muzzles.get(e).ok())
        .map(|t| t.translation())
}

/// Spawns an effect entity with its world transform already set, since it is created
/// after this frame's transform propagation (but before bounds and visibility checks).
fn spawn_now(commands: &mut Commands, bundle: impl Bundle, transform: Transform) {
    commands.spawn((bundle, transform, GlobalTransform::from(transform)));
}

/// Tracers, muzzle flash and flash light for each shot, leaving from the gun's muzzle.
fn spawn_shot_fx(
    mut commands: Commands,
    events: Res<FrameEvents>,
    ground: Option<Res<Ground>>,
    fx: Option<Res<FxAssets>>,
    granny: Query<&Rig, With<GrannyActor>>,
    children: Query<&Children>,
    muzzles: Query<&GlobalTransform, With<Muzzle>>,
) {
    let (Some(ground), Some(fx)) = (ground, fx) else {
        return;
    };
    for e in &events.0 {
        let SimEvent::GunShot { gun, from, ends } = e else {
            continue;
        };
        let muzzle = granny_muzzle(*gun, &granny, &children, &muzzles)
            .filter(|m| m.xz().distance(*from) < 2.5);
        let start =
            muzzle.unwrap_or_else(|| ground_point(*from, ground.height(*from) + MUZZLE_HEIGHT));
        let mut dir_sum = Vec3::ZERO;
        for end in ends {
            let Some(stop) = tracer_stop(*from, start, *end) else {
                continue;
            };
            let d = stop - start;
            let len = d.length().max(0.01);
            dir_sum += d / len;
            let thick = if *gun == Gun::Revolver { 0.05 } else { 0.035 };
            let base = Vec3::new(thick, thick, len);
            let material = if *gun == Gun::Revolver {
                fx.tracer.clone()
            } else {
                fx.tracer_shotgun.clone()
            };
            spawn_now(
                &mut commands,
                (
                    Mesh3d(fx.cube.clone()),
                    MeshMaterial3d(material),
                    Fx {
                        life: 0.11,
                        max: 0.11,
                        kind: FxKind::Shrink {
                            base,
                            axes: Vec3::new(1.0, 1.0, 0.0),
                        },
                    },
                ),
                Transform::from_translation(start + d * 0.5)
                    .looking_to(d, Vec3::Y)
                    .with_scale(base),
            );
        }
        let flash_at = if muzzle.is_some() {
            start
        } else {
            start + dir_sum.normalize_or_zero() * 0.55
        };
        let size = if *gun == Gun::Revolver { 0.22 } else { 0.34 };
        spawn_now(
            &mut commands,
            (
                Mesh3d(fx.sphere.clone()),
                MeshMaterial3d(fx.muzzle.clone()),
                Fx {
                    life: 0.07,
                    max: 0.07,
                    kind: FxKind::Shrink {
                        base: Vec3::splat(size),
                        axes: Vec3::ONE,
                    },
                },
            ),
            Transform::from_translation(flash_at).with_scale(Vec3::splat(size)),
        );
        let peak = if *gun == Gun::Revolver {
            60_000.0
        } else {
            110_000.0
        };
        spawn_now(
            &mut commands,
            (
                PointLight {
                    color: Color::srgb(1.0, 0.75, 0.4),
                    intensity: peak,
                    range: 8.0,
                    ..default()
                },
                Fx {
                    life: 0.08,
                    max: 0.08,
                    kind: FxKind::Light { peak },
                },
            ),
            Transform::from_translation(flash_at),
        );
    }
}

#[derive(Component)]
struct Reticle;

/// Glowing ground ring under the cursor while the mouse aims.
fn aim_reticle(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<AppState>>,
    aim: Res<Aim>,
    ground: Option<Res<Ground>>,
    fx: Option<Res<FxAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut reticle: Query<(&mut Transform, &mut Visibility), With<Reticle>>,
) {
    let (Some(ground), Some(fx)) = (ground, fx) else {
        return;
    };
    let Ok((mut tf, mut vis)) = reticle.single_mut() else {
        let material = mats.add(unlit(LinearRgba::rgb(1.6, 1.3, 0.9), AlphaMode::Add));
        commands.spawn((
            Reticle,
            Mesh3d(fx.ring.clone()),
            MeshMaterial3d(material),
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Visibility::Hidden,
        ));
        return;
    };
    match aim.reticle {
        Some(p) if *state.get() == AppState::Playing => {
            *vis = Visibility::Inherited;
            let pulse = 0.36 + 0.04 * (time.elapsed_secs() * 6.0).sin();
            tf.translation = ground_point(p, ground.height(p) + 0.05);
            tf.scale = Vec3::splat(pulse);
        }
        _ => *vis = Visibility::Hidden,
    }
}

/// Crosshair pointer while a run is being played, the normal arrow elsewhere.
fn cursor_icon(
    mut commands: Commands,
    state: Res<State<AppState>>,
    window: Query<Entity, With<PrimaryWindow>>,
    mut last: Local<Option<bool>>,
) {
    let playing = *state.get() == AppState::Playing;
    if *last == Some(playing) {
        return;
    }
    let Ok(window) = window.single() else { return };
    *last = Some(playing);
    let icon = if playing {
        SystemCursorIcon::Crosshair
    } else {
        SystemCursorIcon::Default
    };
    commands.entity(window).insert(CursorIcon::from(icon));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracer_runs_from_muzzle_to_the_hit_at_muzzle_height() {
        let from = Vec2::ZERO;
        let muzzle = Vec3::new(0.5, 1.2, 0.0);
        let stop = tracer_stop(from, muzzle, Vec2::new(6.0, 0.0)).unwrap();
        assert_eq!(stop, Vec3::new(6.0, 1.2, 0.0));
    }

    #[test]
    fn bigger_and_melee_hits_bleed_more() {
        let gun = DamageSource::Gun(Gun::Revolver);
        assert!(blood_drops(DamageSource::Melee, 20) > blood_drops(gun, 20));
        assert!(blood_drops(gun, 80) > blood_drops(gun, 10));
        assert_eq!(blood_drops(gun, 10_000), blood_drops(gun, 1_000));
    }

    #[test]
    fn blood_sprays_away_from_granny() {
        let away = away_from(Some(Vec2::ZERO), Vec2::new(0.0, 3.0));
        assert_eq!(away, Vec2::Y);
        assert_eq!(away_from(None, Vec2::new(0.0, 3.0)), Vec2::ZERO);
    }

    #[test]
    fn splat_cap_drops_the_oldest_first() {
        let mut items = [(1, 9.0), (2, 0.5), (3, 4.0), (4, 1.0)];
        assert_eq!(oldest_beyond(&mut items, 2), vec![2, 4]);
        assert!(oldest_beyond(&mut items, 4).is_empty());
    }

    #[test]
    fn no_tracer_when_the_target_is_closer_than_the_muzzle() {
        let from = Vec2::ZERO;
        let muzzle = Vec3::new(0.5, 1.2, 0.0);
        assert_eq!(tracer_stop(from, muzzle, Vec2::new(0.3, 0.0)), None);
        assert_eq!(tracer_stop(from, muzzle, Vec2::new(0.5, 0.0)), None);
    }
}
