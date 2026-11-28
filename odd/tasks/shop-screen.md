# Shop screen

Objective: replace the title's Cardio button with a SHOP entry that opens a shop screen listing Cardio and other purchases such as ammo.
Branch: `feat/shop-screen`. Test runner: `cargo test` plus in-game screenshots.

## Specs

- S1. "I want to have a Shop or Equip button that show a new UI" — the title menu gets a SHOP button that opens its own screen; the Cardio button moves off the title.
- S2. "with the cardio" — Cardio is sold in the shop, still one-time and permanent.
- S3. "and with other stuff that we could buy, like ammo" — revolver ammo boxes and shotgun shell packs are sold as consumables that add rounds to the next run.
- Assumption: bought ammo is spent by the next run that starts (the shop is only reachable from the title, so a Restart never finds stock); stock is limited so starting ammo never exceeds the gun's carry cap. Recurring ammo gives banked cash a use after Cardio is owned.

## Tasks

- [x] T1 [S2-S3] Sim: `ShopItem` catalog, generic `SaveStore::purchase`, ammo stock in the save, taking stock at run start, tests — inline
- [x] T2 [S1-S3] Game: `AppState::Shop`, shop UI with items, bank, detail and messages, title SHOP button, keyboard and mouse, README — inline
- [x] T3 Checks: `cargo test`, clippy, screenshots of title and shop

## Log

- L1 (2026-11-28): "In the main menu, we have a Cardio button... I want to have a Shop or Equip button that show a new UI with the cardio and with other stuff that we could buy, like ammo"
- L2: Routes inline: dependent steps, no Writer-trigger reason.
- L3: Sim: `ShopItem { Cardio, RevolverAmmo, ShotgunShells }` with cost and `max_stock` ((cap - initial) / pack: 4 boxes of 12 rounds at 15, 4 packs of 4 shells at 20); `SaveStore::purchase(item)` replaces `purchase_cardio` (`AlreadyOwned` became `Maxed`); save gains `revolver_boxes`/`shotgun_packs` (serde default, older saves load); `take_ammo_stock` hands the rounds to a starting run and clears them; `Player::add_ammo` clamps to the cap.
- L4: Game: `AppState::Shop` with its own screen (bank, items with price/OWNED/FULL, focused-item detail, purchase messages, BACK/ESC/Backspace/B); title menu is PLAY / SHOP (B) / QUIT plus a "Stocked for your next run" line; closing the shop refocuses SHOP (`MenuFocus::next`); the slide-in animation is now a shared `Intro` component; `GRANNY3D_AUTOSTART=shop` for screenshots.
- L5: First launch panicked with Bevy B0001 (a `ParamSet` overlapping the button text queries); replaced by one `ShopText` enum query; tests now initialize the shop and title systems (mutation-checked: a conflicting query fails the test).
- L6: Checks: `cargo test` 69 passed, clippy clean, fmt clean; screenshots of title and shop at 1600x900 (README updated with a Shop column); an autostarted run with 2 boxes + 3 packs started with revolver 48/72 and shotgun 20/24, and the save's stock went to 0.
