//! The player's saves and settings (M10): five save slots and an autosave, each a world file
//! with a small description beside it, and the app's own settings. They live in the user's
//! application data folder (on a Mac, ~/Library/Application Support/Colony), never in the
//! repository; `COLONY_DATA` points them elsewhere (tests and tools use that, so they never touch
//! the player's saves).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Manual slots, numbered from 1. Slot 0 is the autosave; slot 6 keeps the world you were
/// playing when you loaded another or started a new one ("Your previous world"), so neither
/// ever loses it.
pub const SLOTS: u32 = 5;
pub const AUTOSAVE: u32 = 0;
pub const BEFORE: u32 = 6;

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("COLONY_DATA") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Application Support/Colony")
    } else {
        PathBuf::from(home).join(".colony")
    }
}

fn save_path(slot: u32) -> PathBuf {
    let name = match slot {
        AUTOSAVE => "autosave".to_string(),
        BEFORE => "before-load".to_string(),
        n => format!("slot{n}"),
    };
    data_dir().join("saves").join(format!("{name}.colony"))
}

fn meta_path(slot: u32) -> PathBuf {
    save_path(slot).with_extension("json")
}

/// What a slot holds, shown in the menu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlotInfo {
    pub slot: u32,
    /// "Year 2, autumn day 4 · 17 people"
    pub title: String,
    pub seed: u64,
    pub tick: u64,
    /// Seconds since 1970 when it was saved.
    pub saved_at: u64,
}

impl SlotInfo {
    pub fn name(&self) -> String {
        match self.slot {
            AUTOSAVE => "Autosave".into(),
            BEFORE => "Your previous world".into(),
            n => format!("Slot {n}"),
        }
    }

    /// "5 minutes ago", "yesterday", "3 days ago".
    pub fn age(&self) -> String {
        let now = now();
        let s = now.saturating_sub(self.saved_at);
        match s {
            0..60 => "just now".into(),
            60..3600 => format!("{} min ago", s / 60),
            3600..86_400 => format!("{} h ago", s / 3600),
            86_400..172_800 => "yesterday".into(),
            _ => format!("{} days ago", s / 86_400),
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Every slot that holds a world, the autosave first.
pub fn list() -> Vec<SlotInfo> {
    (0..=BEFORE)
        .filter_map(|slot| {
            let text = std::fs::read_to_string(meta_path(slot)).ok()?;
            let info: SlotInfo = serde_json::from_str(&text).ok()?;
            save_path(slot).exists().then_some(info)
        })
        .collect()
}

/// Describe a world for its slot.
pub fn describe(sim: &sim::Sim) -> String {
    let cal = &sim.content.calendar;
    let t = sim.tick;
    format!("Year {}, {} day {} · {} people", cal.year(t), cal.season(t), cal.day_of_season(t), sim.alive())
}

/// Save into a slot: the world on a background thread, its description at once.
pub fn save(sim: &sim::Sim, slot: u32) -> std::io::Result<sim::save::SaveJob> {
    std::fs::create_dir_all(data_dir().join("saves"))?;
    let info = SlotInfo { slot, title: describe(sim), seed: sim.seed, tick: sim.tick, saved_at: now() };
    let job = sim::save::save_background(sim, &save_path(slot));
    std::fs::write(meta_path(slot), serde_json::to_vec_pretty(&info).expect("slot info serialises"))?;
    Ok(job)
}

pub fn load(slot: u32) -> Result<sim::Sim, String> {
    sim::save::load_from(&save_path(slot)).map_err(|e| e.to_string())
}

/// How hard the machine works (ARCHITECTURE.md section 14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Power {
    /// Shadows, the display's full frame rate.
    #[default]
    Normal,
    /// No shadows, at most 60 frames a second, and the rules brain (no servers).
    Lower,
}

/// The app's settings (not the world's: those are saved with the world).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub power: Power,
    /// UI size: 0.85, 1 or 1.2 times the system's.
    #[serde(default = "one")]
    pub ui_scale: f32,
    #[serde(default)]
    pub fullscreen: bool,
}

fn one() -> f32 {
    1.0
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings { power: Power::Normal, ui_scale: 1.0, fullscreen: false }
    }
}

impl AppSettings {
    pub fn load() -> AppSettings {
        std::fs::read_to_string(data_dir().join("settings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn store(&self) {
        let dir = data_dir();
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join("settings.json"), serde_json::to_vec_pretty(self).expect("settings serialise"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_save_list_and_load() {
        // A scratch data folder: tests never touch the player's saves.
        let dir = std::env::temp_dir().join(format!("colony-saves-test-{}", std::process::id()));
        // SAFETY: this test is the only one in the crate that reads or sets COLONY_DATA.
        unsafe { std::env::set_var("COLONY_DATA", &dir) };
        let mut sim = sim::Sim::new(4, 256);
        sim.run_ticks(200);
        save(&sim, 2).unwrap().wait().unwrap();
        save(&sim, AUTOSAVE).unwrap().wait().unwrap();
        let got = list();
        assert_eq!(got.iter().map(|s| s.slot).collect::<Vec<_>>(), vec![AUTOSAVE, 2]);
        assert_eq!(got[1].seed, 4);
        assert!(got[1].title.contains("people"), "{}", got[1].title);
        let back = load(2).unwrap();
        assert_eq!(back.state_hash(), sim.state_hash());
        assert!(load(3).is_err());
        // Settings round-trip too.
        let s = AppSettings { power: Power::Lower, ui_scale: 1.2, fullscreen: true };
        s.store();
        assert_eq!(AppSettings::load(), s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
