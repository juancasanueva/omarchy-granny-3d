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
                    sync_guns,
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

/// A gun model hanging from Granny's right hand.
#[derive(Component)]
struct GunModel(Gun);

/// The muzzle point of a gun model, where shots leave the barrel.
#[derive(Component)]
pub struct Muzzle(pub Gun);

#[derive(Component, Default)]
struct Blend {
    walk: f32,
    /// 0 holds the revolver pose, 1 the shotgun pose; eased so switching guns blends.
    shotgun: f32,
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

fn spawn_actors(mut commands: Commands, assets: Res<GameAssets>) {
    commands.init_resource::<CorpseRing>();
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
            for node in Clip::ALL.into_iter().filter_map(|c| kit_assets.node(c)) {
                player.play(node).pause().set_weight(0.0);
            }
            commands
                .entity(e)
                .insert(AnimationGraphHandle(kit_assets.graph.clone()));
            rig.player = Some(e);
        }
        if *kit == Kit::Granny && names.get(e).is_ok_and(|n| n.as_str() == "hand.R") {
            rig.hand = Some(e);
            // The gun models are exported in the hand bone's own frame.
            commands.entity(e).with_children(|hand| {
                for gun in Gun::ALL {
                    hand.spawn((
                        GunModel(gun),
                        WorldAssetRoot(assets.gun(gun)),
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .observe(mark_muzzle);
                }
            });
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

fn mark_muzzle(
    ready: On<WorldInstanceReady>,
    guns: Query<&GunModel>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut commands: Commands,
) {
    let Ok(gun) = guns.get(ready.entity) else {
        return;
    };
    for e in children.iter_descendants(ready.entity) {
        if names.get(e).is_ok_and(|n| n.as_str() == "Muzzle") {
            commands.entity(e).insert(Muzzle(gun.0));
        }
    }
}

/// Drives the listed clips at `(weight, time)` and silences every other clip.
fn pose(player: &mut AnimationPlayer, kit: &ActorKit, active: &[(Clip, f32, f32)]) {
    for clip in Clip::ALL {
        let Some(anim) = kit.node(clip).and_then(|n| player.animation_mut(n)) else {
            continue;
        };
        match active.iter().find(|a| a.0 == clip) {
            Some(&(_, weight, t)) => {
                anim.set_weight(weight);
                if weight > 0.0 {
                    anim.seek_to(t.clamp(0.0, (kit.duration(clip) - 1e-3).max(0.0)));
                }
            }
            None => {
                anim.set_weight(0.0);
            }
        }
    }
}

/// Seconds since the last shot, when the selected gun's fire clip is still playing.
fn firing(since_shot_ticks: u32, alpha: f32, fire_duration: f32) -> Option<f32> {
    let t = (since_shot_ticks as f32 + alpha) * STEP_S;
    (since_shot_ticks != u32::MAX && t < fire_duration).then_some(t)
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

/// True when the step points away from where the actor faces.
fn backpedaling(step: Vec2, facing: f32) -> bool {
    step.length_squared() > 1e-8 && step.dot(Vec2::from_angle(facing)) < 0.0
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
    let walk_dur = kit.duration(Clip::Walk);
    let mut walk_t = walk_clip_time(p.walk_phase, walk_dur);
    if backpedaling(p.pos - p.prev_pos, p.facing) {
        // Stepping backward: run the stride in reverse instead of moonwalking.
        walk_t = walk_dur - walk_t;
    }
    let target = if p.selected == Gun::Shotgun { 1.0 } else { 0.0 };
    blend.shotgun += (target - blend.shotgun) * (1.0 - (-14.0 * time.delta_secs()).exp());
    if p.hp == 0 {
        let t = sim.ended_for * 1.1;
        pose(&mut player, kit, &[(Clip::Die, 1.0, t)]);
    } else if let Some(swing) = p.swing_t() {
        let active = ROLLING_PIN.active_tick as f32 / ROLLING_PIN.total_ticks as f32;
        let t = attack_clip_time(swing, active, GRANNY_IMPACT, kit.duration(Clip::Attack));
        pose(&mut player, kit, &[(Clip::Attack, 1.0, t)]);
    } else {
        // Legs idle or walk; the upper body holds the selected gun and kicks on each shot.
        let w = blend.walk;
        let idle_t = time.elapsed_secs().rem_euclid(kit.duration(Clip::Idle));
        let mut layers = vec![
            (Clip::IdleLower, 1.0 - w, idle_t),
            (Clip::WalkLower, w, walk_t),
        ];
        let alpha = if sim.showcase { 0.0 } else { clock.alpha };
        for (gun, share) in [
            (Gun::Revolver, 1.0 - blend.shotgun),
            (Gun::Shotgun, blend.shotgun),
        ] {
            let aim_t = time.elapsed_secs().rem_euclid(kit.duration(Clip::aim(gun)));
            match firing(p.since_shot, alpha, kit.duration(Clip::fire(gun)))
                .filter(|_| gun == p.selected)
            {
                Some(t) => layers.push((Clip::fire(gun), share, t)),
                None => layers.push((Clip::aim(gun), share, aim_t)),
            }
        }
        pose(&mut player, kit, &layers);
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
            pose(&mut player, kit, &[(Clip::Attack, 1.0, t)]);
        } else {
            let idle_t =
                (time.elapsed_secs() + slot.0 as f32 * 0.37).rem_euclid(kit.duration(Clip::Idle));
            let walk_t = walk_clip_time(z.walk_phase, kit.duration(Clip::Walk));
            let w = blend.walk;
            pose(
                &mut player,
                kit,
                &[(Clip::Idle, 1.0 - w, idle_t), (Clip::Walk, w, walk_t)],
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
            pose(&mut player, kit, &[(Clip::Die, 1.0, c.t)]);
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

/// Shows the selected gun in Granny's hand; guns are put away while she swings or falls.
fn sync_guns(sim: Option<Res<Sim>>, mut guns: Query<(&GunModel, &mut Visibility)>) {
    let Some(sim) = sim else { return };
    let p = &sim.world.player;
    let holding = p.hp > 0 && p.attack_tick.is_none();
    for (gun, mut vis) in &mut guns {
        *vis = if holding && gun.0 == p.selected {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
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
    fn backpedaling_only_when_stepping_away_from_facing() {
        assert!(backpedaling(Vec2::new(-0.05, 0.0), 0.0));
        assert!(!backpedaling(Vec2::new(0.05, 0.0), 0.0));
        assert!(!backpedaling(Vec2::ZERO, 0.0));
    }

    #[test]
    fn fire_clip_plays_only_right_after_a_shot() {
        assert_eq!(firing(0, 0.0, 0.33), Some(0.0));
        assert!(firing(10, 0.5, 0.33).is_some());
        assert_eq!(firing(30, 0.0, 0.33), None);
        assert_eq!(firing(u32::MAX, 0.0, 0.33), None, "never fired");
    }

    #[test]
    fn walk_time_wraps_each_stride() {
        assert!((walk_clip_time(TAU * 3.5, 1.0) - 0.5).abs() < 1e-4);
    }
}
