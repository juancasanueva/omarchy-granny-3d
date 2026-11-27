//! In-run HUD: health with damage trail, survival clock, horde size, kills and cash,
//! weapon cards with ammo and cooldown, hurt vignette, floating numbers, zombie
//! health bars and pickup toasts.

use bevy::prelude::*;

use super::{clock, grouped, icon, keycap, label, mono};
use crate::game::AppState;
use crate::game::assets::{Fonts, Ground};
use crate::game::fx::pickup_color;
use crate::game::run::{FrameEvents, Sim, SimAdvance, StepClock};
use crate::game::scene::GameCamera;
use crate::palette::UiPalette;
use crate::sim::data::{Gun, PLAYER_HP, ROLLING_PIN, ZOMBIE_POOL_CAPACITY, difficulty_target};
use crate::sim::world::{DamageSource, PickupKind, SimEvent};

const HP_BAR_W: f32 = 340.0;
const FLOATER_POOL: usize = 40;
const FLOATER_LIFE: f32 = 0.8;
const TOAST_LIFE: f32 = 1.8;
const HINT_SECS: f32 = 7.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(OnEnter(AppState::Title), despawn_hud)
            .add_systems(OnEnter(AppState::Summary), hide_hud)
            .add_systems(
                Update,
                (
                    reset_on_new_run,
                    update_stats,
                    update_weapons,
                    update_vignette,
                    spawn_floaters,
                    update_floaters,
                    update_zombie_bars,
                    update_toasts,
                )
                    .chain()
                    .after(SimAdvance)
                    .run_if(|s: Res<State<AppState>>| {
                        matches!(s.get(), AppState::Playing | AppState::Paused)
                    }),
            );
    }
}

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
enum HudText {
    Hp,
    Clock,
    Horde,
    Kills,
    Cash,
    Empty,
    Hint,
}

#[derive(Component)]
struct HpFill;

#[derive(Component)]
struct HpTrail;

#[derive(Component)]
struct WeaponCard(Option<Gun>);

#[derive(Component)]
struct AmmoText(Gun);

#[derive(Component)]
struct AmmoBar(Gun);

#[derive(Component)]
struct CooldownBar(Option<Gun>);

#[derive(Component)]
struct Vignette;

#[derive(Component)]
struct Floater {
    life: f32,
    world: Vec3,
    drift: f32,
}

#[derive(Component)]
struct ZombieBar(usize);

#[derive(Component)]
struct ZombieBarFill;

#[derive(Component)]
struct ToastColumn;

#[derive(Component)]
struct Toast {
    life: f32,
}

#[derive(Resource)]
struct HudState {
    run_id: u32,
    trail: f32,
    trail_hold: f32,
    hurt: f32,
    next_floater: usize,
    shown_secs: f32,
}

fn card(pal: &UiPalette) -> (Node, BackgroundColor, BorderColor, BoxShadow) {
    (
        Node {
            padding: UiRect::axes(px(18), px(12)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
        BackgroundColor(pal.background.with_alpha(0.72)),
        BorderColor::all(pal.panel_hi.with_alpha(0.8)),
        BoxShadow::new(Color::BLACK.with_alpha(0.45), px(0), px(8), px(0), px(20)),
    )
}

fn bar(width: Val, height: f32, color: Color) -> (Node, BackgroundColor) {
    (
        Node {
            width,
            height: px(height),
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            border_radius: BorderRadius::all(px(height * 0.5)),
            ..default()
        },
        BackgroundColor(color),
    )
}

fn spawn_hud(
    mut commands: Commands,
    fonts: Res<Fonts>,
    pal: Res<UiPalette>,
    existing: Query<Entity, With<HudRoot>>,
    mut roots: Query<&mut Visibility, With<HudRoot>>,
) {
    if !existing.is_empty() {
        for mut v in &mut roots {
            *v = Visibility::Inherited;
        }
        return;
    }
    commands.insert_resource(HudState {
        run_id: 0,
        trail: 1.0,
        trail_hold: 0.0,
        hurt: 0.0,
        next_floater: 0,
        shown_secs: 0.0,
    });
    commands
        .spawn((
            HudRoot,
            Pickable::IGNORE,
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
        ))
        .with_children(|root| {
            // Hurt vignette, under everything else.
            root.spawn((
                Vignette,
                Pickable::IGNORE,
                Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
                BackgroundGradient::default(),
            ));

            // Zombie health bars and floating numbers live in screen space.
            for i in 0..ZOMBIE_POOL_CAPACITY {
                root.spawn((
                    ZombieBar(i),
                    Pickable::IGNORE,
                    Visibility::Hidden,
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(46),
                        height: px(6),
                        border_radius: BorderRadius::all(px(3)),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(Color::BLACK.with_alpha(0.6)),
                    BorderColor::all(Color::BLACK.with_alpha(0.8)),
                ))
                .with_children(|b| {
                    b.spawn((ZombieBarFill, Node { width: percent(100), height: percent(100), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(pal.bad)));
                });
            }
            for _ in 0..FLOATER_POOL {
                root.spawn((
                    Floater { life: 0.0, world: Vec3::ZERO, drift: 0.0 },
                    Pickable::IGNORE,
                    Visibility::Hidden,
                    Node { position_type: PositionType::Absolute, width: px(120), justify_content: JustifyContent::Center, ..default() },
                ))
                .with_children(|f| {
                    f.spawn((mono(&fonts, "", 22.0, pal.foreground), TextShadow { offset: Vec2::new(0.0, 2.0), color: Color::BLACK.with_alpha(0.85) }));
                });
            }

            // Top bar: three equal columns so the clock stays centred at any window width.
            let column = |justify| Node { flex_grow: 1.0, flex_basis: px(0), justify_content: justify, column_gap: px(12), ..default() };
            root.spawn((
                Pickable::IGNORE,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(22),
                    width: percent(100),
                    padding: UiRect::horizontal(px(28)),
                    column_gap: px(16),
                    align_items: AlignItems::FlexStart,
                    ..default()
                },
            ))
            .with_children(|top| {
            top.spawn(column(JustifyContent::FlexStart)).with_children(|tl| {
                tl.spawn(card(&pal)).with_children(|c| {
                    c.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, width: px(HP_BAR_W), ..default() })
                        .with_children(|r| {
                            r.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|l| {
                                l.spawn(mono(&fonts, icon::HEART, 16.0, pal.bad));
                                l.spawn(label(&fonts, "GRANNY", 15.0, pal.foreground));
                            });
                            r.spawn((HudText::Hp, mono(&fonts, "100 / 100", 15.0, pal.foreground)));
                        });
                    c.spawn((Node { width: px(HP_BAR_W), height: px(14), border_radius: BorderRadius::all(px(7)), ..default() }, BackgroundColor(pal.panel_hi)))
                        .with_children(|b| {
                            b.spawn((HpTrail, bar(percent(100), 14.0, pal.foreground.with_alpha(0.85))));
                            b.spawn((HpFill, bar(percent(100), 14.0, pal.good)));
                        });
                });
            });

            // Top-centre: survival clock and horde size.
            top.spawn(column(JustifyContent::Center)).with_children(|tc| {
                let (mut node, bg, border, shadow) = card(&pal);
                node.align_items = AlignItems::Center;
                node.padding = UiRect::new(px(26), px(26), px(6), px(10));
                node.row_gap = px(0);
                tc.spawn((node, bg, border, shadow)).with_children(|c| {
                    c.spawn((HudText::Clock, mono(&fonts, "00:00", 44.0, pal.foreground), TextShadow { offset: Vec2::new(0.0, 3.0), color: Color::BLACK.with_alpha(0.7) }));
                    c.spawn((HudText::Horde, mono(&fonts, "", 13.0, pal.muted), TextShadow { offset: Vec2::new(0.0, 2.0), color: Color::BLACK.with_alpha(0.7) }));
                });
            });

            // Top-right: kills and cash.
            top.spawn(column(JustifyContent::FlexEnd)).with_children(|tr| {
                for (text, glyph, name, color) in [(HudText::Kills, icon::SKULL, "KILLS", pal.bad), (HudText::Cash, icon::CASH, "CASH", pal.good)] {
                    tr.spawn(card(&pal)).with_children(|c| {
                        c.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|h| {
                            h.spawn(mono(&fonts, glyph, 14.0, color));
                            h.spawn(mono(&fonts, name, 12.0, pal.muted));
                        });
                        c.spawn((text, mono(&fonts, "0", 28.0, pal.foreground)));
                    });
                }
            });

            });

            // Right edge: pickup toasts.
            root.spawn((
                ToastColumn,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(28),
                    top: px(130),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    row_gap: px(8),
                    ..default()
                },
            ));

            // Bottom-centre: weapons.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: px(26),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(12),
                ..default()
            })
            .with_children(|bc| {
                bc.spawn((HudText::Hint, label(&fonts, "W / S forward / back  ·  MOUSE aim  ·  LMB fire  ·  F auto-target fire  ·  SPACE swing  ·  1 / 2 / Q / wheel switch  ·  ESC pause", 15.0, pal.foreground), TextShadow { offset: Vec2::new(0.0, 2.0), color: Color::BLACK.with_alpha(0.8) }));
                bc.spawn((HudText::Empty, mono(&fonts, "", 15.0, pal.warn), TextShadow { offset: Vec2::new(0.0, 2.0), color: Color::BLACK.with_alpha(0.8) }));
                bc.spawn(Node { column_gap: px(14), align_items: AlignItems::FlexEnd, ..default() }).with_children(|row| {
                    weapon_card(row, &fonts, &pal, None);
                    weapon_card(row, &fonts, &pal, Some(Gun::Revolver));
                    weapon_card(row, &fonts, &pal, Some(Gun::Shotgun));
                });
            });
        });
}

fn weapon_card(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    pal: &UiPalette,
    gun: Option<Gun>,
) {
    let (key, name) = match gun {
        None => ("SPACE", "ROLLING PIN"),
        Some(Gun::Revolver) => ("1", "REVOLVER"),
        Some(Gun::Shotgun) => ("2", "CANE SHOTGUN"),
    };
    let (mut node, bg, border, shadow) = card(pal);
    node.width = px(200);
    node.border = UiRect::all(px(2));
    parent
        .spawn((WeaponCard(gun), node, bg, border, shadow))
        .with_children(|c| {
            c.spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|h| {
                keycap(h, fonts, pal, key);
                h.spawn(label(fonts, name, 14.0, pal.foreground));
            });
            match gun {
                None => {
                    c.spawn(Node {
                        column_gap: px(6),
                        align_items: AlignItems::Baseline,
                        ..default()
                    })
                    .with_children(|r| {
                        r.spawn(mono(fonts, "∞", 26.0, pal.foreground));
                        r.spawn(label(fonts, "melee · knockback", 12.0, pal.muted));
                    });
                }
                Some(g) => {
                    c.spawn((AmmoText(g), mono(fonts, "", 26.0, pal.foreground)));
                }
            }
            c.spawn((
                Node {
                    width: percent(100),
                    height: px(5),
                    border_radius: BorderRadius::all(px(3)),
                    ..default()
                },
                BackgroundColor(pal.panel_hi),
            ))
            .with_children(|b| {
                if let Some(g) = gun {
                    b.spawn((
                        AmmoBar(g),
                        bar(percent(100), 5.0, pickup_color(PickupKind::Ammo(g))),
                    ));
                }
                b.spawn((CooldownBar(gun), bar(percent(0), 5.0, pal.foreground)));
            });
        });
}

fn despawn_hud(mut commands: Commands, q: Query<Entity, With<HudRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn hide_hud(mut q: Query<&mut Visibility, With<HudRoot>>) {
    for mut v in &mut q {
        *v = Visibility::Hidden;
    }
}

fn reset_on_new_run(
    sim: Res<Sim>,
    state: Option<ResMut<HudState>>,
    mut floaters: Query<(&mut Floater, &mut Visibility)>,
    toasts: Query<Entity, With<Toast>>,
    mut commands: Commands,
) {
    let Some(mut st) = state else { return };
    if st.run_id == sim.run_id {
        return;
    }
    *st = HudState {
        run_id: sim.run_id,
        trail: 1.0,
        trail_hold: 0.0,
        hurt: 0.0,
        next_floater: 0,
        shown_secs: 0.0,
    };
    for (mut f, mut v) in &mut floaters {
        f.life = 0.0;
        *v = Visibility::Hidden;
    }
    for e in &toasts {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::type_complexity)]
fn update_stats(
    time: Res<Time>,
    state: Res<State<AppState>>,
    sim: Res<Sim>,
    pal: Res<UiPalette>,
    events: Res<FrameEvents>,
    hud: Option<ResMut<HudState>>,
    mut texts: Query<(&HudText, &mut Text, &mut TextColor)>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), (With<HpFill>, Without<HpTrail>)>,
    mut trail: Query<&mut Node, (With<HpTrail>, Without<HpFill>)>,
) {
    let Some(mut st) = hud else { return };
    let w = &sim.world;
    let dt = if *state.get() == AppState::Paused {
        0.0
    } else {
        time.delta_secs()
    };
    st.shown_secs += dt;
    let hp = w.player.hp.max(0) as f32 / PLAYER_HP as f32;
    if events
        .0
        .iter()
        .any(|e| matches!(e, SimEvent::PlayerHurt { .. }))
    {
        st.trail_hold = 0.45;
        st.hurt = 1.0;
    }
    st.trail_hold -= dt;
    if st.trail_hold <= 0.0 {
        st.trail += (hp - st.trail) * (1.0 - (-6.0 * dt).exp());
    }
    st.trail = st.trail.max(hp);
    let low = hp <= 0.3;
    let hp_color = if hp > 0.6 {
        pal.good
    } else if hp > 0.3 {
        pal.warn
    } else {
        pal.bad
    };
    if let Ok((mut node, mut bg)) = fill.single_mut() {
        node.width = percent(hp * 100.0);
        let beat = if low {
            0.75 + 0.25 * (time.elapsed_secs() * 7.0).sin().abs()
        } else {
            1.0
        };
        bg.0 = hp_color.with_alpha(beat);
    }
    if let Ok(mut node) = trail.single_mut() {
        node.width = percent(st.trail * 100.0);
    }
    let empty = w.player.ammo(w.player.selected) == 0;
    let hint_alpha = (1.0 - (st.shown_secs - HINT_SECS) / 1.0).clamp(0.0, 1.0);
    for (kind, mut text, mut color) in &mut texts {
        let s = match kind {
            HudText::Hp => format!("{} / {}", w.player.hp.max(0), PLAYER_HP),
            HudText::Clock => clock(w.elapsed_secs()),
            HudText::Horde => format!(
                "{}  HORDE {} / {}",
                icon::HORDE,
                w.active_zombies(),
                difficulty_target(w.gameplay_ticks)
            ),
            HudText::Kills => grouped(w.kills as u64),
            HudText::Cash => grouped(w.run_cash),
            HudText::Empty => {
                if empty {
                    format!("{} EMPTY  ·  FIRE SWINGS THE ROLLING PIN", icon::BOLT)
                } else {
                    String::new()
                }
            }
            HudText::Hint => {
                color.0 = pal.foreground.with_alpha(hint_alpha * 0.85);
                if hint_alpha <= 0.0 {
                    String::new()
                } else {
                    continue;
                }
            }
        };
        if let HudText::Hp = kind {
            color.0 = if low { pal.bad } else { pal.foreground };
        }
        if let HudText::Empty = kind {
            color.0 = pal
                .warn
                .with_alpha(0.65 + 0.35 * (time.elapsed_secs() * 6.0).sin().abs());
        }
        if text.0 != s {
            text.0 = s;
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_weapons(
    sim: Res<Sim>,
    pal: Res<UiPalette>,
    mut cards: Query<(
        &WeaponCard,
        &mut BorderColor,
        &mut UiTransform,
        &mut BackgroundColor,
    )>,
    mut ammo: Query<(&AmmoText, &mut Text, &mut TextColor)>,
    mut bars: Query<(&AmmoBar, &mut Node), Without<CooldownBar>>,
    mut cooldowns: Query<(&CooldownBar, &mut Node), Without<AmmoBar>>,
) {
    let p = &sim.world.player;
    for (card, mut border, mut tf, mut bg) in &mut cards {
        let active = match card.0 {
            None => p.attack_tick.is_some(),
            Some(g) => g == p.selected && p.attack_tick.is_none(),
        };
        let selected = card.0 == Some(p.selected);
        let edge = if active {
            card.0
                .map(|g| pickup_color(PickupKind::Ammo(g)))
                .unwrap_or(pal.foreground)
        } else if selected {
            pal.accent
        } else {
            pal.panel_hi.with_alpha(0.8)
        };
        *border = BorderColor::all(edge);
        tf.translation = Val2::px(0.0, if selected || active { -8.0 } else { 0.0 });
        bg.0 = pal
            .background
            .with_alpha(if selected { 0.86 } else { 0.62 });
    }
    for (a, mut text, mut color) in &mut ammo {
        let n = p.ammo(a.0);
        let s = format!("{n} / {}", a.0.spec().ammo_cap);
        if text.0 != s {
            text.0 = s;
        }
        color.0 = if n == 0 { pal.bad } else { pal.foreground };
    }
    for (b, mut node) in &mut bars {
        node.width = percent(p.ammo(b.0) as f32 / b.0.spec().ammo_cap as f32 * 100.0);
    }
    for (c, mut node) in &mut cooldowns {
        let frac = match c.0 {
            None => p
                .attack_tick
                .map(|t| 1.0 - t as f32 / ROLLING_PIN.total_ticks as f32)
                .unwrap_or(0.0),
            Some(g) if g == p.selected => p.gun_cooldown as f32 / g.spec().cooldown_ticks as f32,
            Some(_) => 0.0,
        };
        node.width = percent(frac * 100.0);
    }
}

fn update_vignette(
    time: Res<Time>,
    state: Res<State<AppState>>,
    sim: Res<Sim>,
    pal: Res<UiPalette>,
    hud: Option<ResMut<HudState>>,
    mut q: Query<&mut BackgroundGradient, With<Vignette>>,
    mut last: Local<f32>,
) {
    let Some(mut st) = hud else { return };
    let dt = if *state.get() == AppState::Paused {
        0.0
    } else {
        time.delta_secs()
    };
    st.hurt = (st.hurt - dt * 2.2).max(0.0);
    let hp = sim.world.player.hp.max(0) as f32 / PLAYER_HP as f32;
    let low = if hp <= 0.3 {
        0.35 + 0.2 * (time.elapsed_secs() * 3.5).sin().abs()
    } else {
        0.0
    };
    let alpha = (st.hurt * 0.7).max(low).min(0.85);
    if (alpha - *last).abs() < 0.004 {
        return;
    }
    *last = alpha;
    let Ok(mut g) = q.single_mut() else { return };
    *g = if alpha <= 0.0 {
        BackgroundGradient::default()
    } else {
        BackgroundGradient::from(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestCorner,
            vec![
                ColorStop::new(Color::NONE, percent(45)),
                ColorStop::new(pal.bad.with_alpha(alpha * 0.5), percent(75)),
                ColorStop::new(pal.bad.with_alpha(alpha), percent(100)),
            ],
        ))
    };
}

#[allow(clippy::too_many_arguments)]
fn spawn_floaters(
    events: Res<FrameEvents>,
    ground: Option<Res<Ground>>,
    pal: Res<UiPalette>,
    hud: Option<ResMut<HudState>>,
    mut floaters: Query<(&mut Floater, &Children)>,
    mut texts: Query<(&mut Text, &mut TextColor, &mut TextFont)>,
    mut toasts: Query<Entity, With<ToastColumn>>,
    fonts: Res<Fonts>,
    mut commands: Commands,
) {
    let (Some(ground), Some(mut st)) = (ground, hud) else {
        return;
    };
    let mut slots: Vec<_> = floaters.iter_mut().collect();
    if slots.is_empty() {
        return;
    }
    for e in &events.0 {
        let (at, text, color, size, lift) = match e {
            SimEvent::ZombieHit {
                at, amount, source, ..
            } => {
                let color = if *source == DamageSource::Melee {
                    pal.foreground
                } else {
                    Color::srgb_u8(0xff, 0xf1, 0xcd)
                };
                (
                    *at,
                    format!("-{amount}"),
                    color,
                    if *source == DamageSource::Melee {
                        26.0
                    } else {
                        20.0
                    },
                    1.9,
                )
            }
            SimEvent::PlayerHurt { at, amount } => (*at, format!("-{amount}"), pal.bad, 28.0, 2.3),
            SimEvent::PickupCollected { kind, amount, at } => {
                let text = match kind {
                    PickupKind::Cash => format!("+{amount} {}", icon::CASH),
                    PickupKind::Health => format!("+{amount} {}", icon::HEART),
                    PickupKind::Ammo(_) => format!("+{amount}"),
                };
                if !matches!(kind, PickupKind::Cash)
                    && let Ok(col) = toasts.single_mut()
                {
                    let what = match kind {
                        PickupKind::Ammo(Gun::Revolver) => format!("+{amount} revolver rounds"),
                        PickupKind::Ammo(Gun::Shotgun) => format!("+{amount} shotgun shells"),
                        _ => format!("+{amount} health"),
                    };
                    toast(&mut commands, col, &fonts, &pal, &what, pickup_color(*kind));
                }
                (*at, text, pickup_color(*kind), 20.0, 1.6)
            }
            _ => continue,
        };
        let idx = st.next_floater % slots.len();
        st.next_floater += 1;
        let (f, children) = &mut slots[idx];
        f.life = FLOATER_LIFE;
        f.world = Vec3::new(at.x, ground.height(at) + lift, at.y);
        f.drift = ((st.next_floater * 37) % 21) as f32 - 10.0;
        if let Some(c) = children.first()
            && let Ok((mut t, mut tc, mut font)) = texts.get_mut(*c)
        {
            t.0 = text;
            tc.0 = color;
            font.font_size = FontSize::Px(size);
        }
    }
}

fn toast(
    commands: &mut Commands,
    column: Entity,
    fonts: &Fonts,
    pal: &UiPalette,
    text: &str,
    color: Color,
) {
    commands.entity(column).with_children(|col| {
        col.spawn((
            Toast { life: TOAST_LIFE },
            Node {
                padding: UiRect::axes(px(14), px(8)),
                border: UiRect::left(px(3)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(pal.background.with_alpha(0.78)),
            BorderColor::all(color),
        ))
        .with_children(|t| {
            t.spawn(label(fonts, text, 15.0, pal.foreground));
        });
    });
}

#[allow(clippy::type_complexity)]
fn update_floaters(
    time: Res<Time>,
    state: Res<State<AppState>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    mut floaters: Query<(
        &mut Floater,
        &mut Node,
        &mut Visibility,
        &mut UiTransform,
        &Children,
    )>,
    mut colors: Query<&mut TextColor>,
) {
    let Ok((camera, cam_tf)) = cam.single() else {
        return;
    };
    let dt = if *state.get() == AppState::Paused {
        0.0
    } else {
        time.delta_secs()
    };
    for (mut f, mut node, mut vis, mut tf, children) in &mut floaters {
        if f.life <= 0.0 {
            *vis = Visibility::Hidden;
            continue;
        }
        f.life -= dt;
        let k = 1.0 - (f.life / FLOATER_LIFE).clamp(0.0, 1.0);
        let Ok(screen) = camera.world_to_viewport(cam_tf, f.world) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Inherited;
        node.left = px(screen.x - 60.0 + f.drift * k);
        node.top = px(screen.y - 40.0 * (1.0 - (1.0 - k).powi(2)));
        let pop = if k < 0.15 {
            1.0 + (0.15 - k) * 3.0
        } else {
            1.0
        };
        tf.scale = Vec2::splat(pop);
        let alpha = if k > 0.6 { 1.0 - (k - 0.6) / 0.4 } else { 1.0 };
        for c in children.iter() {
            if let Ok(mut color) = colors.get_mut(c) {
                color.0.set_alpha(alpha);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_zombie_bars(
    sim: Res<Sim>,
    clock: Res<StepClock>,
    pal: Res<UiPalette>,
    ground: Option<Res<Ground>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    mut bars: Query<(&ZombieBar, &mut Node, &mut Visibility, &Children), Without<ZombieBarFill>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<ZombieBarFill>>,
) {
    let (Some(ground), Ok((camera, cam_tf))) = (ground, cam.single()) else {
        return;
    };
    for (bar, mut node, mut vis, children) in &mut bars {
        let z = &sim.world.zombies[bar.0];
        if !z.active || z.hp >= z.max_hp || z.since_hit > 240 {
            *vis = Visibility::Hidden;
            continue;
        }
        let pos = z.prev_pos.lerp(z.pos, clock.alpha);
        let world = Vec3::new(pos.x, ground.height(pos) + 1.85, pos.y);
        let Ok(screen) = camera.world_to_viewport(cam_tf, world) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Inherited;
        node.left = px(screen.x - 23.0);
        node.top = px(screen.y);
        let frac = z.hp as f32 / z.max_hp as f32;
        if let Some(c) = children.first()
            && let Ok((mut n, mut bg)) = fills.get_mut(*c)
        {
            n.width = percent(frac * 100.0);
            bg.0 = if frac > 0.5 { pal.warn } else { pal.bad };
        }
    }
}

fn update_toasts(
    time: Res<Time>,
    state: Res<State<AppState>>,
    mut commands: Commands,
    mut toasts: Query<(Entity, &mut Toast, &mut UiTransform)>,
) {
    let dt = if *state.get() == AppState::Paused {
        0.0
    } else {
        time.delta_secs()
    };
    let count = toasts.iter().count();
    for (i, (e, mut t, mut tf)) in toasts.iter_mut().enumerate() {
        t.life -= dt;
        if t.life <= 0.0 || count - i > 4 {
            commands.entity(e).despawn();
            continue;
        }
        let age = TOAST_LIFE - t.life;
        let slide = (1.0 - (age / 0.2).min(1.0)).powi(2) * 40.0;
        let out = ((0.3 - t.life) / 0.3).clamp(0.0, 1.0) * 60.0;
        tf.translation = Val2::px(slide + out, 0.0);
    }
}
