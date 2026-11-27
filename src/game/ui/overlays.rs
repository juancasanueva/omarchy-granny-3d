//! Loading screen, pause card and the end-of-run summary.

use bevy::prelude::*;

use super::{MenuFocus, UiAction, UiActivate, clock, grouped, icon, label, menu_button, mono};
use crate::game::AppState;
use crate::game::assets::{Fonts, LoadFailures, Loading, loading_progress};
use crate::game::run::{RunCommand, Save, Sim};
use crate::palette::UiPalette;

/// Retry is ignored for this long after the summary opens, so a held key can't skip it.
const RETRY_GRACE: f32 = 0.75;

pub struct OverlaysPlugin;

impl Plugin for OverlaysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_loading)
            .add_systems(Update, update_loading.run_if(in_state(AppState::Loading)))
            .add_systems(OnExit(AppState::Loading), despawn::<LoadingRoot>)
            .add_systems(OnEnter(AppState::Paused), spawn_pause)
            .add_systems(OnExit(AppState::Paused), despawn::<PauseRoot>)
            .add_systems(OnEnter(AppState::Summary), spawn_summary)
            .add_systems(OnExit(AppState::Summary), despawn::<SummaryRoot>)
            .add_systems(Update, (pause_toggle, overlay_actions, animate_cards));
    }
}

#[derive(Component)]
struct LoadingRoot;

#[derive(Component)]
struct LoadingBar;

#[derive(Component)]
struct LoadingStatus;

#[derive(Component)]
struct PauseRoot;

#[derive(Component)]
struct SummaryRoot;

/// Fades/scales a card in when its screen opens.
#[derive(Component)]
struct CardIntro {
    t: f32,
}

fn despawn<T: Component>(mut commands: Commands, q: Query<Entity, With<T>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn fullscreen(pal: &UiPalette, alpha: f32) -> impl Bundle {
    (
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(pal.background.with_alpha(alpha)),
        GlobalZIndex(10),
    )
}

fn panel(pal: &UiPalette, width: f32, accent: Color) -> impl Bundle {
    (
        CardIntro { t: 0.0 },
        Node {
            width: px(width),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(36)),
            row_gap: px(14),
            border: UiRect::all(px(1)).with_top(px(4)),
            border_radius: BorderRadius::all(px(20)),
            ..default()
        },
        BackgroundColor(pal.panel.with_alpha(0.96)),
        BorderColor {
            top: accent,
            ..BorderColor::all(pal.panel_hi)
        },
        BoxShadow::new(Color::BLACK.with_alpha(0.6), px(0), px(18), px(0), px(48)),
    )
}

fn spawn_loading(mut commands: Commands, fonts: Res<Fonts>, pal: Res<UiPalette>) {
    commands
        .spawn((LoadingRoot, fullscreen(&pal, 1.0)))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(18),
                ..default()
            })
            .with_children(|c| {
                c.spawn(label(&fonts, "GRANNY", 72.0, pal.foreground));
                c.spawn(label(&fonts, "vs. THE NEIGHBORHOOD", 26.0, pal.accent));
                c.spawn((
                    Node {
                        width: px(320),
                        height: px(6),
                        border_radius: BorderRadius::all(px(3)),
                        margin: UiRect::top(px(24)),
                        ..default()
                    },
                    BackgroundColor(pal.panel_hi),
                ))
                .with_children(|b| {
                    b.spawn((
                        LoadingBar,
                        Node {
                            width: percent(0),
                            height: percent(100),
                            border_radius: BorderRadius::all(px(3)),
                            ..default()
                        },
                        BackgroundColor(pal.accent),
                    ));
                });
                c.spawn((
                    LoadingStatus,
                    mono(&fonts, "Waking up Maplewood…", 14.0, pal.muted),
                ));
            });
        });
}

fn update_loading(
    time: Res<Time>,
    loading: Option<Res<Loading>>,
    server: Res<AssetServer>,
    pal: Res<UiPalette>,
    failures: Option<Res<LoadFailures>>,
    mut bar: Query<&mut Node, With<LoadingBar>>,
    mut status: Query<(&mut Text, &mut TextColor), With<LoadingStatus>>,
    mut shown: Local<f32>,
) {
    if let Some(failures) = failures
        && let Ok((mut text, mut color)) = status.single_mut()
    {
        let message = format!(
            "Could not load the game's models:\n{}\nCheck the install, or set GRANNY3D_ASSETS to the assets folder.",
            failures.0.join("\n")
        );
        if text.0 != message {
            text.0 = message;
            color.0 = pal.bad;
        }
    }
    let target = loading
        .map(|l| loading_progress(&l, &server))
        .unwrap_or(0.0)
        .max(0.08);
    *shown += (target - *shown) * (1.0 - (-6.0 * time.delta_secs()).exp());
    if let Ok(mut n) = bar.single_mut() {
        n.width = percent(*shown * 100.0);
    }
}

fn spawn_pause(
    mut commands: Commands,
    time: Res<Time>,
    fonts: Res<Fonts>,
    pal: Res<UiPalette>,
    sim: Res<Sim>,
    mut focus: ResMut<MenuFocus>,
) {
    focus.reset(time.elapsed_secs(), 0.1);
    let w = &sim.world;
    commands.spawn((PauseRoot, fullscreen(&pal, 0.62))).with_children(|root| {
        root.spawn(panel(&pal, 440.0, pal.accent)).with_children(|card| {
            card.spawn(mono(&fonts, "RUN IN PROGRESS", 13.0, pal.accent));
            card.spawn(label(&fonts, "Paused", 44.0, pal.foreground));
            card.spawn(Node { column_gap: px(18), margin: UiRect::bottom(px(8)), ..default() }).with_children(|r| {
                for (glyph, value, color) in [
                    (icon::CLOCK, clock(w.elapsed_secs()), pal.info),
                    (icon::SKULL, w.kills.to_string(), pal.bad),
                    (icon::CASH, grouped(w.run_cash), pal.good),
                ] {
                    r.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|s| {
                        s.spawn(mono(&fonts, glyph, 15.0, color));
                        s.spawn(mono(&fonts, value, 17.0, pal.foreground));
                    });
                }
            });
            menu_button(card, &fonts, &pal, "Resume", "ESC", UiAction::Resume, 0);
            menu_button(card, &fonts, &pal, "Restart", "R", UiAction::Restart, 1);
            menu_button(card, &fonts, &pal, "Quit to title", "T", UiAction::ToTitle, 2);
            card.spawn((
                label(&fonts, "Quitting discards this run's unbanked cash and stats. Your bank is kept.", 13.0, pal.muted),
                Node { margin: UiRect::top(px(6)), ..default() },
            ));
        });
    });
}

fn spawn_summary(
    mut commands: Commands,
    time: Res<Time>,
    fonts: Res<Fonts>,
    pal: Res<UiPalette>,
    sim: Res<Sim>,
    save: Res<Save>,
    mut focus: ResMut<MenuFocus>,
) {
    focus.reset(time.elapsed_secs(), RETRY_GRACE);
    let w = &sim.world;
    let settled = sim.settlement;
    let new_time = settled.is_some_and(|s| s.new_best_time);
    let new_kills = settled.is_some_and(|s| s.new_best_kills);
    let save_line = match settled {
        Some(s) if s.saved && s.credited > 0 => {
            format!("{} cash banked and saved.", grouped(s.credited))
        }
        Some(s) if s.saved => "No cash collected this run.".to_string(),
        Some(_) => "Saving unavailable: earnings are kept only until the game closes.".to_string(),
        None => String::new(),
    };
    let accuracy = format!(
        "{} damage dealt  ·  {} shots fired",
        grouped(w.damage_dealt),
        grouped(w.shots_fired as u64)
    );
    commands.spawn((SummaryRoot, fullscreen(&pal, 0.55))).with_children(|root| {
        root.spawn(panel(&pal, 600.0, pal.bad)).with_children(|card| {
            card.spawn(mono(&fonts, "THE NIGHT IS OVER", 13.0, pal.bad));
            card.spawn(label(&fonts, "Granny has fallen", 44.0, pal.foreground));
            card.spawn(label(&fonts, "Maplewood claims another night. The neighborhood will remember her rolling pin.", 15.0, pal.muted));
            card.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(12), row_gap: px(12), margin: UiRect::vertical(px(10)), ..default() })
                .with_children(|grid| {
                    let tiles = [
                        ("SURVIVED", icon::CLOCK, clock(w.elapsed_secs()), pal.info, new_time),
                        ("KILLS", icon::SKULL, grouped(w.kills as u64), pal.bad, new_kills),
                        ("CASH EARNED", icon::CASH, format!("+{}", grouped(w.run_cash)), pal.good, false),
                        ("BANK", icon::CASH, grouped(save.0.data.cash), pal.warn, false),
                    ];
                    for (name, glyph, value, color, best) in tiles {
                        grid.spawn((
                            Node {
                                width: px(250),
                                flex_direction: FlexDirection::Column,
                                padding: UiRect::axes(px(18), px(14)),
                                row_gap: px(4),
                                border: UiRect::left(px(3)),
                                border_radius: BorderRadius::all(px(12)),
                                ..default()
                            },
                            BackgroundColor(pal.background.with_alpha(0.6)),
                            BorderColor::all(color),
                        ))
                        .with_children(|t| {
                            t.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|h| {
                                h.spawn(mono(&fonts, glyph, 13.0, color));
                                h.spawn(mono(&fonts, name, 12.0, pal.muted));
                                if best {
                                    h.spawn((
                                        Node { padding: UiRect::axes(px(6), px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
                                        BackgroundColor(pal.accent),
                                    ))
                                    .with_children(|b| {
                                        b.spawn(mono(&fonts, "NEW BEST", 11.0, pal.background));
                                    });
                                }
                            });
                            t.spawn(mono(&fonts, value, 30.0, pal.foreground));
                        });
                    }
                });
            card.spawn(mono(&fonts, accuracy, 13.0, pal.muted));
            card.spawn(label(&fonts, save_line, 14.0, if settled.is_some_and(|s| s.saved) { pal.good } else { pal.warn }));
            card.spawn(Node { column_gap: px(12), margin: UiRect::top(px(8)), ..default() }).with_children(|row| {
                row.spawn(Node { flex_grow: 1.0, ..default() }).with_children(|c| {
                    menu_button(c, &fonts, &pal, "Retry", "ENTER", UiAction::Restart, 0);
                });
                row.spawn(Node { flex_grow: 1.0, ..default() }).with_children(|c| {
                    menu_button(c, &fonts, &pal, "Title", "ESC", UiAction::ToTitle, 1);
                });
            });
        });
    });
}

fn pause_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    time: Res<Time>,
    focus: Res<MenuFocus>,
    sim: Option<Res<Sim>>,
    mut out: MessageWriter<UiActivate>,
    mut run: MessageWriter<RunCommand>,
) {
    match state.get() {
        AppState::Playing
            if keys.just_pressed(KeyCode::Escape)
                && sim.is_some_and(|s| s.settlement.is_none()) =>
        {
            run.write(RunCommand::Pause);
        }
        AppState::Paused => {
            if keys.just_pressed(KeyCode::Escape) {
                out.write(UiActivate(UiAction::Resume));
            }
            if keys.just_pressed(KeyCode::KeyR) {
                out.write(UiActivate(UiAction::Restart));
            }
            if keys.just_pressed(KeyCode::KeyT) {
                out.write(UiActivate(UiAction::ToTitle));
            }
        }
        AppState::Summary if time.elapsed_secs() >= focus.armed_at => {
            if keys.just_pressed(KeyCode::Escape) {
                out.write(UiActivate(UiAction::ToTitle));
            }
            if keys.just_pressed(KeyCode::KeyR) {
                out.write(UiActivate(UiAction::Restart));
            }
        }
        _ => {}
    }
}

fn overlay_actions(
    state: Res<State<AppState>>,
    mut actions: MessageReader<UiActivate>,
    mut run: MessageWriter<RunCommand>,
) {
    if !matches!(state.get(), AppState::Paused | AppState::Summary) {
        return;
    }
    for UiActivate(action) in actions.read() {
        match action {
            UiAction::Resume => {
                run.write(RunCommand::Resume);
            }
            UiAction::Restart => {
                run.write(RunCommand::Restart);
            }
            UiAction::ToTitle => {
                run.write(RunCommand::ToTitle);
            }
            _ => {}
        }
    }
}

fn animate_cards(time: Res<Time>, mut q: Query<(&mut CardIntro, &mut UiTransform)>) {
    for (mut c, mut tf) in &mut q {
        c.t = (c.t + time.delta_secs() / 0.28).min(1.0);
        let ease = 1.0 - (1.0 - c.t).powi(3);
        tf.scale = Vec2::splat(0.94 + 0.06 * ease);
        tf.translation = Val2::px(0.0, 24.0 * (1.0 - ease));
    }
}
