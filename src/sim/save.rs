//! Banked cash, shop purchases and personal bests, saved as JSON under
//! `$XDG_DATA_HOME/granny3d/save.json` (default `~/.local/share/granny3d/`).
//! Mirrors the web game's `runEarnings.ts`: an unreadable save is preserved
//! untouched and earnings become temporary instead of overwriting it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::data::{Gun, ShopItem};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub cash: u64,
    pub cardio: bool,
    #[serde(default)]
    pub best_time_secs: f32,
    #[serde(default)]
    pub best_kills: u32,
    #[serde(default)]
    pub runs: u32,
    /// Revolver ammo boxes bought for the next run.
    #[serde(default)]
    pub revolver_boxes: u32,
    /// Shotgun shell packs bought for the next run.
    #[serde(default)]
    pub shotgun_packs: u32,
}

impl SaveData {
    /// How many of `item` are owned or stocked.
    pub fn owned(&self, item: ShopItem) -> u32 {
        match item {
            ShopItem::Cardio => self.cardio as u32,
            ShopItem::RevolverAmmo => self.revolver_boxes,
            ShopItem::ShotgunShells => self.shotgun_packs,
        }
    }

    fn add(&mut self, item: ShopItem) {
        match item {
            ShopItem::Cardio => self.cardio = true,
            ShopItem::RevolverAmmo => self.revolver_boxes += 1,
            ShopItem::ShotgunShells => self.shotgun_packs += 1,
        }
    }

    /// Extra starting rounds per gun from the stocked ammo.
    pub fn stocked_rounds(&self, gun: Gun) -> u32 {
        ShopItem::ALL
            .iter()
            .filter_map(|&item| item.ammo().map(|(g, per)| (item, g, per)))
            .filter(|&(_, g, _)| g == gun)
            .map(|(item, _, per)| self.owned(item) * per)
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    /// Saving works.
    Ready,
    /// No writable location: earnings last until the game closes.
    Temporary,
    /// An existing save could not be read; it is left untouched.
    Preserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseResult {
    Purchased,
    /// Cardio is already owned, or the ammo stock is full.
    Maxed,
    NotEnoughCash,
    Unavailable,
}

/// What a finished run added to the save.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settlement {
    pub credited: u64,
    pub balance: u64,
    pub new_best_time: bool,
    pub new_best_kills: bool,
    pub saved: bool,
}

#[derive(Debug, Clone)]
pub struct SaveStore {
    pub data: SaveData,
    pub status: SaveStatus,
    path: Option<PathBuf>,
}

const VERSION: u32 = 3;

impl SaveStore {
    pub fn load_default() -> Self {
        Self::load(default_path())
    }

    pub fn load(path: Option<PathBuf>) -> Self {
        let fresh = SaveData {
            version: VERSION,
            ..Default::default()
        };
        let Some(path) = path else {
            return Self {
                data: fresh,
                status: SaveStatus::Temporary,
                path: None,
            };
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<SaveData>(&text) {
                Ok(data) => Self {
                    data,
                    status: SaveStatus::Ready,
                    path: Some(path),
                },
                Err(_) => Self {
                    data: fresh,
                    status: SaveStatus::Preserved,
                    path: None,
                },
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self {
                data: fresh,
                status: SaveStatus::Ready,
                path: Some(path),
            },
            Err(_) => Self {
                data: fresh,
                status: SaveStatus::Preserved,
                path: None,
            },
        }
    }

    /// In-memory store for tests and fallbacks.
    pub fn temporary() -> Self {
        Self::load(None)
    }

    fn write(&mut self, data: &SaveData) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        let ok = path
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .is_ok()
            && serde_json::to_string_pretty(data)
                .ok()
                .and_then(|json| {
                    let tmp = path.with_extension("json.tmp");
                    std::fs::write(&tmp, json).ok()?;
                    std::fs::rename(&tmp, path).ok()
                })
                .is_some();
        if !ok {
            self.status = SaveStatus::Temporary;
            self.path = None;
        }
        ok
    }

    pub fn can_save(&self) -> bool {
        self.status == SaveStatus::Ready
    }

    /// Writes the save before committing the purchase, like the web game.
    pub fn purchase(&mut self, item: ShopItem) -> PurchaseResult {
        if self.data.owned(item) >= item.max_stock() {
            return PurchaseResult::Maxed;
        }
        if self.data.cash < item.cost() {
            return PurchaseResult::NotEnoughCash;
        }
        if !self.can_save() {
            return PurchaseResult::Unavailable;
        }
        let mut next = self.data.clone();
        next.cash -= item.cost();
        next.add(item);
        if !self.write(&next) {
            return PurchaseResult::Unavailable;
        }
        self.data = next;
        PurchaseResult::Purchased
    }

    /// Hands the stocked ammo to a starting run: returns the extra rounds for
    /// `[revolver, shotgun]` and clears the stock. Like a purchase, the save is
    /// written first: if it cannot record the stock as spent, the run gets nothing
    /// and the stock stays, so a later launch never grants it twice.
    pub fn take_ammo_stock(&mut self) -> [u32; 2] {
        let rounds = Gun::ALL.map(|g| self.data.stocked_rounds(g));
        if rounds == [0, 0] || !self.can_save() {
            return [0, 0];
        }
        let mut next = self.data.clone();
        next.revolver_boxes = 0;
        next.shotgun_packs = 0;
        if !self.write(&next) {
            return [0, 0];
        }
        self.data = next;
        rounds
    }

    /// Credits a finished run once and records personal bests.
    pub fn settle(&mut self, run_cash: u64, time_secs: f32, kills: u32) -> Settlement {
        let mut next = self.data.clone();
        next.version = VERSION;
        next.cash = next.cash.saturating_add(run_cash);
        next.runs += 1;
        let new_best_time = time_secs > next.best_time_secs;
        let new_best_kills = kills > next.best_kills;
        if new_best_time {
            next.best_time_secs = time_secs;
        }
        if new_best_kills {
            next.best_kills = kills;
        }
        let saved = self.write(&next);
        self.data = next;
        Settlement {
            credited: run_cash,
            balance: self.data.cash,
            new_best_time,
            new_best_kills,
            saved,
        }
    }
}

fn default_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GRANNY3D_SAVE") {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("granny3d/save.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::data::{REVOLVER_AMMO_BOX, SHOTGUN_SHELL_PACK};

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "granny3d-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        dir.join("save.json")
    }

    #[test]
    fn settle_then_buy_cardio_persists() {
        let path = scratch();
        let mut s = SaveStore::load(Some(path.clone()));
        assert_eq!(s.status, SaveStatus::Ready);
        assert_eq!(s.purchase(ShopItem::Cardio), PurchaseResult::NotEnoughCash);
        let r = s.settle(60, 42.0, 7);
        assert!(r.saved && r.new_best_time && r.new_best_kills);
        assert_eq!(s.purchase(ShopItem::Cardio), PurchaseResult::Purchased);
        assert_eq!(s.purchase(ShopItem::Cardio), PurchaseResult::Maxed);
        let again = SaveStore::load(Some(path.clone()));
        assert_eq!(again.data.cash, 10);
        assert!(again.data.cardio);
        assert_eq!(again.data.best_kills, 7);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn unreadable_save_is_preserved() {
        let path = scratch();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        let mut s = SaveStore::load(Some(path.clone()));
        assert_eq!(s.status, SaveStatus::Preserved);
        let r = s.settle(25, 1.0, 1);
        assert!(!r.saved);
        assert_eq!(r.balance, 25);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn temporary_store_never_sells_cardio() {
        let mut s = SaveStore::temporary();
        s.settle(100, 1.0, 1);
        assert_eq!(s.purchase(ShopItem::Cardio), PurchaseResult::Unavailable);
        assert_eq!(
            s.purchase(ShopItem::RevolverAmmo),
            PurchaseResult::Unavailable
        );
        assert_eq!(s.data.cash, 100);
    }

    #[test]
    fn ammo_stocks_up_to_the_cap_and_is_spent_by_one_run() {
        let path = scratch();
        let mut s = SaveStore::load(Some(path.clone()));
        s.settle(1000, 1.0, 1);
        let max = ShopItem::RevolverAmmo.max_stock();
        for _ in 0..max {
            assert_eq!(
                s.purchase(ShopItem::RevolverAmmo),
                PurchaseResult::Purchased
            );
        }
        assert_eq!(s.purchase(ShopItem::RevolverAmmo), PurchaseResult::Maxed);
        assert_eq!(
            s.purchase(ShopItem::ShotgunShells),
            PurchaseResult::Purchased
        );
        let spent = max as u64 * ShopItem::RevolverAmmo.cost() + ShopItem::ShotgunShells.cost();
        assert_eq!(s.data.cash, 1000 - spent);

        let reloaded = SaveStore::load(Some(path.clone()));
        assert_eq!(reloaded.data.owned(ShopItem::RevolverAmmo), max);
        assert_eq!(reloaded.data.owned(ShopItem::ShotgunShells), 1);

        let rounds = s.take_ammo_stock();
        assert_eq!(rounds, [max * REVOLVER_AMMO_BOX, SHOTGUN_SHELL_PACK]);
        assert_eq!(s.take_ammo_stock(), [0, 0], "stock is spent once");
        let after = SaveStore::load(Some(path.clone()));
        assert_eq!(after.data.owned(ShopItem::RevolverAmmo), 0);
        assert_eq!(after.data.cash, 1000 - spent);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// A run must not get stocked ammo the save failed to mark as spent, or a later
    /// launch would load the same stock and grant it again.
    #[cfg(unix)]
    #[test]
    fn stock_is_kept_when_spending_it_cannot_be_saved() {
        use std::os::unix::fs::PermissionsExt;
        let path = scratch();
        let dir = path.parent().unwrap().to_path_buf();
        let mut s = SaveStore::load(Some(path.clone()));
        s.settle(100, 1.0, 1);
        assert_eq!(
            s.purchase(ShopItem::RevolverAmmo),
            PurchaseResult::Purchased
        );

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let writable = std::fs::write(dir.join("probe"), "").is_ok();
        let rounds = s.take_ammo_stock();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        if writable {
            // Running as root: permissions cannot make the write fail.
            std::fs::remove_dir_all(&dir).unwrap();
            return;
        }

        assert_eq!(rounds, [0, 0], "no ammo the save could not record as spent");
        assert_eq!(s.status, SaveStatus::Temporary);
        assert_eq!(
            s.data.owned(ShopItem::RevolverAmmo),
            1,
            "stock stays in memory"
        );
        assert_eq!(
            s.take_ammo_stock(),
            [0, 0],
            "still unsaved, still not granted"
        );
        let reloaded = SaveStore::load(Some(path.clone()));
        assert_eq!(
            reloaded.data.owned(ShopItem::RevolverAmmo),
            1,
            "stock stays on disk"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn older_saves_without_ammo_stock_still_load() {
        let path = scratch();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"version":3,"cash":40,"cardio":false}"#).unwrap();
        let s = SaveStore::load(Some(path.clone()));
        assert_eq!(s.status, SaveStatus::Ready);
        assert_eq!(s.data.owned(ShopItem::ShotgunShells), 0);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
