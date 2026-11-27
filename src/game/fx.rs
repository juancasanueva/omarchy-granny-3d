//! World-space combat feedback: tracers, muzzle flashes, impact shards,
//! rings, and the glowing pickups. Presentation only; nothing feeds back into the sim.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};

use super::AppState;
use super::actors::Muzzle;
use super::assets::Ground;
use super::run::{Aim, FrameEvents, Sim, SimAdvance, StepClock};
use crate::sim::data::{Gun, PICKUP_TTL_TICKS, STEP_S};
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
                    aim_reticle,
                    cursor_icon,
                )
                    .chain()
                    .after(SimAdvance),
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
}

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

fn spawn_event_fx(
    mut commands: Commands,
    events: Res<FrameEvents>,
    ground: Option<Res<Ground>>,
    fx: Option<ResMut<FxAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    muzzles: Query<(&Muzzle, &GlobalTransform)>,
) {
    let (Some(ground), Some(mut fx)) = (ground, fx) else {
        return;
    };
    for e in &events.0 {
        match e {
            SimEvent::GunShot { gun, from, ends } => {
                // Shots leave the gun model's muzzle when it is in Granny's hand.
                let muzzle = muzzles
                    .iter()
                    .find(|(m, _)| m.0 == *gun)
                    .map(|(_, t)| t.translation())
                    .filter(|m| m.xz().distance(*from) < 2.5);
                let start = muzzle
                    .unwrap_or_else(|| ground_point(*from, ground.height(*from) + MUZZLE_HEIGHT));
                let mut dir_sum = Vec3::ZERO;
                for end in ends {
                    let stop = ground_point(*end, start.y);
                    let d = stop - start;
                    let len = d.length().max(0.01);
                    dir_sum += d / len;
                    if (*end - *from).dot(stop.xz() - start.xz()) <= 0.0 {
                        continue; // target closer than the muzzle: no tracer
                    }
                    let thick = if *gun == Gun::Revolver { 0.05 } else { 0.035 };
                    let base = Vec3::new(thick, thick, len);
                    commands.spawn((
                        Mesh3d(fx.cube.clone()),
                        MeshMaterial3d(if *gun == Gun::Revolver {
                            fx.tracer.clone()
                        } else {
                            fx.tracer_shotgun.clone()
                        }),
                        Transform::from_translation(start + d * 0.5)
                            .looking_to(d, Vec3::Y)
                            .with_scale(base),
                        Fx {
                            life: 0.11,
                            max: 0.11,
                            kind: FxKind::Shrink {
                                base,
                                axes: Vec3::new(1.0, 1.0, 0.0),
                            },
                        },
                    ));
                }
                let flash_at = if muzzle.is_some() {
                    start
                } else {
                    start + dir_sum.normalize_or_zero() * 0.55
                };
                let size = if *gun == Gun::Revolver { 0.22 } else { 0.34 };
                commands.spawn((
                    Mesh3d(fx.sphere.clone()),
                    MeshMaterial3d(fx.muzzle.clone()),
                    Transform::from_translation(flash_at).with_scale(Vec3::splat(size)),
                    Fx {
                        life: 0.07,
                        max: 0.07,
                        kind: FxKind::Shrink {
                            base: Vec3::splat(size),
                            axes: Vec3::ONE,
                        },
                    },
                ));
                let peak = if *gun == Gun::Revolver {
                    60_000.0
                } else {
                    110_000.0
                };
                commands.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.75, 0.4),
                        intensity: peak,
                        range: 8.0,
                        ..default()
                    },
                    Transform::from_translation(flash_at),
                    Fx {
                        life: 0.08,
                        max: 0.08,
                        kind: FxKind::Light { peak },
                    },
                ));
            }
            SimEvent::ZombieHit { at, source, .. } => {
                let center = ground_point(*at, ground.height(*at) + 1.0);
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
    mut q: Query<(Entity, &mut Fx, &mut Transform, Option<&mut PointLight>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if *state.get() == AppState::Paused {
        return;
    }
    let dt = time.delta_secs();
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
        }
    }
}

fn clear_fx(mut commands: Commands, fx: Query<Entity, Or<(With<Fx>, With<PickupView>)>>) {
    for e in &fx {
        commands.entity(e).despawn();
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
