//! Balance data ported verbatim from the web game's `src/data/*.ts`.
//! Units: metres, metres per second, and 60 Hz simulation ticks.

use std::f32::consts::PI;

/// Simulation step: 60 Hz.
pub const STEP_S: f32 = 1.0 / 60.0;
const DEG: f32 = PI / 180.0;

/// Live feel tunables (design defaults from `tuning.ts`).
#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    pub player_speed: f32,
    pub zombie_speed: f32,
    pub hitstop_ticks: u32,
    pub knockback_impulse: f32,
    pub knockback_damping: f32,
    pub knockback_friction: f32,
    pub stun_ticks: u32,
    pub melee_range: f32,
    pub melee_half_angle_deg: f32,
    pub shake_amplitude: f32,
    pub shake_decay: f32,
    pub follow_rate: f32,
    pub look_ahead_dist: f32,
    pub separation_radius: f32,
    pub separation_weight: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            player_speed: 4.0,
            zombie_speed: 1.6,
            hitstop_ticks: 3,
            knockback_impulse: 12.0,
            knockback_damping: 0.85,
            knockback_friction: 0.1,
            stun_ticks: 18,
            melee_range: 2.2,
            melee_half_angle_deg: 70.0,
            shake_amplitude: 0.15,
            shake_decay: 8.0,
            follow_rate: 8.0,
            look_ahead_dist: 2.5,
            separation_radius: 1.2,
            separation_weight: 1.5,
        }
    }
}

/// One-time Cardio upgrade (`perks.ts`).
pub const CARDIO_COST: u64 = 50;
pub const CARDIO_SPEED_MULTIPLIER: f32 = 1.1;

/// Fresh design defaults with the Cardio perk applied once.
pub fn perk_tuning(cardio: bool) -> Tuning {
    let mut t = Tuning::default();
    if cardio {
        t.player_speed *= CARDIO_SPEED_MULTIPLIER;
    }
    t
}

/// Baseball bat melee timeline (the web game's rolling pin, `weapons.ts`).
pub struct MeleeWeapon {
    pub damage: i32,
    pub active_tick: u32,
    pub total_ticks: u32,
}

pub const BAT: MeleeWeapon = MeleeWeapon {
    damage: 25,
    active_tick: 4,
    total_ticks: 4 + 14,
};

/// Granny's three weapons, selected with 1, 2 and 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weapon {
    Bat,
    Revolver,
    Shotgun,
}

impl Weapon {
    pub const ALL: [Weapon; 3] = [Weapon::Bat, Weapon::Revolver, Weapon::Shotgun];

    pub fn gun(self) -> Option<Gun> {
        match self {
            Weapon::Bat => None,
            Weapon::Revolver => Some(Gun::Revolver),
            Weapon::Shotgun => Some(Gun::Shotgun),
        }
    }

    /// The next weapon for Q and the mouse wheel, wrapping around.
    pub fn next(self) -> Weapon {
        match self {
            Weapon::Bat => Weapon::Revolver,
            Weapon::Revolver => Weapon::Shotgun,
            Weapon::Shotgun => Weapon::Bat,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Weapon::Bat => "Baseball Bat",
            Weapon::Revolver => Gun::Revolver.label(),
            Weapon::Shotgun => Gun::Shotgun.label(),
        }
    }
}

impl From<Gun> for Weapon {
    fn from(gun: Gun) -> Self {
        match gun {
            Gun::Revolver => Weapon::Revolver,
            Gun::Shotgun => Weapon::Shotgun,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gun {
    Revolver,
    Shotgun,
}

impl Gun {
    pub const ALL: [Gun; 2] = [Gun::Revolver, Gun::Shotgun];

    pub fn spec(self) -> &'static RangedWeapon {
        match self {
            Gun::Revolver => &REVOLVER,
            Gun::Shotgun => &SHOTGUN,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Gun::Revolver => "Revolver",
            Gun::Shotgun => "Cane Shotgun",
        }
    }

    pub fn other(self) -> Gun {
        match self {
            Gun::Revolver => Gun::Shotgun,
            Gun::Shotgun => Gun::Revolver,
        }
    }
}

pub struct RangedWeapon {
    pub damage: i32,
    pub range: f32,
    pub cooldown_ticks: u32,
    pub initial_ammo: u32,
    pub ammo_cap: u32,
    pub knockback: f32,
    pub stun_ticks: u32,
    pub pellet_angles: &'static [f32],
}

pub const REVOLVER: RangedWeapon = RangedWeapon {
    damage: 25,
    range: 12.0,
    cooldown_ticks: 18,
    initial_ammo: 24,
    ammo_cap: 72,
    knockback: 2.0,
    stun_ticks: 8,
    pellet_angles: &[0.0],
};

pub const SHOTGUN: RangedWeapon = RangedWeapon {
    damage: 12,
    range: 7.0,
    cooldown_ticks: 54,
    initial_ammo: 8,
    ammo_cap: 24,
    knockback: 5.0,
    stun_ticks: 12,
    pellet_angles: &[
        -15.0 * DEG,
        -9.0 * DEG,
        -3.0 * DEG,
        3.0 * DEG,
        9.0 * DEG,
        15.0 * DEG,
    ],
};

/// Ammo regen and drops.
pub const AMMO_RESERVE_INTERVAL_TICKS: u32 = 180;
pub const AMMO_DROP_EVERY_KILLS: u32 = 3;
pub const REVOLVER_DROP: u32 = 6;
pub const SHOTGUN_DROP: u32 = 2;

/// Pickups (`pickups.ts`).
pub const PICKUP_AMMO_CAP: usize = 64;
pub const PICKUP_HEALTH_CAP: usize = 16;
pub const PICKUP_CASH_CAP: usize = 64;
pub const HEALTH_EVERY_KILLS: u32 = 10;
pub const HEALTH_AMOUNT: u32 = 20;
pub const CASH_EVERY_KILLS: u32 = 1;
pub const CASH_AMOUNT: u32 = 5;
pub const PICKUP_TTL_TICKS: u32 = 1800;
pub const MAGNET_RADIUS: f32 = 3.0;
pub const COLLECT_RADIUS: f32 = 0.6;
pub const MAGNET_SPEED: f32 = 6.0;

/// Shambler archetype (`enemies.ts`).
pub struct EnemyArchetype {
    pub hp: i32,
    pub damage: i32,
    pub swipe_active_tick: u32,
    pub radius: f32,
    pub reach: f32,
    pub swipe_cooldown_ticks: u32,
    pub swipe_ticks: u32,
}

pub const SHAMBLER: EnemyArchetype = EnemyArchetype {
    hp: 50,
    damage: 10,
    swipe_active_tick: 5,
    radius: 0.4,
    reach: 0.9,
    swipe_cooldown_ticks: 60,
    swipe_ticks: 18,
};

pub const ZOMBIE_POOL_CAPACITY: usize = 40;
pub const ACTIVE_HORDE_TARGET: u32 = 20;
pub const SPAWN_INTERVAL_TICKS: u64 = 30;
pub const SPAWN_MIN_DISTANCE_FROM_PLAYER: f32 = 18.0;
pub const MAX_ZOMBIE_TO_PLAYER_SPEED_RATIO: f32 = 0.9;
pub const AVOID_PROBE_SECONDS: f32 = 0.5;

/// Player body (`entities/Player.ts`).
pub const PLAYER_HP: i32 = 100;
pub const PLAYER_RADIUS: f32 = 0.45;
pub const PLAYER_GRACE_TICKS: u64 = 30;
pub const PLAYER_MASS: f32 = 2.0;
pub const ZOMBIE_MASS: f32 = 1.0;
/// Exponential turn rate toward the move direction, per second.
pub const TURN_RATE: f32 = 24.0;
/// Walk cycle: one full stride (2π) every this many metres.
pub const STRIDE_M: f32 = 1.4;

/// Difficulty ramp (`difficulty.ts`).
pub const DIFFICULTY_TICKS_PER_BUDGET: u64 = 360;
pub const OVERFLOW_HP_PER_BUDGET: f64 = 2.5;
pub const OVERFLOW_DAMAGE_PER_BUDGET: f64 = 0.5;

pub fn difficulty_target(gameplay_ticks: u64) -> usize {
    let budget = ACTIVE_HORDE_TARGET as u64 + gameplay_ticks / DIFFICULTY_TICKS_PER_BUDGET;
    (budget as usize).min(ZOMBIE_POOL_CAPACITY)
}

fn overflow_ticks(gameplay_ticks: u64) -> u64 {
    let start =
        (ZOMBIE_POOL_CAPACITY as u64 - ACTIVE_HORDE_TARGET as u64) * DIFFICULTY_TICKS_PER_BUDGET;
    gameplay_ticks.saturating_sub(start)
}

pub fn difficulty_spawn_hp(gameplay_ticks: u64) -> i32 {
    let extra = (OVERFLOW_HP_PER_BUDGET * overflow_ticks(gameplay_ticks) as f64
        / DIFFICULTY_TICKS_PER_BUDGET as f64)
        .floor();
    SHAMBLER.hp + extra as i32
}

pub fn difficulty_spawn_damage(gameplay_ticks: u64) -> i32 {
    let extra = (OVERFLOW_DAMAGE_PER_BUDGET * overflow_ticks(gameplay_ticks) as f64
        / DIFFICULTY_TICKS_PER_BUDGET as f64)
        .floor();
    SHAMBLER.damage + extra as i32
}

/// Fixed oblique camera (`camera.ts`).
pub struct FixedCamera {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub fov_deg: f32,
    pub distance: f32,
}

pub const FIXED_CAMERA: FixedCamera = FixedCamera {
    yaw_deg: -40.0,
    pitch_deg: 40.0,
    fov_deg: 25.0,
    distance: 28.0,
};

/// Camera position offset from its ground target, in render space (x, y, z).
pub fn camera_offset() -> (f32, f32, f32) {
    let pitch = FIXED_CAMERA.pitch_deg * DEG;
    let yaw = FIXED_CAMERA.yaw_deg * DEG;
    let rise = FIXED_CAMERA.distance * pitch.sin();
    let horizontal = FIXED_CAMERA.distance * pitch.cos();
    (horizontal * yaw.sin(), rise, horizontal * yaw.cos())
}

/// Heading (sim radians) that turns a character to face the fixed camera.
pub fn facing_camera() -> f32 {
    let (x, _, z) = camera_offset();
    z.atan2(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_camera_points_at_the_camera_on_the_ground_plane() {
        let (x, _, z) = camera_offset();
        let to_camera = bevy::math::Vec2::new(x, z).normalize();
        let facing = bevy::math::Vec2::from_angle(facing_camera());
        assert!(facing.dot(to_camera) > 0.9999, "{facing} vs {to_camera}");
    }

    #[test]
    fn horde_grows_ten_per_minute_until_cap() {
        assert_eq!(difficulty_target(0), 20);
        assert_eq!(difficulty_target(60 * 60), 30);
        assert_eq!(difficulty_target(120 * 60), 40);
        assert_eq!(difficulty_target(600 * 60), 40);
    }

    #[test]
    fn overflow_strength_only_after_cap() {
        let cap_tick = 20 * 360;
        assert_eq!(difficulty_spawn_hp(cap_tick), 50);
        assert_eq!(difficulty_spawn_damage(cap_tick), 10);
        assert_eq!(difficulty_spawn_hp(cap_tick + 360), 52);
        assert_eq!(difficulty_spawn_hp(cap_tick + 720), 55);
        assert_eq!(difficulty_spawn_damage(cap_tick + 720), 11);
    }

    #[test]
    fn cardio_applies_ten_percent_once() {
        assert!((perk_tuning(true).player_speed - 4.4).abs() < 1e-5);
        assert_eq!(perk_tuning(false).player_speed, 4.0);
    }
}
