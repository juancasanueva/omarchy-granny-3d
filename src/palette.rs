//! UI palette, read from the active Omarchy theme when available.
//!
//! Omarchy keeps the current theme at `$XDG_STATE_HOME/omarchy/current/theme/colors.toml`
//! (older installs: `~/.config/omarchy/current/theme/colors.toml`). The 3D world keeps
//! its own night palette; menus and HUD follow the desktop theme.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct UiPalette {
    pub background: Color,
    pub panel: Color,
    pub panel_hi: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub good: Color,
    pub bad: Color,
    pub warn: Color,
    pub info: Color,
}

impl Default for UiPalette {
    fn default() -> Self {
        Self {
            background: hex("#0e1116"),
            panel: hex("#171b23"),
            panel_hi: hex("#252c3a"),
            foreground: hex("#f2efe6"),
            muted: hex("#8a91a0"),
            accent: hex("#9be564"),
            good: hex("#6fd08c"),
            bad: hex("#ef5d5d"),
            warn: hex("#ffd166"),
            info: hex("#6cb6ff"),
        }
    }
}

impl UiPalette {
    /// Translucent panel fill for glass-style cards over the 3D scene.
    pub fn glass(&self, alpha: f32) -> Color {
        self.background.with_alpha(alpha)
    }
}

pub struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        let palette = theme_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| palette_from_toml(&s))
            .unwrap_or_default();
        app.insert_resource(ClearColor(hex("#05070b")))
            .insert_resource(palette);
    }
}

fn theme_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"));
    [
        state.join("omarchy/current/theme/colors.toml"),
        home.join(".config/omarchy/current/theme/colors.toml"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Map Omarchy `colors.toml` keys onto the UI palette, keeping defaults for missing keys.
pub fn palette_from_toml(src: &str) -> UiPalette {
    let mut p = UiPalette::default();
    let Ok(map) = toml::from_str::<HashMap<String, toml::Value>>(src) else {
        return p;
    };
    let get = |k: &str| map.get(k).and_then(|v| v.as_str()).and_then(parse_hex);
    if let Some(c) = get("darker_background").or(get("background")) {
        p.background = c;
    }
    if let Some(c) = get("dark_background").or(get("background")) {
        p.panel = c;
    }
    if let Some(c) = get("selection").or(get("lighter_background")) {
        p.panel_hi = c;
    }
    if let Some(c) = get("foreground") {
        p.foreground = c;
    }
    if let Some(c) = get("dark_foreground").or(get("muted")) {
        p.muted = c;
    }
    if let Some(c) = get("accent") {
        p.accent = c;
    }
    if let Some(c) = get("green") {
        p.good = c;
    }
    if let Some(c) = get("red") {
        p.bad = c;
    }
    if let Some(c) = get("yellow") {
        p.warn = c;
    }
    if let Some(c) = get("blue") {
        p.info = c;
    }
    p
}

pub fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color::srgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Infallible hex for compile-time-known colors.
pub fn hex(s: &str) -> Color {
    parse_hex(s).unwrap_or(Color::srgb(1.0, 0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_omarchy_keys_and_keeps_defaults() {
        let p = palette_from_toml("accent = \"#ff0000\"\nforeground = \"#00ff00\"\n");
        assert_eq!(p.accent, Color::srgb_u8(255, 0, 0));
        assert_eq!(p.foreground, Color::srgb_u8(0, 255, 0));
        assert_eq!(p.bad, UiPalette::default().bad);
    }

    #[test]
    fn invalid_toml_falls_back() {
        let p = palette_from_toml("not = [valid");
        assert_eq!(p.accent, UiPalette::default().accent);
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(parse_hex("#12345").is_none());
        assert!(parse_hex("zzzzzz").is_none());
    }
}
