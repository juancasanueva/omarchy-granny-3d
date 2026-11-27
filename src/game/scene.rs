//! The Maplewood night: town, street lamps, moonlight, and the fixed oblique follow camera.

use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::GltfMaterialName;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use super::AppState;
use super::assets::{GameAssets, LayoutRes};
use super::run::{FrameEvents, Sim, SimAdvance, StepClock};
use crate::palette::hex;
use crate::sim::data::{FIXED_CAMERA, camera_offset};
use crate::sim::rng::Rng;
use crate::sim::world::SimEvent;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: hex("#5a6f9e"),
            brightness: 320.0,
            affects_lightmapped_meshes: true,
        })
        .insert_resource(Shake {
            trauma: 0.0,
            offset: Vec2::ZERO,
            rng: Rng::new(0x9e37_79b9),
        })
        .add_systems(Startup, spawn_camera)
        .add_systems(OnExit(AppState::Loading), spawn_world)
        .add_systems(
            Update,
            (update_shake, follow_camera).chain().after(SimAdvance),
        );
    }
}

#[derive(Component)]
pub struct GameCamera;

/// Render-only screen shake (`render/shake.ts`): melee hits add trauma, offset ∝ trauma².
#[derive(Resource)]
pub struct Shake {
    pub trauma: f32,
    pub offset: Vec2,
    rng: Rng,
}

impl Shake {
    pub fn add(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }
}

#[derive(Component)]
struct TownRoot;

fn spawn_camera(mut commands: Commands) {
    let (sx, sy, sz) = camera_offset();
    commands.spawn((
        GameCamera,
        Camera3d::default(),
        Hdr,
        Tonemapping::AcesFitted,
        Bloom {
            intensity: 0.22,
            ..Bloom::NATURAL
        },
        Msaa::Sample4,
        Projection::Perspective(PerspectiveProjection {
            fov: FIXED_CAMERA.fov_deg.to_radians(),
            near: 0.5,
            far: 260.0,
            ..default()
        }),
        DistanceFog {
            color: hex("#0b1424"),
            directional_light_color: hex("#3a4a6a").with_alpha(0.3),
            directional_light_exponent: 20.0,
            falloff: FogFalloff::Linear {
                start: 38.0,
                end: 95.0,
            },
        },
        Transform::from_xyz(sx, sy, sz).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_world(mut commands: Commands, assets: Res<GameAssets>, layout: Res<LayoutRes>) {
    let shadow = CascadeShadowConfigBuilder {
        num_cascades: 3,
        minimum_distance: 1.0,
        first_cascade_far_bound: 24.0,
        maximum_distance: 70.0,
        overlap_proportion: 0.2,
    }
    .build();
    commands.spawn((
        DirectionalLight {
            color: hex("#9db9eb"),
            illuminance: 3800.0,
            shadow_maps_enabled: true,
            ..default()
        },
        shadow,
        Transform::from_xyz(20.0, 40.0, 24.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            color: hex("#c4d2e8"),
            illuminance: 1100.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-20.0, 16.0, 24.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands
        .spawn((
            TownRoot,
            WorldAssetRoot(assets.town.clone()),
            Transform::default(),
        ))
        .observe(hide_native_lamps);

    for lamp in &layout.0.lamps {
        commands
            .spawn((
                WorldAssetRoot(assets.lamp.clone()),
                Transform::from_xyz(lamp.pos.x, lamp.base_y, lamp.pos.y),
            ))
            .with_children(|p| {
                p.spawn((
                    PointLight {
                        color: hex("#ffb86b"),
                        intensity: 700_000.0,
                        range: 11.0,
                        radius: 0.2,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 2.95, 0.0),
                ));
            });
    }
}

/// The town's own lamp posts are replaced by the lantern model, as in the web game.
fn hide_native_lamps(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    names: Query<&GltfMaterialName>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ready.entity) {
        if names.get(e).is_ok_and(|n| n.0 == "streets") {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
}

fn update_shake(
    time: Res<Time>,
    events: Res<FrameEvents>,
    mut shake: ResMut<Shake>,
    sim: Option<Res<Sim>>,
) {
    let tuning = sim
        .as_ref()
        .map(|s| s.world.tuning.clone())
        .unwrap_or_default();
    for e in &events.0 {
        match e {
            SimEvent::MeleeHit { intensity, .. } => shake.add(*intensity),
            SimEvent::PlayerHurt { .. } => shake.add(0.35),
            _ => {}
        }
    }
    shake.trauma = (shake.trauma - tuning.shake_decay * time.delta_secs()).max(0.0);
    if shake.trauma <= 0.0 {
        shake.offset = Vec2::ZERO;
        return;
    }
    let magnitude = shake.trauma * shake.trauma * tuning.shake_amplitude;
    let (x, y) = (shake.rng.signed(), shake.rng.signed());
    shake.offset = Vec2::new(x, y) * magnitude;
}

fn follow_camera(
    time: Res<Time>,
    state: Res<State<AppState>>,
    sim: Option<Res<Sim>>,
    clock: Res<StepClock>,
    shake: Res<Shake>,
    mut cam: Query<&mut Transform, With<GameCamera>>,
) {
    let Some(sim) = sim else { return };
    let Ok(mut tf) = cam.single_mut() else { return };
    let (sx, sy, sz) = camera_offset();
    let offset = Vec3::new(sx, sy, sz);
    let target = if sim.showcase || *state.get() == AppState::Title {
        // Title: drift slowly along the street with Granny framed right of the menu.
        let t = time.elapsed_secs();
        let right = Vec3::Y.cross(offset).normalize();
        let p = sim.world.player.pos;
        Vec3::new(p.x, 0.0, p.y) - right * 4.2
            + Vec3::new((t * 0.07).sin() * 1.5, 0.0, (t * 0.05).cos() * 1.0)
    } else {
        let w = &sim.world;
        let c = w.prev_camera.0.lerp(w.camera.0, clock.alpha);
        Vec3::new(c.x + shake.offset.x, 0.0, c.y + shake.offset.y)
    };
    *tf = Transform::from_translation(target + offset).looking_at(target, Vec3::Y);
}
