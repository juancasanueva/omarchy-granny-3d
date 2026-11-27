# Granny vs. The Neighborhood — Rust + Bevy port

Objective: port the web game at `~/Projects/web/granny3d` (TS + Three.js) to Rust + Bevy 0.20 for Omarchy, with a much better UI.
Branch: `main` (new repo). Delivery: `ask-on-risk`. Test runner: `cargo test`.

## Specs

- S1. "I want to make a game for Omarchy, using Rust + Bevy, that is a clone of our 3d granny vs zombies game here /home/juancasanueva/Projects/web/granny3d" — same gameplay: deterministic 60 Hz sim, WASD/arrows camera-relative movement, Space rolling-pin melee with hitstop/knockback/stun, LMB revolver/shotgun hitscan with aim assist, 1/2 + wheel weapon select, shambler AI with swipe, horde difficulty ramp, ammo/health/cash drops, run ends at 0 HP, banked cash + one-time Cardio upgrade, pause (Esc/focus loss), same assets (granny, zombie, Maplewood town, streetlamps), fixed oblique night camera with look-ahead and shake. Numbers copied from `src/data/*.ts`.
- S2. "Make the UI much better" — polished title screen over the live 3D town, upgrade card, themed HUD (HP bar, weapon cards with ammo, kills/time/cash, damage vignette, floating numbers, pickup toasts), pause overlay, run-summary screen. UI follows the Omarchy theme palette.
- S3. (assumption) Omarchy integration: `.desktop` entry, icon and `make install` like the user's other Bevy games.

## Tasks

- T1 [S1] Pure simulation crate module (`src/sim`) with unit tests — inline
- T2 [S1] Bevy world: town, lights, lamps, actors + animation, camera, FX — inline
- T3 [S2] UI: title, HUD, pause, run summary, floating numbers, toasts — inline
- T4 [S3] Packaging, README — inline

## Log

- L1 (2026-11-27): "I want to make a game for Omarchy, using Rust + Bevy, that is a clone of our 3d granny vs zombies game here  /home/juancasanueva/Projects/web/granny3d. Make the UI much better"
- L2: Routes inline: tasks are sequential and dependent (sim → render → UI), no Writer-trigger reason.
