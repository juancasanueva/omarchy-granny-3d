# Baseball bat as a third weapon

Objective: replace the rolling-pin swing with a baseball bat as weapon 1, alongside the revolver (2) and shotgun (3), with Space and LMB using whichever weapon is selected.
Branch: `feat/baseball-bat`. Delivery: `ask-on-risk`. Test runner: `cargo test` plus in-game screenshots.

## Specs

- S1. "when we swing, I want to have a beisball bat" — a baseball bat model in Granny's hand for melee, modelled in Blender, with a hold pose when the bat is selected.
- S2. "So we have three weapons: a bat, the pistos and the shotgut." — three selectable weapons; the bat keeps the rolling pin's melee stats.
- S3. "1 = bat, 2 = pistol, and 3 = shotgun." — number keys select the weapons; Q and the wheel cycle through all three.
- S4. "Space and lmb will fire the currently selected weapon." — both inputs use the selected weapon (bat swings, guns shoot); holding repeats.
- Assumption: firing an empty gun still swings the bat (the existing empty-gun rule); F auto-target also works with the bat.

## Tasks

- [x] T1 [S2-S4] Sim: Weapon enum, selection, unified use input, tests — inline
- [x] T2 [S1] Blender: bat model, AimBat hold clip, check swing clip — inline
- [x] T3 [S1-S4] Game: bat on the hand bone, visibility, input mapping, HUD cards, hints, README — inline

## Log

- L1 (2026-11-27): "when we swing, I want to have a beisball bat. So we have three weapons: a bat, the pistos and the shotgut. 1 = bat, 2 = pistol, and 3 = shotgun. Space and lmb will fire the currently selected weapon."
- L2: Routes inline: dependent steps, no Writer-trigger reason.
- L3: Bat modelled as a lathed profile (ash wood, black grip tape, knob), 0.63 m in Granny's units, leaving the fist on the pinky side so the existing Attack clip reads as an overhead bonk (windup up and back, impact down and forward). New upper-body AimBat clip rests the bat over the right shoulder. Bat exported in the hand bone's frame (`bat.glb`, no Y-up conversion).
- L4: Sim: `Weapon { Bat, Revolver, Shotgun }`, `select`/`cycle` over all three, the old melee input removed; Space and LMB both fire; holding repeats; empty guns still swing the bat; F auto-target uses melee reach with the bat. ROLLING_PIN renamed BAT with the same stats.
- L5: Checks: `cargo test` 50 passed (new: number keys and cycle, bat fire swings and never shoots, held swings repeat, bat auto-target, bat shown for every swing), clippy clean; screenshots of swing frames, the shoulder hold from front and back, HUD cards 1/2/3, and the title legend.
- L6: Review review-19d40f975fa659df approved and acknowledged; fixed its findings: only a held mouse button cancels F auto-target (Space no longer does), scripted runs take a two-second bat spell every six seconds unless a weapon is chosen, and tests pin that the bat never clicks empty while an empty gun does. 52 tests, clippy clean; default autoplay screenshot shows the bat selected mid-swing.
- L7 (2026-11-27): user: "I dont like how the bar and the hand looks when its idle... its not right". Cause: the shoulder hold left the hand open at the chest with the bat starting behind it and passing through the upper arm. AimBat is now a relaxed carry (arm at her side, bat hanging down and back), and the bat sits 4 cm further into the fist so she holds the taped handle; the swing was re-checked with the new grip. 52 tests; in-game idle screenshots.
