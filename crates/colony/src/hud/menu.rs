//! The menus (M10): the main menu at launch (over a world going on behind it), the pause menu
//! (Esc), a new world, saving and loading, and the app's settings.

use super::*;
use crate::saves::{AUTOSAVE, AppSettings, BEFORE, Power, SLOTS, SlotInfo};

/// Which menu is showing.
#[derive(Clone, Debug, PartialEq)]
pub enum Menu {
    /// At launch.
    Main,
    /// Esc during play.
    Pause,
    /// Making a new world: the seed as typed, the size, whether beauty matters, and the packs on.
    NewWorld { seed: String, size: u32, beauty: bool, packs: Vec<String> },
    /// Saving into a slot (true) or picking one to load (false).
    Slots { saving: bool },
    Settings,
}

/// Map sizes on offer, in tiles (a tile is a metre).
pub const SIZES: [(u32, &str); 3] = [(192, "Small"), (256, "Normal"), (320, "Large")];

impl Menu {
    pub fn new_world(seed: u64) -> Menu {
        Menu::NewWorld { seed: seed.to_string(), size: 256, beauty: true, packs: sim::pack::default_ids() }
    }
}

/// A seed nobody chose: from the clock.
pub fn random_seed() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64);
    (sim::hash::hash2(t, 0x5EED, 1) % 1_000_000) + 1
}

impl Hud {
    /// Keys typed while the new-world menu is open go to the seed.
    pub fn typing_seed(&mut self) -> Option<&mut String> {
        match self.menu.as_mut() {
            Some(Menu::NewWorld { seed, .. }) => Some(seed),
            _ => None,
        }
    }

    /// Esc in a menu: back one step. Returns false if there was nothing to go back from.
    pub fn menu_back(&mut self, in_game: bool) -> bool {
        match self.menu.take() {
            Some(Menu::Main) if !in_game => {
                self.menu = Some(Menu::Main);
                false
            }
            Some(Menu::Main) | Some(Menu::Pause) => true,
            Some(_) => {
                self.menu = Some(self.menu_home.clone());
                true
            }
            None => false,
        }
    }

    fn menu_home_or(&self) -> Menu {
        self.menu_home.clone()
    }

    pub(super) fn menu_view(&mut self, sim: &Sim, st: &Status, acts: &mut Vec<Action>) {
        let Some(menu) = self.menu.clone() else { return };
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        // The world dims behind the menu.
        let all = Rect::new(0.0, 0.0, sw, sh);
        ui.rect(all, hex_a("#120F13", if matches!(menu, Menu::Main) && !st.in_game { 0.35 } else { 0.55 }), 0.0);
        ui.block(all);
        match menu {
            Menu::Main | Menu::Pause => self.menu_list(sim, st, &menu, acts),
            Menu::NewWorld { seed, size, beauty, packs } => self.menu_new_world(seed, size, beauty, packs, acts),
            Menu::Slots { saving } => self.menu_slots(st, saving, acts),
            Menu::Settings => self.menu_settings(st, acts),
        }
    }

    /// A centred panel with a title; returns the panel and the y to start below the title.
    fn menu_panel(&mut self, title: &str, w: f32, h: f32) -> (Rect, f32) {
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let (w, h) = (ui.px(w), ui.px(h));
        let r = Rect::new(((sw - w) * 0.5).floor(), ((sh - h) * 0.5).floor(), w, h);
        ui.panel(r);
        ui.block(r);
        let pad = ui.px(22.0);
        ui.text(r.x + pad, r.y + pad, title, t(Font::Pixel, 24.0, TEXT));
        (r, r.y + pad + ui.px(44.0))
    }

    fn menu_list(&mut self, sim: &Sim, st: &Status, menu: &Menu, acts: &mut Vec<Action>) {
        let main = matches!(menu, Menu::Main);
        let latest = st.slots.iter().max_by_key(|s| s.saved_at).cloned();
        // (label, what it does, a line under it)
        let mut items: Vec<(String, Action, Option<String>)> = Vec::new();
        if main && !st.in_game {
            if let Some(s) = &latest {
                items.push(("Continue".into(), Action::Load(s.slot), Some(format!("{}: {} · {}", s.name(), s.title, s.age()))));
            }
            items.push(("New world".into(), Action::OpenMenu(Menu::new_world(random_seed())), Some("Pick a seed, a size, and the rules.".into())));
            items.push(("Play this one".into(), Action::CloseMenu, Some(format!("The world behind this menu (seed {}).", sim.seed))));
        } else {
            items.push(("Resume".into(), Action::CloseMenu, None));
            items.push(("Save".into(), Action::OpenMenu(Menu::Slots { saving: true }), Some(super::saves_describe(sim))));
            items.push(("New world".into(), Action::OpenMenu(Menu::new_world(random_seed())), None));
        }
        if !st.slots.is_empty() {
            items.push(("Load".into(), Action::OpenMenu(Menu::Slots { saving: false }), Some(format!("{} saved {}", st.slots.len(), if st.slots.len() == 1 { "world" } else { "worlds" }))));
        }
        items.push(("Settings".into(), Action::OpenMenu(Menu::Settings), None));
        items.push(("Quit".into(), Action::Quit, if st.in_game { Some("Your world is saved to the autosave first.".into()) } else { None }));
        let lines = items.iter().filter(|i| i.2.is_some()).count() as f32;
        let h = 66.0 + if main { 14.0 } else { 0.0 } + items.len() as f32 * 40.0 + lines * 16.0 + 16.0;
        let (r, mut y) = self.menu_panel(if main { "Colony" } else { "Paused" }, 360.0, h);
        let ui = &mut self.ui;
        let pad = ui.px(22.0);
        if main {
            ui.text(r.x + pad, y - ui.px(14.0), "Watch them live, help or meddle, and compare minds.", t(Font::Sans, 12.0, MUTED));
            y += ui.px(14.0);
        }
        for (label, act, under) in items {
            let b = Rect::new(r.x + pad, y, r.w - 2.0 * pad, ui.px(34.0));
            if ui.button(b, &label, false) {
                acts.push(act);
            }
            y += ui.px(40.0);
            if let Some(u) = under {
                ui.text(r.x + pad + ui.px(4.0), y - ui.px(4.0), &u, t(Font::Sans, 11.0, FAINT));
                y += ui.px(16.0);
            }
        }
    }

    fn menu_new_world(&mut self, seed: String, size: u32, beauty: bool, packs: Vec<String>, acts: &mut Vec<Action>) {
        let rows = sim::pack::PACKS.len().max(1) as f32;
        let (r, mut y) = self.menu_panel("A new world", 500.0, 336.0 + (rows - 1.0) * 40.0 + 18.0);
        let home = self.menu_home_or();
        let mut next = Menu::NewWorld { seed: seed.clone(), size, beauty, packs: packs.clone() };
        let ui = &mut self.ui;
        let pad = ui.px(22.0);
        let label = t(Font::Sans, 13.0, TEXT);
        let note = t(Font::Sans, 11.5, MUTED);
        let bh = ui.px(28.0);
        let right = r.right() - pad;
        // Seed: typed (digits), or rolled.
        ui.text(r.x + pad, y + ui.px(5.0), "Seed", label);
        let rw = ui.button_width("Roll");
        if ui.button(Rect::new(right - rw, y, rw, bh), "Roll", false) {
            next = Menu::NewWorld { seed: random_seed().to_string(), size, beauty, packs: packs.clone() };
        }
        let field = Rect::new(r.x + pad + ui.px(90.0), y, right - rw - ui.px(8.0) - (r.x + pad + ui.px(90.0)), bh);
        ui.rect(field, hex_a("#000000", 0.25), ui.px(6.0));
        ui.outline(field, GOLD, ui.px(6.0));
        let shown = format!("{seed}_");
        ui.text(field.x + ui.px(10.0), field.y + ui.px(6.0), &shown, t(Font::Sans, 13.0, TEXT));
        y += bh + ui.px(6.0);
        ui.text(r.x + pad + ui.px(90.0), y, "Type a number: the same seed makes the same world.", note);
        y += ui.px(30.0);
        // Size.
        ui.text(r.x + pad, y + ui.px(5.0), "Size", label);
        let mut bx = r.x + pad + ui.px(90.0);
        for (n, name) in SIZES {
            let text = format!("{name} ({n} m)");
            let w = ui.button_width(&text);
            if ui.button(Rect::new(bx, y, w, bh), &text, size == n) {
                next = Menu::NewWorld { seed: seed.clone(), size: n, beauty, packs: packs.clone() };
            }
            bx += w + ui.px(6.0);
        }
        y += bh + ui.px(18.0);
        // Rules.
        ui.text(r.x + pad, y + ui.px(5.0), "Beauty", label);
        let bx = r.x + pad + ui.px(90.0);
        for (on, text) in [(true, "Matters"), (false, "Doesn't matter")] {
            let w = ui.button_width(text);
            let off = if on { 0.0 } else { ui.button_width("Matters") + ui.px(6.0) };
            if ui.button(Rect::new(bx + off, y, w, bh), text, beauty == on) {
                next = Menu::NewWorld { seed: seed.clone(), size, beauty: on, packs: packs.clone() };
            }
        }
        y += bh + ui.px(6.0);
        let why = if beauty { "Lovely places lift people's mood and draw their walks." } else { "Beauty has no effect: do they still make it?" };
        ui.text(bx, y, why, note);
        y += ui.px(30.0);
        // Packs: each compiled-in expansion, on or off for this world.
        ui.text(r.x + pad, y + ui.px(5.0), "Packs", label);
        for p in sim::pack::PACKS {
            let on = packs.iter().any(|i| i == p.id());
            let text = format!("{}: {}", p.name(), if on { "on" } else { "off" });
            let w = ui.button_width(&text);
            if ui.button(Rect::new(bx, y, w, bh), &text, on) {
                let mut list = packs.clone();
                if on {
                    list.retain(|i| i != p.id());
                } else {
                    list.push(p.id().to_string());
                }
                next = Menu::NewWorld { seed: seed.clone(), size, beauty, packs: list };
            }
            ui.text(bx, y + bh + ui.px(4.0), p.about(), note);
            y += bh + ui.px(26.0);
        }
        y += ui.px(16.0);
        // Start, or back.
        let parsed = seed.trim().parse::<u64>().ok().filter(|s| *s > 0);
        let sw = ui.button_width("Start");
        let back = "Back";
        let bw = ui.button_width(back);
        if ui.button(Rect::new(right - sw, y, sw, ui.px(32.0)), "Start", parsed.is_some())
            && let Some(seed) = parsed
        {
            acts.push(Action::NewWorld { seed, size, beauty, packs: packs.clone() });
        }
        if ui.button(Rect::new(right - sw - ui.px(8.0) - bw, y, bw, ui.px(32.0)), back, false) {
            next = home;
        }
        if parsed.is_none() {
            ui.text(r.x + pad, y + ui.px(8.0), "The seed must be a whole number above 0.", t(Font::Sans, 11.5, K_CONFLICT));
        }
        self.menu = Some(next);
    }

    fn menu_slots(&mut self, st: &Status, saving: bool, acts: &mut Vec<Action>) {
        let before = !saving && st.slots.iter().any(|s| s.slot == BEFORE);
        let list: Vec<u32> = if saving { (1..=SLOTS).collect() } else { (AUTOSAVE..=SLOTS).chain(before.then_some(BEFORE)).collect() };
        let note = !saving && st.in_game;
        let h = 66.0 + list.len() as f32 * 50.0 + 56.0 + if note { 18.0 } else { 0.0 };
        let (r, mut y) = self.menu_panel(if saving { "Save the world" } else { "Load a world" }, 480.0, h);
        let ui = &mut self.ui;
        let pad = ui.px(22.0);
        let mut back = false;
        if note {
            ui.text(r.x + pad, y - ui.px(10.0), "The world you're playing is kept in \"Your previous world\".", t(Font::Sans, 11.5, MUTED));
            y += ui.px(18.0);
        }
        for slot in list {
            let info: Option<&SlotInfo> = st.slots.iter().find(|s| s.slot == slot);
            let name = match slot {
                AUTOSAVE => "Autosave".to_string(),
                BEFORE => "Your previous world".to_string(),
                n => format!("Slot {n}"),
            };
            ui.text(r.x + pad, y + ui.px(2.0), &name, t(Font::Sans, 13.0, TEXT));
            match info {
                Some(i) => ui.text(r.x + pad, y + ui.px(20.0), &format!("{} · seed {} · {}", i.title, i.seed, i.age()), t(Font::Sans, 11.5, MUTED)),
                None => ui.text(r.x + pad, y + ui.px(20.0), "Empty", t(Font::Sans, 11.5, FAINT)),
            };
            let label = if saving { if info.is_some() { "Save over" } else { "Save here" } } else { "Load" };
            let w = ui.button_width(label);
            let b = Rect::new(r.right() - pad - w, y + ui.px(4.0), w, ui.px(28.0));
            if (saving || info.is_some()) && ui.button(b, label, false) {
                acts.push(if saving { Action::Save(slot) } else { Action::Load(slot) });
            }
            y += ui.px(50.0);
        }
        let bw = ui.button_width("Back");
        if ui.button(Rect::new(r.right() - pad - bw, r.bottom() - pad - ui.px(30.0), bw, ui.px(30.0)), "Back", false) {
            back = true;
        }
        ui.text(r.x + pad, r.bottom() - pad - ui.px(22.0), "Saves are kept in your Application Support folder.", t(Font::Sans, 11.0, FAINT));
        if back {
            self.menu = Some(self.menu_home_or());
        }
    }

    fn menu_settings(&mut self, st: &Status, acts: &mut Vec<Action>) {
        let (r, mut y) = self.menu_panel("Settings", 480.0, 300.0);
        let ui = &mut self.ui;
        let pad = ui.px(22.0);
        let label = t(Font::Sans, 13.0, TEXT);
        let note = t(Font::Sans, 11.5, MUTED);
        let bh = ui.px(28.0);
        let cur = st.app.clone();
        let x0 = r.x + pad + ui.px(132.0);
        let row = |ui: &mut Ui, y: f32, name: &str, opts: &[(&str, bool)]| -> Option<usize> {
            ui.text(r.x + pad, y + ui.px(5.0), name, label);
            let mut bx = x0;
            let mut hit = None;
            for (k, (text, on)) in opts.iter().enumerate() {
                let w = ui.button_width(text);
                if ui.button(Rect::new(bx, y, w, bh), text, *on) {
                    hit = Some(k);
                }
                bx += w + ui.px(6.0);
            }
            hit
        };
        let mut next = cur.clone();
        if let Some(k) = row(ui, y, "Power", &[("Normal", cur.power == Power::Normal), ("Lower power", cur.power == Power::Lower)]) {
            next.power = if k == 0 { Power::Normal } else { Power::Lower };
        }
        y += bh + ui.px(6.0);
        let why = match cur.power {
            Power::Normal => "Shadows, and as many frames as the display shows.",
            Power::Lower => "No shadows, at most 60 frames a second, and the rules brain only.",
        };
        ui.text(x0, y, why, note);
        y += ui.px(32.0);
        let scales = [(0.85, "Small"), (1.0, "Normal"), (1.2, "Large")];
        let opts: Vec<(&str, bool)> = scales.iter().map(|(s, n)| (*n, (cur.ui_scale - s).abs() < 0.01)).collect();
        if let Some(k) = row(ui, y, "Text and panels", &opts) {
            next.ui_scale = scales[k].0;
        }
        y += bh + ui.px(26.0);
        if let Some(k) = row(ui, y, "Window", &[("Windowed", !cur.fullscreen), ("Full screen", cur.fullscreen)]) {
            next.fullscreen = k == 1;
        }
        y += bh + ui.px(6.0);
        ui.text(x0, y, "F toggles it at any time.", note);
        if next != cur {
            acts.push(Action::Settings(next));
        }
        let bw = ui.button_width("Back");
        if ui.button(Rect::new(r.right() - pad - bw, r.bottom() - pad - ui.px(30.0), bw, ui.px(30.0)), "Back", false) {
            self.menu = Some(self.menu_home_or());
        }
        let _ = AppSettings::default;
    }
}
