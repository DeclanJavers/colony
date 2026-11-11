//! The game's UI (M4), laid out like the M0 mockup: top bar, story feed, lenses, the inspector
//! with its tabs, a hover card, and the dev trace view. Built fresh each frame with the ui crate.

use sim::actions::air_temp;
use sim::brain::{Ending, Trace};
use sim::people::{Person, Sex, TRAITS};
use sim::{Event, PersonId, Sim};
use std::collections::VecDeque;
use ui::style::*;
use ui::{Font, Rect, Text, Ui};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Mind,
    Life,
    Body,
    Skills,
    Log,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Detail {
    Minimal,
    Normal,
    Analyst,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lens {
    Off,
    Traffic,
    Mood,
    Needs,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FeedKind {
    Social,
    Trouble,
    Knowledge,
    World,
    Life,
}

impl FeedKind {
    fn colour(self) -> [f32; 4] {
        match self {
            FeedKind::Social => K_SOCIAL,
            FeedKind::Trouble => K_CONFLICT,
            FeedKind::Knowledge => K_KNOWLEDGE,
            FeedKind::World => K_WORLD,
            FeedKind::Life => K_LIFE,
        }
    }
    fn label(self) -> &'static str {
        match self {
            FeedKind::Social => "Social",
            FeedKind::Trouble => "Trouble",
            FeedKind::Knowledge => "Knowledge",
            FeedKind::World => "World",
            FeedKind::Life => "Life",
        }
    }
}

struct FeedItem {
    kind: FeedKind,
    text: String,
    tick: u64,
    person: Option<PersonId>,
    /// For grouping repeats ("3 people are hungry").
    group: Option<(String, Vec<String>)>,
}

/// What the player asked for this frame.
pub enum Action {
    Select(Option<PersonId>),
    Speed(f32),
    TogglePause,
    PauseOnNextDecision(PersonId),
}

/// The world's state the HUD shows but doesn't own.
pub struct Status {
    pub paused: bool,
    pub speed: f32,
    pub fps: f32,
    pub frame_ms: f32,
    pub selected: Option<PersonId>,
    pub hovered: Option<PersonId>,
    pub pause_on: Option<PersonId>,
}

pub struct Hud {
    pub ui: Ui,
    pub tab: Tab,
    pub detail: Detail,
    pub dev: bool,
    pub lens: Lens,
    /// The decision trace being viewed: (person, index into their traces, counting back from the newest).
    pub trace: Option<(PersonId, usize)>,
    feed: VecDeque<FeedItem>,
    log_offset: usize,
    /// Trace view scrolling: (lines of the prompt text, options).
    trace_scroll: (usize, usize),
}

fn t(font: Font, size: f32, colour: [f32; 4]) -> Text {
    Text { font, size, colour }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn clock(sim: &Sim, tick: u64) -> String {
    let m = sim.content.calendar.minute_of_day(tick);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// When something happened: the time today, or "yesterday 21:45", or "day 4, 21:45".
fn when(sim: &Sim, tick: u64) -> String {
    let cal = &sim.content.calendar;
    let (today, then) = (cal.day_index(sim.tick), cal.day_index(tick));
    match today.saturating_sub(then) {
        0 => clock(sim, tick),
        1 => format!("yesterday {}", clock(sim, tick)),
        _ => format!("{} day {}, {}", cal.season(tick), cal.day_of_season(tick), clock(sim, tick)),
    }
}

/// Someone's decisions, newest first: the one under way (if any), then the finished ones.
pub fn decisions(p: &Person) -> Vec<&Trace> {
    p.open_trace.iter().chain(p.traces.iter().rev()).collect()
}

impl Hud {
    pub fn new(assets: &std::path::Path) -> Result<Hud, String> {
        let read = |f: &str| std::fs::read(assets.join("fonts").join(f)).map_err(|e| format!("{f}: {e}"));
        Ok(Hud {
            ui: Ui::new(&read("PixelifySans.ttf")?, &read("Inter.ttf")?)?,
            tab: Tab::Mind,
            detail: Detail::Normal,
            dev: true,
            lens: Lens::Off,
            trace: None,
            feed: VecDeque::new(),
            log_offset: 0,
            trace_scroll: (0, 0),
        })
    }

    /// Turn sim events into story feed lines.
    pub fn take_events(&mut self, sim: &mut Sim) {
        for (tick, e) in sim.take_timed_events() {
            let item = match e {
                Event::FireLit { person, name } => Some(FeedItem { kind: FeedKind::World, text: format!("{name} lit the fire"), tick, person: Some(person), group: None }),
                Event::FireOut => Some(FeedItem { kind: FeedKind::World, text: "The fire went out".into(), tick, person: None, group: None }),
                Event::SkillUp { person, name, skill, level } => Some(FeedItem {
                    kind: FeedKind::Knowledge,
                    text: format!("{name} got better at {skill} (level {level})"),
                    tick,
                    person: Some(person),
                    group: Some((format!("better at {skill}"), vec![name])),
                }),
                Event::NeedCritical { person, name, word, .. } => Some(FeedItem {
                    kind: FeedKind::Trouble,
                    text: format!("{name} is {word}"),
                    tick,
                    person: Some(person),
                    group: Some((word, vec![name])),
                }),
                Event::PersonDied { person, name, cause } => Some(FeedItem { kind: FeedKind::Life, text: format!("{name} died of {cause}"), tick, person: Some(person), group: None }),
                Event::SeasonStarted { season, year } => Some(FeedItem { kind: FeedKind::World, text: format!("{} came (year {year})", capital(&season)), tick, person: None, group: None }),
                _ => None,
            };
            let Some(item) = item else { continue };
            // Group the same thing happening to several people within a few hours.
            let window = 180 / sim.content.calendar.minutes_per_tick as u64;
            if let Some((key, names)) = &item.group
                && let Some(last) = self.feed.iter_mut().find(|f| f.tick + window >= item.tick && f.group.as_ref().is_some_and(|g| &g.0 == key))
            {
                let g = last.group.as_mut().unwrap();
                if !g.1.contains(&names[0]) {
                    g.1.push(names[0].clone());
                    let n = g.1.len();
                    last.text = if key.starts_with("better at") {
                        format!("{n} people got {key}")
                    } else {
                        format!("{n} people are {key}")
                    };
                    last.person = None;
                }
                last.tick = item.tick;
                continue;
            }
            self.feed.push_front(item);
            self.feed.truncate(60);
        }
    }

    /// Lay out the whole UI. Returns what the player asked for.
    pub fn frame(&mut self, sim: &Sim, st: &Status) -> Vec<Action> {
        let mut acts = Vec::new();
        if self.ui.input.scroll != 0.0 && self.tab == Tab::Log {
            let d = if self.ui.input.scroll > 0.0 { -1 } else { 1 };
            self.log_offset = (self.log_offset as i64 + d).max(0) as usize;
        }
        self.top_bar(sim, st, &mut acts);
        if let Some((pid, k)) = self.trace {
            self.trace_view(sim, pid, k, st, &mut acts);
            return acts;
        }
        self.story(sim, &mut acts);
        self.lenses(sim);
        if let Some(id) = st.selected {
            self.inspector(sim, id, st, &mut acts);
        }
        if let Some(h) = st.hovered.filter(|h| Some(*h) != st.selected)
            && !self.ui.over_ui()
        {
            self.hover_card(sim, h, st.selected.is_some());
        }
        self.toolbar();
        acts
    }

    // -----------------------------------------------------------------------------------------

    fn top_bar(&mut self, sim: &Sim, st: &Status, acts: &mut Vec<Action>) {
        let ui = &mut self.ui;
        let (w, _) = ui.size;
        let h = ui.px(44.0);
        let r = Rect::new(0.0, 0.0, w, h);
        ui.rect(r, PANEL, 0.0);
        ui.rect(Rect::new(0.0, h - 1.0, w, 1.0), BORDER, 0.0);
        ui.block(r);
        let pad = ui.px(14.0);
        let mid = |ui: &Ui, tt: Text| (h - ui.line_height(tt)) * 0.5;
        let title = t(Font::Pixel, 16.0, GOLD);
        let mut x = pad;
        x += ui.text(x, mid(ui, title), "Colony", title) + pad;
        ui.rect(Rect::new(x, h * 0.25, 1.0, h * 0.5), BORDER, 0.0);
        x += pad;
        let body = t(Font::Sans, 12.5, TEXT);
        let muted = t(Font::Sans, 12.5, MUTED);
        let cal = &sim.content.calendar;
        let tick = sim.tick;
        x += ui.text(x, mid(ui, t(Font::Pixel, 16.0, TEXT)), "The camp", t(Font::Pixel, 16.0, TEXT)) + ui.px(10.0);
        x += ui.text(x, mid(ui, muted), &format!("{} people · Era 1, foraging", sim.alive()), muted) + pad;
        ui.rect(Rect::new(x, h * 0.25, 1.0, h * 0.5), BORDER, 0.0);
        x += pad;
        let date = format!("{} · day {} · year {}", capital(cal.season(tick)), cal.day_of_season(tick), cal.year(tick));
        x += ui.text(x, mid(ui, body), &date, body) + ui.px(10.0);
        let clock_t = t(Font::Sans, 14.0, TEXT);
        x += ui.text(x, mid(ui, clock_t), &clock(sim, tick), clock_t) + ui.px(10.0);
        let (sx, sy) = sim.camp.stores_at;
        let temp = air_temp(&sim.content, &sim.map, sx as u32, sy as u32, tick);
        let fire = if sim.camp.fire_lit { format!("fire lit, {:.0} h of fuel", sim.camp.fire_hours) } else { "fire out".into() };
        let weather = format!("{temp:.0} C, {fire}");
        let ww = ui.text(x, mid(ui, muted), &weather, muted);
        let wr = Rect::new(x, 0.0, ww, h);
        if ui.hovered(wr) {
            ui.tooltip(vec!["Air temperature at the camp.".into(), "People get cold below 10 C unless they're by a lit fire.".into()]);
        }
        x += ww + pad;
        // Speed
        let bh = ui.px(28.0);
        let by = (h - bh) * 0.5;
        for (label, speed) in [("II", 0.0), ("1x", 1.0), ("3x", 3.0), ("10x", 10.0)] {
            let bw = ui.button_width(label).max(ui.px(36.0));
            let on = if speed == 0.0 { st.paused } else { !st.paused && st.speed == speed };
            if ui.button(Rect::new(x, by, bw, bh), label, on) {
                acts.push(if speed == 0.0 { Action::TogglePause } else { Action::Speed(speed) });
            }
            x += bw + ui.px(4.0);
        }
        // Right side: toggles and fps.
        let mut rx = w - pad;
        let fps = format!("{:.0} fps", st.fps);
        let faint = t(Font::Sans, 11.5, FAINT);
        let fw = ui.measure(&fps, faint);
        rx -= fw;
        ui.text(rx, mid(ui, faint), &fps, faint);
        let fr = Rect::new(rx, 0.0, fw, h);
        if ui.hovered(fr) {
            ui.tooltip(vec![format!("{:.1} ms per frame", st.frame_ms)]);
        }
        rx -= pad;
        let dev_label = if self.dev { "Dev: on" } else { "Dev: off" };
        let dw = ui.button_width(dev_label);
        rx -= dw;
        if ui.button(Rect::new(rx, by, dw, bh), dev_label, self.dev) {
            self.dev = !self.dev;
        }
        if ui.hovered(Rect::new(rx, by, dw, bh)) {
            ui.tooltip(vec!["Developer view: brains and decision traces.".into()]);
        }
        rx -= ui.px(6.0);
        let det = match self.detail {
            Detail::Minimal => "Detail: minimal",
            Detail::Normal => "Detail: normal",
            Detail::Analyst => "Detail: analyst",
        };
        let detw = ui.button_width(det);
        rx -= detw;
        if ui.button(Rect::new(rx, by, detw, bh), det, false) {
            self.detail = match self.detail {
                Detail::Minimal => Detail::Normal,
                Detail::Normal => Detail::Analyst,
                Detail::Analyst => Detail::Minimal,
            };
        }
        if ui.hovered(Rect::new(rx, by, detw, bh)) {
            ui.tooltip(vec!["Minimal: words only. Normal: words and reasons.".into(), "Analyst: the numbers too.".into()]);
        }
        let _ = x;
    }

    fn story(&mut self, sim: &Sim, acts: &mut Vec<Action>) {
        let ui = &mut self.ui;
        let (x, y) = (ui.px(16.0), ui.px(44.0) + ui.px(16.0));
        let w = ui.px(300.0);
        let pad = ui.px(12.0);
        let head = t(Font::Pixel, 16.0, TEXT);
        let item_t = t(Font::Sans, 12.5, TEXT);
        let when_t = t(Font::Sans, 11.0, FAINT);
        let lh = ui.line_height(item_t);
        let shown: Vec<usize> = (0..self.feed.len().min(8)).collect();
        let mut hgt = pad * 2.0 + ui.line_height(head) + ui.px(6.0);
        for &k in &shown {
            hgt += ui.wrapped_height(w - pad * 2.0 - ui.px(18.0), &self.feed[k].text, item_t) + ui.line_height(when_t) + ui.px(6.0);
        }
        if shown.is_empty() {
            hgt += lh;
        }
        hgt += ui.px(30.0);
        let panel = Rect::new(x, y, w, hgt);
        ui.panel(panel);
        let mut yy = y + pad;
        ui.text(x + pad, yy, "Story", head);
        let today = t(Font::Sans, 11.0, MUTED);
        let tw = ui.measure("newest first", today);
        ui.text(x + w - pad - tw, yy + ui.px(3.0), "newest first", today);
        yy += ui.line_height(head) + ui.px(6.0);
        if shown.is_empty() {
            ui.text(x + pad, yy, "Nothing yet. Things that happen appear here.", t(Font::Sans, 12.0, FAINT));
        }
        for &k in &shown {
            let it = &self.feed[k];
            let tw = w - pad * 2.0 - ui.px(18.0);
            let th = ui.wrapped_height(tw, &it.text, item_t) + ui.line_height(when_t);
            let row = Rect::new(x + ui.px(4.0), yy - ui.px(3.0), w - ui.px(8.0), th + ui.px(6.0));
            if it.person.is_some() && ui.hovered(row) {
                ui.rect(row, CHIP, ui.px(6.0));
            }
            let dot = ui.px(8.0);
            ui.rect(Rect::new(x + pad, yy + ui.px(5.0), dot, dot), it.kind.colour(), dot * 0.5);
            ui.wrapped(x + pad + ui.px(18.0), yy, tw, &it.text, item_t);
            let used = ui.wrapped_height(tw, &it.text, item_t);
            ui.text(x + pad + ui.px(18.0), yy + used, &when(sim, it.tick), when_t);
            if let Some(p) = it.person
                && ui.clicked(row)
            {
                acts.push(Action::Select(Some(p)));
            }
            yy += th + ui.px(6.0);
        }
        // Legend
        let mut lx = x + pad;
        let ly = panel.bottom() - pad - ui.px(12.0);
        ui.rect(Rect::new(x + pad, ly - ui.px(8.0), w - 2.0 * pad, 1.0), BORDER, 0.0);
        for k in [FeedKind::Social, FeedKind::Trouble, FeedKind::Knowledge, FeedKind::World, FeedKind::Life] {
            let lt = t(Font::Sans, 11.0, MUTED);
            let d = ui.px(7.0);
            ui.rect(Rect::new(lx, ly + ui.px(4.0), d, d), k.colour(), d * 0.5);
            lx += d + ui.px(4.0);
            lx += ui.text(lx, ly, k.label(), lt) + ui.px(9.0);
        }
    }

    fn lenses(&mut self, sim: &Sim) {
        let ui = &mut self.ui;
        let mut x = ui.px(16.0) + ui.px(300.0) + ui.px(16.0);
        let y = ui.px(44.0) + ui.px(16.0);
        let bh = ui.px(28.0);
        let chip = t(Font::Sans, 12.0, MUTED);
        let cw = ui.measure("Lenses", chip) + ui.px(16.0);
        let cr = Rect::new(x, y, cw, bh);
        ui.rect(cr, PANEL, ui.px(14.0));
        ui.text(x + ui.px(8.0), y + (bh - ui.line_height(chip)) * 0.5, "Lenses", chip);
        ui.block(cr);
        x += cw + ui.px(6.0);
        for (label, lens) in [("Off", Lens::Off), ("Traffic", Lens::Traffic), ("Mood", Lens::Mood), ("Needs", Lens::Needs)] {
            let bw = ui.button_width(label);
            if ui.button(Rect::new(x, y, bw, bh), label, self.lens == lens) {
                self.lens = lens;
            }
            x += bw + ui.px(4.0);
        }
        type Legend<'a> = Option<(&'a str, &'a str, Vec<([f32; 4], &'a str)>)>;
        let legend: Legend = match self.lens {
            Lens::Off => None,
            Lens::Traffic => Some(("Traffic lens", "Footsteps worn into the ground", vec![(hex_c("#5C3A22"), "few"), (hex_c("#FFB040"), "many")])),
            Lens::Mood => Some(("Mood lens", "How each person feels", vec![(GOOD, "content"), (TEXT, "okay"), (GOLD, "uneasy"), (BAD, "unhappy")])),
            Lens::Needs => Some(("Needs lens", "Each person's most pressing need", vec![(BODY_NEED, "fine"), (hex_c("#7FB8E6"), "thirst"), (WARN, "hunger"), (MIND_NEED, "rest"), (BAD, "cold")])),
        };
        if let Some((title, sub, keys)) = legend {
            let lx = ui.px(16.0) + ui.px(300.0) + ui.px(16.0);
            let ly = y + bh + ui.px(8.0);
            let pad = ui.px(10.0);
            let w = ui.px(270.0);
            let h = ui.px(78.0);
            ui.panel(Rect::new(lx, ly, w, h));
            ui.text(lx + pad, ly + pad, title, t(Font::Pixel, 16.0, TEXT));
            ui.text(lx + pad, ly + pad + ui.px(22.0), sub, t(Font::Sans, 11.5, MUTED));
            let mut kx = lx + pad;
            let ky = ly + pad + ui.px(42.0);
            for (c, label) in keys {
                let d = ui.px(9.0);
                ui.rect(Rect::new(kx, ky + ui.px(3.0), d, d), c, d * 0.5);
                kx += d + ui.px(4.0);
                kx += ui.text(kx, ky, label, t(Font::Sans, 11.5, TEXT)) + ui.px(10.0);
            }
            if self.lens == Lens::Traffic {
                let worn = sim.map.footfall.iter().filter(|f| **f > sim::path::TRAIL_STEPS * 0.45).count();
                let s = format!("{worn} tiles worn into trails");
                ui.text(lx + pad + ui.px(150.0), ly + pad + ui.px(2.0), &s, t(Font::Sans, 11.0, FAINT));
            }
        }
    }

    fn toolbar(&mut self) {
        let ui = &mut self.ui;
        let (w, h) = ui.size;
        let text = "God tools arrive later: speak (M8), gifts (M10)";
        let tt = t(Font::Sans, 11.5, FAINT);
        let tw = ui.measure(text, tt) + ui.px(28.0);
        let bh = ui.px(32.0);
        let r = Rect::new((w - tw) * 0.5, h - bh - ui.px(16.0), tw, bh);
        ui.panel(r);
        ui.text(r.x + ui.px(14.0), r.y + (bh - ui.line_height(tt)) * 0.5, text, tt);
    }

    fn hover_card(&mut self, sim: &Sim, id: PersonId, inspecting: bool) {
        let Some(p) = sim.people.get(id) else { return };
        let c = &sim.content;
        let ui = &mut self.ui;
        let (cx, cy) = ui.input.cursor;
        let pad = ui.px(10.0);
        let doing_probe = if p.asleep { "Asleep".to_string() } else { p.action.as_ref().map_or("Thinking".to_string(), |a| a.label.clone()) };
        let w = (ui.measure(&doing_probe, t(Font::Sans, 12.5, TEXT)) + 2.0 * pad + ui.px(2.0)).clamp(ui.px(180.0), ui.px(300.0));
        let doing = if p.asleep { "Asleep".to_string() } else { p.action.as_ref().map_or("Thinking".to_string(), |a| a.label.clone()) };
        let body = t(Font::Sans, 12.5, TEXT);
        let dh = ui.wrapped_height(w - 2.0 * pad, &doing, body);
        let h = pad * 2.0 + ui.px(20.0) + dh + ui.px(36.0);
        let mut r = Rect::new(cx + ui.px(18.0), cy + ui.px(18.0), w, h);
        // Keep clear of the inspector on the right.
        let limit = if inspecting { ui.size.0 - ui.px(432.0) } else { ui.size.0 };
        if r.right() > limit {
            r.x = cx - w - ui.px(18.0);
        }
        if r.bottom() > ui.size.1 {
            r.y = cy - h - ui.px(18.0);
        }
        ui.panel(r);
        let mut y = r.y + pad;
        let nw = ui.text(r.x + pad, y, &p.name, t(Font::Pixel, 16.0, TEXT));
        ui.text(r.x + pad + nw + ui.px(8.0), y + ui.px(3.0), &format!("{} · {}", p.age_years(sim.tick, c) as u32, role(p, c)), t(Font::Sans, 11.5, MUTED));
        y += ui.px(22.0);
        y += ui.wrapped(r.x + pad, y, w - 2.0 * pad, &doing, body);
        let pressing = p.pressing(c).map_or("nothing pressing".to_string(), |(i, v)| format!("most pressing: {}", c.needs[i].word(v)));
        ui.text(r.x + pad, y, &capital(&pressing), t(Font::Sans, 11.5, MUTED));
        y += ui.px(16.0);
        ui.text(r.x + pad, y, "Click to inspect", t(Font::Sans, 11.0, FAINT));
    }

    // -----------------------------------------------------------------------------------------
    // The inspector

    fn inspector(&mut self, sim: &Sim, id: PersonId, st: &Status, acts: &mut Vec<Action>) {
        let Some(p) = sim.people.get(id) else { return };
        let c = &sim.content;
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let w = ui.px(400.0);
        let top = ui.px(44.0) + ui.px(16.0);
        let panel = Rect::new(sw - w - ui.px(16.0), top, w, sh - top - ui.px(16.0));
        ui.panel(panel);
        let pad = ui.px(14.0);
        let (x0, inner) = (panel.x + pad, w - 2.0 * pad);
        let mut y = panel.y + pad;
        // Header
        let dot = ui.px(46.0);
        let clothes = rgb(p.looks.clothes);
        ui.rect(Rect::new(x0, y, dot, dot), clothes, dot * 0.5);
        ui.outline(Rect::new(x0, y, dot, dot), GOLD, dot * 0.5);
        let init = p.name.chars().next().unwrap_or('?').to_string();
        let it = t(Font::Pixel, 20.0, TEXT);
        let iw = ui.measure(&init, it);
        ui.text(x0 + (dot - iw) * 0.5, y + (dot - ui.line_height(it)) * 0.5, &init, it);
        let nx = x0 + dot + ui.px(12.0);
        let nw = ui.text(nx, y + ui.px(2.0), &p.name, t(Font::Pixel, 20.0, TEXT));
        ui.text(nx + nw + ui.px(8.0), y + ui.px(8.0), &format!("{}", p.age_years(sim.tick, c) as u32), t(Font::Sans, 13.0, MUTED));
        let sex = if p.sex == Sex::Female { "woman" } else { "man" };
        ui.text(nx, y + ui.px(26.0), &format!("{} · {sex} · the camp", role(p, c)), t(Font::Sans, 12.5, MUTED));
        let close = Rect::new(panel.right() - pad - ui.px(28.0), y, ui.px(28.0), ui.px(28.0));
        if ui.button(close, "x", false) {
            acts.push(Action::Select(None));
        }
        y += dot + ui.px(10.0);
        if !p.alive {
            let cause = p.died.as_ref().map_or("unknown".to_string(), |d| d.1.clone());
            ui.text(x0, y, &format!("Died of {cause}"), t(Font::Sans, 13.0, DANGER));
            return;
        }
        let doing = if p.asleep { "Asleep".to_string() } else { p.action.as_ref().map_or("Thinking".to_string(), |a| a.label.clone()) };
        let body = t(Font::Sans, 13.0, TEXT);
        let lbl = t(Font::Sans, 12.0, MUTED);
        let lw = ui.text(x0, y, "Now ", lbl);
        y += ui.wrapped(x0 + lw, y, inner - lw, &doing, body);
        y += ui.wrapped(x0, y + ui.px(2.0), inner, &format!("\"{}\"", p.thought), t(Font::Sans, 13.0, THOUGHT)) + ui.px(4.0);
        if self.dev {
            let chip_t = t(Font::Sans, 11.5, TEXT);
            let mut cx = x0;
            for s in [format!("Brain: {}", p.brain), format!("{} decisions", p.tally.decisions)] {
                let cw = ui.measure(&s, chip_t) + ui.px(16.0);
                let r = Rect::new(cx, y, cw, ui.px(22.0));
                ui.rect(r, CHIP, ui.px(11.0));
                ui.outline(r, BORDER, ui.px(11.0));
                ui.text(cx + ui.px(8.0), y + ui.px(3.0), &s, chip_t);
                cx += cw + ui.px(6.0);
            }
            y += ui.px(28.0);
        }
        // Tabs
        let tabs = [(Tab::Mind, "Mind"), (Tab::Life, "Life"), (Tab::Body, "Body"), (Tab::Skills, "Skills"), (Tab::Log, "Log")];
        let mut tx = x0;
        let tab_t = |on: bool| t(Font::Sans, 13.0, if on { TEXT } else { MUTED });
        let th = ui.px(30.0);
        for (tab, label) in tabs {
            let on = self.tab == tab;
            let tw = ui.measure(label, tab_t(on)) + ui.px(18.0);
            let r = Rect::new(tx, y, tw, th);
            if ui.hovered(r) && !on {
                ui.rect(r, CHIP, ui.px(6.0));
            }
            ui.text(tx + ui.px(9.0), y + ui.px(6.0), label, tab_t(on));
            if on {
                ui.rect(Rect::new(tx + ui.px(4.0), y + th - ui.px(2.0), tw - ui.px(8.0), ui.px(2.0)), GOLD, 0.0);
            }
            if ui.clicked(r) {
                self.tab = tab;
                self.log_offset = 0;
            }
            tx += tw;
        }
        y += th;
        ui.rect(Rect::new(x0, y, inner, 1.0), BORDER, 0.0);
        y += ui.px(8.0);
        let bottom = panel.bottom() - pad;
        let area = Rect::new(x0, y, inner, bottom - y);
        match self.tab {
            Tab::Mind => self.mind_tab(sim, p, area),
            Tab::Life => {
                let ui = &mut self.ui;
                ui.wrapped(area.x, area.y + ui.px(8.0), area.w, "Families, relationships and memories arrive with M5.", t(Font::Sans, 12.5, FAINT));
            }
            Tab::Body => self.body_tab(sim, p, area),
            Tab::Skills => self.skills_tab(sim, p, area),
            Tab::Log => self.log_tab(sim, id, p, area, st, acts),
        }
    }

    fn section(ui: &mut Ui, x: f32, y: f32, s: &str) -> f32 {
        let st = t(Font::Pixel, 16.0, TEXT);
        ui.text(x, y + ui.px(4.0), s, st);
        ui.line_height(st) + ui.px(8.0)
    }

    fn mind_tab(&mut self, sim: &Sim, p: &Person, area: Rect) {
        let c = &sim.content;
        let analyst = self.detail == Detail::Analyst;
        let ui = &mut self.ui;
        let (x, w) = (area.x, area.w);
        let mut y = area.y;
        y += Self::section(ui, x, y, "Needs");
        let company = near_others(sim, p);
        let by_fire = by_fire(sim, p);
        for (i, n) in c.needs.iter().enumerate() {
            let v = p.needs[i];
            let row = Rect::new(x, y, w, ui.px(34.0));
            let name_t = t(Font::Sans, 12.5, TEXT);
            ui.text(x, y, &n.name, name_t);
            let word = capital(n.word(v));
            let wt = t(Font::Sans, 12.5, if v >= 0.5 { WARN } else { TEXT });
            let shown = if analyst { format!("{word} · {:.0}/100", (1.0 - v) * 100.0) } else { word };
            let ww = ui.measure(&shown, wt);
            ui.text(x + w - ww, y, &shown, wt);
            let colour = if n.kind == sim::content::NeedKind::Body { BODY_NEED } else { MIND_NEED };
            ui.bar(Rect::new(x, y + ui.px(18.0), w, ui.px(8.0)), 1.0 - v, colour);
            if ui.hovered(row) {
                ui.tooltip(vec![format!("{}: {}", n.name, n.word(v)), need_why(sim, p, &n.id, company, by_fire)]);
            }
            y += ui.px(34.0);
        }
        let hw = ui.text(x, y, "Health", t(Font::Sans, 12.5, TEXT));
        let _ = hw;
        let hs = format!("{:.0}%", p.health * 100.0);
        let ht = t(Font::Sans, 12.5, if p.health < 0.7 { WARN } else { TEXT });
        let hsw = ui.measure(&hs, ht);
        ui.text(x + w - hsw, y, &hs, ht);
        ui.bar(Rect::new(x, y + ui.px(18.0), w, ui.px(8.0)), p.health, GOOD);
        y += ui.px(40.0);
        let (score, why) = p.mood(c, company, by_fire);
        let mw = ui.text(x, y + ui.px(4.0), &format!("Mood: {}", Person::mood_word(score)), t(Font::Pixel, 16.0, TEXT));
        ui.text(x + mw + ui.px(8.0), y + ui.px(7.0), &format!("{score:+}"), t(Font::Sans, 12.5, MUTED));
        y += ui.px(30.0);
        if self.detail != Detail::Minimal {
            for (v, reason) in why.iter().take(5) {
                let vt = t(Font::Sans, 12.5, if *v > 0 { GOOD } else { BAD });
                let vs = format!("{v:+}");
                let vw = ui.measure(&vs, vt);
                ui.text(x + ui.px(26.0) - vw, y, &vs, vt);
                ui.text(x + ui.px(36.0), y, reason, t(Font::Sans, 12.5, TEXT));
                y += ui.px(19.0);
            }
            if why.is_empty() {
                ui.text(x, y, "Nothing much either way.", t(Font::Sans, 12.5, FAINT));
                y += ui.px(19.0);
            }
        }
        y += ui.px(8.0);
        if y + ui.px(60.0) < area.bottom() {
            y += Self::section(ui, x, y, "Recent thoughts");
            for tr in decisions(p).into_iter().take(4) {
                if y + ui.px(36.0) > area.bottom() {
                    break;
                }
                let chosen = &tr.request.options[tr.choice];
                let thought = sim::actions::thought(&tr.request, chosen, c);
                let tt = t(Font::Sans, 12.5, THOUGHT);
                ui.text(x, y, &clock(sim, tr.tick), t(Font::Sans, 11.5, FAINT));
                y += ui.wrapped(x + ui.px(46.0), y, w - ui.px(46.0), &format!("\"{thought}\""), tt) + ui.px(4.0);
            }
        }
    }

    fn body_tab(&mut self, sim: &Sim, p: &Person, area: Rect) {
        let c = &sim.content;
        let g = &c.genes;
        let analyst = self.detail == Detail::Analyst;
        let ui = &mut self.ui;
        let (x, w) = (area.x, area.w);
        let mut y = area.y;
        y += Self::section(ui, x, y, "Body");
        let born_years = -p.born as f64 / (c.calendar.ticks_per_day() * c.calendar.days_per_year()) as f64;
        let born = if p.born < 0 { format!("{:.0} years before the camp", born_years) } else { "in the camp".into() };
        let build = if p.looks.build < 0.35 { "slight" } else if p.looks.build > 0.65 { "broad" } else { "medium" };
        for (k, v) in [
            ("Age", format!("{}", p.age_years(sim.tick, c) as u32)),
            ("Born", born),
            ("Height", format!("{:.2} m", p.looks.height)),
            ("Build", build.to_string()),
            ("Health", format!("{:.0}%", p.health * 100.0)),
        ] {
            ui.text(x, y, k, t(Font::Sans, 12.5, MUTED));
            ui.text(x + w * 0.35, y, &v, t(Font::Sans, 12.5, TEXT));
            y += ui.px(20.0);
        }
        y += ui.px(6.0);
        y += Self::section(ui, x, y, "DNA");
        let rows = [
            ("Height", format!("{:.2} m", p.looks.height), "height"),
            ("Build", build.to_string(), "build"),
            ("Skin", p.looks.skin_word.clone(), "skin"),
            ("Hair", p.looks.hair_word.clone(), "hair_dark"),
            ("Eyes", p.looks.eye_word.clone(), "eye_brown"),
            ("Curiosity", word_level(p.personality[0]).into(), "curiosity"),
        ];
        for (k, v, trait_id) in rows {
            let row = Rect::new(x, y, w, ui.px(22.0));
            ui.text(x, y, k, t(Font::Sans, 12.5, MUTED));
            ui.text(x + w * 0.35, y, &capital(&v), t(Font::Sans, 12.5, TEXT));
            let from = if analyst { p.genome.allele_text(g, trait_id) } else { "founder".to_string() };
            let ft = t(Font::Sans, 11.5, if analyst { GOLD } else { FAINT });
            let fw = ui.measure(&from, ft);
            ui.text(x + w - fw, y + ui.px(1.0), &from, ft);
            if ui.hovered(row) {
                ui.tooltip(vec![
                    format!("Alleles: {}", p.genome.allele_text(g, trait_id)),
                    "A founder: their parents weren't in the camp, so the source is unknown.".into(),
                ]);
            }
            ui.rect(Rect::new(x, y + ui.px(20.0), w, 1.0), CHIP, 0.0);
            y += ui.px(24.0);
        }
        y += ui.px(8.0);
        y += Self::section(ui, x, y, "Personality");
        for (k, v) in TRAITS.iter().zip(p.personality) {
            let row = Rect::new(x, y, w, ui.px(20.0));
            ui.text(x, y, &capital(k), t(Font::Sans, 12.5, TEXT));
            ui.bar(Rect::new(x + w * 0.35, y + ui.px(5.0), w * 0.45, ui.px(8.0)), v, TRAIT);
            let lv = if analyst { format!("{:.0}", v * 10.0) } else { word_level(v).to_string() };
            let lt = t(Font::Sans, 12.0, MUTED);
            let lw = ui.measure(&lv, lt);
            ui.text(x + w - lw, y, &lv, lt);
            if ui.hovered(row) {
                ui.tooltip(vec![format!("{}: {:.0} of 10", capital(k), v * 10.0), "From DNA (half) and upbringing and chance (half).".into()]);
            }
            y += ui.px(22.0);
        }
    }

    fn skills_tab(&mut self, sim: &Sim, p: &Person, area: Rect) {
        let c = &sim.content;
        let analyst = self.detail == Detail::Analyst;
        let ui = &mut self.ui;
        let (x, w) = (area.x, area.w);
        let mut y = area.y;
        y += Self::section(ui, x, y, "Skills");
        for (i, s) in c.skills.skills.iter().enumerate() {
            let lv = p.skill_level(i, c);
            let row = Rect::new(x, y, w, ui.px(34.0));
            let nw = ui.text(x, y, &s.name, t(Font::Sans, 12.5, TEXT));
            ui.text(x + nw + ui.px(8.0), y, &format!("level {lv}"), t(Font::Sans, 12.5, GOLD));
            let how = if analyst { format!("{:.0} min practised", p.practice[i]) } else { format!("works at {:.0}% speed", p.work_speed(i, c) * 100.0) };
            let ht = t(Font::Sans, 11.5, MUTED);
            let hw = ui.measure(&how, ht);
            ui.text(x + w - hw, y + ui.px(1.0), &how, ht);
            ui.bar(Rect::new(x, y + ui.px(20.0), w, ui.px(7.0)), lv as f32 / 10.0, SKILL);
            if ui.hovered(row) {
                let next = ((lv + 1) as f32).powi(2) * c.skills.minutes_per_level_sq;
                ui.tooltip(vec![
                    format!("{} level {lv} of 10, from practice", s.name),
                    format!("{:.0} minutes practised; level {} at {:.0}", p.practice[i], lv + 1, next),
                ]);
            }
            y += ui.px(36.0);
        }
        y += ui.px(4.0);
        y += Self::section(ui, x, y, "Knowledge held");
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (k, def) in c.nodes.iter().enumerate() {
            let n = p.knowledge.nodes.values().filter(|kn| kn.kind as usize == k && kn.amount >= 1.0).count();
            if n > 0 {
                counts.push((def.name.clone(), n));
            }
        }
        counts.sort_by_key(|c| std::cmp::Reverse(c.1));
        for (name, n) in counts.iter().take(6) {
            ui.text(x, y, name, t(Font::Sans, 12.5, TEXT));
            let s = format!("{n} known");
            let st = t(Font::Sans, 12.0, MUTED);
            let sw = ui.measure(&s, st);
            ui.text(x + w - sw, y, &s, st);
            y += ui.px(20.0);
        }
        for (k, v) in [
            ("Fresh water spots", format!("{} known", p.knowledge.water.len())),
            ("Explored", format!("{:.0}% of the region", p.knowledge.explored_share() * 100.0)),
        ] {
            ui.text(x, y, k, t(Font::Sans, 12.5, TEXT));
            let st = t(Font::Sans, 12.0, MUTED);
            let sw = ui.measure(&v, st);
            ui.text(x + w - sw, y, &v, st);
            y += ui.px(20.0);
        }
        let row = Rect::new(x, y + ui.px(4.0), w, ui.px(36.0));
        ui.wrapped(x, y + ui.px(6.0), w, "Only what they've seen or been told. Choices come from this, not from the map.", t(Font::Sans, 11.5, FAINT));
        let _ = row;
    }

    fn log_tab(&mut self, sim: &Sim, id: PersonId, p: &Person, area: Rect, _st: &Status, acts: &mut Vec<Action>) {
        let ui = &mut self.ui;
        let (x, w) = (area.x, area.w);
        let mut y = area.y;
        y += Self::section(ui, x, y, "Decisions");
        let list = decisions(p);
        let total = list.len();
        if total == 0 {
            ui.text(x, y, "No decisions yet.", t(Font::Sans, 12.5, FAINT));
            return;
        }
        self.log_offset = self.log_offset.min(total.saturating_sub(1));
        let mut k = self.log_offset;
        while k < total && y + ui.px(48.0) < area.bottom() - ui.px(36.0) {
            let tr = list[k];
            let chosen = &tr.request.options[tr.choice];
            ui.text(x, y, &clock(sim, tr.tick), t(Font::Sans, 11.5, FAINT));
            let lx = x + ui.px(46.0);
            let used = ui.wrapped(lx, y, w - ui.px(46.0), &chosen.label, t(Font::Sans, 12.5, TEXT));
            y += used + ui.px(2.0);
            let (agree, colour) = if tr.choice == tr.rules_choice {
                ("Rules agreed".to_string(), GOOD)
            } else {
                (format!("Rules: {}", tr.request.options[tr.rules_choice].label), WARN)
            };
            let chip_t = t(Font::Sans, 11.0, colour);
            let cw = ui.measure(&agree, chip_t).min(w * 0.6) + ui.px(14.0);
            let cr = Rect::new(lx, y, cw, ui.px(20.0));
            ui.rect(cr, CHIP, ui.px(10.0));
            ui.text(lx + ui.px(7.0), y + ui.px(2.0), &agree, chip_t);
            let mut ox = lx + cw + ui.px(8.0);
            if tr.outcome.is_none() {
                ox += ui.text(ox, y + ui.px(2.0), "under way", t(Font::Sans, 11.0, GOLD)) + ui.px(8.0);
            }
            if let Some(o) = &tr.outcome {
                let s = match &o.result {
                    Ending::Done => format!("done, {} min", o.minutes),
                    Ending::Failed(w) => format!("failed: {w}"),
                    Ending::Interrupted(_) => "stopped".to_string(),
                };
                ox += ui.text(ox, y + ui.px(2.0), &s, t(Font::Sans, 11.0, FAINT)) + ui.px(8.0);
            }
            if self.dev {
                let lt = t(Font::Sans, 11.5, GOLD);
                let label = "Open trace";
                let lw = ui.measure(label, lt);
                let r = Rect::new(x + w - lw, y, lw, ui.px(20.0));
                if ox < r.x {
                    ui.text(r.x, y + ui.px(2.0), label, lt);
                    ui.rect(Rect::new(r.x, y + ui.px(17.0), lw, 1.0), GOLD, 0.0);
                    if ui.clicked(r) {
                        self.trace = Some((id, k));
                        self.trace_scroll = (0, 0);
                    }
                }
            }
            y += ui.px(28.0);
            k += 1;
        }
        let shown = format!("{}-{} of {} (scroll for more)", self.log_offset + 1, k, total);
        ui.text(x, area.bottom() - ui.px(18.0), &shown, t(Font::Sans, 11.0, FAINT));
        if !self.dev {
            ui.text(x, area.bottom() - ui.px(36.0), "Turn on Dev in the top bar to open decision traces.", t(Font::Sans, 11.0, FAINT));
        }
        let _ = acts;
    }

    // -----------------------------------------------------------------------------------------
    // The dev trace view

    fn trace_view(&mut self, sim: &Sim, pid: PersonId, k: usize, st: &Status, acts: &mut Vec<Action>) {
        let Some(p) = sim.people.get(pid) else {
            self.trace = None;
            return;
        };
        let list = decisions(p);
        let Some(&tr) = list.get(k) else {
            self.trace = None;
            return;
        };
        let tr: &Trace = tr;
        let scroll = self.ui.input.scroll;
        let step = if scroll > 0.0 { -1i64 } else if scroll < 0.0 { 1 } else { 0 };
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let top = ui.px(44.0);
        let page = Rect::new(0.0, top, sw, sh - top);
        ui.rect(page, hex_c("#1E1A1F"), 0.0);
        ui.block(page);
        let pad = ui.px(24.0);
        let mut y = top + ui.px(18.0);
        let bh = ui.px(30.0);
        let back = format!("< Back to {}", p.name);
        let bw = ui.button_width(&back);
        if ui.button(Rect::new(pad, y, bw, bh), &back, false) {
            self.trace = None;
            self.tab = Tab::Log;
            acts.push(Action::Select(Some(pid)));
        }
        let mut x = pad + bw + ui.px(14.0);
        x += ui.text(x, y + ui.px(2.0), "Decision trace", t(Font::Pixel, 24.0, TEXT)) + ui.px(14.0);
        ui.text(x, y + ui.px(8.0), &format!("{} · {}", p.name, tr.when), t(Font::Sans, 13.0, MUTED));
        // chips on the right
        let mut cx = sw - pad;
        for s in [
            if tr.fallback { "Fallback: yes".to_string() } else { "Fallback: no".to_string() },
            if tr.latency_us == 0 { "Took <1 us".to_string() } else if tr.latency_us < 1000 { format!("Took {} us", tr.latency_us) } else { format!("Took {:.1} ms", tr.latency_us as f32 / 1000.0) },
            format!("Mode: {}", tr.mode),
            format!("Brain: {}", tr.brain),
        ] {
            let ct = t(Font::Sans, 11.5, TEXT);
            let cw = ui.measure(&s, ct) + ui.px(16.0);
            cx -= cw;
            let r = Rect::new(cx, y + ui.px(4.0), cw, ui.px(22.0));
            ui.rect(r, CHIP, ui.px(11.0));
            ui.outline(r, BORDER, ui.px(11.0));
            ui.text(cx + ui.px(8.0), y + ui.px(7.0), &s, ct);
            cx -= ui.px(6.0);
        }
        y += bh + ui.px(18.0);
        let gap = ui.px(20.0);
        let lw = (sw - 2.0 * pad - gap) * 0.45;
        let rw = sw - 2.0 * pad - gap - lw;
        let bottom = sh - ui.px(20.0);
        // Left: what the brain saw
        let left = Rect::new(pad, y, lw, bottom - y);
        ui.panel(left);
        let ip = ui.px(14.0);
        let mut ly = left.y + ip;
        ui.text(left.x + ip, ly, "What the brain saw", t(Font::Pixel, 16.0, TEXT));
        ly += ui.px(26.0);
        ui.text(left.x + ip, ly, "Text sent, exactly", t(Font::Sans, 11.5, MUTED));
        ly += ui.px(18.0);
        let text_t = t(Font::Sans, 12.0, THOUGHT);
        let box_w = left.w - 2.0 * ip;
        let lines: Vec<&str> = tr.request.text.lines().collect();
        let mut text_h = 0.0;
        for l in &lines {
            text_h += if l.is_empty() { ui.px(8.0) } else { ui.wrapped_height(box_w - ui.px(16.0), l, text_t) };
        }
        let max_text = (left.bottom() - ly) * 0.62;
        let box_h = (text_h + ui.px(16.0)).min(max_text);
        let tb = Rect::new(left.x + ip, ly, box_w, box_h);
        ui.rect(tb, hex_c("#151216"), ui.px(8.0));
        ui.outline(tb, TRACK, ui.px(8.0));
        // Scroll the text when it doesn't fit (wheel over the box).
        if ui.hovered(tb) && step != 0 {
            self.trace_scroll.0 = (self.trace_scroll.0 as i64 + step).clamp(0, lines.len() as i64 - 1) as usize;
        }
        let first = self.trace_scroll.0.min(lines.len().saturating_sub(1));
        let mut ty = ly + ui.px(8.0);
        let line_h = ui.line_height(text_t);
        for (n, l) in lines.iter().enumerate().skip(first) {
            let h = if l.is_empty() { ui.px(8.0) } else { ui.wrapped_height(box_w - ui.px(16.0), l, text_t) };
            if ty + h > tb.bottom() - ui.px(6.0) {
                let more = format!("{} more lines: scroll", lines.len() - n);
                let mt = t(Font::Sans, 11.0, GOLD);
                let mw = ui.measure(&more, mt);
                ui.rect(Rect::new(tb.right() - mw - ui.px(18.0), tb.bottom() - line_h - ui.px(4.0), mw + ui.px(12.0), line_h), CHIP, ui.px(6.0));
                ui.text(tb.right() - mw - ui.px(12.0), tb.bottom() - line_h - ui.px(4.0), &more, mt);
                break;
            }
            ty += if l.is_empty() { ui.px(8.0) } else { ui.wrapped(tb.x + ui.px(8.0), ty, box_w - ui.px(16.0), l, text_t) };
        }
        ly += box_h + ui.px(12.0);
        if self.detail != Detail::Minimal {
            ui.text(left.x + ip, ly, "Features (numbers)", t(Font::Sans, 11.5, MUTED));
            ly += ui.px(18.0);
            let colw = (box_w - ui.px(16.0)) * 0.5;
            let ft = t(Font::Sans, 11.5, MUTED);
            let vt = t(Font::Sans, 11.5, TEXT);
            let rows_fit = ((left.bottom() - ip - ly) / ui.px(17.0)).floor().max(0.0) as usize;
            let fits = rows_fit * 2;
            let state = &tr.request.state;
            let fits = if state.len() > fits { fits.saturating_sub(1) } else { fits };
            if state.len() > fits {
                let more = format!("{} more features", state.len() - fits);
                let mt = t(Font::Sans, 11.0, FAINT);
                let mw = ui.measure(&more, mt);
                ui.text(left.right() - ip - mw, ly - ui.px(18.0), &more, mt);
            }
            for (i, (name, v)) in state.iter().enumerate().take(fits) {
                let col = i % 2;
                let row = i / 2;
                let fx = left.x + ip + col as f32 * (colw + ui.px(16.0));
                let fy = ly + row as f32 * ui.px(17.0);
                ui.text(fx, fy, name, ft);
                let s = format!("{v:.2}");
                let sw = ui.measure(&s, vt);
                ui.text(fx + colw - sw, fy, &s, vt);
            }
        }
        // Right: what it picked
        let right = Rect::new(pad + lw + gap, y, rw, (bottom - y) * 0.68);
        ui.panel(right);
        let mut ry = right.y + ip;
        ui.text(right.x + ip, ry, "What it picked", t(Font::Pixel, 16.0, TEXT));
        let key = t(Font::Sans, 11.5, MUTED);
        let mut kx = right.right() - ip;
        for (label, colour) in [("rules (second opinion)", RULES), ("brain", BRAIN)] {
            let w = ui.measure(label, key);
            kx -= w;
            ui.text(kx, ry + ui.px(3.0), label, key);
            kx -= ui.px(14.0);
            ui.rect(Rect::new(kx, ry + ui.px(6.0), ui.px(10.0), ui.px(10.0)), colour, ui.px(2.0));
            kx -= ui.px(14.0);
        }
        ry += ui.px(28.0);
        let probs = sim::sim::confidence(&tr.scores);
        let rprobs = sim::sim::confidence(&tr.rules_scores);
        let label_w = (rw - 2.0 * ip) * 0.46;
        let bar_x = right.x + ip + label_w + ui.px(10.0);
        let bar_w = right.right() - ip - ui.px(44.0) - bar_x;
        let mut order: Vec<usize> = (0..tr.request.options.len()).collect();
        order.sort_by(|a, b| {
            let pa = tr.request.options[*a].possible;
            let pb = tr.request.options[*b].possible;
            pb.cmp(&pa).then(probs[*b].total_cmp(&probs[*a]))
        });
        let list_area = Rect::new(right.x, ry, rw, right.bottom() - ry - ui.px(30.0));
        if ui.hovered(list_area) && step != 0 {
            self.trace_scroll.1 = (self.trace_scroll.1 as i64 + step).clamp(0, order.len() as i64 - 1) as usize;
        }
        let first = self.trace_scroll.1.min(order.len().saturating_sub(1));
        for (n, &i) in order.iter().enumerate().skip(first) {
            if ry + ui.px(30.0) > list_area.bottom() {
                let more = format!("{} more options: scroll", order.len() - n);
                ui.text(right.x + ip, ry, &more, t(Font::Sans, 11.0, GOLD));
                break;
            }
            let o = &tr.request.options[i];
            let chosen = i == tr.choice;
            let row = Rect::new(right.x + ui.px(6.0), ry - ui.px(3.0), rw - ui.px(12.0), ui.px(30.0));
            if chosen {
                ui.rect(row, hex_a("#E8C468", 0.12), ui.px(6.0));
            }
            let lt = t(Font::Sans, 12.0, if o.possible { TEXT } else { FAINT });
            let mut label = o.label.clone();
            while ui.measure(&label, lt) > label_w && label.len() > 4 {
                label.pop();
            }
            if label.len() < o.label.len() {
                label.push_str("...");
            }
            ui.text(right.x + ip, ry, &label, lt);
            if o.possible {
                let pct = |v: f32| format!("{:.0}%", v * 100.0);
                ui.bar(Rect::new(bar_x, ry + ui.px(2.0), bar_w, ui.px(7.0)), probs[i], BRAIN);
                ui.bar(Rect::new(bar_x, ry + ui.px(13.0), bar_w, ui.px(7.0)), rprobs[i], RULES);
                let pt = t(Font::Sans, 11.0, TEXT);
                ui.text(bar_x + bar_w + ui.px(6.0), ry - ui.px(2.0), &pct(probs[i]), pt);
                ui.text(bar_x + bar_w + ui.px(6.0), ry + ui.px(10.0), &pct(rprobs[i]), t(Font::Sans, 11.0, FAINT));
            } else if let Some(w) = &o.why_not {
                ui.text(bar_x, ry + ui.px(2.0), &format!("Not possible: {w}"), t(Font::Sans, 11.0, FAINT));
            }
            if ui.hovered(row) && o.possible {
                let mut lines = vec![o.label.clone()];
                for (k, v) in o.features.iter().take(8) {
                    lines.push(format!("{k} = {v:.2}"));
                }
                ui.tooltip(lines);
            }
            ry += ui.px(30.0);
        }
        let rp = format!("Rules would have picked: {}", tr.request.options[tr.rules_choice].label);
        ui.text(right.x + ip, right.bottom() - ip - ui.px(16.0), &rp, t(Font::Sans, 12.0, MUTED));
        // Below: reason, outcome, pause
        let mut by = right.bottom() + ui.px(12.0);
        let half = (rw - ui.px(12.0)) * 0.5;
        let reason = tr.reason.clone().unwrap_or_else(|| "The rules brain writes no reason; its scores are the reason (hover an option for its numbers).".into());
        let outcome = match &tr.outcome {
            None => "Still under way.".to_string(),
            Some(o) => {
                let r = match &o.result {
                    Ending::Done => "Done".to_string(),
                    Ending::Failed(w) => format!("Failed: {w}"),
                    Ending::Interrupted(w) => format!("Stopped: {w}"),
                };
                let mut s = format!("{r} in {} minutes.", o.minutes);
                let changes: Vec<String> = sim
                    .content
                    .needs
                    .iter()
                    .zip(&o.need_change)
                    .filter(|(_, d)| d.abs() > 0.05)
                    .map(|(n, d)| format!("{} {} {:.0}", n.name.to_lowercase(), if *d < 0.0 { "eased" } else { "grew" }, d.abs() * 100.0))
                    .collect();
                if !changes.is_empty() {
                    s += &format!(" Meanwhile {}.", changes.join(", "));
                }
                if !o.stored.is_empty() {
                    let st: Vec<String> = o.stored.iter().map(|(k, n)| format!("{n} {k}")).collect();
                    s += &format!(" Brought back {}.", st.join(", "));
                }
                s
            }
        };
        let box_h = bottom - by - bh - ui.px(12.0);
        for (k, (title, body)) in [("Reason", reason), ("What happened next", outcome)].into_iter().enumerate() {
            let r = Rect::new(right.x + k as f32 * (half + ui.px(12.0)), by, half, box_h);
            ui.panel(r);
            ui.text(r.x + ip, r.y + ip, title, t(Font::Pixel, 16.0, TEXT));
            ui.wrapped(r.x + ip, r.y + ip + ui.px(26.0), r.w - 2.0 * ip, &body, t(Font::Sans, 12.5, if k == 0 { THOUGHT } else { TEXT }));
        }
        by += box_h + ui.px(12.0);
        let pause_label = if st.pause_on == Some(pid) { format!("Will pause on {}'s next decision", p.name) } else { format!("Pause on {}'s next decision", p.name) };
        let pw = ui.button_width(&pause_label);
        if ui.button(Rect::new(right.x, by, pw, bh), &pause_label, st.pause_on == Some(pid)) {
            acts.push(Action::PauseOnNextDecision(pid));
        }
        ui.text(right.x + pw + ui.px(12.0), by + ui.px(7.0), "Traces are also the training data: this row is exported as-is.", t(Font::Sans, 11.5, FAINT));
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

fn hex_c(s: &str) -> [f32; 4] {
    ui::hex(s, 1.0)
}

fn hex_a(s: &str, a: f32) -> [f32; 4] {
    ui::hex(s, a)
}

fn rgb(c: [u8; 3]) -> [f32; 4] {
    hex_c(&format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
}

fn word_level(v: f32) -> &'static str {
    if v > 0.68 {
        "high"
    } else if v < 0.32 {
        "low"
    } else {
        "medium"
    }
}

/// A short role from what they've practised most.
pub fn role(p: &Person, c: &sim::Content) -> &'static str {
    let best = (0..p.practice.len()).max_by(|a, b| p.practice[*a].total_cmp(&p.practice[*b])).unwrap_or(0);
    match c.skills.skills[best].id.as_str() {
        "foraging" => "Forager",
        "woodcutting" => "Woodgatherer",
        "stonework" => "Stoneworker",
        "firemaking" => "Firekeeper",
        _ => "Explorer",
    }
}

fn near_others(sim: &Sim, p: &Person) -> bool {
    let r = sim.content.people.company_reach;
    sim.people.iter().filter(|(_, q)| q.alive && !q.asleep && !std::ptr::eq(*q, p)).any(|(_, q)| ((q.pos.0 - p.pos.0).powi(2) + (q.pos.1 - p.pos.1).powi(2)).sqrt() <= r)
}

/// Why a need is where it is, in words.
fn need_why(sim: &Sim, p: &Person, need: &str, company: bool, by_fire: bool) -> String {
    let c = &sim.content;
    let hours = |tick: u64| (sim.tick.saturating_sub(tick)) as f32 * c.calendar.minutes_per_tick as f32 / 60.0;
    let ago = |tick: u64| {
        let h = hours(tick);
        if h < 1.0 { format!("{:.0} min ago", h * 60.0) } else { format!("{h:.0} h ago") }
    };
    let berries = c.item("berries");
    match need {
        "hunger" => format!("Last ate {}. Carrying {} berries; the stores hold {}.", ago(p.last.ate), p.carry[berries], sim.camp.stores[berries]),
        "thirst" => format!("Last drank {}. Knows {} fresh water spots.", ago(p.last.drank), p.knowledge.water.len()),
        "rest" => {
            if p.asleep {
                "Asleep now: rest is coming back.".to_string()
            } else {
                format!("Woke {}. The body clock says sleep around 21:30.", ago(p.last.woke))
            }
        }
        "warmth" => {
            let temp = air_temp(c, &sim.map, p.pos.0 as u32, p.pos.1 as u32, sim.tick);
            if by_fire {
                format!("{temp:.0} C here, but warm by the fire.")
            } else if temp < c.people.comfort_temp {
                format!("{temp:.0} C here, below {:.0} C: getting colder away from a fire.", c.people.comfort_temp)
            } else {
                format!("{temp:.0} C here: comfortable.")
            }
        }
        "social" => {
            if company {
                "Others are close by: company eases loneliness.".to_string()
            } else {
                "Nobody close by.".to_string()
            }
        }
        "fun" => format!("Last rested by the fire {}.", ago(p.last.rested)),
        _ => String::new(),
    }
}

/// The ring colour under someone for the mood and needs lenses (linear RGB).
pub fn ring_colour(sim: &Sim, p: &Person, lens: Lens) -> Option<[f32; 3]> {
    let c = &sim.content;
    let col = match lens {
        Lens::Off | Lens::Traffic => return None,
        Lens::Mood => {
            let company = near_others(sim, p);
            let (score, _) = p.mood(c, company, by_fire(sim, p));
            if score >= 2 {
                GOOD
            } else if score >= -1 {
                TEXT
            } else if score >= -4 {
                GOLD
            } else {
                BAD
            }
        }
        Lens::Needs => need_colour(sim, p),
    };
    Some([col[0], col[1], col[2]])
}

/// The colour of someone's most pressing need (the needs lens), or "fine".
fn need_colour(sim: &Sim, p: &Person) -> [f32; 4] {
    let c = &sim.content;
    match p.pressing(c) {
        Some((i, v)) if v >= 0.5 => match c.needs[i].id.as_str() {
            "thirst" => hex_c("#7FB8E6"),
            "hunger" => WARN,
            "rest" => MIND_NEED,
            "warmth" => BAD,
            _ => MIND_NEED,
        },
        _ => BODY_NEED,
    }
}

fn by_fire(sim: &Sim, p: &Person) -> bool {
    let (fx, fy) = sim.camp.fire_pos();
    sim.camp.fire_lit && ((p.pos.0 - fx).powi(2) + (p.pos.1 - fy).powi(2)).sqrt() <= sim.content.people.fire_reach
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lay out every tab, lens, detail level and a few traces with the cursor swept over the
    /// screen (so every tooltip path runs), at two UI scales. Checks nothing panics, the
    /// inspector blocks clicks from reaching the world, and clicking a tab switches to it.
    #[test]
    fn every_view_lays_out() {
        let assets = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"));
        let mut sim = Sim::new(2, 128);
        sim.run_ticks(sim.content.calendar.ticks_per_day() + 300);
        let mut hud = Hud::new(&assets).expect("fonts");
        hud.take_events(&mut sim);
        let who = sim.people.iter().find(|(_, p)| p.alive).map(|(id, _)| id).unwrap();
        let st = Status { paused: false, speed: 1.0, fps: 60.0, frame_ms: 16.0, selected: Some(who), hovered: None, pause_on: None };
        for scale in [1.0, 2.0] {
            let size = (1280.0 * scale, 800.0 * scale);
            for tab in [Tab::Mind, Tab::Life, Tab::Body, Tab::Skills, Tab::Log] {
                for lens in [Lens::Off, Lens::Traffic, Lens::Mood, Lens::Needs] {
                    for detail in [Detail::Minimal, Detail::Normal, Detail::Analyst] {
                        for trace in [None, Some((who, 0)), Some((who, 3))] {
                            (hud.tab, hud.lens, hud.detail, hud.trace) = (tab, lens, detail, trace);
                            for k in 0..12 {
                                let cursor = (size.0 * (k as f32 + 0.5) / 12.0, size.1 * ((k * 7) % 12) as f32 / 12.0);
                                hud.ui.begin(size, scale, ui::Input { cursor, clicked: false, scroll: -1.0 });
                                hud.frame(&sim, &st);
                                hud.ui.finish();
                            }
                        }
                    }
                }
            }
            // The inspector sits on the right: the middle of its panel is UI, the middle of the screen isn't.
            hud.trace = None;
            hud.ui.begin(size, scale, ui::Input { cursor: (size.0 - 200.0 * scale, size.1 * 0.6), clicked: false, scroll: 0.0 });
            hud.frame(&sim, &st);
            assert!(hud.ui.over_ui(), "inspector should block the world");
            hud.ui.begin(size, scale, ui::Input { cursor: (size.0 * 0.5, size.1 * 0.5), clicked: false, scroll: 0.0 });
            hud.frame(&sim, &st);
            assert!(!hud.ui.over_ui(), "the middle of the screen is world");
        }
        // Click across the tab row until the Log tab opens.
        hud.tab = Tab::Mind;
        let mut opened = false;
        for y in (100..260).step_by(4) {
            for x in (900..1280).step_by(8) {
                hud.ui.begin((1280.0, 800.0), 1.0, ui::Input { cursor: (x as f32, y as f32), clicked: true, scroll: 0.0 });
                let acts = hud.frame(&sim, &st);
                hud.ui.finish();
                if hud.tab == Tab::Log && acts.is_empty() {
                    opened = true;
                    break;
                }
                hud.tab = Tab::Mind;
                hud.trace = None;
            }
            if opened {
                break;
            }
        }
        assert!(opened, "clicking the Log tab should open it");
    }
}
