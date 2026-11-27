//! Skinned Granny and shambler actors. Clip times are driven from the simulation
//! (walk phase, swing progress) like the web game's `syncRigs.ts`, so feet match
//! stride and attack impact frames land on the simulation's active tick.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use super::AppState;
use super::assets::{ActorKit, Clip, GameAssets, Ground};
use super::run::{FrameEvents, Sim, SimAdvance, StepClock};
use crate::sim::data::{Gun, ROLLING_PIN, SHAMBLER, STEP_S, ZOMBIE_POOL_CAPACITY};
use crate::sim::world::SimEvent;

pub const GRANNY_SCALE: f32 = 1.99 / 1.3136;
pub const ZOMBIE_SCALE: f32 = 0.8;
const CORPSE_POOL: usize = 8;
/// Fraction of each attack clip where the blow connects.
const GRANNY_IMPACT: f32 = 11.0 / 24.0;
const ZOMBIE_IMPACT: f32 = 14.0 / 24.0;
const CORPSE_LINGER: f32 = 1.4;
const CORPSE_SINK: f32 = 1.2;

pub struct ActorsPlugin;

impl Plugin for ActorsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::Loading), spawn_actors)
            .add_systems(
                Update,
                (
                    sync_granny,
                    sync_firearms,
                    sync_zombies,
                    spawn_corpses,
                    sync_corpses,
                    flash_zombies,
                )
                    .chain()
                    .after(SimAdvance),
            );
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Kit {
    Granny,
    Zombie,
}

/// The entity holding the actor's `AnimationPlayer`, found once its scene is ready.
#[derive(Component, Default)]
pub struct Rig {
    pub player: Option<Entity>,
    /// Granny's right-hand bone, which carries her guns.
    pub hand: Option<Entity>,
    pub materials: Vec<Handle<StandardMaterial>>,
}

/// Root of Granny's primitive firearms; its +X axis points where she faces.
#[derive(Component)]
struct Firearms;

#[derive(Component)]
struct FirearmModel(Gun);

#[derive(Component, Default)]
struct Blend {
    walk: f32,
}

#[derive(Component)]
pub struct GrannyActor;

#[derive(Component)]
pub struct ZombieActor(pub usize);

#[derive(Component, Default)]
struct Corpse {
    active: bool,
    t: f32,
    pos: Vec2,
    facing: f32,
}

#[derive(Resource, Default)]
struct CorpseRing {
    next: usize,
    run_id: u32,
}

fn actor_transform(pos: Vec2, y: f32, facing: f32, scale: f32) -> Transform {
    Transform {
        translation: Vec3::new(pos.x, y, pos.y),
        rotation: Quat::from_rotation_y(-facing) * Quat::from_rotation_y(FRAC_PI_2),
        scale: Vec3::splat(scale),
    }
}

/// Revolver and cane shotgun from primitives, barrels along +X (`render/grannyWeapons.ts`).
fn spawn_firearms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
) {
    let metal = mats.add(StandardMaterial {
        base_color: Color::srgb_u8(0x64, 0x72, 0x7e),
        metallic: 0.65,
        perceptual_roughness: 0.35,
        ..default()
    });
    let wood = mats.add(StandardMaterial {
        base_color: Color::srgb_u8(0x85, 0x54, 0x34),
        perceptual_roughness: 0.7,
        ..default()
    });
    let brass = mats.add(StandardMaterial {
        base_color: Color::srgb_u8(0xe1, 0xb5, 0x5b),
        metallic: 0.5,
        perceptual_roughness: 0.4,
        ..default()
    });
    let mut part_box = |size: Vec3, at: Vec3, mat: &Handle<StandardMaterial>, roll: f32| {
        (
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_z(roll)),
        )
    };
    let revolver = [
        part_box(
            Vec3::new(0.12, 0.23, 0.12),
            Vec3::new(-0.03, -0.06, 0.0),
            &wood,
            -0.2,
        ),
        part_box(
            Vec3::new(0.24, 0.1, 0.12),
            Vec3::new(0.06, 0.1, 0.0),
            &metal,
            0.0,
        ),
        part_box(
            Vec3::new(0.16, 0.19, 0.19),
            Vec3::new(0.1, 0.16, 0.0),
            &metal,
            0.0,
        ),
        part_box(
            Vec3::new(0.32, 0.08, 0.08),
            Vec3::new(0.33, 0.17, 0.0),
            &metal,
            0.0,
        ),
        part_box(
            Vec3::new(0.03, 0.03, 0.025),
            Vec3::new(0.46, 0.22, 0.0),
            &brass,
            0.0,
        ),
    ];
    let shotgun = [
        part_box(
            Vec3::new(0.32, 0.13, 0.13),
            Vec3::new(-0.12, 0.0, 0.0),
            &wood,
            0.0,
        ),
        part_box(
            Vec3::new(0.9, 0.1, 0.1),
            Vec3::new(0.46, 0.07, 0.0),
            &metal,
            0.0,
        ),
        part_box(
            Vec3::new(0.24, 0.15, 0.15),
            Vec3::new(0.38, -0.03, 0.0),
            &wood,
            0.0,
        ),
        part_box(
            Vec3::new(0.1, 0.24, 0.09),
            Vec3::new(-0.3, -0.1, 0.0),
            &brass,
            0.35,
        ),
    ];
    commands
        .spawn((Firearms, Transform::default(), Visibility::Hidden))
        .with_children(|root| {
            root.spawn((
                FirearmModel(Gun::Revolver),
                Transform::default(),
                Visibility::Inherited,
            ))
            .with_children(|g| {
                for part in revolver {
                    g.spawn(part);
                }
            });
            root.spawn((
                FirearmModel(Gun::Shotgun),
                Transform::default(),
                Visibility::Hidden,
            ))
            .with_children(|g| {
                for part in shotgun {
                    g.spawn(part);
                }
            });
        });
}

fn sync_firearms(
    sim: Option<Res<Sim>>,
    granny: Query<&Rig, With<GrannyActor>>,
    hands: Query<&GlobalTransform>,
    mut root: Query<(&mut Transform, &mut Visibility), (With<Firearms>, Without<FirearmModel>)>,
    mut models: Query<(&FirearmModel, &mut Visibility), Without<Firearms>>,
) {
    let Some(sim) = sim else { return };
    let (Ok(rig), Ok((mut tf, mut vis))) = (granny.single(), root.single_mut()) else {
        return;
    };
    let p = &sim.world.player;
    let Some(hand) = rig.hand.and_then(|h| hands.get(h).ok()) else {
        *vis = Visibility::Hidden;
        return;
    };
    let show = p.hp > 0 && p.attack_tick.is_none();
    *vis = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    *tf = Transform::from_translation(hand.translation())
        .with_rotation(Quat::from_rotation_y(-p.facing));
    for (m, mut v) in &mut models {
        *v = if m.0 == p.selected {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn spawn_actors(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    commands.init_resource::<CorpseRing>();
    spawn_firearms(&mut commands, &mut meshes, &mut mats);
    commands
        .spawn((
            GrannyActor,
            Kit::Granny,
            Rig::default(),
            Blend::default(),
            WorldAssetRoot(assets.granny.scene.clone()),
            Transform::from_scale(Vec3::splat(GRANNY_SCALE)),
        ))
        .observe(attach_rig);
    for i in 0..ZOMBIE_POOL_CAPACITY {
        commands
            .spawn((
                ZombieActor(i),
                Kit::Zombie,
                Rig::default(),
                Blend::default(),
                WorldAssetRoot(assets.zombie.scene.clone()),
                Transform::from_scale(Vec3::splat(ZOMBIE_SCALE)),
                Visibility::Hidden,
            ))
            .observe(attach_rig);
    }
    for _ in 0..CORPSE_POOL {
        commands
            .spawn((
                Corpse::default(),
                Kit::Zombie,
                Rig::default(),
                WorldAssetRoot(assets.zombie.scene.clone()),
                Transform::from_scale(Vec3::splat(ZOMBIE_SCALE)),
                Visibility::Hidden,
            ))
            .observe(attach_rig);
    }
}

/// Wires the animation graph into the spawned scene and gives zombies their own
/// material copies so hit flashes stay per-zombie.
#[allow(clippy::too_many_arguments)]
fn attach_rig(
    ready: On<WorldInstanceReady>,
    assets: Res<GameAssets>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mats: Query<&MeshMaterial3d<StandardMaterial>>,
    names: Query<&Name>,
    mut rigs: Query<(&mut Rig, &Kit)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let root = ready.entity;
    let Ok((mut rig, kit)) = rigs.get_mut(root) else {
        return;
    };
    let kit_assets: &ActorKit = match kit {
        Kit::Granny => &assets.granny,
        Kit::Zombie => &assets.zombie,
    };
    for e in children.iter_descendants(root) {
        if let Ok(mut player) = players.get_mut(e) {
            for clip in Clip::ALL {
                player.play(kit_assets.node(clip)).pause().set_weight(0.0);
            }
            commands
                .entity(e)
                .insert(AnimationGraphHandle(kit_assets.graph.clone()));
            rig.player = Some(e);
        }
        if *kit == Kit::Granny && names.get(e).is_ok_and(|n| n.as_str() == "hand.R") {
            rig.hand = Some(e);
        }
        if *kit == Kit::Zombie
            && let Ok(mat) = mats.get(e)
            && let Some(copy) = materials.get(&mat.0).cloned()
        {
            let handle = materials.add(copy);
            commands.entity(e).insert(MeshMaterial3d(handle.clone()));
            rig.materials.push(handle);
        }
    }
}

/// Sets clip weights and times; `None` time leaves the clip where it is.
fn pose(player: &mut AnimationPlayer, kit: &ActorKit, weights: [f32; 4], times: [f32; 4]) {
    for clip in Clip::ALL {
        let i = clip as usize;
        if let Some(anim) = player.animation_mut(kit.node(clip)) {
            anim.set_weight(weights[i]);
            if weights[i] > 0.0 {
                anim.seek_to(times[i].clamp(0.0, (kit.duration(clip) - 1e-3).max(0.0)));
            }
        }
    }
}

/// Piecewise map so `impact` of the clip lands exactly at the simulation's active fraction.
fn attack_clip_time(progress: f32, active: f32, impact: f32, duration: f32) -> f32 {
    let f = if progress < active {
        progress / active * impact
    } else {
        impact + (progress - active) / (1.0 - active) * (1.0 - impact)
    };
    f * duration
}

fn walk_clip_time(phase: f32, duration: f32) -> f32 {
    (phase / TAU).rem_euclid(1.0) * duration
}

fn ease_walk(blend: &mut Blend, speed: f32, dt: f32) {
    let target = if speed > 0.05 { 1.0 } else { 0.0 };
    let k = 1.0 - 0.8f32.powf(dt / STEP_S);
    blend.walk += (target - blend.walk) * k;
}

fn sync_granny(
    time: Res<Time>,
    sim: Option<Res<Sim>>,
    clock: Res<StepClock>,
    ground: Option<Res<Ground>>,
    assets: Option<Res<GameAssets>>,
    mut q: Query<(&Rig, &mut Blend, &mut Transform), With<GrannyActor>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let (Some(sim), Some(ground), Some(assets)) = (sim, ground, assets) else {
        return;
    };
    let Ok((rig, mut blend, mut tf)) = q.single_mut() else {
        return;
    };
    let p = &sim.world.player;
    let pos = p.prev_pos.lerp(p.pos, clock.alpha);
    *tf = actor_transform(pos, ground.height(pos), p.facing, GRANNY_SCALE);
    let Some(mut player) = rig.player.and_then(|e| players.get_mut(e).ok()) else {
        return;
    };
    let kit = &assets.granny;
    let speed = (p.pos - p.prev_pos).length() / STEP_S;
    ease_walk(&mut blend, speed, time.delta_secs());
    let walk_t = walk_clip_time(p.walk_phase, kit.duration(Clip::Walk));
    if p.hp == 0 {
        let t = sim.ended_for * 1.1;
        pose(&mut player, kit, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, t]);
    } else if let Some(swing) = p.swing_t() {
        let active = ROLLING_PIN.active_tick as f32 / ROLLING_PIN.total_ticks as f32;
        let t = attack_clip_time(swing, active, GRANNY_IMPACT, kit.duration(Clip::Attack));
        pose(&mut player, kit, [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, t, 0.0]);
    } else {
        let idle_t = time.elapsed_secs().rem_euclid(kit.duration(Clip::Idle));
        let w = blend.walk;
        pose(
            &mut player,
            kit,
            [1.0 - w, w, 0.0, 0.0],
            [idle_t, walk_t, 0.0, 0.0],
        );
    }
}

#[allow(clippy::type_complexity)]
fn sync_zombies(
    time: Res<Time>,
    sim: Option<Res<Sim>>,
    clock: Res<StepClock>,
    ground: Option<Res<Ground>>,
    assets: Option<Res<GameAssets>>,
    mut q: Query<(
        &ZombieActor,
        &Rig,
        &mut Blend,
        &mut Transform,
        &mut Visibility,
    )>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let (Some(sim), Some(ground), Some(assets)) = (sim, ground, assets) else {
        return;
    };
    let kit = &assets.zombie;
    for (slot, rig, mut blend, mut tf, mut vis) in &mut q {
        let z = &sim.world.zombies[slot.0];
        if !z.active || sim.showcase {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let pos = z.prev_pos.lerp(z.pos, clock.alpha);
        *tf = actor_transform(pos, ground.height(pos), z.facing, ZOMBIE_SCALE);
        let Some(mut player) = rig.player.and_then(|e| players.get_mut(e).ok()) else {
            continue;
        };
        let speed = z.vel.length();
        ease_walk(&mut blend, speed, time.delta_secs());
        if let Some(swipe) = z.swipe_t() {
            let active = SHAMBLER.swipe_active_tick as f32 / SHAMBLER.swipe_ticks as f32;
            let t = attack_clip_time(swipe, active, ZOMBIE_IMPACT, kit.duration(Clip::Attack));
            pose(&mut player, kit, [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, t, 0.0]);
        } else {
            let idle_t =
                (time.elapsed_secs() + slot.0 as f32 * 0.37).rem_euclid(kit.duration(Clip::Idle));
            let walk_t = walk_clip_time(z.walk_phase, kit.duration(Clip::Walk));
            let w = blend.walk;
            pose(
                &mut player,
                kit,
                [1.0 - w, w, 0.0, 0.0],
                [idle_t, walk_t, 0.0, 0.0],
            );
        }
    }
}

fn spawn_corpses(
    sim: Option<Res<Sim>>,
    events: Res<FrameEvents>,
    ring: Option<ResMut<CorpseRing>>,
    mut corpses: Query<&mut Corpse>,
) {
    let (Some(sim), Some(mut ring)) = (sim, ring) else {
        return;
    };
    if ring.run_id != sim.run_id {
        ring.run_id = sim.run_id;
        for mut c in &mut corpses {
            c.active = false;
        }
    }
    for e in &events.0 {
        if let SimEvent::ZombieDeath { at, facing, .. } = e {
            let idx = ring.next;
            ring.next = (ring.next + 1) % CORPSE_POOL;
            if let Some(mut c) = corpses.iter_mut().nth(idx) {
                *c = Corpse {
                    active: true,
                    t: 0.0,
                    pos: *at,
                    facing: *facing,
                };
            }
        }
    }
}

fn sync_corpses(
    time: Res<Time>,
    state: Res<State<AppState>>,
    ground: Option<Res<Ground>>,
    assets: Option<Res<GameAssets>>,
    mut q: Query<(&mut Corpse, &Rig, &mut Transform, &mut Visibility)>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let (Some(ground), Some(assets)) = (ground, assets) else {
        return;
    };
    let kit = &assets.zombie;
    let dt = if *state.get() == AppState::Paused {
        0.0
    } else {
        time.delta_secs()
    };
    let die = kit.duration(Clip::Die);
    for (mut c, rig, mut tf, mut vis) in &mut q {
        if !c.active {
            *vis = Visibility::Hidden;
            continue;
        }
        c.t += dt;
        let sink_t = (c.t - die - CORPSE_LINGER).max(0.0) / CORPSE_SINK;
        if sink_t >= 1.0 {
            c.active = false;
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let y = ground.height(c.pos) - sink_t * sink_t * 0.9;
        *tf = actor_transform(c.pos, y, c.facing, ZOMBIE_SCALE);
        if let Some(mut player) = rig.player.and_then(|e| players.get_mut(e).ok()) {
            pose(&mut player, kit, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, c.t]);
        }
    }
}

/// Brief warm-white emissive pulse on a freshly hit zombie.
fn flash_zombies(
    sim: Option<Res<Sim>>,
    q: Query<(&ZombieActor, &Rig)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut lit: Local<Vec<bool>>,
) {
    let Some(sim) = sim else { return };
    lit.resize(ZOMBIE_POOL_CAPACITY, false);
    for (slot, rig) in &q {
        let z = &sim.world.zombies[slot.0];
        let flash = z.active && z.since_hit < 7;
        if !flash && !lit[slot.0] {
            continue;
        }
        let strength = if flash {
            1.0 - z.since_hit as f32 / 7.0
        } else {
            0.0
        };
        for h in &rig.materials {
            if let Some(mut m) = materials.get_mut(h) {
                m.emissive = LinearRgba::rgb(4.0, 3.2, 2.4) * strength;
            }
        }
        lit[slot.0] = flash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_impact_lands_on_active_tick() {
        let active = ROLLING_PIN.active_tick as f32 / ROLLING_PIN.total_ticks as f32;
        let t = attack_clip_time(active, active, GRANNY_IMPACT, 2.0);
        assert!((t - GRANNY_IMPACT * 2.0).abs() < 1e-5);
        assert_eq!(attack_clip_time(0.0, active, GRANNY_IMPACT, 2.0), 0.0);
        assert!((attack_clip_time(1.0, active, GRANNY_IMPACT, 2.0) - 2.0).abs() < 1e-5);
    }

    #[test]
    fn walk_time_wraps_each_stride() {
        assert!((walk_clip_time(TAU * 3.5, 1.0) - 0.5).abs() < 1e-4);
    }
}
