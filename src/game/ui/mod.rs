//! Menus, HUD and overlays. Colors follow the active Omarchy theme (`UiPalette`).
//! Every menu works with both keyboard (arrows/WASD + Enter, plus shortcuts) and mouse.

mod hud;
mod overlays;
mod title;

use bevy::prelude::*;

use super::assets::Fonts;
use crate::palette::UiPalette;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuFocus>()
            .add_message::<UiActivate>()
            .add_systems(Update, (menu_keyboard, style_buttons).chain())
            .add_plugins((title::TitlePlugin, hud::HudPlugin, overlays::OverlaysPlugin));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    Play,
    BuyCardio,
    Quit,
    Resume,
    Restart,
    ToTitle,
}

/// A focusable menu entry; `order` is its position in keyboard navigation.
#[derive(Component)]
pub struct MenuButton {
    pub action: UiAction,
    pub order: usize,
    pub enabled: bool,
}

#[derive(Component)]
struct ButtonLabel;

#[derive(Component)]
struct ButtonHint;

/// Focused menu entry; `armed_at` blocks activation briefly after a screen opens.
#[derive(Resource, Default)]
pub struct MenuFocus {
    pub index: usize,
    pub armed_at: f32,
}

impl MenuFocus {
    pub fn reset(&mut self, now: f32, grace: f32) {
        self.index = 0;
        self.armed_at = now + grace;
    }
}

#[derive(Message, Debug, Clone, Copy)]
pub struct UiActivate(pub UiAction);

pub fn label(fonts: &Fonts, text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text), fonts.display(size), TextColor(color))
}

pub fn mono(fonts: &Fonts, text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text), fonts.mono(size), TextColor(color))
}

/// A keycap chip such as `ESC` or `SPACE`.
pub fn keycap(parent: &mut ChildSpawnerCommands, fonts: &Fonts, pal: &UiPalette, key: &str) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(8), px(3)),
                border: UiRect::all(px(1)).with_bottom(px(3)),
                border_radius: BorderRadius::all(px(6)),
                min_width: px(26),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(pal.panel_hi),
            BorderColor::all(pal.muted.with_alpha(0.5)),
        ))
        .with_children(|k| {
            k.spawn(mono(fonts, key, 13.0, pal.foreground));
        });
}

/// A full-width menu button with a label and a keycap hint on the right.
pub fn menu_button(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    pal: &UiPalette,
    text: &str,
    hint: &str,
    action: UiAction,
    order: usize,
) {
    parent
        .spawn((
            MenuButton {
                action,
                order,
                enabled: true,
            },
            Node {
                width: percent(100),
                padding: UiRect::axes(px(22), px(14)),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(12)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(pal.panel.with_alpha(0.85)),
            BorderColor::all(pal.panel_hi),
            BoxShadow::new(Color::BLACK.with_alpha(0.35), px(0), px(6), px(0), px(14)),
        ))
        .observe(hover_focus)
        .observe(click_activate)
        .with_children(|b| {
            b.spawn((ButtonLabel, label(fonts, text, 20.0, pal.foreground)));
            b.spawn((ButtonHint, mono(fonts, hint, 13.0, pal.muted)));
        });
}

fn hover_focus(over: On<PointerOver>, buttons: Query<&MenuButton>, mut focus: ResMut<MenuFocus>) {
    if let Ok(b) = buttons.get(over.event_target()) {
        focus.index = b.order;
    }
}

fn click_activate(
    click: On<PointerClick>,
    time: Res<Time>,
    buttons: Query<&MenuButton>,
    focus: Res<MenuFocus>,
    mut out: MessageWriter<UiActivate>,
) {
    if let Ok(b) = buttons.get(click.event_target())
        && b.enabled
        && time.elapsed_secs() >= focus.armed_at
    {
        out.write(UiActivate(b.action));
    }
}

fn menu_keyboard(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<&MenuButton>,
    mut focus: ResMut<MenuFocus>,
    mut out: MessageWriter<UiActivate>,
) {
    let count = buttons.iter().count();
    if count == 0 {
        return;
    }
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        focus.index = (focus.index + count - 1) % count;
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        focus.index = (focus.index + 1) % count;
    }
    focus.index = focus.index.min(count - 1);
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter])
        && time.elapsed_secs() >= focus.armed_at
        && let Some(b) = buttons.iter().find(|b| b.order == focus.index)
        && b.enabled
    {
        out.write(UiActivate(b.action));
    }
}

#[allow(clippy::type_complexity)]
fn style_buttons(
    time: Res<Time>,
    pal: Res<UiPalette>,
    focus: Res<MenuFocus>,
    mut buttons: Query<(
        &MenuButton,
        &Children,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut UiTransform,
    )>,
    mut labels: Query<&mut TextColor, (With<ButtonLabel>, Without<ButtonHint>)>,
    mut hints: Query<&mut TextColor, (With<ButtonHint>, Without<ButtonLabel>)>,
) {
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 4.0).sin();
    for (b, children, mut bg, mut border, mut ui_tf) in &mut buttons {
        let focused = b.order == focus.index;
        let (fill, edge, text, hint) = match (focused, b.enabled) {
            (true, true) => (
                pal.accent,
                pal.accent.lighter(0.08 * pulse),
                pal.background,
                pal.background.with_alpha(0.7),
            ),
            (true, false) => (pal.panel_hi, pal.muted, pal.muted, pal.muted),
            (false, true) => (
                pal.panel.with_alpha(0.85),
                pal.panel_hi,
                pal.foreground,
                pal.muted,
            ),
            (false, false) => (
                pal.panel.with_alpha(0.6),
                pal.panel_hi.with_alpha(0.5),
                pal.muted,
                pal.muted.with_alpha(0.6),
            ),
        };
        bg.0 = fill;
        *border = BorderColor::all(edge);
        ui_tf.translation = Val2::px(if focused { 6.0 } else { 0.0 }, 0.0);
        for c in children.iter() {
            if let Ok(mut t) = labels.get_mut(c) {
                t.0 = text;
            }
            if let Ok(mut t) = hints.get_mut(c) {
                t.0 = hint;
            }
        }
    }
}

/// `mm:ss` (or `h:mm:ss`) for survival times.
pub fn clock(secs: f32) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// Thousands separators for scores and cash.
pub fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Nerd Font glyphs available in the bundled JetBrains Mono.
pub mod icon {
    pub const HEART: &str = "\u{f004}";
    pub const SKULL: &str = "\u{f068c}";
    pub const CASH: &str = "\u{f0114}";
    pub const CLOCK: &str = "\u{f017}";
    pub const TROPHY: &str = "\u{f091}";
    pub const BOLT: &str = "\u{f0e7}";
    pub const HORDE: &str = "\u{f0c0}";
    pub const PLAY: &str = "\u{f04b}";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_clock_and_groups() {
        assert_eq!(clock(0.0), "00:00");
        assert_eq!(clock(125.9), "02:05");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(grouped(999), "999");
    }
}
