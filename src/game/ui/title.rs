//! Title screen over the live night-time town: menu, Cardio upgrade, bank and personal bests.

use bevy::prelude::*;

use super::{
    ButtonHint, ButtonLabel, MenuButton, MenuFocus, UiAction, UiActivate, clock, grouped, icon,
    keycap, label, menu_button, mono,
};
use crate::game::AppState;
use crate::game::assets::Fonts;
use crate::game::run::{RunCommand, Save};
use crate::palette::UiPalette;
use crate::sim::data::{CARDIO_COST, CARDIO_SPEED_MULTIPLIER, Tuning};
use crate::sim::save::{PurchaseResult, SaveStatus};

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TitleMessage>()
            .add_systems(OnEnter(AppState::Title), spawn_title)
            .add_systems(OnExit(AppState::Title), despawn_title)
            .add_systems(
                Update,
                (title_shortcuts, title_actions, refresh_title, animate_title)
                    .chain()
                    .run_if(in_state(AppState::Title)),
            );
    }
}

#[derive(Component)]
struct TitleRoot;

#[derive(Component)]
struct TitleIntro {
    delay: f32,
}

#[derive(Component)]
enum StatText {
    Bank,
    BestTime,
    BestKills,
    Runs,
}

#[derive(Component)]
struct MessageText;

#[derive(Component)]
struct SaveWarning;

#[derive(Resource, Default)]
struct TitleMessage(Option<(String, bool)>);

fn spawn_title(
    mut commands: Commands,
    time: Res<Time>,
    fonts: Res<Fonts>,
    pal: Res<UiPalette>,
    mut focus: ResMut<MenuFocus>,
    mut message: ResMut<TitleMessage>,
) {
    focus.reset(time.elapsed_secs(), 0.15);
    message.0 = None;
    let base = Tuning::default().player_speed;
    let boosted = base * CARDIO_SPEED_MULTIPLIER;
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
                col.spawn((TitleIntro { delay: 0.0 }, mono(&fonts, "A MAPLEWOOD NIGHT  ·  FOR OMARCHY", 14.0, pal.accent)));
                col.spawn((
                    TitleIntro { delay: 0.08 },
                    label(&fonts, "GRANNY", 112.0, pal.foreground),
                    TextShadow { offset: Vec2::new(0.0, 6.0), color: Color::BLACK.with_alpha(0.6) },
                ));
                col.spawn((
                    TitleIntro { delay: 0.16 },
                    label(&fonts, "vs. THE NEIGHBORHOOD", 36.0, pal.accent),
                    TextShadow { offset: Vec2::new(0.0, 3.0), color: Color::BLACK.with_alpha(0.6) },
                ));
                col.spawn((
                    TitleIntro { delay: 0.24 },
                    label(
                        &fonts,
                        "The dead are shambling down Maplewood Lane. Granny has a rolling pin, a revolver and a cane shotgun. Survive as long as you can.",
                        17.0,
                        pal.muted,
                    ),
                    Node { max_width: px(460), margin: UiRect::vertical(px(14)), ..default() },
                ));
                col.spawn((
                    TitleIntro { delay: 0.32 },
                    Node { width: px(440), flex_direction: FlexDirection::Column, row_gap: px(12), ..default() },
                ))
                .with_children(|menu| {
                    menu_button(menu, &fonts, &pal, "PLAY", "ENTER", UiAction::Play, 0);
                    menu_button(menu, &fonts, &pal, "Cardio upgrade", "", UiAction::BuyCardio, 1);
                    menu_button(menu, &fonts, &pal, "QUIT", "ESC", UiAction::Quit, 2);
                });
                col.spawn((
                    TitleIntro { delay: 0.4 },
                    label(
                        &fonts,
                        format!("Cardio permanently raises Granny's walk speed by 10% (from {base:.1} to {boosted:.1} m/s)."),
                        14.0,
                        pal.muted,
                    ),
                    Node { max_width: px(440), margin: UiRect::top(px(6)), ..default() },
                ));
                col.spawn((MessageText, label(&fonts, "", 15.0, pal.good), Node { min_height: px(22), ..default() }));
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
                    .spawn((TitleIntro { delay: 0.5 }, Node { column_gap: px(12), ..default() }))
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
                TitleIntro { delay: 0.6 },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(72),
                    bottom: px(40),
                    column_gap: px(18),
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    max_width: px(900),
                    ..default()
                },
            ))
            .with_children(|row| {
                for (keys, what) in [
                    (&["WASD"][..], "move"),
                    (&["SPACE"][..], "swing"),
                    (&["LMB", "F"][..], "fire"),
                    (&["1", "2", "Q"][..], "weapon"),
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
    if keys.just_pressed(KeyCode::KeyC) {
        out.write(UiAction::BuyCardio.into());
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
    mut save: ResMut<Save>,
    mut message: ResMut<TitleMessage>,
    mut run: MessageWriter<RunCommand>,
    mut exit: MessageWriter<AppExit>,
) {
    for UiActivate(action) in actions.read() {
        match action {
            UiAction::Play => {
                run.write(RunCommand::Start);
            }
            UiAction::BuyCardio => {
                let result = save.0.purchase_cardio();
                message.0 = Some(match result {
                    PurchaseResult::Purchased => ("Cardio purchased and saved. Your next run starts 10% faster.".into(), true),
                    PurchaseResult::AlreadyOwned => ("Cardio is already yours.".into(), true),
                    PurchaseResult::NotEnoughCash => (
                        format!("Not enough banked cash: {} more needed.", CARDIO_COST.saturating_sub(save.0.data.cash)),
                        false,
                    ),
                    PurchaseResult::Unavailable => {
                        ("Cardio not purchased: saving is unavailable, so cash and ownership are unchanged.".into(), false)
                    }
                });
            }
            UiAction::Quit => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn refresh_title(
    save: Res<Save>,
    pal: Res<UiPalette>,
    message: Res<TitleMessage>,
    mut buttons: Query<(&mut MenuButton, &Children)>,
    mut labels: Query<
        &mut Text,
        (
            With<ButtonLabel>,
            Without<ButtonHint>,
            Without<StatText>,
            Without<MessageText>,
            Without<SaveWarning>,
        ),
    >,
    mut hints: Query<
        &mut Text,
        (
            With<ButtonHint>,
            Without<ButtonLabel>,
            Without<StatText>,
            Without<MessageText>,
            Without<SaveWarning>,
        ),
    >,
    mut stats: Query<(&StatText, &mut Text), (Without<MessageText>, Without<SaveWarning>)>,
    mut msg: Query<
        (&mut Text, &mut TextColor),
        (With<MessageText>, Without<SaveWarning>, Without<StatText>),
    >,
    mut warning: Query<&mut Text, (With<SaveWarning>, Without<MessageText>, Without<StatText>)>,
) {
    let d = &save.0.data;
    for (mut b, children) in &mut buttons {
        if b.action != UiAction::BuyCardio {
            continue;
        }
        b.enabled = !d.cardio;
        let (title, hint) = if d.cardio {
            ("Cardio owned · +10% speed".to_string(), "OWNED".to_string())
        } else {
            (
                "Buy Cardio · +10% speed".to_string(),
                format!("{} {}   C", icon::CASH, CARDIO_COST),
            )
        };
        for c in children.iter() {
            if let Ok(mut t) = labels.get_mut(c) {
                set(&mut t, &title);
            }
            if let Ok(mut t) = hints.get_mut(c) {
                set(&mut t, &hint);
            }
        }
    }
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
        set(&mut t, &s);
    }
    if let Ok((mut t, mut color)) = msg.single_mut() {
        let (s, ok) = message.0.clone().unwrap_or_default();
        set(&mut t, &s);
        color.0 = if ok { pal.good } else { pal.bad };
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
        set(&mut t, s);
    }
}

fn set(t: &mut Text, s: &str) {
    if t.0 != s {
        t.0 = s.to_string();
    }
}

/// Staggered slide-and-fade entrance.
fn animate_title(
    time: Res<Time>,
    mut q: Query<(&TitleIntro, &mut UiTransform)>,
    mut start: Local<Option<f32>>,
    mut seen: Local<usize>,
) {
    let count = q.iter().count();
    if count != *seen {
        *seen = count;
        *start = Some(time.elapsed_secs());
    }
    let t0 = start.unwrap_or(0.0);
    for (intro, mut tf) in &mut q {
        let k = ((time.elapsed_secs() - t0 - intro.delay) / 0.45).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - k).powi(3);
        tf.translation = Val2::px(-36.0 * (1.0 - ease), 0.0);
        tf.scale = Vec2::splat(0.98 + 0.02 * ease);
    }
}
