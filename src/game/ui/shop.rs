//! Shop over the live night-time town: spend banked cash on Cardio and ammo for the next run.

use bevy::prelude::*;

use super::{
    ButtonHint, ButtonLabel, Intro, MenuButton, MenuFocus, UiAction, UiActivate, grouped, icon,
    label, menu_button, mono, set_text,
};
use crate::game::AppState;
use crate::game::assets::Fonts;
use crate::game::run::Save;
use crate::palette::UiPalette;
use crate::sim::data::{CARDIO_SPEED_MULTIPLIER, ShopItem, Tuning};
use crate::sim::save::{PurchaseResult, SaveData};

/// Menu position of the title's SHOP entry, focused again when the shop closes.
const TITLE_SHOP_ENTRY: usize = 1;

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShopMessage>()
            .add_systems(OnEnter(AppState::Shop), spawn_shop)
            .add_systems(OnExit(AppState::Shop), despawn_shop)
            .add_systems(
                Update,
                (shop_shortcuts, shop_actions, refresh_shop)
                    .chain()
                    .run_if(in_state(AppState::Shop)),
            );
    }
}

#[derive(Component)]
struct ShopRoot;

/// Shop texts refreshed every frame.
#[derive(Component, Clone, Copy)]
enum ShopText {
    Bank,
    /// Describes the focused entry.
    Detail,
    /// The latest purchase result.
    Message,
}

/// The latest purchase result and whether it went through.
#[derive(Resource, Default)]
struct ShopMessage(Option<(String, bool)>);

fn spawn_shop(
    mut commands: Commands,
    time: Res<Time>,
    fonts: Res<Fonts>,
    pal: Res<UiPalette>,
    mut focus: ResMut<MenuFocus>,
    mut message: ResMut<ShopMessage>,
) {
    focus.reset(time.elapsed_secs(), 0.15);
    message.0 = None;
    commands
        .spawn((
            ShopRoot,
            Node {
                width: percent(100),
                height: percent(100),
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundGradient::from(LinearGradient::to_right(vec![
                ColorStop::new(pal.background.with_alpha(0.95), percent(0)),
                ColorStop::new(pal.background.with_alpha(0.82), percent(36)),
                ColorStop::new(pal.background.with_alpha(0.0), percent(66)),
            ])),
        ))
        .with_children(|root| {
            root.spawn(Node {
                width: px(640),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(px(72), px(56)),
                row_gap: px(10),
                ..default()
            })
            .with_children(|col| {
                col.spawn((
                    Intro { delay: 0.0 },
                    mono(
                        &fonts,
                        "MAPLEWOOD PANTRY  ·  SPEND YOUR BANKED CASH",
                        14.0,
                        pal.accent,
                    ),
                ));
                col.spawn((
                    Intro { delay: 0.08 },
                    label(&fonts, "SHOP", 96.0, pal.foreground),
                    TextShadow {
                        offset: Vec2::new(0.0, 6.0),
                        color: Color::BLACK.with_alpha(0.6),
                    },
                ));
                col.spawn((
                    Intro { delay: 0.16 },
                    Node {
                        column_gap: px(10),
                        align_items: AlignItems::Center,
                        margin: UiRect::vertical(px(10)),
                        ..default()
                    },
                ))
                .with_children(|row| {
                    row.spawn(mono(&fonts, icon::CASH, 22.0, pal.good));
                    row.spawn(mono(&fonts, "BANK", 14.0, pal.muted));
                    row.spawn((ShopText::Bank, mono(&fonts, "-", 26.0, pal.foreground)));
                });
                col.spawn((
                    Intro { delay: 0.24 },
                    Node {
                        width: px(480),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(12),
                        ..default()
                    },
                ))
                .with_children(|menu| {
                    for (order, item) in ShopItem::ALL.into_iter().enumerate() {
                        menu_button(
                            menu,
                            &fonts,
                            &pal,
                            item.label(),
                            "",
                            UiAction::Buy(item),
                            order,
                        );
                    }
                    menu_button(
                        menu,
                        &fonts,
                        &pal,
                        "BACK",
                        "ESC",
                        UiAction::Back,
                        ShopItem::ALL.len(),
                    );
                });
                col.spawn((
                    Intro { delay: 0.32 },
                    ShopText::Detail,
                    label(&fonts, "", 16.0, pal.muted),
                    Node {
                        max_width: px(480),
                        min_height: px(44),
                        margin: UiRect::top(px(8)),
                        ..default()
                    },
                ));
                col.spawn((
                    ShopText::Message,
                    label(&fonts, "", 15.0, pal.good),
                    Node {
                        max_width: px(480),
                        min_height: px(22),
                        ..default()
                    },
                ));
            });
        });
}

fn despawn_shop(mut commands: Commands, q: Query<Entity, With<ShopRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn shop_shortcuts(keys: Res<ButtonInput<KeyCode>>, mut out: MessageWriter<UiActivate>) {
    if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyB]) {
        out.write(UiActivate(UiAction::Back));
    }
}

fn shop_actions(
    mut actions: MessageReader<UiActivate>,
    mut save: ResMut<Save>,
    mut message: ResMut<ShopMessage>,
    mut focus: ResMut<MenuFocus>,
    mut next: ResMut<NextState<AppState>>,
) {
    for UiActivate(action) in actions.read() {
        match *action {
            UiAction::Buy(item) => {
                let result = save.0.purchase(item);
                message.0 = Some(purchase_message(item, result, &save.0.data));
            }
            UiAction::Back => {
                focus.next = Some(TITLE_SHOP_ENTRY);
                next.set(AppState::Title);
            }
            _ => {}
        }
    }
}

fn purchase_message(item: ShopItem, result: PurchaseResult, d: &SaveData) -> (String, bool) {
    match (result, item.ammo()) {
        (PurchaseResult::Purchased, None) => (
            "Cardio purchased and saved. Your next run starts 10% faster.".into(),
            true,
        ),
        (PurchaseResult::Purchased, Some((_, per))) => (
            format!("{} stocked: +{per} rounds for your next run.", item.label()),
            true,
        ),
        (PurchaseResult::Maxed, None) => ("Cardio is already yours.".into(), true),
        (PurchaseResult::Maxed, Some((gun, _))) => (
            format!("{} is fully stocked for your next run.", gun.label()),
            true,
        ),
        (PurchaseResult::NotEnoughCash, _) => (
            format!(
                "Not enough banked cash: {} more needed.",
                item.cost().saturating_sub(d.cash)
            ),
            false,
        ),
        (PurchaseResult::Unavailable, _) => (
            format!(
                "{} not purchased: saving is unavailable, so your cash is unchanged.",
                item.label()
            ),
            false,
        ),
    }
}

/// The label and price hint of a shop entry.
fn entry_text(item: ShopItem, d: &SaveData) -> (String, String) {
    let owned = d.owned(item);
    let max = item.max_stock();
    let title = match item.ammo() {
        None => format!("{} · +10% speed", item.label()),
        Some(_) => format!("{} · {owned}/{max}", item.label()),
    };
    let hint = match (owned >= max, item.ammo()) {
        (false, _) => format!("{} {}", icon::CASH, item.cost()),
        (true, None) => "OWNED".into(),
        (true, Some(_)) => "FULL".into(),
    };
    (title, hint)
}

fn detail_text(action: UiAction, d: &SaveData) -> String {
    let UiAction::Buy(item) = action else {
        return "Return to the title screen.".into();
    };
    match item.ammo() {
        None => {
            let base = Tuning::default().player_speed;
            let boosted = base * CARDIO_SPEED_MULTIPLIER;
            format!(
                "Permanently raises Granny's walk speed by 10% (from {base:.1} to {boosted:.1} m/s)."
            )
        }
        Some((gun, per)) => format!(
            "+{per} {} rounds at the start of your next run. Stocked: +{} of {} max. Spent when the run starts.",
            gun.label().to_lowercase(),
            d.stocked_rounds(gun),
            item.max_stock() * per,
        ),
    }
}

#[allow(clippy::type_complexity)]
fn refresh_shop(
    save: Res<Save>,
    pal: Res<UiPalette>,
    focus: Res<MenuFocus>,
    message: Res<ShopMessage>,
    mut buttons: Query<(&mut MenuButton, &Children)>,
    mut labels: Query<&mut Text, (With<ButtonLabel>, Without<ButtonHint>, Without<ShopText>)>,
    mut hints: Query<&mut Text, (With<ButtonHint>, Without<ButtonLabel>, Without<ShopText>)>,
    mut texts: Query<
        (&ShopText, &mut Text, &mut TextColor),
        (Without<ButtonLabel>, Without<ButtonHint>),
    >,
) {
    let d = &save.0.data;
    let mut focused = UiAction::Back;
    for (mut b, children) in &mut buttons {
        if b.order == focus.index {
            focused = b.action;
        }
        let UiAction::Buy(item) = b.action else {
            continue;
        };
        b.enabled = d.owned(item) < item.max_stock();
        let (title, hint) = entry_text(item, d);
        for c in children.iter() {
            if let Ok(mut t) = labels.get_mut(c) {
                set_text(&mut t, &title);
            }
            if let Ok(mut t) = hints.get_mut(c) {
                set_text(&mut t, &hint);
            }
        }
    }
    for (kind, mut t, mut color) in &mut texts {
        match kind {
            ShopText::Bank => set_text(&mut t, &grouped(d.cash)),
            ShopText::Detail => set_text(&mut t, &detail_text(focused, d)),
            ShopText::Message => {
                let (s, ok) = message.0.clone().unwrap_or_default();
                set_text(&mut t, &s);
                color.0 = if ok { pal.good } else { pal.bad };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bevy only rejects conflicting queries at runtime, so build every system the shop runs.
    #[test]
    fn shop_systems_have_disjoint_queries() {
        let mut world = World::new();
        let mut init = |mut s: Box<dyn System<In = (), Out = ()>>| {
            s.initialize(&mut world);
        };
        init(Box::new(IntoSystem::into_system(refresh_shop)));
        init(Box::new(IntoSystem::into_system(shop_actions)));
        init(Box::new(IntoSystem::into_system(spawn_shop)));
    }

    #[test]
    fn entries_show_price_until_owned_or_full() {
        let mut d = SaveData::default();
        assert_eq!(
            entry_text(ShopItem::Cardio, &d).1,
            format!("{} 50", icon::CASH)
        );
        d.cardio = true;
        assert_eq!(entry_text(ShopItem::Cardio, &d).1, "OWNED");
        d.shotgun_packs = ShopItem::ShotgunShells.max_stock();
        let (label, hint) = entry_text(ShopItem::ShotgunShells, &d);
        assert_eq!(hint, "FULL");
        assert!(label.ends_with("4/4"), "{label}");
    }

    #[test]
    fn short_cash_message_names_the_missing_amount() {
        let d = SaveData {
            cash: 5,
            ..default()
        };
        let (msg, ok) = purchase_message(ShopItem::RevolverAmmo, PurchaseResult::NotEnoughCash, &d);
        assert!(!ok);
        assert!(msg.contains("10 more"), "{msg}");
    }
}
