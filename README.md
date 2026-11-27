# Granny vs. The Neighborhood

A 3D zombie-survival brawler for Omarchy, built with Rust and Bevy. Granny holds off a shambling horde on Maplewood Lane with a baseball bat, a revolver and a cane shotgun. This is a port of the web game in `~/Projects/web/granny3d`: the rules, balance and assets are the same. The UI has been redesigned, and aiming now uses the mouse (or auto-targeting on `F`), and movement is two keys: forward toward the aim and back away from it.

## Quick path

```bash
make run        # play from the source tree
make install    # install to ~/.local so it appears in the Omarchy launcher
make uninstall
```

## Controls

| Input | Action |
|-------|--------|
| `W` / `↑`, `S` / `↓` | Walk forward toward where Granny aims, or back away while still facing it |
| Mouse | Aim: Granny faces the cursor while you move, so you can back away and shoot |
| `Space` / left mouse (hold) | Use the selected weapon toward the cursor: swing the bat (hitstop, knockback, stun) or fire a gun with aim assist |
| `F` (hold) | Keyboard play: turn to the nearest zombie in reach and use the selected weapon |
| `1` / `2` / `3`, `Q`, mouse wheel | Baseball bat / revolver / cane shotgun / switch |
| `Esc` | Pause (also pauses when the window loses focus) |

Moving or clicking the mouse switches to mouse aim; pressing `F` switches back to keyboard aim, where forward is simply the way Granny currently faces and `F` turns her toward the nearest zombie. Menus work with the arrow keys plus `Enter`, their shortcut keys, or the mouse.

## How a run works

- The horde starts at 20, grows by 10 a minute up to 40, then gets tougher.
- Kills drop cash, plus ammo every 3 kills and health every 10. Pickups are pulled toward you once you're close.
- An empty gun swings the bat instead. The revolver regains 1 round every 3 seconds.
- When the run ends, its cash goes into your bank. **Cardio** (50 cash, title screen) permanently gives +10% walk speed.

## Details

| Topic | Where / decision |
|-------|------------------|
| Game rules | `src/sim`: deterministic 60 Hz simulation, no Bevy rendering, unit-tested |
| Presentation | `src/game`: scene, actors, effects and UI read the sim and never write to it |
| UI colors | Taken from the active Omarchy theme (`~/.local/state/omarchy/current/theme/colors.toml`) |
| Save file | `~/.local/share/granny3d/save.json` (bank, Cardio, personal bests); an unreadable save is never overwritten |
| Assets | `assets/models/*.glb` and `assets/layout.json`, copied from the web game; fonts are embedded |

## Development

```bash
cargo test                      # simulation, save, layout and UI-format tests
make lint                       # clippy, warnings as errors
cargo run --features dev        # faster incremental builds (dynamic linking)
```

Optional environment variables for unattended checks:

| Variable | Effect |
|----------|--------|
| `GRANNY3D_AUTOSTART=play\|pause\|summary` | Skip the title screen and go to that screen |
| `GRANNY3D_AUTOPLAY=1` | Drive Granny with scripted input |
| `GRANNY3D_AUTOPLAY_GUN=bat\|revolver\|shotgun` | Weapon the scripted run holds |
| `GRANNY3D_SHOT=/path.png` | Save a screenshot after `GRANNY3D_SHOT_AFTER` seconds (default 6), then quit |
| `GRANNY3D_SAVE=/path.json` | Use a different save file |
| `GRANNY3D_ASSETS=/dir` | Load models from another directory |
