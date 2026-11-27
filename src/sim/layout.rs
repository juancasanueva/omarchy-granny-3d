//! Maplewood street block: bounds, AABB colliders, spawns and lamp placements,
//! parsed from `assets/layout.json` (exported from the town's Blender file).

use bevy::math::Vec2;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Aabb {
    #[serde(rename = "minX")]
    pub min_x: f32,
    #[serde(rename = "maxX")]
    pub max_x: f32,
    #[serde(rename = "minY")]
    pub min_y: f32,
    #[serde(rename = "maxY")]
    pub max_y: f32,
}

impl Aabb {
    pub fn center(&self) -> Vec2 {
        Vec2::new(
            (self.min_x + self.max_x) * 0.5,
            (self.min_y + self.max_y) * 0.5,
        )
    }

    pub fn around(c: Vec2, half: f32) -> Self {
        Self {
            min_x: c.x - half,
            max_x: c.x + half,
            min_y: c.y - half,
            max_y: c.y + half,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawCollider {
    id: String,
    #[serde(flatten)]
    aabb: Aabb,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct RawPoint {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawLayout {
    bounds: Aabb,
    #[serde(rename = "playerSpawn")]
    player_spawn: RawPoint,
    #[serde(rename = "spawnPoints")]
    spawn_points: Vec<RawPoint>,
    colliders: Vec<RawCollider>,
}

/// A street lamp's ground position and the height of its base.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LampPlacement {
    pub pos: Vec2,
    pub base_y: f32,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub bounds: Aabb,
    pub player_spawn: Vec2,
    pub spawn_points: Vec<Vec2>,
    pub colliders: Vec<Aabb>,
    pub lamps: Vec<LampPlacement>,
}

pub const LAMP_HALF_WIDTH: f32 = 0.27;
const MAX_SPAWN_POINTS: usize = 64;

pub const LAYOUT_JSON: &str = include_str!("../../assets/layout.json");

impl Layout {
    pub fn maplewood() -> Self {
        Self::parse(LAYOUT_JSON).expect("embedded layout.json is valid")
    }

    pub fn parse(src: &str) -> Result<Self, serde_json::Error> {
        let raw: RawLayout = serde_json::from_str(src)?;
        let native_ids = native_lamp_ids();
        let mut lamps = Vec::new();
        for (i, id) in native_ids.iter().enumerate() {
            if let Some(c) = raw.colliders.iter().find(|c| &c.id == id) {
                lamps.push(LampPlacement {
                    pos: c.aabb.center(),
                    base_y: if i < 12 { 0.34 } else { 0.37 },
                });
            }
        }
        // Native lamp footprints shrink to the lantern post; other colliders are kept verbatim.
        let mut colliders: Vec<Aabb> = raw
            .colliders
            .iter()
            .map(|c| {
                if native_ids.contains(&c.id) {
                    Aabb::around(c.aabb.center(), LAMP_HALF_WIDTH)
                } else {
                    c.aabb
                }
            })
            .collect();
        for lamp in supplemental_lamps() {
            colliders.push(Aabb::around(lamp.pos, LAMP_HALF_WIDTH));
            lamps.push(lamp);
        }
        Ok(Self {
            bounds: raw.bounds,
            player_spawn: Vec2::new(raw.player_spawn.x, raw.player_spawn.y),
            spawn_points: raw
                .spawn_points
                .iter()
                .take(MAX_SPAWN_POINTS)
                .map(|p| Vec2::new(p.x, p.y))
                .collect(),
            colliders,
            lamps,
        })
    }
}

fn native_lamp_ids() -> Vec<String> {
    let outer = (0..12).map(|i| {
        if i == 0 {
            "solid:Expansion Streetlight1A".to_string()
        } else {
            format!("solid:Expansion Streetlight1A.{i:03}")
        }
    });
    let inner = (0..8).map(|i| format!("solid:polySurface132.{:03}", i + 2));
    outer.chain(inner).collect()
}

/// Matching transverse road shoulders added by the web port.
fn supplemental_lamps() -> Vec<LampPlacement> {
    let rows = [(-64.65, 0.34), (-19.7, 0.37), (19.7, 0.37), (64.65, 0.34)];
    let xs = [-75.0, -51.0, -39.0, -15.0, 9.0, 33.0, 45.0, 69.0];
    rows.iter()
        .flat_map(|&(z, base_y)| {
            xs.iter().map(move |&x| LampPlacement {
                pos: Vec2::new(x, z),
                base_y,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maplewood_layout_loads_with_all_lamps() {
        let l = Layout::maplewood();
        assert_eq!(l.spawn_points.len(), 19);
        assert_eq!(l.lamps.len(), 20 + 32);
        assert_eq!(l.colliders.len(), 481 + 32);
        assert_eq!(l.player_spawn, Vec2::new(-24.0, 0.0));
        assert_eq!(l.bounds.max_x, 80.0);
    }
}
