//! Title screen over the live night-time town: menu, bank, stocked ammo and personal bests.

use bevy::prelude::*;

use super::{
    Intro, MenuFocus, UiAction, UiActivate, clock, grouped, icon, keycap, label, menu_button, mono,
    set_text,
};
use crate::game::AppState;
use crate::game::assets::Fonts;
use crate::game::run::{RunCommand, Save};
use crate::palette::UiPalette;
use crate::sim::data::Gun;
use crate::sim::save::SaveStatus;

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Title), spawn_title)
            .add_systems(OnExit(AppState::Title), despawn_title)
            .add_systems(
                Update,
                (title_shortcuts, title_actions, refresh_title)
                    .chain()
                    .run_if(in_state(AppState::Title)),
            );
    }
}

#[derive(Component)]
struct TitleRoot;

#[derive(Component)]
enum StatText {
    Bank,
    BestTime,
    BestKills,
    Runs,
}

/// What the shop has stocked for the next run.
#[derive(Component)]
struct StockText;

#[derive(Component)]
struct SaveWarning;

fn spawn_title(
    mut commands: Commands,
    time: Res<Time>,
    fonts: Res<Fonts>,
    pal: Res<UiPalette>,
    mut focus: ResMut<MenuFocus>,
) {
    focus.reset(time.elapsed_secs(), 0.15);
    commands
        .spawn((
            TitleRoot,
            Node {
                width: percent(100),
                height: percent(100),
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundGradient::from(LinearGradient::to_right(vec![
                ColorStop::new(pal.background.with_alpha(0.94), percent(0)),
                ColorStop::new(pal.background.with_alpha(0.78), percent(32)),
                ColorStop::new(pal.background.with_alpha(0.0), percent(62)),
            ])),
        ))
        .with_children(|root| {
            // Left column: identity and menu.
            root.spawn(Node {
                width: px(600),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(px(72), px(56)),
                row_gap: px(10),
                ..default()
            })
            .with_children(|col| {
                col.spawn((Intro { delay: 0.0 }, mono(&fonts, "A MAPLEWOOD NIGHT  ·  FOR OMARCHY", 14.0, pal.accent)));
                col.spawn((
                    Intro { delay: 0.08 },
                    label(&fonts, "GRANNY", 112.0, pal.foreground),
                    TextShadow { offset: Vec2::new(0.0, 6.0), color: Color::BLACK.with_alpha(0.6) },
                ));
                col.spawn((
                    Intro { delay: 0.16 },
                    label(&fonts, "vs. THE NEIGHBORHOOD", 36.0, pal.accent),
                    TextShadow { offset: Vec2::new(0.0, 3.0), color: Color::BLACK.with_alpha(0.6) },
                ));
                col.spawn((
                    Intro { delay: 0.24 },
                    label(
                        &fonts,
                        "The dead are shambling down Maplewood Lane. Granny has a baseball bat, a revolver and a cane shotgun. Survive as long as you can.",
                        17.0,
                        pal.muted,
                    ),
                    Node { max_width: px(460), margin: UiRect::vertical(px(14)), ..default() },
                ));
                col.spawn((
                    Intro { delay: 0.32 },
                    Node { width: px(440), flex_direction: FlexDirection::Column, row_gap: px(12), ..default() },
                ))
                .with_children(|menu| {
                    menu_button(menu, &fonts, &pal, "PLAY", "ENTER", UiAction::Play, 0);
                    menu_button(menu, &fonts, &pal, "SHOP", "B", UiAction::Shop, 1);
                    menu_button(menu, &fonts, &pal, "QUIT", "ESC", UiAction::Quit, 2);
                });
                col.spawn((
                    Intro { delay: 0.4 },
                    StockText,
                    label(&fonts, "", 15.0, pal.good),
                    Node { max_width: px(440), min_height: px(22), margin: UiRect::top(px(6)), ..default() },
                ));
            });

            // Bottom right: personal record tiles.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                right: px(48),
                bottom: px(44),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: px(10),
                ..default()
            })
            .with_children(|right| {
                right
                    .spawn((Intro { delay: 0.5 }, Node { column_gap: px(12), ..default() }))
                    .with_children(|row| {
                        for (stat, name, glyph, color) in [
                            (StatText::Bank, "BANK", icon::CASH, pal.good),
                            (StatText::BestTime, "LONGEST NIGHT", icon::CLOCK, pal.info),
                            (StatText::BestKills, "MOST KILLS", icon::SKULL, pal.bad),
                            (StatText::Runs, "RUNS", icon::TROPHY, pal.warn),
                        ] {
                            stat_tile(row, &fonts, &pal, stat, name, glyph, color);
                        }
                    });
                right.spawn((SaveWarning, label(&fonts, "", 14.0, pal.warn)));
            });

            // Bottom left: controls legend.
            root.spawn((
                Intro { delay: 0.6 },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(72),
                    bottom: px(40),
                    column_gap: px(18),
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    max_width: percent(40),
                    row_gap: px(10),
                    ..default()
                },
            ))
            .with_children(|row| {
                for (keys, what) in [
                    (&["W", "S"][..], "forward / back"),
                    (&["MOUSE"][..], "aim"),
                    (&["SPACE", "LMB"][..], "use weapon"),
                    (&["F"][..], "auto-target"),
                    (&["1", "2", "3", "Q"][..], "bat · revolver · shotgun"),
                    (&["ESC"][..], "pause"),
                ] {
                    row.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|c| {
                        for k in keys {
                            keycap(c, &fonts, &pal, k);
                        }
                        c.spawn(label(&fonts, what, 14.0, pal.muted));
                    });
                }
            });
        });
}

fn stat_tile(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    pal: &UiPalette,
    stat: StatText,
    name: &str,
    glyph: &str,
    color: Color,
) {
    parent
        .spawn((
            Node {
                min_width: px(150),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(18), px(14)),
                row_gap: px(4),
                border: UiRect::left(px(3)),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(pal.background.with_alpha(0.72)),
            BorderColor::all(color),
            BoxShadow::new(Color::BLACK.with_alpha(0.4), px(0), px(8), px(0), px(18)),
        ))
        .with_children(|t| {
            t.spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|h| {
                h.spawn(mono(fonts, glyph, 14.0, color));
                h.spawn(mono(fonts, name, 12.0, pal.muted));
            });
            t.spawn((stat, mono(fonts, "-", 26.0, pal.foreground)));
        });
}

fn despawn_title(mut commands: Commands, q: Query<Entity, With<TitleRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn title_shortcuts(keys: Res<ButtonInput<KeyCode>>, mut out: MessageWriter<UiActivate>) {
    if keys.just_pressed(KeyCode::KeyP) {
        out.write(UiActivate(UiAction::Play));
    }
    if keys.just_pressed(KeyCode::KeyB) {
        out.write(UiAction::Shop.into());
    }
    if keys.just_pressed(KeyCode::Escape) {
        out.write(UiActivate(UiAction::Quit));
    }
}

impl From<UiAction> for UiActivate {
    fn from(a: UiAction) -> Self {
        UiActivate(a)
    }
}

fn title_actions(
    mut actions: MessageReader<UiActivate>,
    mut run: MessageWriter<RunCommand>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    for UiActivate(action) in actions.read() {
        match action {
            UiAction::Play => {
                run.write(RunCommand::Start);
            }
            UiAction::Shop => next.set(AppState::Shop),
            UiAction::Quit => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

fn refresh_title(
    save: Res<Save>,
    mut stats: Query<(&StatText, &mut Text), (Without<StockText>, Without<SaveWarning>)>,
    mut stock: Query<&mut Text, (With<StockText>, Without<SaveWarning>, Without<StatText>)>,
    mut warning: Query<&mut Text, (With<SaveWarning>, Without<StockText>, Without<StatText>)>,
) {
    let d = &save.0.data;
    for (stat, mut t) in &mut stats {
        let s = match stat {
            StatText::Bank => grouped(d.cash),
            StatText::BestTime => {
                if d.runs == 0 {
                    "--:--".into()
                } else {
                    clock(d.best_time_secs)
                }
            }
            StatText::BestKills => d.best_kills.to_string(),
            StatText::Runs => d.runs.to_string(),
        };
        set_text(&mut t, &s);
    }
    if let Ok(mut t) = stock.single_mut() {
        let parts: Vec<String> = [
            (Gun::Revolver, "revolver rounds"),
            (Gun::Shotgun, "shotgun shells"),
        ]
        .into_iter()
        .filter_map(|(gun, what)| {
            let n = d.stocked_rounds(gun);
            (n > 0).then(|| format!("+{n} {what}"))
        })
        .collect();
        let s = if parts.is_empty() {
            String::new()
        } else {
            format!("Stocked for your next run: {}", parts.join(" · "))
        };
        set_text(&mut t, &s);
    }
    if let Ok(mut t) = warning.single_mut() {
        let s = match save.0.status {
            SaveStatus::Ready => "",
            SaveStatus::Temporary => {
                "Saving unavailable. New earnings are temporary and lost when the game closes."
            }
            SaveStatus::Preserved => {
                "Existing save could not be read and was left unchanged. New earnings are temporary."
            }
        };
        set_text(&mut t, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bevy only rejects conflicting queries at runtime, so build every system the title runs.
    #[test]
    fn title_systems_have_disjoint_queries() {
        let mut world = World::new();
        let mut init = |mut s: Box<dyn System<In = (), Out = ()>>| {
            s.initialize(&mut world);
        };
        init(Box::new(IntoSystem::into_system(refresh_title)));
        init(Box::new(IntoSystem::into_system(title_actions)));
        init(Box::new(IntoSystem::into_system(spawn_title)));
    }
}
