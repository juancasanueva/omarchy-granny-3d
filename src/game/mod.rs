//! Bevy presentation layer: assets, 3D scene, actors, effects and UI around `sim::World`.

pub mod actors;
pub mod assets;
pub mod dev;
pub mod fx;
pub mod run;
pub mod scene;
pub mod ui;

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use crate::palette::PalettePlugin;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Loading,
    Title,
    Shop,
    Playing,
    Paused,
    Summary,
}

impl AppState {
    /// States that show a run's world (not the title showcase).
    pub fn in_run(self) -> bool {
        matches!(
            self,
            AppState::Playing | AppState::Paused | AppState::Summary
        )
    }
}

pub fn run() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Granny vs. The Neighborhood".into(),
                        name: Some("granny3d".into()),
                        resolution: WindowResolution::new(1600, 900),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: assets_dir(),
                    ..default()
                })
                // No bindless materials. With texture binding arrays, Bevy keeps every
                // material in one shared table that shaders index into; on Intel Iris
                // Plus (Ice Lake, Mesa 26.2) those lookups go wrong some frames, so
                // models flicker or lose their textures. Hiding the feature makes Bevy
                // bind each material on its own, which is plenty fast for this scene.
                .set(bevy::render::RenderPlugin {
                    render_creation: bevy::render::settings::WgpuSettings {
                        disabled_features: Some(
                            bevy::render::settings::WgpuFeatures::TEXTURE_BINDING_ARRAY,
                        ),
                        ..default()
                    }
                    .into(),
                    ..default()
                }),
        )
        .add_plugins(PalettePlugin)
        .init_state::<AppState>()
        .add_plugins((
            assets::AssetsPlugin,
            scene::ScenePlugin,
            run::RunPlugin,
            actors::ActorsPlugin,
            fx::FxPlugin,
            ui::UiPlugin,
            dev::DevPlugin,
        ))
        .run();
}

/// Assets live next to the binary when installed (`/usr/share/granny3d/assets` or
/// `$PREFIX/share/granny3d/assets`), or in the crate's `assets/` during development.
fn assets_dir() -> String {
    if let Ok(dir) = std::env::var("GRANNY3D_ASSETS") {
        return dir;
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    if let Some(dir) = exe_dir {
        let installed = dir.join("../share/granny3d/assets");
        if installed.join("models/granny.glb").is_file() {
            return installed.to_string_lossy().into_owned();
        }
    }
    concat!(env!("CARGO_MANIFEST_DIR"), "/assets").to_string()
}
