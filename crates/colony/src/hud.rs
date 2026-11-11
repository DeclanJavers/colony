//! The game's UI (M4), laid out like the M0 mockup: top bar, story feed, lenses, the inspector
//! with its tabs, a hover card, and the dev trace view. Built fresh each frame with the ui crate.

use sim::actions::air_temp;
use sim::brain::{Ending, Trace};
use sim::people::social::Stage;
use sim::people::{Person, Sex, TRAITS};
use sim::{Event, PersonId, Sim};
use std::collections::VecDeque;
use ui::style::*;
use ui::{Font, Rect, Text, Ui};

use crate::view::Pick;

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
    Soil,
    Beauty,
}

impl Lens {
    /// What the renderer draws on the ground for this lens.
    pub fn ground(self) -> render::GroundLens {
        match self {
            Lens::Traffic => render::GroundLens::Traffic,
            Lens::Soil => render::GroundLens::Soil,
            Lens::Beauty => render::GroundLens::Beauty,
            _ => render::GroundLens::Off,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FeedKind {
    Social,
    Trouble,
    Knowledge,
    World,
    Life,
    /// What you did (M8: speaking to someone).
    Acts,
}

impl FeedKind {
    fn colour(self) -> [f32; 4] {
        match self {
            FeedKind::Social => K_SOCIAL,
            FeedKind::Trouble => K_CONFLICT,
            FeedKind::Knowledge => K_KNOWLEDGE,
            FeedKind::World => K_WORLD,
            FeedKind::Life => K_LIFE,
            FeedKind::Acts => hex_c("#F2A7C9"),
        }
    }
    fn label(self) -> &'static str {
        match self {
            FeedKind::Social => "Social",
            FeedKind::Trouble => "Trouble",
            FeedKind::Knowledge => "Knowledge",
            FeedKind::World => "World",
            FeedKind::Life => "Life",
            FeedKind::Acts => "You",
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
    Select(Option<Pick>),
    Speed(f32),
    TogglePause,
    PauseOnNextDecision(PersonId),
    /// A command for the sim (world settings).
    Command(sim::Command),
    /// Change the AI budget.
    SetAi(Ai),
}

/// The AI budget (ARCHITECTURE.md section 6): which brain does each job. Low is the rules brain
/// with no servers; Max sends everyday decisions to Claude Haiku (the game pauses while it
/// decides). Medium and High need the scorer and text model (M9) and a local LLM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ai {
    Low,
    Medium,
    High,
    Max,
}

/// The world's state the HUD shows but doesn't own.
pub struct Status {
    pub paused: bool,
    pub speed: f32,
    pub fps: f32,
    pub frame_ms: f32,
    pub selected: Option<Pick>,
    pub hovered: Option<Pick>,
    pub pause_on: Option<PersonId>,
    pub ai: Ai,
    /// Decisions a brain is thinking about right now (Haiku on Max).
    pub deciding: usize,
    /// Where each person's head is on screen, for speech bubbles.
    pub heads: Vec<(PersonId, (f32, f32))>,
}

pub struct Hud {
    pub ui: Ui,
    pub tab: Tab,
    pub detail: Detail,
    pub dev: bool,
    pub lens: Lens,
    /// The decision trace being viewed: (person, index into their traces, counting back from the newest).
    pub trace: Option<(PersonId, usize)>,
    /// The History ledger is open (scrolled this many entries from the newest).
    pub history: Option<usize>,
    /// The world settings panel is open.
    pub settings: bool,
    /// Speaking to someone (M8): who, and the words typed so far.
    pub speaking: Option<(PersonId, String)>,
    /// The Arena ledger is open (M9), with the report it read.
    pub arena: Option<String>,
    /// The repository root (for runs/arena/report.md).
    root: std::path::PathBuf,
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

/// A date in words, for history: "today at 14:05", or "spring day 3, year 2".
fn when_long(sim: &Sim, tick: u64) -> String {
    let cal = &sim.content.calendar;
    if cal.day_index(sim.tick) == cal.day_index(tick) {
        return format!("today at {}", clock(sim, tick));
    }
    format!("{} day {}, year {}", cal.season(tick), cal.day_of_season(tick), cal.year(tick))
}

/// How long ago: hours and days, then years as people count their age (so "8 years ago"
/// matches someone being 8 years older since).
fn ago(sim: &Sim, tick: u64) -> String {
    let c = &sim.content;
    let ticks = sim.tick.saturating_sub(tick);
    let mins = ticks * c.calendar.minutes_per_tick as u64;
    let day = 24 * 60;
    let years = (ticks as f64 / c.ticks_per_age_year()).floor() as u64;
    match mins {
        m if m < 60 => "just now".into(),
        m if m < day => format!("{} h ago", m / 60),
        m if m < 2 * day => "yesterday".into(),
        _ if years < 1 => format!("{} days ago", mins / day),
        _ if years == 1 => "a year ago".into(),
        _ => format!("{years} years ago"),
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
            history: None,
            settings: false,
            speaking: None,
            arena: None,
            root: assets.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
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
                Event::Born { child, name, parents, .. } => Some(FeedItem { kind: FeedKind::Life, text: format!("{name} was born to {parents}"), tick, person: Some(child), group: None }),
                Event::Expecting { mother, name, father } => Some(FeedItem { kind: FeedKind::Life, text: format!("{name} and {father} are expecting a child"), tick, person: Some(mother), group: None }),
                Event::Partnered { a, names, .. } => Some(FeedItem { kind: FeedKind::Social, text: format!("{names} became partners"), tick, person: Some(a), group: None }),
                Event::GrewUp { person, name } => Some(FeedItem { kind: FeedKind::Life, text: format!("{name} came of age"), tick, person: Some(person), group: None }),
                Event::Argued { a, names, .. } => Some(FeedItem { kind: FeedKind::Trouble, text: format!("{names} quarrelled"), tick, person: Some(a), group: None }),
                Event::Learned { learner, name, skill, teacher } => Some(FeedItem { kind: FeedKind::Knowledge, text: format!("{name} learned {skill} from {teacher}"), tick, person: Some(learner), group: None }),
                Event::SkillLost { skill, last } => Some(FeedItem { kind: FeedKind::Knowledge, text: format!("{} is lost: {last} was the last who knew it", capital(&skill)), tick, person: None, group: None }),
                Event::Buried { person, name, by } => Some(FeedItem { kind: FeedKind::Life, text: format!("{by} buried {name}"), tick, person: Some(person), group: None }),
                Event::Shared { to, names, what, .. } => Some(FeedItem { kind: FeedKind::Social, text: format!("{names} {what}"), tick, person: Some(to), group: None }),
                // Settling down (M6)
                Event::BuildingStarted { person, name, kind, .. } => {
                    let what = if kind == "field" { "started clearing a field".to_string() } else { format!("started building a {kind}") };
                    Some(FeedItem { kind: FeedKind::World, text: format!("{name} {what}"), tick, person: Some(person), group: None })
                }
                Event::BuildingDone { kind, owner, .. } => {
                    let whose = owner.and_then(|o| sim.people.get(o)).map(|o| format!("{}'s ", o.name)).unwrap_or_default();
                    let text = if kind == "field" { "A new field is ready to sow".to_string() } else { capital(&format!("{whose}{kind} is finished")) };
                    Some(FeedItem { kind: FeedKind::World, text, tick, person: owner, group: None })
                }
                Event::FieldSown { person, name, .. } => Some(FeedItem { kind: FeedKind::World, text: format!("{name} sowed a field"), tick, person: Some(person), group: None }),
                Event::Harvested { person, name, amount, .. } => Some(FeedItem { kind: FeedKind::World, text: format!("{name} harvested {amount} grain"), tick, person: Some(person), group: None }),
                Event::CropFailed { .. } => Some(FeedItem { kind: FeedKind::Trouble, text: "Frost killed a field of grain".into(), tick, person: None, group: None }),
                Event::SpearMade { person, name } => Some(FeedItem { kind: FeedKind::Knowledge, text: format!("{name} made a spear"), tick, person: Some(person), group: None }),
                Event::Hunted { person, name, animal, .. } => Some(FeedItem { kind: FeedKind::World, text: format!("{name} killed a {animal}"), tick, person: Some(person), group: None }),
                Event::HobbyFound { person, name, hobby } => Some(FeedItem { kind: FeedKind::Life, text: format!("{name} took up {hobby}"), tick, person: Some(person), group: None }),
                Event::Made { person, name, what } => Some(FeedItem { kind: FeedKind::World, text: format!("{name} {what}"), tick, person: Some(person), group: None }),
                Event::SettingChanged { what } => Some(FeedItem { kind: FeedKind::World, text: what, tick, person: None, group: None }),
                Event::Spoken { person, name, words, read_as } => Some(FeedItem { kind: FeedKind::Acts, text: format!("You told {name}: \"{words}\" ({read_as})"), tick, person: Some(person), group: None }),
                Event::Weather { sky, snap } => {
                    let text = if snap { "A cold snap came".to_string() } else { match sky.as_str() { "rain" => "It began to rain".to_string(), "snow" => "Snow began to fall".to_string(), s => format!("The weather turned {s}") } };
                    Some(FeedItem { kind: FeedKind::World, text, tick, person: None, group: None })
                }
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
        // Speech bubbles first, so panels sit over them.
        self.bubbles(sim, st);
        self.top_bar(sim, st, &mut acts);
        if let Some((pid, k)) = self.trace {
            self.trace_view(sim, pid, k, st, &mut acts);
            return acts;
        }
        if self.history.is_some() {
            self.history_view(sim, &mut acts);
            return acts;
        }
        if self.arena.is_some() {
            self.arena_view();
            return acts;
        }
        self.story(sim, &mut acts);
        self.lenses(sim);
        match st.selected {
            Some(Pick::Person(id)) => self.inspector(sim, id, st, &mut acts),
            Some(thing) => self.thing_inspector(sim, thing, &mut acts),
            None => {}
        }
        if let Some(h) = st.hovered.filter(|h| Some(*h) != st.selected)
            && !self.ui.over_ui()
        {
            match h {
                Pick::Person(id) => self.hover_card(sim, id, st.selected.is_some()),
                thing => self.thing_card(sim, thing, st.selected.is_some()),
            }
        }
        self.toolbar(sim);
        // Settings float over everything else, in the middle under the top bar.
        if self.settings {
            self.settings_panel(sim, st, &mut acts);
        }
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
        // What fits: the right side's buttons and the speed buttons always show; the era, the
        // weather and the camp's name give way on narrow windows.
        let dev_label = if self.dev { "Dev: on" } else { "Dev: off" };
        let det_label = match self.detail {
            Detail::Minimal => "Detail: minimal",
            Detail::Normal => "Detail: normal",
            Detail::Analyst => "Detail: analyst",
        };
        let right_w = ui.measure("000 fps", t(Font::Sans, 11.5, FAINT)) + pad + ui.button_width(dev_label) + ui.button_width(det_label) + ui.button_width("History") + ui.button_width("World") + if self.dev { ui.button_width("Arena") + ui.px(6.0) } else { 0.0 } + ui.px(18.0);
        let speed_w: f32 = ["II", "1x", "3x", "10x"].iter().map(|l| ui.button_width(l).max(ui.px(36.0)) + ui.px(4.0)).sum();
        let alive = sim.alive();
        let young = sim.people.iter().filter(|(_, p)| p.alive && !p.grown(tick, &sim.content)).count();
        let people = if young > 0 { format!("{alive} people ({young} young)") } else { format!("{alive} people") };
        let date = format!("{} · day {} · year {}", capital(cal.season(tick)), cal.day_of_season(tick), cal.year(tick));
        let (sx, sy) = sim.camp.stores_at;
        let temp = air_temp(&sim.content, &sim.map, sx as u32, sy as u32, tick);
        let fire = if sim.camp.fire_lit { format!("fire lit, {:.0} h of fuel", sim.camp.fire_hours) } else { "fire out".into() };
        let wx = &sim.map.weather;
        let sky = if wx.snap_left > 0 { format!("{}, cold snap", capital(wx.sky.word())) } else { capital(wx.sky.word()) };
        let weather = format!("{sky}, {temp:.0} C, {fire}");
        let camp_t = t(Font::Pixel, 16.0, TEXT);
        let clock_t = t(Font::Sans, 14.0, TEXT);
        let base = x + ui.measure(&people, muted) + ui.measure(&date, body) + ui.measure("00:00", clock_t) + 4.0 * pad + speed_w;
        let room = w - right_w - pad - base;
        let era_label = era(sim);
        let (camp_w, era_w, weather_w) = (ui.measure("The camp", camp_t) + ui.px(10.0), ui.measure(&format!(" · {era_label}"), muted), ui.measure(&weather, muted) + pad);
        let show_weather = room >= weather_w;
        let show_camp = room >= weather_w * show_weather as u8 as f32 + camp_w;
        let show_era = room >= weather_w * show_weather as u8 as f32 + camp_w * show_camp as u8 as f32 + era_w;
        if show_camp {
            x += ui.text(x, mid(ui, camp_t), "The camp", camp_t) + ui.px(10.0);
        }
        let people = if show_era { format!("{people} · {era_label}") } else { people };
        x += ui.text(x, mid(ui, muted), &people, muted) + pad;
        ui.rect(Rect::new(x, h * 0.25, 1.0, h * 0.5), BORDER, 0.0);
        x += pad;
        x += ui.text(x, mid(ui, body), &date, body) + ui.px(10.0);
        x += ui.text(x, mid(ui, clock_t), &clock(sim, tick), clock_t) + ui.px(10.0);
        if show_weather {
            let ww = ui.text(x, mid(ui, muted), &weather, muted);
            let wr = Rect::new(x, 0.0, ww, h);
            if ui.hovered(wr) {
                let mut tip = vec![
                    "Today's sky and the air temperature at the camp.".into(),
                    "People get cold below 10 C unless they're by a lit fire or at home.".into(),
                    "Rain and snow chill anyone caught out in them.".into(),
                ];
                if wx.snow > 0.05 {
                    tip.push(format!("Snow lies on the ground ({:.0}% cover).", wx.snow * 100.0));
                }
                if wx.snap_left > 0 {
                    tip.push(format!("A cold snap: {} more days.", wx.snap_left));
                }
                ui.tooltip(tip);
            }
            x += ww + pad;
        }
        // Speed
        let bh = ui.px(28.0);
        let by = (h - bh) * 0.5;
        // A brain thinking (Haiku on Max): time waits for it.
        if st.deciding > 0 {
            let s = format!("Haiku is deciding {} choice{}...", st.deciding, if st.deciding == 1 { "" } else { "s" });
            let dt = t(Font::Sans, 12.0, GOLD);
            let dw = ui.measure(&s, dt) + ui.px(16.0);
            let r = Rect::new((w - dw) * 0.5, h + ui.px(8.0), dw, ui.px(24.0));
            ui.panel(r);
            ui.text(r.x + ui.px(8.0), r.y + ui.px(4.0), &s, dt);
        }
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
        let dw = ui.button_width(dev_label);
        rx -= dw;
        if ui.button(Rect::new(rx, by, dw, bh), dev_label, self.dev) {
            self.dev = !self.dev;
        }
        if ui.hovered(Rect::new(rx, by, dw, bh)) {
            ui.tooltip(vec!["Developer view: brains and decision traces.".into()]);
        }
        rx -= ui.px(6.0);
        let det = det_label;
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
        rx -= ui.px(6.0);
        if self.dev {
            let aw = ui.button_width("Arena");
            rx -= aw;
            let open = self.arena.is_some();
            if ui.button(Rect::new(rx, by, aw, bh), "Arena", open) {
                self.arena = if open { None } else { Some(std::fs::read_to_string(self.root.join("runs/arena/report.md")).unwrap_or_default()) };
                self.history = None;
            }
            if ui.hovered(Rect::new(rx, by, aw, bh)) {
                ui.tooltip(vec!["The Arena: the same worlds with different brains, compared (colony-run arena).".into()]);
            }
            rx -= ui.px(6.0);
        }
        let ww = ui.button_width("World");
        rx -= ww;
        if ui.button(Rect::new(rx, by, ww, bh), "World", self.settings) {
            self.settings = !self.settings;
        }
        if ui.hovered(Rect::new(rx, by, ww, bh)) {
            ui.tooltip(vec!["World settings: the rules of this world.".into()]);
        }
        rx -= ui.px(6.0);
        let hw = ui.button_width("History");
        rx -= hw;
        let open = self.history.is_some();
        if ui.button(Rect::new(rx, by, hw, bh), "History", open) {
            self.history = if open { None } else { Some(0) };
            self.trace = None;
        }
        if ui.hovered(Rect::new(rx, by, hw, bh)) {
            ui.tooltip(vec!["The camp's history: births, deaths, partners, skills learned and lost.".into()]);
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
                acts.push(Action::Select(Some(Pick::Person(p))));
            }
            yy += th + ui.px(6.0);
        }
        // Legend
        let mut lx = x + pad;
        let ly = panel.bottom() - pad - ui.px(12.0);
        ui.rect(Rect::new(x + pad, ly - ui.px(8.0), w - 2.0 * pad, 1.0), BORDER, 0.0);
        for k in [FeedKind::Social, FeedKind::Trouble, FeedKind::Knowledge, FeedKind::World, FeedKind::Life, FeedKind::Acts] {
            let lt = t(Font::Sans, 11.0, MUTED);
            let d = ui.px(7.0);
            ui.rect(Rect::new(lx, ly + ui.px(4.0), d, d), k.colour(), d * 0.5);
            lx += d + ui.px(3.0);
            lx += ui.text(lx, ly, k.label(), lt) + ui.px(7.0);
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
        for (label, lens) in [("Off", Lens::Off), ("Traffic", Lens::Traffic), ("Mood", Lens::Mood), ("Needs", Lens::Needs), ("Soil", Lens::Soil), ("Beauty", Lens::Beauty)] {
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
            Lens::Soil => Some(("Soil lens", "Where grain grows well", vec![(hex_c("#CCB880"), "poor"), (hex_c("#86913D"), "fair"), (hex_c("#3F8A35"), "rich")])),
            Lens::Beauty => Some(("Beauty lens", "How lovely each place is", vec![(hex_c("#6B5A8E"), "a little"), (hex_c("#B49CF0"), "pretty"), (hex_c("#F3EADB"), "lovely")])),
        };
        if let Some((title, sub, keys)) = legend {
            let lx = ui.px(16.0) + ui.px(300.0) + ui.px(16.0);
            let ly = y + bh + ui.px(8.0);
            let pad = ui.px(10.0);
            let w = ui.px(if self.lens == Lens::Traffic { 330.0 } else { 270.0 });
            let extra = matches!(self.lens, Lens::Traffic | Lens::Soil | Lens::Beauty);
            let h = ui.px(if extra { 98.0 } else { 78.0 });
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
            let note_y = ly + pad + ui.px(62.0);
            if self.lens == Lens::Traffic {
                let worn = sim.map.footfall.iter().filter(|f| **f > sim::path::TRAIL_STEPS * 0.45).count();
                let s = format!("{worn} tiles worn into trails");
                ui.text(kx + ui.px(6.0), ky, &s, t(Font::Sans, 11.0, FAINT));
                // The trip-time stat: how much the trails save on the walks people make.
                let c = &sim.content;
                let season = (c.calendar.day_index(sim.tick) / c.calendar.days_per_season as u64) as usize;
                let st = &sim.stats;
                let now = st.trail_saving(season).or(st.trail_saving(season.saturating_sub(1)));
                let pace = st.pace(season).or(st.pace(season.saturating_sub(1)));
                let line = match (now, pace, st.pace(0)) {
                    (Some(s), Some(p), Some(p0)) => format!("Trails save {:.0}% of walking time · {p:.0} min/100 m (at first {p0:.0})", s * 100.0),
                    _ => "Trip times: not enough walking yet".to_string(),
                };
                let r = Rect::new(lx + pad, note_y, w - 2.0 * pad, ui.px(16.0));
                ui.text(r.x, r.y, &line, t(Font::Sans, 11.0, MUTED));
                if ui.hovered(r) {
                    ui.tooltip(vec![
                        "Trip times: minutes of walking per 100 m, by everyone, this season.".into(),
                        "The saving compares the same walks over unworn ground: busy routes wear into trails that are quicker to walk.".into(),
                    ]);
                }
            }
            if self.lens == Lens::Beauty {
                let line = if sim.settings.beauty_matters { "Flowers, statues and decorated homes make it." } else { "Beauty doesn't matter in this world (World)." };
                ui.text(lx + pad, note_y, line, t(Font::Sans, 11.0, MUTED));
            }
            if self.lens == Lens::Soil {
                ui.text(lx + pad, note_y, "Darker is wetter. Rain wets it; sun dries it.", t(Font::Sans, 11.0, MUTED));
            }
        }
    }

    /// What people just said, in bubbles over their heads (a fixed size on screen; never
    /// overlapping: when two would, the nearer the camera wins).
    fn bubbles(&mut self, sim: &Sim, st: &Status) {
        let ui = &mut self.ui;
        let fresh = 25 / sim.content.calendar.minutes_per_tick as u64;
        let mut items: Vec<(f32, f32, String)> = st
            .heads
            .iter()
            .filter_map(|(id, (x, y))| {
                let p = sim.people.get(*id)?;
                let (line, when) = p.said.as_ref()?;
                (p.alive && !p.asleep && sim.tick.saturating_sub(*when) <= fresh).then(|| (*x, *y, line.clone()))
            })
            .collect();
        // Nearer the camera (lower on screen) first.
        items.sort_by(|a, b| b.1.total_cmp(&a.1));
        let tt = t(Font::Sans, 12.0, TEXT);
        let pad = ui.px(7.0);
        let mut placed: Vec<Rect> = Vec::new();
        for (x, y, line) in items {
            let w = (ui.measure(&line, tt) + 2.0 * pad).min(ui.px(230.0));
            let h = ui.wrapped_height(w - 2.0 * pad, &line, tt) + 2.0 * pad;
            let r = Rect::new(x - w * 0.5, y - h - ui.px(10.0), w, h);
            if r.y < ui.px(46.0) || r.x < 0.0 || r.right() > ui.size.0 {
                continue;
            }
            let gap = ui.px(4.0);
            if placed.iter().any(|q| r.x < q.right() + gap && q.x < r.right() + gap && r.y < q.bottom() + gap && q.y < r.bottom() + gap) {
                continue;
            }
            ui.rect(r, hex_c("#F3EADB"), ui.px(8.0));
            ui.outline(r, hex_c("#2A1E1A"), ui.px(8.0));
            // The tail, pointing at the speaker.
            let tail = ui.px(6.0);
            ui.rect(Rect::new(x - tail * 0.5, r.bottom() - 1.0, tail, tail), hex_c("#F3EADB"), ui.px(2.0));
            ui.wrapped(r.x + pad, r.y + pad, w - 2.0 * pad, &line, t(Font::Sans, 12.0, hex_c("#2A1E1A")));
            placed.push(r);
        }
    }

    /// World settings (M7): the rules of this world, changed by commands like any god act.
    fn settings_panel(&mut self, sim: &Sim, st: &Status, acts: &mut Vec<Action>) {
        let ui = &mut self.ui;
        let (w, _) = ui.size;
        let pad = ui.px(12.0);
        let pw = ui.px(330.0);
        let r = Rect::new((w - pw) * 0.5, ui.px(102.0), pw, ui.px(270.0));
        ui.panel(r);
        ui.block(r);
        let mut y = r.y + pad;
        ui.text(r.x + pad, y, "World settings", t(Font::Pixel, 16.0, TEXT));
        if ui.button(Rect::new(r.right() - pad - ui.px(22.0), y - ui.px(2.0), ui.px(22.0), ui.px(22.0)), "x", false) {
            self.settings = false;
        }
        y += ui.px(30.0);
        let on = sim.settings.beauty_matters;
        ui.text(r.x + pad, y + ui.px(4.0), "Beauty matters", t(Font::Sans, 13.0, TEXT));
        let label = if on { "On" } else { "Off" };
        let bw = ui.button_width("Off").max(ui.px(48.0));
        let br = Rect::new(r.right() - pad - bw, y, bw, ui.px(26.0));
        if ui.button(br, label, on) {
            acts.push(Action::Command(sim::Command::SetBeautyMatters { on: !on }));
        }
        y += ui.px(34.0);
        let note = if on {
            "Flowers, statues and decorated homes lift the mood of anyone nearby, and ease boredom. Strolls head for lovely places."
        } else {
            "Beauty has no effect on anything: mood, needs or choices. People still make things, for the fun of it. The beauty lens still shows it."
        };
        y += ui.wrapped(r.x + pad, y, pw - 2.0 * pad, note, t(Font::Sans, 11.5, MUTED)) + ui.px(14.0);
        // The AI budget (M8).
        ui.rect(Rect::new(r.x + pad, y - ui.px(6.0), pw - 2.0 * pad, 1.0), BORDER, 0.0);
        ui.text(r.x + pad, y + ui.px(4.0), "AI budget", t(Font::Sans, 13.0, TEXT));
        let mut bx = r.right() - pad;
        let presets = [(Ai::Max, "Max"), (Ai::High, "High"), (Ai::Medium, "Medium"), (Ai::Low, "Low")];
        for (ai, label) in presets {
            let bw = ui.button_width(label).max(ui.px(40.0));
            bx -= bw;
            let br = Rect::new(bx, y, bw, ui.px(26.0));
            let ready = matches!(ai, Ai::Low | Ai::Max);
            if ui.button(br, label, st.ai == ai) && ready {
                acts.push(Action::SetAi(ai));
            }
            if ui.hovered(br) {
                ui.tooltip(match ai {
                    Ai::Low => vec!["Low: the rules brain decides. No servers, no models.".into()],
                    Ai::Medium | Ai::High => vec![format!("{label}: not yet. It needs the scorer and text model (M9) and a local language model (a download)."), "Until then it's Low.".into()],
                    Ai::Max => vec!["Max: Claude Haiku makes everyday decisions, through the bridge (tools/haiku_driver).".into(), "The game pauses while it decides, a few seconds at a time. It uses your Claude plan.".into()],
                });
            }
            bx -= ui.px(4.0);
        }
        y += ui.px(34.0);
        let note = match st.ai {
            Ai::Max => "Claude Haiku decides for everyone; the game waits while it thinks. Rules decide when an answer can't be used.",
            _ => "The rules brain decides for everyone. Speaking to someone is read by keyword.",
        };
        ui.wrapped(r.x + pad, y, pw - 2.0 * pad, note, t(Font::Sans, 11.5, MUTED));
    }

    fn toolbar(&mut self, sim: &Sim) {
        if let Some((id, words)) = self.speaking.clone() {
            let ui = &mut self.ui;
            let (w, h) = ui.size;
            let name = sim.people.get(id).map_or("them".to_string(), |p| p.name.clone());
            let bw = ui.px(560.0).min(w - ui.px(32.0));
            let bh = ui.px(64.0);
            let r = Rect::new((w - bw) * 0.5, h - bh - ui.px(16.0), bw, bh);
            ui.panel(r);
            ui.block(r);
            let pad = ui.px(12.0);
            let lt = t(Font::Sans, 11.5, MUTED);
            ui.text(r.x + pad, r.y + pad * 0.7, &format!("Say to {name} (Enter to speak, Esc to stop)"), lt);
            let typed = format!("{words}_");
            ui.text(r.x + pad, r.y + pad * 0.7 + ui.px(20.0), &typed, t(Font::Sans, 14.0, TEXT));
            let (_, read) = sim::speak::read(&words, &sim.content);
            let hint = if words.trim().is_empty() { "For example: \"fetch some wood\", \"plant flowers\", \"go and rest\"".to_string() } else if read.is_empty() { "Not understood yet: try words like wood, water, food, fire, build, farm, hunt, rest".to_string() } else { format!("Read as: {}", read.join(", ")) };
            let ht = t(Font::Sans, 11.0, if read.is_empty() { FAINT } else { GOLD });
            let hw = ui.measure(&hint, ht);
            ui.text(r.right() - pad - hw, r.y + pad * 0.7, &hint, ht);
            return;
        }
        let ui = &mut self.ui;
        let (w, h) = ui.size;
        let text = "God tools: speak to someone from their panel. Gifts arrive later (M10)";
        let tt = t(Font::Sans, 11.5, FAINT);
        let tw = ui.measure(text, tt) + ui.px(28.0);
        let bh = ui.px(32.0);
        let r = Rect::new((w - tw) * 0.5, h - bh - ui.px(16.0), tw, bh);
        ui.panel(r);
        ui.text(r.x + ui.px(14.0), r.y + (bh - ui.line_height(tt)) * 0.5, text, tt);
    }

    /// A small card for a thing under the cursor.
    fn thing_card(&mut self, sim: &Sim, pick: Pick, inspecting: bool) {
        let Some(info) = thing_info(sim, pick) else { return };
        let ui = &mut self.ui;
        let (cx, cy) = ui.input.cursor;
        let pad = ui.px(10.0);
        let body = t(Font::Sans, 12.5, TEXT);
        let w = (ui.measure(&info.summary, body).max(ui.measure(&info.title, t(Font::Pixel, 16.0, TEXT))) + 2.0 * pad + ui.px(2.0)).clamp(ui.px(160.0), ui.px(300.0));
        let h = pad * 2.0 + ui.px(22.0) + ui.wrapped_height(w - 2.0 * pad, &info.summary, body) + ui.px(18.0);
        let mut r = Rect::new(cx + ui.px(18.0), cy + ui.px(18.0), w, h);
        let limit = if inspecting { ui.size.0 - ui.px(432.0) } else { ui.size.0 };
        if r.right() > limit {
            r.x = cx - w - ui.px(18.0);
        }
        if r.bottom() > ui.size.1 {
            r.y = cy - h - ui.px(18.0);
        }
        // Never off the window or over the top bar.
        r.x = r.x.clamp(0.0, (ui.size.0 - w).max(0.0));
        r.y = r.y.clamp(ui.px(46.0), (ui.size.1 - h).max(ui.px(46.0)));
        ui.panel(r);
        let mut y = r.y + pad;
        ui.text(r.x + pad, y, &info.title, t(Font::Pixel, 16.0, TEXT));
        y += ui.px(22.0);
        y += ui.wrapped(r.x + pad, y, w - 2.0 * pad, &info.summary, body);
        ui.text(r.x + pad, y, "Click to inspect", t(Font::Sans, 11.0, FAINT));
    }

    /// The inspector for a thing: a resource, a grave, the fire, the stores.
    fn thing_inspector(&mut self, sim: &Sim, pick: Pick, acts: &mut Vec<Action>) {
        let Some(info) = thing_info(sim, pick) else {
            acts.push(Action::Select(None));
            return;
        };
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let w = ui.px(400.0);
        let top = ui.px(44.0) + ui.px(16.0);
        let content_h = {
            let mut hgt = ui.px(46.0) + ui.px(14.0) * 2.0 + ui.px(6.0);
            for sec in &info.sections {
                hgt += ui.px(28.0);
                let body = t(Font::Sans, 12.5, TEXT);
                let lh = ui.line_height(body);
                for r in &sec.rows {
                    let link_lines = (r.links.len() as f32 / 4.0).ceil();
                    let text = if r.value.is_empty() { 0.0 } else { ui.wrapped_height((w - ui.px(28.0)) * 0.64, &r.value, body) };
                    hgt += (link_lines * lh + if r.links.is_empty() { text } else { 0.0 }).max(lh) + ui.px(4.0);
                }
                if !sec.people.is_empty() {
                    hgt += ui.px(24.0) * ((sec.people.len() as f32) / 6.0).ceil();
                }
                if let Some(n) = &sec.note {
                    hgt += ui.wrapped_height(w - ui.px(28.0), n, t(Font::Sans, 11.5, FAINT)) + ui.px(4.0);
                }
                hgt += ui.px(8.0);
            }
            hgt
        };
        let panel = Rect::new(sw - w - ui.px(16.0), top, w, content_h.min(sh - top - ui.px(16.0)));
        ui.panel(panel);
        let pad = ui.px(14.0);
        let (x0, inner) = (panel.x + pad, w - 2.0 * pad);
        let mut y = panel.y + pad;
        let dot = ui.px(46.0);
        ui.rect(Rect::new(x0, y, dot, dot), info.colour, dot * 0.5);
        ui.outline(Rect::new(x0, y, dot, dot), GOLD, dot * 0.5);
        let nx = x0 + dot + ui.px(12.0);
        ui.text(nx, y + ui.px(2.0), &info.title, t(Font::Pixel, 20.0, TEXT));
        ui.text(nx, y + ui.px(26.0), &info.subtitle, t(Font::Sans, 12.5, MUTED));
        let close = Rect::new(panel.right() - pad - ui.px(28.0), y, ui.px(28.0), ui.px(28.0));
        if ui.button(close, "x", false) {
            acts.push(Action::Select(None));
        }
        y += dot + ui.px(14.0);
        let muted = t(Font::Sans, 12.5, MUTED);
        let body = t(Font::Sans, 12.5, TEXT);
        for sec in &info.sections {
            if y + ui.px(40.0) > panel.bottom() {
                break;
            }
            y += Self::section(ui, x0, y, &sec.title);
            for row in &sec.rows {
                let vx = x0 + inner * 0.36;
                let vw = inner * 0.64;
                let lh = ui.line_height(body);
                let top = y;
                ui.text(x0, y, &row.label, muted);
                let mut nx = vx;
                for (k, id) in row.links.iter().enumerate() {
                    let sep = if k + 1 < row.links.len() { ", " } else { "" };
                    let need = ui.measure(&format!("{}{sep}", sim.people.get(*id).map_or("", |o| &o.name)), body);
                    if nx + need > vx + vw {
                        nx = vx;
                        y += lh;
                    }
                    nx += Self::name_link(ui, nx, y, sim, *id, acts);
                    if !sep.is_empty() {
                        nx += ui.text(nx, y, sep, muted);
                    }
                }
                if !row.value.is_empty() {
                    if row.links.is_empty() {
                        y += ui.wrapped(vx, y, vw, &row.value, body).max(lh);
                    } else {
                        let rest = format!(", {}", row.value);
                        if nx + ui.measure(&rest, muted) <= vx + vw {
                            ui.text(nx, y, &rest, muted);
                            y += lh;
                        } else {
                            y += lh;
                            y += ui.wrapped(vx, y, vw, &row.value, muted);
                        }
                    }
                } else {
                    y += lh;
                }
                y += ui.px(4.0);
                let r = Rect::new(x0, top, inner, y - top);
                if !row.tip.is_empty() && ui.hovered(r) {
                    ui.tooltip(row.tip.clone());
                }
            }
            if !sec.people.is_empty() {
                let mut nx = x0;
                let lh = ui.px(24.0);
                for (k, (_, id)) in sec.people.iter().enumerate() {
                    let sep = if k + 1 < sec.people.len() { ", " } else { "" };
                    let need = ui.measure(&format!("{}{sep}", sim.people.get(*id).map_or("", |o| &o.name)), body);
                    if nx + need > x0 + inner {
                        nx = x0;
                        y += lh;
                    }
                    nx += Self::name_link(ui, nx, y, sim, *id, acts);
                    if !sep.is_empty() {
                        nx += ui.text(nx, y, sep, muted);
                    }
                }
                y += lh;
            }
            if let Some(n) = &sec.note {
                y += ui.wrapped(x0, y, inner, n, t(Font::Sans, 11.5, FAINT)) + ui.px(4.0);
            }
            y += ui.px(8.0);
        }
    }

    fn hover_card(&mut self, sim: &Sim, id: PersonId, inspecting: bool) {
        let Some(p) = sim.people.get(id) else { return };
        let c = &sim.content;
        let ui = &mut self.ui;
        let (cx, cy) = ui.input.cursor;
        let pad = ui.px(10.0);
        let doing = if !p.alive {
            format!("Died of {}", p.died.as_ref().map_or("unknown", |d| d.1.as_str()))
        } else if p.stage(sim.tick, c) == Stage::Baby {
            if p.asleep { "A baby, asleep".to_string() } else { "A baby, carried about".to_string() }
        } else if p.asleep {
            "Asleep".to_string()
        } else {
            p.action.as_ref().map_or("Thinking".to_string(), |a| a.label.clone())
        };
        let w = (ui.measure(&doing, t(Font::Sans, 12.5, TEXT)) + 2.0 * pad + ui.px(2.0)).clamp(ui.px(180.0), ui.px(300.0));
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
        // Never off the window or over the top bar.
        r.x = r.x.clamp(0.0, (ui.size.0 - w).max(0.0));
        r.y = r.y.clamp(ui.px(46.0), (ui.size.1 - h).max(ui.px(46.0)));
        ui.panel(r);
        let mut y = r.y + pad;
        let nw = ui.text(r.x + pad, y, &p.name, t(Font::Pixel, 16.0, TEXT));
        ui.text(r.x + pad + nw + ui.px(8.0), y + ui.px(3.0), &format!("{} · {}", p.age_years(sim.tick, c) as u32, role(p, sim.tick, c)), t(Font::Sans, 11.5, MUTED));
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
        let stage = p.stage(sim.tick, c);
        let sex = match (p.sex, stage) {
            (Sex::Female, Stage::Baby | Stage::Child) => "girl",
            (Sex::Male, Stage::Baby | Stage::Child) => "boy",
            (Sex::Female, _) => "woman",
            (Sex::Male, _) => "man",
        };
        let expecting = if p.pregnant.is_some() { " · expecting" } else { "" };
        ui.text(nx, y + ui.px(26.0), &format!("{} · {sex} · the camp{expecting}", role(p, sim.tick, c)), t(Font::Sans, 12.5, MUTED));
        let close = Rect::new(panel.right() - pad - ui.px(28.0), y, ui.px(28.0), ui.px(28.0));
        if ui.button(close, "x", false) {
            acts.push(Action::Select(None));
        }
        y += dot + ui.px(10.0);
        if !p.alive {
            // The dead: how and when, then their life and family (Life, Body, Skills).
            let (when, cause) = p.died.clone().unwrap_or((sim.tick, "unknown".into()));
            let resting = if p.buried { "buried" } else { "not yet buried" };
            y += ui.wrapped(x0, y, inner, &format!("Died of {cause}, {}; {resting}.", when_long(sim, when)), t(Font::Sans, 13.0, DANGER)) + ui.px(6.0);
            if matches!(self.tab, Tab::Mind | Tab::Log) {
                self.tab = Tab::Life;
            }
        } else if stage == Stage::Baby {
            let carer = sim::family::carer(&sim::family::seen(&sim.people, sim.tick, c), sim::family::find(&sim::family::seen(&sim.people, sim.tick, c), id).unwrap());
            let with = carer.and_then(|cid| sim.people.get(cid)).map_or("someone".to_string(), |q| q.name.clone());
            let doing = if p.asleep { format!("Asleep, with {with}") } else { format!("Carried by {with}") };
            y += ui.wrapped(x0, y, inner, &doing, t(Font::Sans, 13.0, TEXT)) + ui.px(6.0);
            if matches!(self.tab, Tab::Log) {
                self.tab = Tab::Mind;
            }
        } else {
            let doing = if p.asleep { "Asleep".to_string() } else { p.action.as_ref().map_or("Thinking".to_string(), |a| a.label.clone()) };
            let body = t(Font::Sans, 13.0, TEXT);
            let lbl = t(Font::Sans, 12.0, MUTED);
            let lw = ui.text(x0, y, "Now ", lbl);
            y += ui.wrapped(x0 + lw, y, inner - lw, &doing, body);
            y += ui.wrapped(x0, y + ui.px(2.0), inner, &format!("\"{}\"", p.thought), t(Font::Sans, 13.0, THOUGHT)) + ui.px(4.0);
            // What they heard you say (M8), while it still counts.
            if let Some(h) = p.heard.as_ref().filter(|h| sim.tick < h.until) {
                let line = format!("Heard you: \"{}\" (read as: {})", h.words, h.read_as.join(", "));
                y += ui.wrapped(x0, y, inner, &line, t(Font::Sans, 12.0, GOLD)) + ui.px(4.0);
            }
            if p.alive {
                let label = format!("Speak to {}", p.name);
                let bw = ui.button_width(&label);
                if ui.button(Rect::new(x0, y, bw, ui.px(24.0)), &label, self.speaking.as_ref().is_some_and(|s| s.0 == id)) {
                    self.speaking = Some((id, String::new()));
                }
                y += ui.px(30.0);
            }
        }
        if self.dev && p.alive && stage != Stage::Baby {
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
        let all_tabs = [(Tab::Mind, "Mind"), (Tab::Life, "Life"), (Tab::Body, "Body"), (Tab::Skills, "Skills"), (Tab::Log, "Log")];
        let tabs: Vec<(Tab, &str)> = all_tabs.into_iter().filter(|(t, _)| p.alive || !matches!(t, Tab::Mind | Tab::Log)).filter(|(t, _)| stage != Stage::Baby || *t != Tab::Log).collect();
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
            Tab::Life => self.life_tab(sim, id, p, area, acts),
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
        let (score, why) = p.mood(c, company, by_fire, sim.beauty_for(p.pos));
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
        // Hobbies (M7): what they've taken up, and what they're coming to enjoy.
        y += Self::section(ui, x, y, "Hobbies");
        let l = &c.leisure;
        let mut enjoyed: Vec<&sim::people::Hobby> = p.hobbies.iter().collect();
        enjoyed.sort_by(|a, b| b.since.is_some().cmp(&a.since.is_some()).then(b.joy.total_cmp(&a.joy)));
        if enjoyed.is_empty() {
            ui.text(x, y, "None yet: no free time spent on anything", t(Font::Sans, 12.0, MUTED));
            y += ui.px(22.0);
        }
        for h in enjoyed.iter().take(4) {
            let name = capital(sim::leisure::hobby_name(c, h.act));
            let row = Rect::new(x, y, w, ui.px(30.0));
            let nw = ui.text(x, y, &name, t(Font::Sans, 12.5, TEXT));
            let (word, col) = match h.since {
                Some(t0) => (format!("a hobby since {}", when_long(sim, t0)), GOLD),
                None => (format!("enjoying it ({} times)", h.times), MUTED),
            };
            ui.text(x + nw + ui.px(8.0), y, &word, t(Font::Sans, 12.0, col));
            let share = if h.since.is_some() { 1.0 } else { (h.joy / l.acts[h.act as usize].hobby_joy).min(1.0) };
            ui.bar(Rect::new(x, y + ui.px(19.0), w, ui.px(5.0)), share, if h.since.is_some() { GOLD } else { SKILL });
            if ui.hovered(row) {
                let suits = sim::leisure::appeal(p, c, h.act);
                ui.tooltip(vec![
                    format!("Done {} times; it suits their personality {}.", h.times, if suits > 0.6 { "well" } else if suits > 0.4 { "fairly" } else { "a little" }),
                    "Each time adds to how much they've enjoyed it: more when it suits them, and when they needed the fun. Past a point it becomes a hobby (two at most).".into(),
                ]);
            }
            y += ui.px(32.0);
        }
        y += ui.px(4.0);
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

    /// A name that selects that person when clicked; returns its width.
    fn name_link(ui: &mut Ui, x: f32, y: f32, sim: &Sim, id: PersonId, acts: &mut Vec<Action>) -> f32 {
        let Some(q) = sim.people.get(id) else { return 0.0 };
        let colour = if q.alive { GOLD } else { MUTED };
        let lt = t(Font::Sans, 12.5, colour);
        let w = ui.text(x, y, &q.name, lt);
        let r = Rect::new(x, y, w, ui.line_height(lt));
        if ui.hovered(r) {
            ui.rect(Rect::new(x, y + ui.line_height(lt) - ui.px(2.0), w, 1.0), colour, 0.0);
            let c = &sim.content;
            let state = if q.alive { format!("{}, {}", q.age_years(sim.tick, c) as u32, role(q, sim.tick, c).to_lowercase()) } else { format!("died aged {}", q.age_years(sim.tick, c) as u32) };
            ui.tooltip(vec![format!("{}: {state}", q.name), "Click to inspect".into()]);
        }
        if ui.clicked(r) {
            acts.push(Action::Select(Some(Pick::Person(id))));
        }
        w
    }

    fn life_tab(&mut self, sim: &Sim, id: PersonId, p: &Person, area: Rect, acts: &mut Vec<Action>) {
        let c = &sim.content;
        let analyst = self.detail == Detail::Analyst;
        let ui = &mut self.ui;
        let (x, w) = (area.x, area.w);
        let mut y = area.y;
        let muted = t(Font::Sans, 12.5, MUTED);
        let small = t(Font::Sans, 11.5, FAINT);
        // Who they are in the family's story.
        let mut facts = vec![if p.family.generation == 0 { "A founder of the camp".to_string() } else { format!("Generation {} in the camp", p.family.generation + 1) }];
        if let Some(g) = &p.pregnant {
            let days = (g.due.saturating_sub(sim.tick)) as f32 / c.calendar.ticks_per_day() as f32;
            let days = days.max(1.0).round();
            facts.push(if days <= 1.0 { "expecting a child (due within a day)".to_string() } else { format!("expecting a child (due in about {days:.0} days)") });
        }
        y += ui.wrapped(x, y, w, &facts.join(", "), muted) + ui.px(6.0);
        y += Self::section(ui, x, y, "Family");
        // Partner (or the one they lost).
        let late = sim.people.iter().filter(|(_, q)| !q.alive && q.family.partner == Some(id)).map(|(i, _)| i).last();
        let rows: Vec<(&str, Vec<PersonId>)> = vec![
            (if p.family.partner.is_some() || late.is_none() { "Partner" } else { "Late partner" }, p.family.partner.or(late).into_iter().collect()),
            ("Mother", p.family.mother.into_iter().collect()),
            ("Father", p.family.father.into_iter().collect()),
            ("Children", p.family.children.clone()),
            (
                "Siblings",
                sim.people
                    .iter()
                    .filter(|(q, other)| *q != id && other.family.mother.is_some() && (other.family.mother == p.family.mother || other.family.father == p.family.father) && p.family.mother.is_some())
                    .map(|(q, _)| q)
                    .collect(),
            ),
        ];
        let grown = p.grown(sim.tick, c);
        for (label, ids) in rows {
            if ids.is_empty() && !matches!(label, "Partner" | "Mother" | "Father") {
                continue;
            }
            if ids.is_empty() && label == "Partner" && !grown {
                continue;
            }
            ui.text(x, y, label, muted);
            let mut nx = x + w * 0.28;
            let lh = ui.line_height(muted);
            if ids.is_empty() {
                let none = if p.family.generation == 0 && label != "Partner" { "before the camp" } else { "none" };
                ui.text(nx, y, none, small);
            }
            for (k, q) in ids.iter().enumerate() {
                let sep = if k + 1 < ids.len() { ", " } else { "" };
                let need = ui.measure(&format!("{}{sep}", sim.people.get(*q).map_or("", |o| &o.name)), t(Font::Sans, 12.5, TEXT));
                if nx + need > x + w {
                    nx = x + w * 0.28;
                    y += lh;
                }
                nx += Self::name_link(ui, nx, y, sim, *q, acts);
                if !sep.is_empty() {
                    nx += ui.text(nx, y, sep, muted);
                }
            }
            y += lh + ui.px(3.0);
        }
        y += ui.px(6.0);
        // The people who matter most to them.
        y += Self::section(ui, x, y, "Close to");
        let mut rels: Vec<(PersonId, &sim::people::social::Rel)> = p.rels.iter().filter(|(q, _)| sim.people.get(**q).is_some_and(|o| o.alive || p.family.partner == Some(**q))).map(|(q, r)| (*q, r)).collect();
        rels.sort_by(|a, b| {
            let k = |r: &sim::people::social::Rel| r.familiarity * 0.5 + r.affection.abs() + r.romance;
            k(b.1).total_cmp(&k(a.1)).then(a.0.cmp(&b.0))
        });
        let shown = rels.iter().filter(|(_, r)| r.familiarity > 0.15 || r.affection.abs() > 0.15).take(6).count();
        if shown == 0 {
            ui.text(x, y, "Nobody yet.", small);
            y += ui.px(20.0);
        }
        for (q, r) in rels.iter().filter(|(_, r)| r.familiarity > 0.15 || r.affection.abs() > 0.15).take(6) {
            let row = Rect::new(x, y, w, ui.px(22.0));
            Self::name_link(ui, x, y, sim, *q, acts);
            // Family first ("mother", "son"), else how they feel ("close friend").
            let other = sim.people.get(*q);
            let female = other.is_some_and(|o| o.sex == Sex::Female);
            let word = if p.family.mother == Some(*q) {
                "mother"
            } else if p.family.father == Some(*q) {
                "father"
            } else if p.family.children.contains(q) {
                if female { "daughter" } else { "son" }
            } else if p.family.mother.is_some() && other.is_some_and(|o| o.family.mother == p.family.mother || (o.family.father.is_some() && o.family.father == p.family.father)) {
                if female { "sister" } else { "brother" }
            } else {
                r.word(p.family.partner == Some(*q))
            };
            ui.text(x + w * 0.28, y, word, muted);
            let bx = x + w * 0.62;
            let bw = w * 0.38;
            if r.affection >= 0.0 {
                ui.bar(Rect::new(bx, y + ui.px(5.0), bw, ui.px(7.0)), r.affection, AFFECTION);
            } else {
                ui.bar(Rect::new(bx, y + ui.px(5.0), bw, ui.px(7.0)), -r.affection, BAD);
            }
            if ui.hovered(row) {
                let name = sim.people.get(*q).map_or("them".to_string(), |o| o.name.clone());
                let mut lines = vec![format!("How {} feels about {name}", p.name)];
                if analyst {
                    lines.push(format!("affection {:+.2}, familiarity {:.2}, respect {:.2}, romance {:.2}", r.affection, r.familiarity, r.respect, r.romance));
                } else {
                    let fam = if r.familiarity > 0.6 { "knows them well" } else if r.familiarity > 0.3 { "knows them" } else { "barely knows them" };
                    let aff = if r.affection > 0.5 { "very fond of them" } else if r.affection > 0.15 { "likes them" } else if r.affection < -0.15 { "dislikes them" } else { "neither here nor there" };
                    lines.push(format!("{}, {}", capital(fam), aff));
                }
                lines.push(format!("Last together {}", ago(sim, r.last)));
                ui.tooltip(lines);
            }
            y += ui.px(22.0);
        }
        y += ui.px(6.0);
        // What they remember.
        if y + ui.px(60.0) < area.bottom() {
            y += Self::section(ui, x, y, "Memories");
            let mut mems: Vec<&sim::people::social::Memory> = p.memories.iter().collect();
            mems.sort_by_key(|m| std::cmp::Reverse(m.tick));
            if mems.is_empty() {
                ui.text(x, y, "Nothing that stays with them yet.", small);
            }
            for m in mems {
                if y + ui.px(20.0) > area.bottom() {
                    break;
                }
                let base = if m.feeling > 0.15 { GOOD } else if m.feeling < -0.15 { BAD } else { TEXT };
                let fade = 0.45 + 0.55 * m.strength.clamp(0.0, 1.0);
                let col = [base[0], base[1], base[2], fade];
                let when = ago(sim, m.tick);
                let ww = ui.measure(&when, small);
                ui.text(x + w - ww, y, &when, small);
                let row = Rect::new(x, y, w, ui.px(19.0));
                let mut text = m.text.clone();
                while ui.measure(&text, t(Font::Sans, 12.5, col)) > w - ww - ui.px(10.0) && text.len() > 4 {
                    text.pop();
                }
                ui.text(x, y, &text, t(Font::Sans, 12.5, col));
                if ui.hovered(row) {
                    let mut lines = vec![m.text.clone(), format!("Counts towards mood as \"{}\" while it's fresh", m.mood)];
                    if analyst {
                        lines.push(format!("feeling {:+.2}, importance {:.2}, strength {:.2}", m.feeling, m.importance, m.strength));
                    } else {
                        lines.push(if m.importance > 0.7 { "They'll remember this a long time.".into() } else { "This will fade.".into() });
                    }
                    ui.tooltip(lines);
                }
                y += ui.px(19.0);
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The History ledger

    /// The Arena ledger (M9): the last arena report, each measure as bars so the brains compare
    /// at a glance.
    fn arena_view(&mut self) {
        let report = self.arena.clone().unwrap_or_default();
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let top = ui.px(44.0);
        let page = Rect::new(0.0, top, sw, sh - top);
        ui.rect(page, hex_c("#1E1A1F"), 0.0);
        ui.block(page);
        let pad = ui.px(24.0);
        let mut y = top + ui.px(18.0);
        let bh = ui.px(30.0);
        let back = "< Back to the camp";
        let bw = ui.button_width(back);
        if ui.button(Rect::new(pad, y, bw, bh), back, false) {
            self.arena = None;
            return;
        }
        let x = pad + bw + ui.px(14.0);
        let tw = ui.text(x, y + ui.px(2.0), "Arena", t(Font::Pixel, 24.0, TEXT));
        let title = report.lines().find(|l| l.starts_with("# ")).map_or("No arena run yet".to_string(), |l| l.trim_start_matches("# ").trim_start_matches("Arena: ").to_string());
        ui.text(x + tw + ui.px(14.0), y + ui.px(9.0), &title, t(Font::Sans, 13.0, MUTED));
        y += ui.px(56.0);
        // The table: a header row, then one row per brain.
        let rows: Vec<Vec<String>> = report.lines().filter(|l| l.starts_with('|') && !l.starts_with("|---")).map(|l| l.trim_matches('|').split('|').map(|c| c.trim().to_string()).collect()).collect();
        if rows.len() < 2 {
            ui.wrapped(pad, y, sw - 2.0 * pad, "Run the arena from the repository folder, for example: colony-run arena --seeds 1..3 --days 48 --brains rules,scorer@rules-v1. Its report shows here.", t(Font::Sans, 13.0, MUTED));
            return;
        }
        let head = &rows[0];
        let brains: Vec<&Vec<String>> = rows[1..].iter().collect();
        let palette = [hex_c("#E8C468"), hex_c("#7FB8E6"), hex_c("#C3A6F0"), hex_c("#8CC56A"), hex_c("#F0997B")];
        // Legend: one colour per brain.
        let mut lx = pad;
        for (k, b) in brains.iter().enumerate() {
            let d = ui.px(10.0);
            ui.rect(Rect::new(lx, y + ui.px(4.0), d, d), palette[k % palette.len()], d * 0.5);
            lx += d + ui.px(6.0);
            lx += ui.text(lx, y, &b[0], t(Font::Sans, 13.0, TEXT)) + ui.px(18.0);
        }
        y += ui.px(30.0);
        let col_w = ((sw - 2.0 * pad) / 3.0).floor();
        let num = |s: &str| s.trim_end_matches('%').parse::<f32>().unwrap_or(0.0);
        for (i, name) in head.iter().enumerate().skip(1) {
            let cx = pad + ((i - 1) % 3) as f32 * col_w;
            let cy = y + ((i - 1) / 3) as f32 * ui.px(30.0 + 22.0 * brains.len() as f32);
            ui.text(cx, cy, name, t(Font::Sans, 12.5, MUTED));
            let max = brains.iter().map(|b| num(b.get(i).map_or("0", |s| s.as_str()))).fold(0.0f32, f32::max).max(1e-6);
            for (k, b) in brains.iter().enumerate() {
                let v = b.get(i).cloned().unwrap_or_default();
                let by = cy + ui.px(20.0 + 22.0 * k as f32);
                let w = (col_w - ui.px(90.0)) * (num(&v) / max).clamp(0.0, 1.0);
                ui.rect(Rect::new(cx, by + ui.px(3.0), w.max(ui.px(2.0)), ui.px(12.0)), palette[k % palette.len()], ui.px(3.0));
                ui.text(cx + w + ui.px(8.0), by, &v, t(Font::Sans, 12.0, TEXT));
            }
        }
    }

    fn history_view(&mut self, sim: &Sim, acts: &mut Vec<Action>) {
        let c = &sim.content;
        let min_importance = match self.detail {
            Detail::Minimal => 0.6,
            Detail::Normal => 0.3,
            Detail::Analyst => 0.0,
        };
        let entries: Vec<&sim::chronicle::Entry> = sim.chronicle.entries.iter().filter(|e| e.importance >= min_importance).collect();
        let scroll = self.ui.input.scroll;
        let mut first = self.history.unwrap_or(0);
        if scroll != 0.0 {
            first = (first as i64 + if scroll > 0.0 { -3 } else { 3 }).clamp(0, entries.len().saturating_sub(1) as i64) as usize;
        }
        self.history = Some(first);
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let top = ui.px(44.0);
        let page = Rect::new(0.0, top, sw, sh - top);
        ui.rect(page, hex_c("#1E1A1F"), 0.0);
        ui.block(page);
        let pad = ui.px(24.0);
        let mut y = top + ui.px(18.0);
        let bh = ui.px(30.0);
        let back = "< Back to the camp";
        let bw = ui.button_width(back);
        if ui.button(Rect::new(pad, y, bw, bh), back, false) {
            self.history = None;
            return;
        }
        let mut x = pad + bw + ui.px(14.0);
        x += ui.text(x, y + ui.px(2.0), "History", t(Font::Pixel, 24.0, TEXT)) + ui.px(14.0);
        // The camp in numbers.
        let living = sim.alive();
        let born = sim.people.iter().filter(|(_, p)| p.family.mother.is_some()).count();
        let died = sim.people.iter().filter(|(_, p)| !p.alive).count();
        let gens = sim.people.iter().map(|(_, p)| p.family.generation).max().unwrap_or(0) + 1;
        let lost: Vec<String> = c
            .skills
            .skills
            .iter()
            .enumerate()
            .filter(|(k, s)| s.taught && !sim.people.iter().any(|(_, p)| p.alive && p.skill_level(*k, c) >= 1))
            .map(|(_, s)| s.name.to_lowercase())
            .collect();
        let mut summary = format!("Year {} · {living} living · {gens} generations · {born} born here, {died} dead", c.calendar.year(sim.tick));
        if !lost.is_empty() {
            summary += &format!(" · lost: {}", lost.join(", "));
        }
        ui.text(x, y + ui.px(8.0), &summary, t(Font::Sans, 13.0, MUTED));
        let shown = match self.detail {
            Detail::Minimal => "Showing the great events (Detail: minimal)",
            Detail::Normal => "Showing the main events (Detail: normal)",
            Detail::Analyst => "Showing everything (Detail: analyst)",
        };
        let st = t(Font::Sans, 11.5, FAINT);
        let stw = ui.measure(shown, st);
        ui.text(sw - pad - stw, y + ui.px(9.0), shown, st);
        y += bh + ui.px(18.0);
        let panel = Rect::new(pad, y, sw - 2.0 * pad, sh - y - ui.px(20.0));
        ui.panel(panel);
        let ip = ui.px(18.0);
        let mut ey = panel.y + ip;
        let entry_t = t(Font::Sans, 13.0, TEXT);
        let date_t = t(Font::Sans, 11.5, FAINT);
        let text_x = panel.x + ip + ui.px(170.0);
        let text_w = panel.right() - ip - text_x;
        let mut last_year = 0;
        // Newest first.
        let order: Vec<&sim::chronicle::Entry> = entries.iter().rev().copied().collect();
        if order.is_empty() {
            ui.text(panel.x + ip, ey, "Nothing has happened yet.", t(Font::Sans, 13.0, FAINT));
        }
        for e in order.iter().skip(first) {
            let year = c.calendar.year(e.tick);
            if year != last_year {
                if ey + ui.px(60.0) > panel.bottom() {
                    break;
                }
                if last_year != 0 {
                    ey += ui.px(8.0);
                }
                ui.text(panel.x + ip, ey, &format!("Year {year}"), t(Font::Sans, 15.0, GOLD));
                ui.rect(Rect::new(panel.x + ip, ey + ui.px(24.0), panel.w - 2.0 * ip, 1.0), BORDER, 0.0);
                ey += ui.px(32.0);
                last_year = year;
            }
            let h = ui.wrapped_height(text_w, &e.text, entry_t).max(ui.line_height(entry_t));
            if ey + h > panel.bottom() - ip {
                break;
            }
            let row = Rect::new(panel.x + ui.px(6.0), ey - ui.px(3.0), panel.w - ui.px(12.0), h + ui.px(6.0));
            let person = e.people.first().copied();
            if person.is_some() && ui.hovered(row) {
                ui.rect(row, CHIP, ui.px(6.0));
            }
            let kind = match e.kind {
                sim::chronicle::Kind::Life => FeedKind::Life,
                sim::chronicle::Kind::Social => FeedKind::Social,
                sim::chronicle::Kind::Knowledge => FeedKind::Knowledge,
                sim::chronicle::Kind::World => FeedKind::World,
                sim::chronicle::Kind::Trouble => FeedKind::Trouble,
            };
            let d = ui.px(8.0);
            ui.rect(Rect::new(panel.x + ip, ey + ui.px(5.0), d, d), kind.colour(), d * 0.5);
            let date = format!("{} day {}, {}", capital(c.calendar.season(e.tick)), c.calendar.day_of_season(e.tick), clock(sim, e.tick));
            ui.text(panel.x + ip + ui.px(16.0), ey + ui.px(2.0), &date, date_t);
            let col = if e.importance >= 0.8 { TEXT } else { THOUGHT };
            ui.wrapped(text_x, ey, text_w, &e.text, t(Font::Sans, 13.0, col));
            if let Some(q) = person
                && ui.clicked(row)
            {
                acts.push(Action::Select(Some(Pick::Person(q))));
                self.history = None;
                self.tab = Tab::Life;
            }
            ey += h + ui.px(8.0);
        }
        let more = order.len().saturating_sub(first);
        if first > 0 || more > 0 {
            let s = format!("{} of {} entries · scroll for more", order.len().min(more), order.len());
            let w = ui.measure(&s, date_t);
            ui.text(panel.right() - ip - w, panel.bottom() - ip - ui.px(4.0), &s, date_t);
        }
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
            acts.push(Action::Select(Some(Pick::Person(pid))));
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

/// A short role: their stage of life, or for grown people what they've practised most.
pub fn role(p: &Person, tick: u64, c: &sim::Content) -> &'static str {
    match p.stage(tick, c) {
        Stage::Baby => return "Baby",
        Stage::Child => return "Child",
        Stage::Elder => return "Elder",
        Stage::Adult => {}
    }
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

// ---------------------------------------------------------------------------------------------
// Things: what the inspector and hover card say about a node, a grave, the fire or the stores.

struct Row {
    label: String,
    value: String,
    tip: Vec<String>,
    /// People named in the row, drawn as links before the value.
    links: Vec<PersonId>,
}

struct Section {
    title: String,
    rows: Vec<Row>,
    /// People to list as links.
    people: Vec<(String, PersonId)>,
    note: Option<String>,
}

struct ThingInfo {
    title: String,
    subtitle: String,
    colour: [f32; 4],
    /// One line for the hover card.
    summary: String,
    sections: Vec<Section>,
}

fn row(label: &str, value: impl Into<String>) -> Row {
    Row { label: label.into(), value: value.into(), tip: Vec::new(), links: Vec::new() }
}

fn row_tip(label: &str, value: impl Into<String>, tip: &[&str]) -> Row {
    Row { label: label.into(), value: value.into(), tip: tip.iter().map(|s| s.to_string()).collect(), links: Vec::new() }
}

/// A row naming people (as links), with optional words after them.
fn row_people(label: &str, people: Vec<PersonId>, after: impl Into<String>) -> Row {
    Row { label: label.into(), value: after.into(), tip: Vec::new(), links: people }
}

fn section(title: &str, rows: Vec<Row>) -> Section {
    Section { title: title.into(), rows, people: Vec::new(), note: None }
}

/// "12 m north-east of the camp" (or "at the camp").
fn from_camp(sim: &Sim, at: (f32, f32)) -> String {
    let camp = sim.camp.stores_pos();
    let d = ((at.0 - camp.0).powi(2) + (at.1 - camp.1).powi(2)).sqrt();
    if d < 4.0 {
        return "at the camp".into();
    }
    let dir = sim::actions::direction(camp, at).trim_start_matches("to the ");
    format!("{d:.0} m {dir} of the camp")
}

fn plural(n: f32, one: &str) -> String {
    let n = n.floor().max(0.0) as u32;
    let lower = one.to_lowercase();
    if n == 1 { format!("1 {}", lower.trim_end_matches('s')) } else { format!("{n} {lower}") }
}

fn thing_info(sim: &Sim, pick: Pick) -> Option<ThingInfo> {
    let c = &sim.content;
    let alive = sim.alive();
    match pick {
        Pick::Person(_) => None,
        Pick::Fire => {
            let camp = &sim.camp;
            let pos = camp.fire_pos();
            let (sticks, wood) = (c.item("sticks"), c.item("wood"));
            let warm = sim.people.iter().filter(|(_, p)| p.alive && ((p.pos.0 - pos.0).powi(2) + (p.pos.1 - pos.1).powi(2)).sqrt() <= c.people.fire_reach).count();
            let fire = c.skill("firemaking");
            let knowers = sim.people.iter().filter(|(_, p)| p.alive && p.skill_level(fire, c) >= 1).count();
            let fuel = camp.stores[wood] as f32 * c.items[wood].fuel + camp.stores[sticks] as f32 * c.items[sticks].fuel;
            let state = if camp.fire_lit { format!("lit, about {:.0} h of fuel left", camp.fire_hours) } else { "out".to_string() };
            let mut now = section("Now", vec![
                row("Burning", state.clone()),
                row_tip("Warms", format!("within {:.0} m ({warm} there now)", c.people.fire_reach), &["People close to a lit fire get warm.", "Without one, cold nights hurt, and in winter they can kill."]),
            ]);
            if let Some((who, when)) = camp.lit_by
                && sim.people.get(who).is_some()
            {
                now.rows.push(row_people("Last lit by", vec![who], when_long(sim, when)));
            }
            now.rows.push(row_tip(
                "Can light it",
                if knowers == 0 { "nobody living".to_string() } else { format!("{knowers} people know how") },
                &["Firemaking has to be taught. If the last person who knows it dies, nobody can light a fire."],
            ));
            let stock = section("Fuel in the stores", vec![
                row("Wood", format!("{} ({:.0} h)", camp.stores[wood], camp.stores[wood] as f32 * c.items[wood].fuel)),
                row("Sticks", format!("{} ({:.0} h, and kindling)", camp.stores[sticks], camp.stores[sticks] as f32 * c.items[sticks].fuel)),
                row("In all", format!("about {fuel:.0} more hours")),
            ]);
            Some(ThingInfo {
                title: "The fire".into(),
                subtitle: format!("The camp's fire pit, {}", if camp.fire_lit { "burning" } else { "cold" }),
                colour: if camp.fire_lit { hex_c("#FFB040") } else { hex_c("#6E6A68") },
                summary: format!("Fire: {state}"),
                sections: vec![now, stock],
            })
        }
        Pick::Stores => {
            let st = &sim.camp.stores;
            let berries = c.item("berries");
            let days = sim::actions::food_in(c, st) / (alive.max(1) as f32 * sim::actions::daily_hunger(c));
            let kept = sim::buildings::has_storehouse(&sim.structures, c);
            let mut rows = vec![row_tip(
                "Food in all",
                format!("enough for {alive} for about {days:.1} days"),
                &["Berries, grain and meat together. People eat meat first (it spoils fastest), then berries, then grain."],
            )];
            for (i, it) in c.items.iter().enumerate() {
                let (open, keep) = c.spoil(i);
                let rot = if open > 0.0 { format!(", {:.1}% a day spoils", if kept { keep } else { open } * 100.0) } else { String::new() };
                let what = match it.id.as_str() {
                    "berries" | "grain" | "meat" => format!("{}{rot}", st[i]),
                    "water" => format!("{} ({} drinks)", st[i], st[i]),
                    "wood" | "sticks" => format!("{} ({:.0} h of fire)", st[i], st[i] as f32 * it.fuel),
                    _ => format!("{}", st[i]),
                };
                rows.push(row(&it.name, what));
            }
            let knows = sim.people.iter().filter(|(_, p)| p.alive && sim.tick.saturating_sub(p.knowledge.stores.when) < 6 * 60).count();
            let mut who = section("People", vec![row_tip(
                "Up to date",
                format!("{knows} of {alive} looked in the last 6 h"),
                &["People decide from what they last saw here, which may be out of date."],
            )]);
            who.note = Some(if kept {
                "Gatherers bring what they find here; anyone can eat or drink from it. The storehouse keeps the food from spoiling quickly."
            } else {
                "Gatherers bring what they find here; anyone can eat or drink from it. Food left in the open spoils; a storehouse would keep it."
            }.into());
            Some(ThingInfo {
                title: "The stores".into(),
                subtitle: "The camp's food, water and firewood".into(),
                colour: hex_c("#9C6B3F"),
                summary: format!("Food for {days:.1} days; {} berries, {} grain, {} water, {} wood", st[berries], st[c.item("grain")], st[c.item("water")], st[c.item("wood")]),
                sections: vec![section("Holding", rows), who],
            })
        }
        Pick::Animal(id) => animal_info(sim, id),
        Pick::Structure(id) => structure_info(sim, id),
        Pick::Node(id) => {
            let n = sim.nodes.get(id)?;
            let def = &c.nodes[n.kind as usize];
            let at = (n.x as f32 + 0.5, n.y as f32 + 0.5);
            let colour = hex_c(&def.colour);
            if def.id == "grave" {
                // Who lies here.
                let dead = sim.people.iter().find(|(_, p)| p.grave == Some((n.x, n.y)));
                let Some((pid, p)) = dead else {
                    return Some(ThingInfo { title: "A grave".into(), subtitle: from_camp(sim, at), colour, summary: "A grave".into(), sections: Vec::new() });
                };
                let age = p.age_years(sim.tick, c) as u32;
                let (when, cause) = p.died.clone().unwrap_or((sim.tick, "unknown".into()));
                let mut here = section("Here lies", vec![
                    row_people("Name", vec![pid], ""),
                    row("Born", if p.family.generation == 0 { "before the camp".to_string() } else { when_long(sim, p.born.max(0) as u64) }),
                    row("Died", format!("{}, aged {age}", when_long(sim, when))),
                    row("Of", cause.clone()),
                ]);
                if let Some(by) = p.buried_by.filter(|b| sim.people.get(*b).is_some()) {
                    here.rows.push(row_people("Buried by", vec![by], ""));
                }
                // The family: their partner (or the partner who outlived them), parents, children.
                let partner = p.family.partner.or_else(|| sim.people.iter().find(|(_, q)| q.family.partner == Some(pid)).map(|(i, _)| i));
                let mut family = section("Family", Vec::new());
                for (label, ids) in [
                    ("Partner", partner.into_iter().collect::<Vec<_>>()),
                    ("Mother", p.family.mother.into_iter().collect()),
                    ("Father", p.family.father.into_iter().collect()),
                    ("Children", p.family.children.clone()),
                ] {
                    let ids: Vec<PersonId> = ids.into_iter().filter(|q| sim.people.get(*q).is_some()).collect();
                    if !ids.is_empty() {
                        family.rows.push(row_people(label, ids, ""));
                    }
                }
                if family.rows.is_empty() {
                    family.note = Some("No family in the camp.".into());
                }
                let mut remembered = section("Remembered by", Vec::new());
                for (q, o) in sim.people.iter().filter(|(_, o)| o.alive && o.memories.iter().any(|m| m.about == Some(pid))) {
                    remembered.people.push((o.name.clone(), q));
                }
                if remembered.people.is_empty() {
                    remembered.note = Some("Nobody living still thinks of them.".into());
                }
                return Some(ThingInfo {
                    title: format!("Grave of {}", p.name),
                    subtitle: format!("{} · {}", from_camp(sim, at), if p.sex == Sex::Female { "a woman" } else { "a man" }),
                    colour,
                    summary: format!("{}, aged {age}, died of {cause}", p.name),
                    sections: vec![here, family, remembered],
                });
            }
            let uses: Vec<&sim::content::GatherDef> = c.actions.gather.iter().filter(|g| g.node == def.id).collect();
            if uses.is_empty() {
                // Beauty (M7): flowers, flower beds and statues; and scenery (reeds).
                let source = c.leisure.beauty.sources.iter().find(|s| s.0 == def.id);
                let work = sim.works.iter().find(|w| w.node == id);
                let mut sections = Vec::new();
                if let Some(w) = work {
                    let mut made = section("Made", vec![row("When", when_long(sim, w.when))]);
                    if sim.people.get(w.by).is_some() {
                        made.rows.insert(0, row_people("By", vec![w.by], ""));
                    }
                    if def.id == "flower_bed" {
                        made.rows.push(row_tip("Blooming", format!("{:.0}%", n.amount * 100.0), &["A flower bed fades unless someone tends it."]));
                    }
                    sections.push(made);
                }
                let here = sim::leisure::beauty_at(&sim.beauty, &sim.map, at);
                let mut b = section("Beauty", Vec::new());
                if source.is_some() {
                    let reach = source.map_or(0.0, |s| s.2);
                    b.rows.push(row_tip("Here", format!("{:.0}% (spreads about {reach:.0} m)", here * 100.0), &["Beauty adds up from flowers, statues and decorated homes nearby. The beauty lens shows it."]));
                    b.note = Some(if sim.settings.beauty_matters {
                        "People nearby are in a better mood, and less bored.".into()
                    } else {
                        "Beauty doesn't matter in this world (World settings), so it changes nothing.".into()
                    });
                } else {
                    b.rows.push(row("Gathered", "not yet"));
                    b.note = Some("Nobody uses these yet.".into());
                }
                sections.push(b);
                let summary = match work {
                    Some(w) => format!("{} made by {}", def.name, w_name(sim, w)),
                    None => def.name.clone(),
                };
                return Some(ThingInfo { title: def.name.clone(), subtitle: from_camp(sim, at), colour, summary, sections });
            }
            let item = &c.items[c.item(&uses[0].item)];
            let left = plural(n.amount, &item.name);
            let grows = if def.id == "berry_bush" {
                let r = &c.actions.berry_regrow;
                format!("{:.0}/{:.0}/{:.0} a day in spring/summer/autumn; none in winter ({:.0}% rots)", r[0], r[1], r[2], c.actions.winter_rot * 100.0)
            } else if def.regrow > 0.0 {
                format!("{} a day, up to {:.0}", def.regrow, def.amount)
            } else {
                "doesn't; gone once used up".to_string()
            };
            let here = section("What's here", vec![
                row("Left", left.clone()),
                row_tip("Grows back", grows, &["Regrowth happens each morning."]),
            ]);
            let mut used = section("Used for", Vec::new());
            for g in &uses {
                let k = c.skill(&g.skill);
                let sk = &c.skills.skills[k];
                used.rows.push(row_tip(&g.name, format!("{} · {} min for {}", sk.name.to_lowercase(), g.minutes, g.yield_per), &["Faster with skill: 0.6x at level 0 up to 1.2x at level 10."]));
                if sk.taught && !sim.people.iter().any(|(_, p)| p.alive && p.skill_level(k, c) >= 1) {
                    used.note = Some(format!("Nobody living knows {}, so nobody can do this.", sk.name.to_lowercase()));
                }
            }
            let known = sim.people.iter().filter(|(_, p)| p.alive && p.knowledge.nodes.contains_key(&id)).count();
            let mut people = section("People", vec![row_tip(
                "Known to",
                format!("{known} of {alive}"),
                &["People only go to places they know: seen themselves, or told about."],
            )]);
            for (q, p) in sim.people.iter().filter(|(_, p)| p.alive) {
                let here = matches!(p.action.as_ref().map(|a| &a.kind), Some(sim::actions::Kind::Gather { node, .. }) | Some(sim::actions::Kind::EatAt { node }) if *node == id);
                if here {
                    people.people.push((p.name.clone(), q));
                }
            }
            if people.people.is_empty() {
                people.note = Some("Nobody is working here right now.".into());
            } else {
                people.rows.push(row("Working here", format!("{}", people.people.len())));
            }
            Some(ThingInfo {
                title: def.name.clone(),
                subtitle: from_camp(sim, at),
                colour,
                summary: format!("{left} left"),
                sections: vec![here, used, people],
            })
        }
    }
}

/// Who made a thing (their name, even if they've since died).
fn w_name(sim: &Sim, w: &sim::sim::Work) -> String {
    sim.people.get(w.by).map_or(w.name.clone(), |p| p.name.clone())
}

/// The era the camp is in: foraging until the first home or field, then settling.
fn era(sim: &Sim) -> &'static str {
    let c = &sim.content;
    let settled = sim.structures.iter().any(|(_, s)| s.done() && (c.buildings[s.kind as usize].id == "house" || s.crop.is_some() || s.harvested > 0));
    if settled { "Era 2, settling" } else { "Era 1, foraging" }
}

/// An animal: what it is, what it's doing, its herd, and how its kind is faring.
fn animal_info(sim: &Sim, id: sim::animals::AnimalId) -> Option<ThingInfo> {
    let c = &sim.content;
    let a = sim.animals.get(id)?;
    let def = &c.animals[a.kind as usize];
    let name = def.name.to_lowercase();
    let what = match (def.id.as_str(), a.female) {
        ("deer", false) => "A stag".to_string(),
        ("deer", true) => "A doe".to_string(),
        (_, _) => format!("A {name}"),
    };
    let days = sim.tick.saturating_sub(a.born) / c.calendar.ticks_per_day();
    let years = sim.tick.saturating_sub(a.born) as f64 / (c.calendar.ticks_per_day() * c.calendar.days_per_year()) as f64;
    let age = if days < c.calendar.days_per_season as u64 {
        "young, born this spring".to_string()
    } else if years < 1.0 {
        "under a year old".to_string()
    } else if years < 2.0 {
        "a year old".to_string()
    } else {
        format!("{:.0} years old", years.floor())
    };
    let doing = match a.state {
        sim::animals::State::Graze => "grazing",
        sim::animals::State::Walk => "wandering",
        sim::animals::State::Rest => "resting",
        sim::animals::State::Flee => "running from people",
    };
    let herd = sim.animals.iter().filter(|(_, b)| b.herd == a.herd).count();
    let all = sim.animals.iter().filter(|(_, b)| b.kind == a.kind).count();
    let cap = sim.wildlife.capacity[a.kind as usize];
    let hunted = sim.wildlife.hunted[a.kind as usize];
    let fear = sim.wildlife.fear_at(&sim.map, a.pos.0, a.pos.1);
    let now = section("Now", vec![
        row("Doing", doing),
        row("Age", age),
        row_tip("Herd", format!("{herd} together"), &["Herds keep to a home range, which drifts slowly over good ground."]),
        row_tip(
            "Wary",
            format!("runs from people within about {:.0} m{}", def.wary * (1.0 + fear * 0.8), if fear > 0.2 { " (hunted ground: warier)" } else { "" }),
            &["A hunter stalking with a spear gets much closer before they bolt."],
        ),
    ]);
    let mut land = section(&format!("{} in the land", capital(&def.name)), vec![
        row_tip("Count", format!("{all} (the land holds about {cap})"), &["Young are born each spring, fewer when the land is full. Winter thins herds above what the land holds; old age takes the rest.", "When numbers fall low, new herds wander in from the edges of the map."]),
        row("Hunted", format!("{hunted} so far")),
        row("Meat", format!("{} for a kill", def.meat)),
    ]);
    if fear > 0.2 {
        land.note = Some("This ground has been hunted: herds move their range away from it until the fear fades.".into());
    }
    Some(ThingInfo {
        title: what.clone(),
        subtitle: from_camp(sim, a.pos),
        colour: hex_c(if def.id == "deer" { "#9C6B3F" } else { "#B7A58C" }),
        summary: format!("{what}, {doing}"),
        sections: vec![now, land],
    })
}

/// A building, building site or field.
fn structure_info(sim: &Sim, id: sim::buildings::StructureId) -> Option<ThingInfo> {
    let c = &sim.content;
    let s = sim.structures.get(id)?;
    let def = &c.buildings[s.kind as usize];
    let name = def.name.to_lowercase();
    let centre = s.centre(c);
    let owner = s.owner.and_then(|o| sim.people.get(o).map(|p| (o, p)));
    let mut sections = Vec::new();
    // Who's working on it right now.
    let working: Vec<PersonId> = sim
        .people
        .iter()
        .filter(|(_, p)| {
            p.alive
                && matches!(p.action.as_ref().map(|a| &a.kind), Some(sim::actions::Kind::Build { site }) | Some(sim::actions::Kind::Sow { field: site }) | Some(sim::actions::Kind::Tend { field: site }) | Some(sim::actions::Kind::Harvest { field: site }) if *site == id)
        })
        .map(|(q, _)| q)
        .collect();
    let title = match (&owner, def.id.as_str()) {
        (_, "field") => "A grain field".to_string(),
        (_, "storehouse") => "The storehouse".to_string(),
        (Some((_, o)), _) => format!("{}'s {name}", o.name),
        (None, _) => format!("An empty {name}"),
    };
    if !s.done() {
        let owed = s.owed(c);
        let owed_s = if owed.is_empty() { "everything is here".to_string() } else { owed.iter().map(|(i, n)| format!("{n} {}", c.items[*i].name.to_lowercase())).collect::<Vec<_>>().join(", ") };
        let mut b = section("Being built", vec![
            row_tip("Done", format!("{:.0}%", s.progress * 100.0), &[&format!("{} minutes of work in all; skilled builders work faster.", def.minutes)]),
            row_tip("Still needs", owed_s, &["Builders carry materials from the stores. Work can't get ahead of what's been brought."]),
            row("Started", when_long(sim, s.started)),
        ]);
        if !working.is_empty() {
            b.rows.push(row_people("Working on it", working.clone(), ""));
        } else {
            b.note = Some("Nobody is working on it right now.".into());
        }
        sections.push(b);
    }
    let summary;
    if def.id == "field" {
        let f = &c.farming;
        let mut rows = Vec::new();
        let (mut moist, mut fert) = (0.0, 0.0);
        for y in s.y..s.y + def.h {
            for x in s.x..s.x + def.w {
                let i = sim.map.idx(x as u32, y as u32);
                moist += sim.map.moisture[i];
                fert += sim.map.fertility[i];
            }
        }
        let tiles = (def.w * def.h) as f32;
        let (moist, fert) = (moist / tiles, fert / tiles);
        let season = c.calendar.season_index(sim.tick);
        let temp = sim::buildings::day_temp(c, &sim.map, centre.0 as u32, centre.1 as u32, season);
        let g = sim::buildings::growth_factors(c, moist, fert, temp, season);
        summary = match &s.crop {
            None if s.done() => "Ready to sow".to_string(),
            None => format!("Being cleared, {:.0}%", s.progress * 100.0),
            Some(k) if k.dead => "The crop died in a frost".to_string(),
            Some(k) if k.growth >= 1.0 => "Ripe: ready to harvest".to_string(),
            Some(k) => format!("Grain growing, {:.0}% grown", k.growth * 100.0),
        };
        if let Some(k) = &s.crop {
            let per_day = g.total / f.days_to_ripe * (1.0 - k.weeds * 0.6);
            let left = if k.growth >= 1.0 || k.dead || per_day <= 0.0 { None } else { Some((1.0 - k.growth) / per_day) };
            rows.push(row("Crop", summary.clone()));
            if !k.dead {
                rows.push(row_tip("Ripe in", left.map_or(if k.growth >= 1.0 { "now".to_string() } else { "not while it's this cold".to_string() }, |d| format!("about {d:.0} days at this rate")), &["Growth depends on the soil's moisture and richness, the warmth and the season. Weeds slow it."]));
                rows.push(row_tip("Health", format!("{:.0}%", k.health * 100.0), &["The share of a full harvest it will give. Frost scorches young grain; heavy weeds and leaving it standing once ripe lose some."]));
                rows.push(row_tip("Weeds", format!("{:.0}%", k.weeds * 100.0), &["Weeds grow every day and slow the grain; weeding pulls them."]));
            }
            if let Some(by) = k.by.filter(|b| sim.people.get(*b).is_some()) {
                rows.push(row_people("Sown by", vec![by], format!(" {}", when_long(sim, k.sown))));
            }
        }
        let pc = |v: f32| format!("{:.0}%", v * 100.0);
        sections.push(section("The crop", rows));
        let mut grow = section("Growing conditions today", vec![
            row_tip("Moisture", format!("{} (soil {})", pc(g.moisture), pc(moist)), &[&format!("Best between {} and {} soil moisture. Rain wets the soil; warm dry days dry it.", pc(f.best_moisture.0), pc(f.best_moisture.1))]),
            row_tip("Richness", format!("{} (soil {})", pc(g.fertility), pc(fert)), &["Rich soil grows grain faster. The soil lens shows where it's rich."]),
            row_tip("Warmth", format!("{} ({temp:.0} C)", pc(g.warmth)), &[&format!("Best between {:.0} and {:.0} C. Below {:.0} C at night, frost scorches unripe grain.", f.best_temp.0, f.best_temp.1, f.frost)]),
            row("Season", pc(g.season)),
        ]);
        grow.note = Some(format!("Altogether {} of the best growth.", pc(g.total)));
        sections.push(grow);
        let mut work = section("Work", vec![row("Harvested", format!("{} grain from this field so far", s.harvested))]);
        if !working.is_empty() && s.done() {
            work.rows.push(row_people("Working here", working, ""));
        }
        sections.push(work);
    } else {
        let mut home = Vec::new();
        if def.sleeps > 0 {
            let lives: Vec<PersonId> = sim
                .people
                .iter()
                .filter(|(q, p)| p.alive && sim::settle::home_of(p, *q, p.stage(sim.tick, c) >= Stage::Adult, &sim.structures, c) == Some(id))
                .map(|(q, _)| q)
                .collect();
            let inside: Vec<PersonId> = lives.iter().copied().filter(|q| sim.people.get(*q).is_some_and(|p| p.asleep && p.indoors.is_some())).collect();
            summary = if !s.done() { format!("Being built, {:.0}%", s.progress * 100.0) } else if lives.is_empty() { format!("An empty {name}") } else { format!("Home to {}", lives.len()) };
            if lives.is_empty() {
                home.push(row("Lives here", "nobody"));
            } else {
                home.push(row_people("Lives here", lives.clone(), ""));
            }
            if !inside.is_empty() {
                home.push(row_people("Asleep inside", inside, ""));
            }
            home.push(row_tip("Sleeps", format!("up to {}", def.sleeps), &["Families live in houses; people on their own in shelters, until they have a family."]));
            home.push(row_tip("Shelter", format!("keeps out {:.0}% of the cold, and all the rain", (1.0 - def.chill) * 100.0), &["People sleeping at home stay warm and dry."]));
            let mut sec = section("Home", home);
            if s.done() && lives.is_empty() && def.id == "shelter" {
                sec.note = Some("An empty shelter falls down after a few days unless someone moves in.".into());
            }
            if s.done() {
                sections.push(sec);
            }
        } else {
            summary = if s.done() { "Keeps the camp's food from spoiling".to_string() } else { format!("Being built, {:.0}%", s.progress * 100.0) };
            if s.done() {
                let berries = c.item("berries");
                let (open, keep) = c.spoil(berries);
                let mut sec = section("Keeping", vec![
                    row_tip("Berries", format!("{:.1}% spoil a day (not {:.0}% in the open)", keep * 100.0, open * 100.0), &["Spoilage per day of what's in the stores."]),
                    row("Grain", "keeps"),
                    row("Meat", format!("{:.0}% a day (not {:.0}%)", c.spoil(c.item("meat")).1 * 100.0, c.spoil(c.item("meat")).0 * 100.0)),
                ]);
                sec.note = Some("With a storehouse built, food in the camp's stores spoils this slowly.".into());
                sections.push(sec);
            }
        }
        if s.done() {
            sections.push(section("Built", vec![row("Finished", s.finished.map_or("-".to_string(), |t| when_long(sim, t)))]));
        }
    }
    let _ = owner;
    Some(ThingInfo { title, subtitle: from_camp(sim, centre), colour: hex_c(if def.id == "field" { "#7DB040" } else { "#9C6B3F" }), summary, sections })
}

/// The ring colour under someone for the mood and needs lenses (linear RGB).
pub fn ring_colour(sim: &Sim, p: &Person, lens: Lens) -> Option<[f32; 3]> {
    let c = &sim.content;
    let col = match lens {
        Lens::Off | Lens::Traffic | Lens::Soil | Lens::Beauty => return None,
        Lens::Mood => {
            let company = near_others(sim, p);
            let (score, _) = p.mood(c, company, by_fire(sim, p), sim.beauty_for(p.pos));
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
        // Long enough (in release) for partners, babies and memories to show in the Life tab.
        let days = if cfg!(debug_assertions) { 1 } else { 9 };
        sim.run_ticks(sim.content.calendar.ticks_per_day() * days + 300);
        let mut hud = Hud::new(&assets).expect("fonts");
        hud.take_events(&mut sim);
        let who = sim.people.iter().find(|(_, p)| p.alive).map(|(id, _)| id).unwrap();
        // Inspect a grown person, and a baby or child if there is one.
        let young = sim.people.iter().find(|(_, p)| p.alive && !p.grown(sim.tick, &sim.content)).map(|(id, _)| id);
        let mut st = Status { paused: false, speed: 1.0, fps: 60.0, frame_ms: 16.0, selected: Some(Pick::Person(who)), hovered: young.map(Pick::Person), pause_on: None, ai: Ai::Low, deciding: 0, heads: Vec::new() };
        if let Some(y) = young {
            for tab in [Tab::Mind, Tab::Life, Tab::Body] {
                hud.tab = tab;
                st.selected = Some(Pick::Person(y));
                hud.ui.begin((1280.0, 800.0), 1.0, ui::Input { cursor: (640.0, 400.0), clicked: false, scroll: 0.0 });
                hud.frame(&sim, &st);
                hud.ui.finish();
            }
            st.selected = Some(Pick::Person(who));
        }
        for scale in [1.0, 2.0] {
            let size = (1280.0 * scale, 800.0 * scale);
            for tab in [Tab::Mind, Tab::Life, Tab::Body, Tab::Skills, Tab::Log] {
                for lens in [Lens::Off, Lens::Traffic, Lens::Mood, Lens::Needs] {
                    for detail in [Detail::Minimal, Detail::Normal, Detail::Analyst] {
                        for (trace, history) in [(None, None), (Some((who, 0)), None), (Some((who, 3)), None), (None, Some(0)), (None, Some(5))] {
                            (hud.tab, hud.lens, hud.detail, hud.trace, hud.history) = (tab, lens, detail, trace, history);
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
            // Things: the fire, the stores, a resource, and a grave if there is one, inspected and hovered.
            let node = sim.nodes.iter().next().map(|(id, _)| Pick::Node(id));
            let grave = sim.nodes.iter().find(|(_, n)| sim.content.nodes[n.kind as usize].id == "grave").map(|(id, _)| Pick::Node(id));
            for thing in [Some(Pick::Fire), Some(Pick::Stores), node, grave].into_iter().flatten() {
                let st = Status { selected: Some(thing), hovered: Some(Pick::Stores), ..Status { paused: false, speed: 1.0, fps: 60.0, frame_ms: 16.0, selected: None, hovered: None, pause_on: None, ai: Ai::Low, deciding: 0, heads: Vec::new() } };
                for k in 0..6 {
                    let cursor = (size.0 * (k as f32 + 0.5) / 6.0, size.1 * 0.5);
                    hud.ui.begin(size, scale, ui::Input { cursor, clicked: false, scroll: 0.0 });
                    hud.frame(&sim, &st);
                    hud.ui.finish();
                }
            }
            // The inspector sits on the right: the middle of its panel is UI, the middle of the screen isn't.
            hud.trace = None;
            hud.history = None;
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
