//! The deterministic 60 Hz simulation: one `World`, stepped with an `InputSnapshot`.
//! Ported system-for-system from the web game's `src/systems/*`.
//! Coordinates are on the ground plane: sim (x, y) maps to render (x, 0, y).

use bevy::math::Vec2;

use super::data::*;
use super::layout::{Aabb, Layout};
use super::rng::Rng;

/// Per-tick player intent, already in world space.
#[derive(Debug, Clone, Default)]
pub struct InputSnapshot {
    /// Free movement in world space (scripted input); players use `throttle`.
    pub move_dir: Vec2,
    /// Forward (+1) toward where Granny faces, or back (-1) away from it, keeping her facing.
    pub throttle: f32,
    pub melee_pulse: bool,
    pub fire_held: bool,
    pub fire_pulse: bool,
    pub select: Option<Gun>,
    pub cycle: bool,
    /// Ground point under the mouse cursor; when present, Granny faces it.
    pub aim_point: Option<Vec2>,
    /// Firing turns Granny to the nearest visible zombie first (keyboard play).
    pub auto_target: bool,
}

/// Inside this distance the cursor no longer turns Granny, so she can stand on it calmly.
const AIM_DEAD_ZONE: f32 = 0.3;
/// Walking forward slows over this distance before the cursor and stops at the dead zone.
const ARRIVE_SLOW: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickupKind {
    Ammo(Gun),
    Health,
    Cash,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DamageSource {
    Melee,
    Gun(Gun),
}

/// Presentation events emitted during a step; drained by the renderer and UI.
#[derive(Debug, Clone, PartialEq)]
pub enum SimEvent {
    GunShot {
        gun: Gun,
        from: Vec2,
        ends: Vec<Vec2>,
    },
    MeleeHit {
        count: u32,
        at: Vec2,
        intensity: f32,
    },
    ZombieHit {
        slot: usize,
        at: Vec2,
        amount: i32,
        source: DamageSource,
    },
    ZombieDeath {
        slot: usize,
        at: Vec2,
        facing: f32,
    },
    PlayerHurt {
        at: Vec2,
        amount: i32,
    },
    PickupCollected {
        kind: PickupKind,
        amount: u32,
        at: Vec2,
    },
    EmptyClick,
    RunEnded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Ended,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub pos: Vec2,
    pub prev_pos: Vec2,
    pub vel: Vec2,
    pub facing: f32,
    pub hp: i32,
    pub grace_until: u64,
    /// Ticks into the current swing, `None` when idle.
    pub attack_tick: Option<u32>,
    pub selected: Gun,
    pub revolver_ammo: u32,
    pub shotgun_ammo: u32,
    pub gun_cooldown: u32,
    pub reserve_ticks: u32,
    pub walk_phase: f32,
    /// Ticks since the last shot, for muzzle/recoil presentation.
    pub since_shot: u32,
}

impl Player {
    fn new(pos: Vec2) -> Self {
        Self {
            pos,
            prev_pos: pos,
            vel: Vec2::ZERO,
            facing: 0.0,
            hp: PLAYER_HP,
            grace_until: 0,
            attack_tick: None,
            selected: Gun::Revolver,
            revolver_ammo: REVOLVER.initial_ammo,
            shotgun_ammo: SHOTGUN.initial_ammo,
            gun_cooldown: 0,
            reserve_ticks: 0,
            walk_phase: 0.0,
            since_shot: u32::MAX,
        }
    }

    pub fn ammo(&self, gun: Gun) -> u32 {
        match gun {
            Gun::Revolver => self.revolver_ammo,
            Gun::Shotgun => self.shotgun_ammo,
        }
    }

    fn set_ammo(&mut self, gun: Gun, value: u32) {
        let v = value.min(gun.spec().ammo_cap);
        match gun {
            Gun::Revolver => self.revolver_ammo = v,
            Gun::Shotgun => self.shotgun_ammo = v,
        }
    }

    /// Swing progress `0..1`, or `None` when idle.
    pub fn swing_t(&self) -> Option<f32> {
        self.attack_tick
            .map(|t| t as f32 / ROLLING_PIN.total_ticks as f32)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Zombie {
    pub active: bool,
    /// Bumped on every spawn so presentation can tell a recycled slot apart.
    pub generation: u32,
    pub pos: Vec2,
    pub prev_pos: Vec2,
    pub vel: Vec2,
    pub kb_vel: Vec2,
    pub facing: f32,
    pub hp: i32,
    pub max_hp: i32,
    pub damage: i32,
    pub radius: f32,
    pub stun_ticks: u32,
    pub attack_tick: Option<u32>,
    pub swipe_attempted: bool,
    pub swipe_cooldown: u32,
    pub hit_swing_id: u32,
    pub walk_phase: f32,
    /// Ticks since this zombie last took damage (hit flash).
    pub since_hit: u32,
}

impl Zombie {
    pub fn swipe_t(&self) -> Option<f32> {
        self.attack_tick
            .map(|t| t as f32 / SHAMBLER.swipe_ticks as f32)
    }
}

#[derive(Debug, Clone)]
pub struct Pickup {
    pub kind: PickupKind,
    pub amount: u32,
    pub pos: Vec2,
    pub prev_pos: Vec2,
    pub age_ticks: u32,
    /// Stable identity for presentation.
    pub id: u64,
}

/// Ground target of the follow camera; the pose is derived from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraTarget(pub Vec2);

pub struct World {
    pub tuning: Tuning,
    pub layout: Layout,
    pub phase: Phase,
    pub tick: u64,
    pub gameplay_ticks: u64,
    pub hitstop_ticks: u32,
    pub player: Player,
    pub zombies: Vec<Zombie>,
    pub pickups: Vec<Pickup>,
    next_pickup_id: u64,
    pub kills: u32,
    pub run_cash: u64,
    pub shots_fired: u32,
    pub damage_dealt: u64,
    ammo_drop_count: u32,
    swing_id: u32,
    pub camera: CameraTarget,
    pub prev_camera: CameraTarget,
    rng: Rng,
    pub events: Vec<SimEvent>,
}

impl World {
    pub fn new(layout: Layout, tuning: Tuning, seed: u32) -> Self {
        let spawn = layout.player_spawn;
        let mut world = Self {
            tuning,
            layout,
            phase: Phase::Running,
            tick: 0,
            gameplay_ticks: 0,
            hitstop_ticks: 0,
            player: Player::new(spawn),
            zombies: vec![Zombie::default(); ZOMBIE_POOL_CAPACITY],
            pickups: Vec::new(),
            next_pickup_id: 0,
            kills: 0,
            run_cash: 0,
            shots_fired: 0,
            damage_dealt: 0,
            ammo_drop_count: 0,
            swing_id: 0,
            camera: CameraTarget(spawn),
            prev_camera: CameraTarget(spawn),
            rng: Rng::new(seed),
            events: Vec::new(),
        };
        let look = world.look_ahead_target();
        world.camera = CameraTarget(look);
        world.prev_camera = world.camera;
        world.replenish_horde();
        world
    }

    pub fn active_zombies(&self) -> usize {
        self.zombies.iter().filter(|z| z.active).count()
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.gameplay_ticks as f32 * STEP_S
    }

    /// One fixed 60 Hz step.
    pub fn step(&mut self, input: &InputSnapshot) {
        if self.phase == Phase::Ended {
            return;
        }
        self.player.prev_pos = self.player.pos;
        for z in self.zombies.iter_mut().filter(|z| z.active) {
            z.prev_pos = z.pos;
        }
        for p in &mut self.pickups {
            p.prev_pos = p.pos;
        }
        self.prev_camera = self.camera;
        self.tick += 1;

        self.input_system(input);
        if self.hitstop_ticks > 0 {
            self.hitstop_ticks -= 1;
            return;
        }
        self.gameplay_ticks += 1;

        type System = fn(&mut World, &InputSnapshot);
        let systems: [System; 9] = [
            |w, i| w.ranged_system(i),
            |w, _| w.combat_system(),
            |w, _| w.ai_system(),
            |w, _| w.movement_system(),
            |w, _| w.collision_system(),
            |w, _| w.pickup_system(),
            |w, _| w.spawn_system(),
            |w, _| w.animation_system(),
            |w, _| w.camera_system(),
        ];
        for system in systems {
            system(self, input);
            if self.player.hp == 0 {
                return;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Systems
    // -----------------------------------------------------------------------

    fn input_system(&mut self, input: &InputSnapshot) {
        let tuning_speed = self.tuning.player_speed;
        let p = &mut self.player;
        if self.hitstop_ticks == 0 {
            if let Some(gun) = input.select {
                p.selected = gun;
            } else if input.cycle {
                p.selected = p.selected.other();
            }
        }
        let cursor = input.aim_point.map(|a| a - p.pos);
        let to_cursor = cursor.filter(|d| d.length() > AIM_DEAD_ZONE);
        if let Some(dir) = to_cursor {
            // Mouse aim: facing is independent of movement and turns instantly.
            p.facing = dir.to_angle();
        } else if input.move_dir.length_squared() > 0.0 {
            let target = input.move_dir.y.atan2(input.move_dir.x);
            let delta = target - p.facing;
            let shortest = delta.sin().atan2(delta.cos());
            p.facing += shortest * (1.0 - (-TURN_RATE * STEP_S).exp());
        }
        let mut throttle = input.throttle.clamp(-1.0, 1.0);
        if throttle > 0.0
            && let Some(d) = cursor
        {
            // Arrive at the cursor instead of overshooting it and spinning around.
            throttle *= ((d.length() - AIM_DEAD_ZONE) / ARRIVE_SLOW).clamp(0.0, 1.0);
        }
        let walk = input.move_dir + Vec2::from_angle(p.facing) * throttle;
        p.vel = walk.clamp_length_max(1.0) * tuning_speed;
        if input.melee_pulse && p.attack_tick.is_none() {
            p.attack_tick = Some(0);
            self.swing_id += 1;
        }
    }

    fn ranged_system(&mut self, input: &InputSnapshot) {
        let p = &mut self.player;
        p.since_shot = p.since_shot.saturating_add(1);
        if p.gun_cooldown > 0 {
            p.gun_cooldown -= 1;
        }
        p.reserve_ticks += 1;
        if p.reserve_ticks >= AMMO_RESERVE_INTERVAL_TICKS {
            p.reserve_ticks = 0;
            let ammo = p.revolver_ammo + 1;
            p.set_ammo(Gun::Revolver, ammo);
        }
        let fire = input.fire_pulse || input.fire_held;
        if !fire || p.attack_tick.is_some() {
            return;
        }
        let gun = p.selected;
        if input.auto_target {
            let range = if p.ammo(gun) == 0 {
                self.tuning.melee_range + SHAMBLER.radius
            } else {
                gun.spec().range
            };
            if let Some(facing) = self.nearest_visible_zombie(range) {
                self.player.facing = facing;
            }
        }
        let p = &mut self.player;
        if p.ammo(gun) == 0 {
            p.attack_tick = Some(0);
            self.swing_id += 1;
            self.events.push(SimEvent::EmptyClick);
            return;
        }
        if p.gun_cooldown > 0 {
            return;
        }
        let weapon = gun.spec();
        let facing = self.select_shot_facing(weapon.range);
        let p = &mut self.player;
        p.set_ammo(gun, p.ammo(gun) - 1);
        p.gun_cooldown = weapon.cooldown_ticks;
        p.since_shot = 0;
        self.shots_fired += 1;
        let origin = p.pos;
        let mut damage = vec![0i32; self.zombies.len()];
        let mut ends = Vec::with_capacity(weapon.pellet_angles.len());
        for angle in weapon.pellet_angles {
            let dir = Vec2::from_angle(facing + angle);
            let (hit, distance) = self.trace_shot(dir, weapon.range);
            if let Some(i) = hit {
                damage[i] += weapon.damage;
            }
            ends.push(origin + dir * distance);
        }
        let dir = Vec2::from_angle(facing);
        for (i, amount) in damage.into_iter().enumerate() {
            if amount > 0 {
                self.damage_zombie(
                    i,
                    amount,
                    origin,
                    dir,
                    weapon.knockback,
                    weapon.stun_ticks,
                    DamageSource::Gun(gun),
                );
            }
        }
        self.events.push(SimEvent::GunShot {
            gun,
            from: origin,
            ends,
        });
    }

    fn combat_system(&mut self) {
        let Some(t) = self.player.attack_tick else {
            return;
        };
        if t == ROLLING_PIN.active_tick {
            self.resolve_swing();
        }
        let next = t + 1;
        self.player.attack_tick = (next < ROLLING_PIN.total_ticks).then_some(next);
    }

    fn resolve_swing(&mut self) {
        let aim = Vec2::from_angle(self.player.facing);
        let cos_half = self.tuning.melee_half_angle_deg.to_radians().cos();
        let range = self.tuning.melee_range;
        let origin = self.player.pos;
        let mut count = 0u32;
        let mut sum = Vec2::ZERO;
        for i in 0..self.zombies.len() {
            let z = &self.zombies[i];
            if !z.active || z.hp <= 0 || z.hit_swing_id == self.swing_id {
                continue;
            }
            if !in_melee_sector(origin, aim, cos_half, range, z.pos, z.radius) {
                continue;
            }
            let at = z.pos;
            self.zombies[i].hit_swing_id = self.swing_id;
            sum += at;
            count += 1;
            let (kb, stun) = (self.tuning.knockback_impulse, self.tuning.stun_ticks);
            self.damage_zombie(
                i,
                ROLLING_PIN.damage,
                origin,
                aim,
                kb,
                stun,
                DamageSource::Melee,
            );
        }
        if count > 0 {
            self.hitstop_ticks = self.tuning.hitstop_ticks;
            let intensity = (0.5 + 0.25 * (count - 1) as f32).min(1.0);
            self.events.push(SimEvent::MeleeHit {
                count,
                at: sum / count as f32,
                intensity,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn damage_zombie(
        &mut self,
        slot: usize,
        amount: i32,
        origin: Vec2,
        fallback_dir: Vec2,
        knockback: f32,
        stun: u32,
        source: DamageSource,
    ) {
        let z = &mut self.zombies[slot];
        if !z.active || z.hp <= 0 {
            return;
        }
        let before = z.hp;
        z.hp = (z.hp - amount).max(0);
        let dealt = before - z.hp;
        if dealt <= 0 {
            return;
        }
        z.since_hit = 0;
        let at = z.pos;
        self.damage_dealt += dealt as u64;
        self.events.push(SimEvent::ZombieHit {
            slot,
            at,
            amount: dealt,
            source,
        });
        if z.hp == 0 {
            let facing = z.facing;
            z.active = false;
            self.kills += 1;
            self.spawn_kill_drops(at);
            self.events.push(SimEvent::ZombieDeath { slot, at, facing });
            return;
        }
        let away = (z.pos - origin).try_normalize().unwrap_or(fallback_dir);
        z.kb_vel = away * knockback;
        z.stun_ticks = stun;
        z.attack_tick = None;
    }

    fn ai_system(&mut self) {
        let speed = self
            .tuning
            .zombie_speed
            .min(self.tuning.player_speed * MAX_ZOMBIE_TO_PLAYER_SPEED_RATIO);
        let sep_radius = self.tuning.separation_radius;
        let sep_weight = self.tuning.separation_weight;
        for i in 0..self.zombies.len() {
            if !self.zombies[i].active || self.zombies[i].hp <= 0 {
                continue;
            }
            let to_player = self.player.pos - self.zombies[i].pos;
            let dist = to_player.length();
            {
                let z = &mut self.zombies[i];
                z.since_hit = z.since_hit.saturating_add(1);
                z.swipe_cooldown = z.swipe_cooldown.saturating_sub(1);
                if z.stun_ticks > 0 {
                    z.attack_tick = None;
                    z.stun_ticks -= 1;
                    z.vel = Vec2::ZERO;
                    continue;
                }
            }
            self.update_swipe(i, dist);
            if self.player.hp == 0 {
                return;
            }
            let chase = to_player.normalize_or_zero();
            let me = self.zombies[i].pos;
            let mut sep = Vec2::ZERO;
            for (j, other) in self.zombies.iter().enumerate() {
                if j == i || !other.active {
                    continue;
                }
                let d = me - other.pos;
                let len = d.length();
                if len == 0.0 || len >= sep_radius {
                    continue;
                }
                sep += d * ((1.0 - len / sep_radius) / len);
            }
            let mut steer = chase + sep * sep_weight;
            let radius = self.zombies[i].radius;
            let intent = steer.length();
            let reach = if intent == 0.0 {
                0.0
            } else {
                speed * AVOID_PROBE_SECONDS / intent
            };
            let probe = me + steer * reach;
            let perp = Vec2::new(-chase.y, chase.x);
            for b in &self.layout.colliders {
                if probe.x <= b.min_x - radius
                    || probe.x >= b.max_x + radius
                    || probe.y <= b.min_y - radius
                    || probe.y >= b.max_y + radius
                {
                    continue;
                }
                let away = me - b.center();
                let sign = if perp.dot(away) >= 0.0 { 1.0 } else { -1.0 };
                steer += perp * sign;
            }
            let z = &mut self.zombies[i];
            z.vel = steer.normalize_or_zero() * speed;
            if z.vel.length_squared() > 0.0 {
                z.facing = z.vel.y.atan2(z.vel.x);
            }
        }
    }

    fn update_swipe(&mut self, i: usize, dist: f32) {
        let z = &mut self.zombies[i];
        if let Some(t) = z.attack_tick {
            let next = t + 1;
            z.attack_tick = (next < SHAMBLER.swipe_ticks).then_some(next);
        }
        if z.attack_tick.is_none() && z.swipe_cooldown == 0 && dist <= SHAMBLER.reach {
            z.attack_tick = Some(0);
            z.swipe_attempted = false;
            z.swipe_cooldown = SHAMBLER.swipe_cooldown_ticks;
        }
        if z.attack_tick == Some(SHAMBLER.swipe_active_tick) && !z.swipe_attempted {
            z.swipe_attempted = true;
            if dist <= SHAMBLER.reach {
                let damage = z.damage;
                self.accept_player_damage(damage);
            }
        }
    }

    fn accept_player_damage(&mut self, damage: i32) -> bool {
        let p = &mut self.player;
        if self.phase == Phase::Ended || p.hp <= 0 || self.gameplay_ticks < p.grace_until {
            return false;
        }
        let before = p.hp;
        p.hp = (p.hp - damage).max(0);
        p.grace_until = self.gameplay_ticks + PLAYER_GRACE_TICKS;
        let at = p.pos;
        self.events.push(SimEvent::PlayerHurt {
            at,
            amount: before - p.hp,
        });
        if p.hp == 0 {
            self.end_run();
        }
        true
    }

    fn end_run(&mut self) {
        self.phase = Phase::Ended;
        self.player.attack_tick = None;
        self.player.vel = Vec2::ZERO;
        self.player.prev_pos = self.player.pos;
        for z in &mut self.zombies {
            z.vel = Vec2::ZERO;
            z.kb_vel = Vec2::ZERO;
            z.prev_pos = z.pos;
        }
        self.events.push(SimEvent::RunEnded);
    }

    fn movement_system(&mut self) {
        let (damping, friction) = (
            self.tuning.knockback_damping,
            self.tuning.knockback_friction,
        );
        self.player.pos += self.player.vel * STEP_S;
        for z in self.zombies.iter_mut().filter(|z| z.active) {
            z.pos += (z.vel + z.kb_vel) * STEP_S;
            z.kb_vel *= damping;
            let speed = z.kb_vel.length();
            let remaining = speed - friction;
            z.kb_vel = if remaining < 0.01 || speed == 0.0 {
                Vec2::ZERO
            } else {
                z.kb_vel * (remaining / speed)
            };
        }
    }

    fn collision_system(&mut self) {
        for _ in 0..2 {
            // Body pairs: the player (mass 2) against zombies (mass 1), then zombie pairs.
            for i in 0..self.zombies.len() {
                if !self.zombies[i].active {
                    continue;
                }
                let (pos, r) = (self.zombies[i].pos, self.zombies[i].radius);
                if let Some((push_a, push_b)) = separate(
                    self.player.pos,
                    PLAYER_RADIUS,
                    PLAYER_MASS,
                    pos,
                    r,
                    ZOMBIE_MASS,
                ) {
                    self.player.pos += push_a;
                    self.zombies[i].pos += push_b;
                }
                for j in (i + 1)..self.zombies.len() {
                    if !self.zombies[j].active {
                        continue;
                    }
                    let (a, b) = (self.zombies[i].pos, self.zombies[j].pos);
                    if let Some((pa, pb)) =
                        separate(a, r, ZOMBIE_MASS, b, self.zombies[j].radius, ZOMBIE_MASS)
                    {
                        self.zombies[i].pos += pa;
                        self.zombies[j].pos += pb;
                    }
                }
            }
            let bounds = self.layout.bounds;
            let colliders = &self.layout.colliders;
            resolve_static(&mut self.player.pos, PLAYER_RADIUS, colliders, bounds);
            for z in self.zombies.iter_mut().filter(|z| z.active) {
                resolve_static(&mut z.pos, z.radius, colliders, bounds);
            }
        }
    }

    fn pickup_system(&mut self) {
        let player_pos = self.player.pos;
        let mut i = 0;
        while i < self.pickups.len() {
            let pk = &mut self.pickups[i];
            pk.age_ticks += 1;
            if pk.age_ticks >= PICKUP_TTL_TICKS {
                self.pickups.swap_remove(i);
                continue;
            }
            let to = player_pos - pk.pos;
            let dist = to.length();
            if dist > MAGNET_RADIUS {
                i += 1;
                continue;
            }
            let step = dist.min(MAGNET_SPEED * STEP_S);
            if step == dist {
                pk.pos = player_pos;
            } else if dist > 0.0 {
                pk.pos += to * (step / dist);
            }
            if dist - step > COLLECT_RADIUS {
                i += 1;
                continue;
            }
            let (kind, offered, at) = (pk.kind, pk.amount, pk.pos);
            let p = &mut self.player;
            let amount = match kind {
                PickupKind::Health => {
                    if p.hp >= PLAYER_HP {
                        i += 1;
                        continue;
                    }
                    let a = (offered as i32).min(PLAYER_HP - p.hp);
                    p.hp += a;
                    a as u32
                }
                PickupKind::Cash => {
                    self.run_cash += offered as u64;
                    offered
                }
                PickupKind::Ammo(gun) => {
                    let before = p.ammo(gun);
                    if before >= gun.spec().ammo_cap {
                        i += 1;
                        continue;
                    }
                    p.set_ammo(gun, before + offered);
                    p.ammo(gun) - before
                }
            };
            self.pickups.swap_remove(i);
            self.events
                .push(SimEvent::PickupCollected { kind, amount, at });
        }
    }

    fn spawn_kill_drops(&mut self, at: Vec2) {
        if self.kills.is_multiple_of(AMMO_DROP_EVERY_KILLS) {
            let gun = if self.ammo_drop_count.is_multiple_of(2) {
                Gun::Revolver
            } else {
                Gun::Shotgun
            };
            self.ammo_drop_count += 1;
            let amount = if gun == Gun::Revolver {
                REVOLVER_DROP
            } else {
                SHOTGUN_DROP
            };
            self.spawn_pickup(PickupKind::Ammo(gun), amount, at);
        }
        if self.kills.is_multiple_of(HEALTH_EVERY_KILLS) {
            self.spawn_pickup(PickupKind::Health, HEALTH_AMOUNT, at);
        }
        if self.kills.is_multiple_of(CASH_EVERY_KILLS) {
            self.spawn_pickup(PickupKind::Cash, CASH_AMOUNT, at);
        }
    }

    fn spawn_pickup(&mut self, kind: PickupKind, amount: u32, at: Vec2) {
        let same = |k: PickupKind| match (k, kind) {
            (PickupKind::Ammo(_), PickupKind::Ammo(_)) => true,
            (a, b) => a == b,
        };
        let cap = match kind {
            PickupKind::Ammo(_) => PICKUP_AMMO_CAP,
            PickupKind::Health => PICKUP_HEALTH_CAP,
            PickupKind::Cash => PICKUP_CASH_CAP,
        };
        if self.pickups.iter().filter(|p| same(p.kind)).count() >= cap {
            return;
        }
        self.next_pickup_id += 1;
        self.pickups.push(Pickup {
            kind,
            amount,
            pos: at,
            prev_pos: at,
            age_ticks: 0,
            id: self.next_pickup_id,
        });
    }

    fn spawn_system(&mut self) {
        if self.gameplay_ticks % SPAWN_INTERVAL_TICKS != 1 {
            return;
        }
        self.replenish_horde();
    }

    fn replenish_horde(&mut self) {
        let target = difficulty_target(self.gameplay_ticks);
        let mut active = self.active_zombies();
        if active >= target {
            return;
        }
        let candidates: Vec<Vec2> = self
            .layout
            .spawn_points
            .iter()
            .copied()
            .filter(|&p| self.valid_spawn(p))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let hp = difficulty_spawn_hp(self.gameplay_ticks);
        let damage = difficulty_spawn_damage(self.gameplay_ticks);
        while active < target {
            let Some(slot) = self.zombies.iter().position(|z| !z.active) else {
                break;
            };
            let pos = candidates[self.rng.next_index(candidates.len())];
            let facing = (self.player.pos - pos).to_angle();
            let z = &mut self.zombies[slot];
            *z = Zombie {
                active: true,
                generation: z.generation.wrapping_add(1),
                pos,
                prev_pos: pos,
                facing,
                hp,
                max_hp: hp,
                damage,
                radius: SHAMBLER.radius,
                hit_swing_id: u32::MAX,
                since_hit: u32::MAX,
                walk_phase: self.rng.next_f32() * std::f32::consts::TAU,
                ..Default::default()
            };
            active += 1;
        }
    }

    fn valid_spawn(&self, p: Vec2) -> bool {
        if p.distance(self.player.pos) <= SPAWN_MIN_DISTANCE_FROM_PLAYER {
            return false;
        }
        let r = SHAMBLER.radius;
        let b = self.layout.bounds;
        if p.x - r < b.min_x || p.x + r > b.max_x || p.y - r < b.min_y || p.y + r > b.max_y {
            return false;
        }
        if self.layout.colliders.iter().any(|c| {
            p.x > c.min_x - r && p.x < c.max_x + r && p.y > c.min_y - r && p.y < c.max_y + r
        }) {
            return false;
        }
        self.spawn_hidden(p)
    }

    /// Off-screen proof against the fixed camera frustum's top and bottom planes, for the
    /// previous, current and desired camera targets (`world/spawnVisibility.ts`).
    fn spawn_hidden(&self, p: Vec2) -> bool {
        const ENVELOPE: f32 = 3.0;
        const MAX_SHAKE: f32 = 0.5;
        let pitch = FIXED_CAMERA.pitch_deg.to_radians();
        let half_fov = (FIXED_CAMERA.fov_deg * 0.5).to_radians();
        let (sin_yaw, cos_yaw) = FIXED_CAMERA.yaw_deg.to_radians().sin_cos();
        let plane_offset = FIXED_CAMERA.distance * half_fov.sin();
        let top = -(pitch - half_fov).sin();
        let bottom = (pitch + half_fov).sin();
        let outside = |q: f32, coeff: f32| {
            let shake = MAX_SHAKE * coeff.abs() * (sin_yaw.abs() + cos_yaw.abs());
            coeff * q - plane_offset > ENVELOPE + shake + 1e-5
        };
        let depth = |t: Vec2| (p.x - t.x) * sin_yaw + (p.y - t.y) * cos_yaw;
        let targets = [self.prev_camera.0, self.camera.0, self.look_ahead_target()];
        [top, bottom]
            .iter()
            .any(|&plane| targets.iter().all(|&t| outside(depth(t), plane)))
    }

    fn look_ahead_target(&self) -> Vec2 {
        self.player.pos + Vec2::from_angle(self.player.facing) * self.tuning.look_ahead_dist
    }

    fn animation_system(&mut self) {
        let stride = std::f32::consts::TAU / STRIDE_M;
        let p = &mut self.player;
        p.walk_phase += (p.pos - p.prev_pos).length() * stride;
        for z in self.zombies.iter_mut().filter(|z| z.active) {
            z.walk_phase += (z.pos - z.prev_pos).length() * stride;
        }
    }

    fn camera_system(&mut self) {
        let desired = self.look_ahead_target();
        let k = 1.0 - (-self.tuning.follow_rate * STEP_S).exp();
        self.camera = CameraTarget(self.camera.0 + (desired - self.camera.0) * k);
    }

    // -----------------------------------------------------------------------
    // Hitscan
    // -----------------------------------------------------------------------

    /// Aim assist: snap to the nearest visible zombie inside a 25° cone (2° tie window).
    fn select_shot_facing(&self, range: f32) -> f32 {
        let cone = 25f32.to_radians();
        let tie = 2f32.to_radians();
        let f = Vec2::from_angle(self.player.facing);
        let origin = self.player.pos;
        let visible: Vec<(usize, f32, f32)> = self
            .zombies
            .iter()
            .enumerate()
            .filter(|(_, z)| z.active)
            .filter_map(|(i, z)| {
                let d = z.pos - origin;
                let d2 = d.length_squared();
                if d2 == 0.0 || d2 > range * range + 1e-6 {
                    return None;
                }
                let error = f.perp_dot(d).atan2(f.dot(d)).abs();
                if error > cone + 1e-6 {
                    return None;
                }
                let (hit, _) = self.trace_shot(d / d2.sqrt(), range);
                (hit == Some(i)).then_some((i, error, d2))
            })
            .collect();
        let Some(min_error) = visible.iter().map(|v| v.1).reduce(f32::min) else {
            return self.player.facing;
        };
        visible
            .iter()
            .filter(|v| v.1 <= min_error + tie + 1e-6)
            .min_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)))
            .map(|v| (self.zombies[v.0].pos - origin).to_angle())
            .unwrap_or(self.player.facing)
    }

    /// Heading to the closest zombie within `range` that a shot would actually reach.
    fn nearest_visible_zombie(&self, range: f32) -> Option<f32> {
        let origin = self.player.pos;
        self.zombies
            .iter()
            .enumerate()
            .filter(|(_, z)| z.active && z.hp > 0)
            .filter_map(|(i, z)| {
                let d = z.pos - origin;
                let dist = d.length();
                if dist == 0.0 || dist > range {
                    return None;
                }
                let (hit, _) = self.trace_shot(d / dist, range);
                (hit == Some(i)).then_some((dist, d.to_angle()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, facing)| facing)
    }

    /// First zombie hit along a unit ray, and the ray's travel distance. Walls win ties.
    fn trace_shot(&self, dir: Vec2, range: f32) -> (Option<usize>, f32) {
        let o = self.player.pos;
        let b = self.layout.bounds;
        let mut dist = f32::INFINITY;
        if dir.x > 0.0 {
            dist = dist.min((b.max_x - o.x) / dir.x);
        }
        if dir.x < 0.0 {
            dist = dist.min((b.min_x - o.x) / dir.x);
        }
        if dir.y > 0.0 {
            dist = dist.min((b.max_y - o.y) / dir.y);
        }
        if dir.y < 0.0 {
            dist = dist.min((b.min_y - o.y) / dir.y);
        }
        dist = dist.max(0.0);
        for wall in &self.layout.colliders {
            dist = dist.min(ray_box(o, dir, wall));
        }
        let blocker = dist;
        let mut dist = range.min(dist);
        let mut hit = None;
        for (i, z) in self.zombies.iter().enumerate() {
            if !z.active {
                continue;
            }
            let d = ray_circle(o, dir, z.pos, z.radius);
            if d <= range && d < blocker && (hit.is_none() || d < dist) {
                dist = d;
                hit = Some(i);
            }
        }
        (hit, dist)
    }
}

/// Circle versus finite sector around a unit aim direction (`combatSystem.ts`).
pub fn in_melee_sector(
    origin: Vec2,
    aim: Vec2,
    cos_half: f32,
    range: f32,
    target: Vec2,
    target_r: f32,
) -> bool {
    let d = target - origin;
    let dist_sq = d.length_squared();
    let reach = range + target_r;
    if dist_sq > reach * reach + 1e-6 {
        return false;
    }
    let radius_sq = target_r * target_r + 1e-6;
    if dist_sq <= radius_sq {
        return true;
    }
    let forward = d.dot(aim);
    if forward >= dist_sq.sqrt() * cos_half {
        return true;
    }
    let sideways = d.y * aim.x - d.x * aim.y;
    let sin_half = (1.0 - cos_half * cos_half).max(0.0).sqrt();
    [-1.0f32, 1.0].iter().any(|&side| {
        let edge_y = side * sin_half;
        let proj = (forward * cos_half + sideways * edge_y).clamp(0.0, range);
        let gx = forward - proj * cos_half;
        let gy = sideways - proj * edge_y;
        gx * gx + gy * gy <= radius_sq
    })
}

fn ray_circle(o: Vec2, dir: Vec2, c: Vec2, r: f32) -> f32 {
    let x = c - o;
    let cc = x.length_squared() - r * r;
    if cc <= 0.0 {
        return 0.0;
    }
    let projection = x.dot(dir);
    if projection < 0.0 {
        return f32::INFINITY;
    }
    let perpendicular = x.x * dir.y - x.y * dir.x;
    let disc = r * r - perpendicular * perpendicular;
    if disc < -1e-6 {
        return f32::INFINITY;
    }
    (projection - disc.max(0.0).sqrt()).max(0.0)
}

fn ray_box(o: Vec2, dir: Vec2, b: &Aabb) -> f32 {
    let mut near = 0.0f32;
    let mut far = f32::INFINITY;
    for (oc, dc, lo, hi) in [
        (o.x, dir.x, b.min_x, b.max_x),
        (o.y, dir.y, b.min_y, b.max_y),
    ] {
        if dc == 0.0 {
            if oc < lo || oc > hi {
                return f32::INFINITY;
            }
        } else {
            let a = (lo - oc) / dc;
            let c = (hi - oc) / dc;
            near = near.max(a.min(c));
            far = far.min(a.max(c));
        }
    }
    if near <= far { near } else { f32::INFINITY }
}

/// Mass-weighted push-apart for two overlapping circles; exact overlap separates along +x.
fn separate(a: Vec2, ra: f32, ma: f32, b: Vec2, rb: f32, mb: f32) -> Option<(Vec2, Vec2)> {
    let d = b - a;
    let min = ra + rb;
    let len_sq = d.length_squared();
    if len_sq >= min * min {
        return None;
    }
    let len = len_sq.sqrt();
    let n = if len > 0.0 { d / len } else { Vec2::X };
    let overlap = min - len;
    let total = ma + mb;
    Some((-n * overlap * (mb / total), n * overlap * (ma / total)))
}

/// Push a circle out of every AABB along the shallowest axis, then clamp to the bounds.
fn resolve_static(pos: &mut Vec2, r: f32, colliders: &[Aabb], bounds: Aabb) {
    for c in colliders {
        let (lo_x, hi_x, lo_y, hi_y) = (c.min_x - r, c.max_x + r, c.min_y - r, c.max_y + r);
        if pos.x <= lo_x || pos.x >= hi_x || pos.y <= lo_y || pos.y >= hi_y {
            continue;
        }
        let pushes = [
            (lo_x - pos.x, 0),
            (hi_x - pos.x, 0),
            (lo_y - pos.y, 1),
            (hi_y - pos.y, 1),
        ];
        let (amount, axis) = pushes
            .into_iter()
            .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
            .expect("four candidates");
        if axis == 0 {
            pos.x += amount;
        } else {
            pos.y += amount;
        }
    }
    pos.x = pos.x.clamp(bounds.min_x + r, bounds.max_x - r);
    pos.y = pos.y.clamp(bounds.min_y + r, bounds.max_y - r);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World::new(Layout::maplewood(), Tuning::default(), 1)
    }

    fn step_n(w: &mut World, n: usize, input: &InputSnapshot) {
        for _ in 0..n {
            w.step(input);
        }
    }

    fn clear_horde(w: &mut World) {
        for z in &mut w.zombies {
            z.active = false;
        }
    }

    fn place_zombie(w: &mut World, slot: usize, offset: Vec2) {
        let pos = w.player.pos + offset;
        w.zombies[slot] = Zombie {
            active: true,
            pos,
            prev_pos: pos,
            hp: 50,
            max_hp: 50,
            damage: 10,
            radius: SHAMBLER.radius,
            hit_swing_id: u32::MAX,
            ..Default::default()
        };
    }

    /// Park the player in open street so tests are not disturbed by colliders.
    fn open_street(w: &mut World) {
        w.player.pos = Vec2::new(-24.0, 0.0);
        w.player.prev_pos = w.player.pos;
        w.player.facing = 0.0;
    }

    #[test]
    fn initial_horde_spawns_off_screen_and_far() {
        let w = world();
        assert_eq!(w.active_zombies(), 20);
        for z in w.zombies.iter().filter(|z| z.active) {
            assert!(z.pos.distance(w.player.pos) > SPAWN_MIN_DISTANCE_FROM_PLAYER);
        }
    }

    #[test]
    fn same_seed_is_deterministic() {
        let mut a = world();
        let mut b = world();
        let input = InputSnapshot {
            move_dir: Vec2::X,
            fire_held: true,
            ..default()
        };
        step_n(&mut a, 600, &input);
        step_n(&mut b, 600, &input);
        assert_eq!(a.player.pos, b.player.pos);
        assert_eq!(a.kills, b.kills);
        let pa: Vec<Vec2> = a.zombies.iter().map(|z| z.pos).collect();
        let pb: Vec<Vec2> = b.zombies.iter().map(|z| z.pos).collect();
        assert_eq!(pa, pb);
    }

    #[test]
    fn cursor_aim_sets_facing_independently_of_movement() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        let target = w.player.pos + Vec2::new(0.0, 5.0);
        let input = InputSnapshot {
            move_dir: Vec2::NEG_X,
            aim_point: Some(target),
            ..default()
        };
        w.step(&input);
        let facing = Vec2::from_angle(w.player.facing);
        assert!(
            facing.dot(Vec2::Y) > 0.999,
            "faces the cursor, got {facing}"
        );
        assert!(w.player.vel.x < 0.0, "still walks the other way");
    }

    #[test]
    fn without_cursor_facing_still_follows_movement() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        let input = InputSnapshot {
            move_dir: Vec2::Y,
            ..default()
        };
        step_n(&mut w, 30, &input);
        assert!(Vec2::from_angle(w.player.facing).dot(Vec2::Y) > 0.99);
    }

    #[test]
    fn swing_goes_toward_the_cursor() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        place_zombie(&mut w, 0, Vec2::new(-1.5, 0.0));
        let aim = Some(w.player.pos + Vec2::new(-3.0, 0.0));
        w.step(&InputSnapshot {
            melee_pulse: true,
            aim_point: aim,
            ..default()
        });
        step_n(
            &mut w,
            8,
            &InputSnapshot {
                aim_point: aim,
                ..default()
            },
        );
        assert_eq!(w.zombies[0].hp, 25, "zombie behind the old facing is hit");
    }

    #[test]
    fn auto_target_fire_turns_to_the_nearest_zombie() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        place_zombie(&mut w, 0, Vec2::new(-5.0, 0.0));
        place_zombie(&mut w, 1, Vec2::new(0.0, 9.0));
        let input = InputSnapshot {
            fire_held: true,
            auto_target: true,
            ..default()
        };
        w.step(&input);
        assert_eq!(
            w.zombies[0].hp, 25,
            "nearest zombie, directly behind, is shot"
        );
        assert_eq!(w.zombies[1].hp, 50);
        assert!(Vec2::from_angle(w.player.facing).dot(Vec2::NEG_X) > 0.999);
    }

    #[test]
    fn auto_target_without_zombies_in_range_fires_ahead() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        let input = InputSnapshot {
            fire_held: true,
            auto_target: true,
            ..default()
        };
        w.step(&input);
        assert_eq!(w.player.facing, 0.0);
        assert_eq!(w.player.revolver_ammo, REVOLVER.initial_ammo - 1);
    }

    #[test]
    fn forward_walks_toward_the_cursor_and_back_retreats_facing_it() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        let start = w.player.pos;
        let aim = Some(start + Vec2::new(0.0, 6.0));
        step_n(
            &mut w,
            30,
            &InputSnapshot {
                throttle: 1.0,
                aim_point: aim,
                ..default()
            },
        );
        assert!(
            w.player.pos.y > start.y + 1.5,
            "forward moves toward the cursor"
        );
        assert!((w.player.pos.x - start.x).abs() < 1e-3);
        let mid = w.player.pos;
        step_n(
            &mut w,
            30,
            &InputSnapshot {
                throttle: -1.0,
                aim_point: aim,
                ..default()
            },
        );
        assert!(
            w.player.pos.y < mid.y - 1.5,
            "back moves away from the cursor"
        );
        assert!(
            Vec2::from_angle(w.player.facing).dot(Vec2::Y) > 0.999,
            "still faces the cursor"
        );
    }

    #[test]
    fn forward_stops_at_the_cursor_without_spinning() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        let aim = w.player.pos + Vec2::new(2.0, 0.0);
        step_n(
            &mut w,
            120,
            &InputSnapshot {
                throttle: 1.0,
                aim_point: Some(aim),
                ..default()
            },
        );
        assert!(w.player.pos.distance(aim) < 0.6, "arrives at the cursor");
        assert!(
            Vec2::from_angle(w.player.facing).dot(Vec2::X) > 0.99,
            "keeps facing the same way"
        );
    }

    #[test]
    fn forward_without_cursor_follows_current_facing() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        w.player.facing = std::f32::consts::FRAC_PI_2;
        let start = w.player.pos;
        step_n(
            &mut w,
            30,
            &InputSnapshot {
                throttle: 1.0,
                ..default()
            },
        );
        assert!(w.player.pos.y > start.y + 1.5);
        assert!((w.player.pos.x - start.x).abs() < 1e-3);
    }

    #[test]
    fn melee_hits_in_front_with_hitstop_and_knockback() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        place_zombie(&mut w, 0, Vec2::new(1.5, 0.0));
        place_zombie(&mut w, 1, Vec2::new(-1.5, 0.0));
        w.step(&InputSnapshot {
            melee_pulse: true,
            ..default()
        });
        let idle = InputSnapshot::default();
        let mut hit = false;
        for _ in 0..10 {
            w.step(&idle);
            if w.zombies[0].hp < 50 {
                hit = true;
                break;
            }
        }
        assert!(hit, "zombie in front takes the swing");
        assert_eq!(w.zombies[0].hp, 25);
        assert_eq!(w.zombies[1].hp, 50, "zombie behind is untouched");
        assert_eq!(w.hitstop_ticks, w.tuning.hitstop_ticks);
        assert!(w.zombies[0].kb_vel.x > 0.0);
        assert!(
            w.events
                .iter()
                .any(|e| matches!(e, SimEvent::MeleeHit { count: 1, .. }))
        );
    }

    #[test]
    fn revolver_kills_in_two_shots_and_drops_cash() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        place_zombie(&mut w, 0, Vec2::new(5.0, 0.0));
        let fire = InputSnapshot {
            fire_held: true,
            ..default()
        };
        step_n(&mut w, 30, &fire);
        assert_eq!(w.kills, 1);
        assert_eq!(w.player.revolver_ammo, REVOLVER.initial_ammo - 2);
        assert!(w.pickups.iter().any(|p| p.kind == PickupKind::Cash));
    }

    #[test]
    fn empty_gun_swings_instead() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        w.player.revolver_ammo = 0;
        w.player.reserve_ticks = 0;
        w.step(&InputSnapshot {
            fire_pulse: true,
            ..default()
        });
        assert!(w.player.attack_tick.is_some());
    }

    #[test]
    fn zombie_swipe_damages_then_grace_blocks() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        place_zombie(&mut w, 0, Vec2::new(0.85, 0.0));
        let idle = InputSnapshot::default();
        step_n(&mut w, 10, &idle);
        assert_eq!(w.player.hp, PLAYER_HP - 10);
        step_n(&mut w, 20, &idle);
        assert_eq!(w.player.hp, PLAYER_HP - 10, "cooldown and grace hold");
    }

    #[test]
    fn run_ends_at_zero_hp() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        w.player.hp = 5;
        place_zombie(&mut w, 0, Vec2::new(0.85, 0.0));
        step_n(&mut w, 30, &InputSnapshot::default());
        assert_eq!(w.player.hp, 0);
        assert_eq!(w.phase, Phase::Ended);
        assert!(w.events.contains(&SimEvent::RunEnded));
    }

    #[test]
    fn health_pickup_not_taken_at_full_hp() {
        let mut w = world();
        clear_horde(&mut w);
        open_street(&mut w);
        let at = w.player.pos;
        w.spawn_pickup(PickupKind::Health, 20, at);
        w.step(&InputSnapshot::default());
        assert_eq!(w.pickups.len(), 1);
        w.player.hp = 50;
        w.step(&InputSnapshot::default());
        assert_eq!(w.pickups.len(), 0);
        assert_eq!(w.player.hp, 70);
    }

    #[test]
    fn player_cannot_walk_through_buildings_or_bounds() {
        let mut w = world();
        clear_horde(&mut w);
        let input = InputSnapshot {
            move_dir: Vec2::new(0.0, 1.0),
            ..default()
        };
        step_n(&mut w, 60 * 60, &input);
        let p = w.player.pos;
        assert!(p.y <= w.layout.bounds.max_y - PLAYER_RADIUS + 1e-3);
        for c in &w.layout.colliders {
            let inside = p.x > c.min_x && p.x < c.max_x && p.y > c.min_y && p.y < c.max_y;
            assert!(!inside, "player inside collider at {p}");
        }
    }

    #[test]
    fn sector_rejects_behind_and_accepts_edge_graze() {
        let aim = Vec2::X;
        let cos = 70f32.to_radians().cos();
        assert!(in_melee_sector(
            Vec2::ZERO,
            aim,
            cos,
            2.2,
            Vec2::new(1.0, 0.0),
            0.4
        ));
        assert!(!in_melee_sector(
            Vec2::ZERO,
            aim,
            cos,
            2.2,
            Vec2::new(-1.5, 0.0),
            0.4
        ));
        assert!(!in_melee_sector(
            Vec2::ZERO,
            aim,
            cos,
            2.2,
            Vec2::new(3.0, 0.0),
            0.4
        ));
    }

    fn default() -> InputSnapshot {
        InputSnapshot::default()
    }
}
