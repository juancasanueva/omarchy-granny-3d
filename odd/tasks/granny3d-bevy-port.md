# Granny vs. The Neighborhood — Rust + Bevy port

Objective: port the web game at `~/Projects/web/granny3d` (TS + Three.js) to Rust + Bevy 0.20 for Omarchy, with a much better UI.
Branch: `main` (new repo). Delivery: `ask-on-risk`. Test runner: `cargo test`.

## Specs

- S1. "I want to make a game for Omarchy, using Rust + Bevy, that is a clone of our 3d granny vs zombies game here /home/juancasanueva/Projects/web/granny3d" — same gameplay: deterministic 60 Hz sim, WASD/arrows camera-relative movement, Space rolling-pin melee with hitstop/knockback/stun, LMB revolver/shotgun hitscan with aim assist, 1/2 + wheel weapon select, shambler AI with swipe, horde difficulty ramp, ammo/health/cash drops, run ends at 0 HP, banked cash + one-time Cardio upgrade, pause (Esc/focus loss), same assets (granny, zombie, Maplewood town, streetlamps), fixed oblique night camera with look-ahead and shake. Numbers copied from `src/data/*.ts`.
- S2. "Make the UI much better" — polished title screen over the live 3D town, upgrade card, themed HUD (HP bar, weapon cards with ammo, kills/time/cash, damage vignette, floating numbers, pickup toasts), pause overlay, run-summary screen. UI follows the Omarchy theme palette.
- S3. (assumption) Omarchy integration: `.desktop` entry, icon and `make install` like the user's other Bevy games.

## Tasks

- [x] T1 [S1] Pure simulation module (`src/sim`) with unit tests — inline — c2642b7
- [x] T2 [S1] Bevy world: town, lights, lamps, actors + animation, camera, FX — inline — f0241b8
- [x] T3 [S2] UI: title, HUD, pause, run summary, floating numbers, toasts — inline — f0241b8
- [x] T4 [S3] Packaging, README — inline — eec208b

## Log

- L1 (2026-11-27): "I want to make a game for Omarchy, using Rust + Bevy, that is a clone of our 3d granny vs zombies game here  /home/juancasanueva/Projects/web/granny3d. Make the UI much better"
- L2: Routes inline: tasks are sequential and dependent (sim → render → UI), no Writer-trigger reason.
- L3: Verification: `cargo test` 27 passed; `make lint` clean; screenshots via `GRANNY3D_SHOT` of title, autoplay gameplay, pause and summary at wide and narrow window sizes; `make install PREFIX=<scratch>` binary loads assets from the install prefix.
- L4: Fixed during visual checks: scene too dark, solid swing arc, missing glyphs (`→`, bolt in display font), top HUD overlap in narrow windows, summary grid wrap, pickup views reused across runs. Test runs had written `~/.local/share/granny3d/save.json`; removed and added `GRANNY3D_SAVE`.
- L5: `gentle-ai review assess` over c2642b7..HEAD: risk high (process_boundary: env var reads in src/lib.rs scope), no `review_due` field returned, so native review was not started. Not playtested by a human yet; the gameplay feel is unverified beyond automated input.
