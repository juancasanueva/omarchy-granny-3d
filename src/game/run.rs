//! Drives `sim::World` at a fixed 60 Hz from wall-clock frames, samples input,
//! and publishes each frame's simulation events to presentation systems.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow, WindowFocused};

use super::AppState;
use super::assets::{Ground, LayoutRes};
use super::scene::GameCamera;
use crate::sim::data::{Gun, STEP_S, facing_camera, perk_tuning};
use crate::sim::save::{SaveStore, Settlement};
use crate::sim::world::{InputSnapshot, Phase, SimEvent, World};

/// Every run uses seed 1, like the web game; restart replays the same horde.
pub const RUN_SEED: u32 = 1;
const MAX_STEPS_PER_FRAME: u32 = 5;
/// Seconds between Granny falling and the run summary appearing.
pub const SUMMARY_DELAY: f32 = 1.8;

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Save(SaveStore::load_default()))
            .init_resource::<PendingInput>()
            .init_resource::<Aim>()
            .init_resource::<FrameEvents>()
            .init_resource::<StepClock>()
            .add_message::<RunCommand>()
            .add_systems(OnExit(AppState::Loading), spawn_showcase_world)
            .add_systems(
                Update,
                (
                    sample_input.run_if(in_state(AppState::Playing)),
                    advance_sim,
                    watch_run_end.run_if(in_state(AppState::Playing)),
                    pause_on_focus_loss.run_if(in_state(AppState::Playing)),
                    apply_run_commands,
                )
                    .chain()
                    .in_set(SimAdvance),
            );
    }
}

/// Presentation systems that read this frame's simulation state run `.after(SimAdvance)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimAdvance;

#[derive(Resource)]
pub struct Sim {
    pub world: World,
    /// True for the idle title-screen world, which never steps.
    pub showcase: bool,
    pub ended_for: f32,
    pub settlement: Option<Settlement>,
    /// Unique per world, so presentation can reset when a new run replaces the old one.
    pub run_id: u32,
}

impl Sim {
    fn new(mut world: World, showcase: bool) -> Self {
        if showcase {
            // The title screen shows Granny's face, not her back.
            // Three-quarter view: her face reads and the aimed gun points across the screen.
            world.player.facing = facing_camera() + TITLE_TURN;
        }
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let run_id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            world,
            showcase,
            ended_for: 0.0,
            settlement: None,
            run_id,
        }
    }
}

#[derive(Resource)]
pub struct Save(pub SaveStore);

/// Interpolation between the previous and current simulation tick.
#[derive(Resource, Default)]
pub struct StepClock {
    accumulator: f32,
    pub alpha: f32,
}

/// Simulation events produced this frame (cleared at the start of every frame).
#[derive(Resource, Default)]
pub struct FrameEvents(pub Vec<SimEvent>);

/// Edge-triggered input survives until a simulation step consumes it.
#[derive(Resource, Default)]
struct PendingInput {
    melee: bool,
    fire: bool,
    select: Option<Gun>,
    cycle: bool,
    /// The latest fire press came from F, which auto-targets.
    auto_target: bool,
}

/// How Granny is aimed: the mouse after any mouse motion or click, the keyboard after F.
#[derive(Resource, Default)]
pub struct Aim {
    pub mouse: bool,
    /// Aim target: the cursor ray at a zombie's chest height, while the mouse aims.
    pub point: Option<Vec2>,
    /// Where the cursor ray meets the ground, for drawing the reticle under the pointer.
    pub reticle: Option<Vec2>,
}

/// Height above the ground where the cursor ray is intersected: roughly a zombie's chest,
/// so pointing at a body aims at that body rather than the ground behind it.
const AIM_PLANE_HEIGHT: f32 = 1.0;

/// How far Granny turns from facing the camera on the title screen, in radians.
const TITLE_TURN: f32 = 0.85;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunCommand {
    Start,
    Pause,
    Resume,
    Restart,
    ToTitle,
}

fn new_world(layout: &LayoutRes, save: &Save) -> World {
    World::new(layout.0.clone(), perk_tuning(save.0.data.cardio), RUN_SEED)
}

fn spawn_showcase_world(mut commands: Commands, layout: Res<LayoutRes>, save: Res<Save>) {
    commands.insert_resource(Sim::new(new_world(&layout, &save), true));
}

fn sample_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    mut motion: MessageReader<CursorMoved>,
    mut pending: ResMut<PendingInput>,
    mut aim: ResMut<Aim>,
    sim: Res<Sim>,
    ground: Option<Res<Ground>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
) {
    if keys.just_pressed(KeyCode::Space) {
        pending.melee = true;
    }
    if motion.read().count() > 0 || mouse.just_pressed(MouseButton::Left) {
        aim.mouse = true;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        aim.mouse = false;
        pending.fire = true;
        pending.auto_target = true;
    }
    if mouse.just_pressed(MouseButton::Left) {
        pending.fire = true;
        pending.auto_target = false;
    }
    aim.point = None;
    aim.reticle = None;
    if aim.mouse
        && let (Ok(window), Ok((cam, cam_tf))) = (windows.single(), camera.single())
        && let Some(cursor) = window.cursor_position()
        && let Ok(ray) = cam.viewport_to_world(cam_tf, cursor)
    {
        let p = sim.world.player.pos;
        let ground_y = ground.map(|g| g.height(p)).unwrap_or(0.0);
        let dy = ray.direction.y;
        if dy < -1e-4 {
            let at = |y: f32| (ray.origin + *ray.direction * ((y - ray.origin.y) / dy)).xz();
            aim.point = Some(at(ground_y + AIM_PLANE_HEIGHT));
            aim.reticle = Some(at(ground_y));
        }
    }
    if super::dev::autoplay() {
        // Scripted aim sweeps around Granny, independent of her walking direction.
        let w = &sim.world;
        aim.point = Some(w.player.pos + Vec2::from_angle(-(w.tick as f32) / 60.0 * 1.1) * 5.0);
        aim.reticle = aim.point;
    }
    if keys.just_pressed(KeyCode::Digit1) {
        pending.select = Some(Gun::Revolver);
    }
    if keys.just_pressed(KeyCode::Digit2) {
        pending.select = Some(Gun::Shotgun);
    }
    if keys.just_pressed(KeyCode::KeyQ) || wheel.read().any(|w| w.y != 0.0) {
        pending.cycle = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn advance_sim(
    time: Res<Time>,
    state: Res<State<AppState>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    sim: Option<ResMut<Sim>>,
    aim: Res<Aim>,
    mut pending: ResMut<PendingInput>,
    mut clock: ResMut<StepClock>,
    mut events: ResMut<FrameEvents>,
) {
    events.0.clear();
    let Some(mut sim) = sim else { return };
    if *state.get() != AppState::Playing || sim.showcase {
        clock.accumulator = 0.0;
        clock.alpha = 1.0;
        return;
    }
    let held = |k: &[KeyCode]| keys.any_pressed(k.iter().copied());
    // Two-key movement: forward toward where Granny aims, back away from it.
    let throttle = held(&[KeyCode::KeyW, KeyCode::ArrowUp]) as i32 as f32
        - held(&[KeyCode::KeyS, KeyCode::ArrowDown]) as i32 as f32;
    let mouse_fire = mouse.pressed(MouseButton::Left);
    let key_fire = keys.pressed(KeyCode::KeyF);
    let mut fire_held = mouse_fire || key_fire;
    // Holding F auto-targets unless the mouse button is also held.
    let auto_held = key_fire && !mouse_fire;
    let mut move_dir = Vec2::ZERO;
    let scripted = super::dev::autoplay();
    let scripted_gun = scripted.then(super::dev::autoplay_gun).flatten();
    if scripted {
        let t = sim.world.tick as f32 / 60.0;
        move_dir = Vec2::from_angle(t * 0.5) * if (t as u32) % 4 == 3 { 0.0 } else { 1.0 };
        fire_held = !(t as u32).is_multiple_of(3);
    }

    clock.accumulator =
        (clock.accumulator + time.delta_secs()).min(STEP_S * MAX_STEPS_PER_FRAME as f32);
    while clock.accumulator >= STEP_S {
        clock.accumulator -= STEP_S;
        let input = InputSnapshot {
            move_dir,
            throttle,
            melee_pulse: std::mem::take(&mut pending.melee)
                || (scripted && sim.world.tick % 40 == 0),
            fire_held,
            fire_pulse: std::mem::take(&mut pending.fire),
            select: pending.select.take().or(scripted_gun),
            cycle: std::mem::take(&mut pending.cycle),
            aim_point: aim.point,
            auto_target: std::mem::take(&mut pending.auto_target) || auto_held,
        };
        sim.world.step(&input);
        let drained = std::mem::take(&mut sim.world.events);
        events.0.extend(drained);
    }
    clock.alpha = clock.accumulator / STEP_S;
    if sim.world.phase == Phase::Ended {
        sim.ended_for += time.delta_secs();
        clock.alpha = 1.0;
    }
}

fn watch_run_end(
    mut sim: ResMut<Sim>,
    mut save: ResMut<Save>,
    mut next: ResMut<NextState<AppState>>,
) {
    if sim.world.phase != Phase::Ended {
        return;
    }
    if sim.settlement.is_none() {
        let w = &sim.world;
        let result = save.0.settle(w.run_cash, w.elapsed_secs(), w.kills);
        sim.settlement = Some(result);
    }
    if sim.ended_for >= SUMMARY_DELAY {
        next.set(AppState::Summary);
    }
}

fn pause_on_focus_loss(
    mut focus: MessageReader<WindowFocused>,
    mut commands: MessageWriter<RunCommand>,
) {
    if focus.read().any(|f| !f.focused) && !super::dev::autoplay() {
        commands.write(RunCommand::Pause);
    }
}

fn apply_run_commands(
    mut commands_in: MessageReader<RunCommand>,
    mut commands: Commands,
    layout: Res<LayoutRes>,
    save: Res<Save>,
    state: Res<State<AppState>>,
    mut pending: ResMut<PendingInput>,
    mut next: ResMut<NextState<AppState>>,
) {
    for cmd in commands_in.read() {
        match cmd {
            RunCommand::Start | RunCommand::Restart => {
                *pending = PendingInput::default();
                commands.insert_resource(Sim::new(new_world(&layout, &save), false));
                next.set(AppState::Playing);
            }
            RunCommand::Pause if *state.get() == AppState::Playing => next.set(AppState::Paused),
            RunCommand::Resume if *state.get() == AppState::Paused => {
                *pending = PendingInput::default();
                next.set(AppState::Playing);
            }
            RunCommand::ToTitle => {
                commands.insert_resource(Sim::new(new_world(&layout, &save), true));
                next.set(AppState::Title);
            }
            _ => {}
        }
    }
}
