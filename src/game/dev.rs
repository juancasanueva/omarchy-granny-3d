//! Development aids, driven by environment variables (all optional):
//! - `GRANNY3D_AUTOSTART=play|pause|summary` skips the title screen.
//! - `GRANNY3D_AUTOPLAY=1` drives Granny with scripted input.
//! - `GRANNY3D_SHOT=/path.png` saves a screenshot after `GRANNY3D_SHOT_AFTER` seconds (default 6) and exits.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use super::AppState;
use super::run::{RunCommand, Sim};
use crate::sim::world::Phase;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Title), autostart)
            .add_systems(Update, (autoplay_state, screenshot));
    }
}

/// Scripted input for unattended runs, read by the run driver.
pub fn autoplay() -> bool {
    std::env::var("GRANNY3D_AUTOPLAY").is_ok_and(|v| v == "1")
}

fn autostart(mut run: MessageWriter<RunCommand>, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    if matches!(
        std::env::var("GRANNY3D_AUTOSTART").as_deref(),
        Ok("play" | "pause" | "summary")
    ) {
        run.write(RunCommand::Start);
    }
}

fn autoplay_state(
    time: Res<Time>,
    state: Res<State<AppState>>,
    sim: Option<ResMut<Sim>>,
    mut run: MessageWriter<RunCommand>,
    mut playing_since: Local<Option<f32>>,
) {
    let mode = std::env::var("GRANNY3D_AUTOSTART").unwrap_or_default();
    if *state.get() != AppState::Playing {
        return;
    }
    let since = *playing_since.get_or_insert(time.elapsed_secs());
    let t = time.elapsed_secs() - since;
    match mode.as_str() {
        "pause" if t > 3.0 => {
            run.write(RunCommand::Pause);
        }
        "summary" if t > 2.0 => {
            if let Some(mut sim) = sim
                && sim.world.phase == Phase::Running
            {
                sim.world.player.hp = 1;
                sim.world.player.grace_until = 0;
                let p = sim.world.player.pos;
                if let Some(z) = sim.world.zombies.iter_mut().find(|z| z.active) {
                    z.pos = p + Vec2::new(0.8, 0.0);
                    z.prev_pos = z.pos;
                    z.swipe_cooldown = 0;
                }
            }
        }
        _ => {}
    }
}

fn screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
    mut taken: Local<Option<f32>>,
) {
    let Ok(path) = std::env::var("GRANNY3D_SHOT") else {
        return;
    };
    let after: f32 = std::env::var("GRANNY3D_SHOT_AFTER")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6.0);
    let now = time.elapsed_secs();
    match *taken {
        None if now >= after => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
            *taken = Some(now);
        }
        Some(at) if now > at + 1.5 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
