# Granny gun models, poses and fire animations

Objective: replace the primitive revolver and cane shotgun with real models, and give Granny gun-holding poses and firing animations, authored in Blender.
Branch: `feat/gun-poses`. Delivery: `ask-on-risk`. Test runner: `cargo test` plus in-game screenshots.

## Specs

- S1. "the gun and the shotgun model dont look very nice" — model a better revolver and cane shotgun in Blender (bevelled shapes, cylinder detail, wood and brass materials) and use them in the game.
- S2. "Granny does not have a gun and shotgun pose to hold them" — while a gun is selected and she is not swinging, her upper body holds the gun in a pose: revolver one-handed, shotgun two-handed. Her legs keep walking and idling.
- S3. "and animations to fire them" — each shot plays a short recoil animation for that gun.
- S4. "Use the blender MCP if you need to make 3d models and animations" — models and clips are authored in Blender and exported to `assets/models/`.
- Assumption: the guns attach to the `hand.R` bone; muzzle effects start at a `Muzzle` point on each gun model.

## Tasks

- [x] T1 [S1, S4] Model revolver and cane shotgun in Blender, export GLBs with a Muzzle node — inline
- [x] T2 [S2, S3, S4] Author AimRevolver, FireRevolver, AimShotgun, FireShotgun on Granny's rig; re-export `granny.glb` — inline
- [x] T3 [S1-S3] Game: attach gun models to the hand bone, upper-body masked aim/fire layers, muzzle-based tracers — inline
- [x] T4 Verify in game (screenshots), docs — inline; PR pending the user's go-ahead

## Log

- L1 (2026-11-27): "the gun and the shotgun model dont look very nice. Also, Granny does not have a gun and shotgun pose to hold them and animations to fire them. Use the blender MCP if you need to make 3d models and animations"
- L2: Routes inline: dependent steps (models → poses → game wiring), no Writer-trigger reason.
- L3: Blender source saved at `assets-src/granny-guns.blend`. Guns are built from bevelled primitives (walnut, brass, gunmetal), revolver 0.41 m and shotgun 0.80 m in Granny's model units, grip at the origin.
- L4: Bugs found and fixed: (a) authoring keys with the action assigned let animation evaluation leak into baked IK poses, so keys are now captured with no action and written afterwards; (b) glTF joints keep Blender bone axes, so gun meshes in bone space must be exported with `export_yup=False`. A suspected mask failure turned out to be the title pose aiming straight at the high camera; the title now turns Granny 0.85 rad for a three-quarter view.
- L5: Verification: `cargo test` 37 passed, clippy clean; in-game screenshots of the title pose, revolver aim and fire (flash and tracer at the muzzle), and shotgun hip pose and kick.
- L6: Review review-4d7eb26a79a68931 (reliability lens) approved and acknowledged; advisory R3-001 fixed by testing bone name paths against the shipped granny.glb, the bone-to-group mapping and the per-layer masks (mutation-checked); R3-002..R3-004 left as later work.
