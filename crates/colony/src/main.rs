//! Colony, the game: a window onto the sim, drawn by the renderer.
//!
//!   colony [--seed N] [--size 256] [--time 10] [--day 1] [--zoom 26]
//!          [--window 1280x800] [--background] [--novsync]
//!   colony --shot out.png [--shot-size 1600x1000] ...     render one frame with no window, save it
//!   colony --bench 3024x1964 [--frames 300] ...           time frames with no window (GPU cost)
//!   colony --perf 10 ...                                  run windowed for N seconds, print fps
//!
//! UI options for shots and benches: --inspect K (select the K-th person), --tab mind|life|body|
//! skills|log, --trace N (open their N-th newest decision trace), --history [N] (the History
//! ledger, scrolled N entries), --pick fire|stores|KIND (inspect the camp's fire or stores, or
//! the nearest node of a kind to the camp, e.g. grave, berry_bush), --lens traffic|mood|needs,
//! --detail minimal|normal|analyst, --cursor X,Y (hover there), --no-ui.
//!
//! Menus (M10) show at launch, over the world from --seed; --no-menu skips them. For shots,
//! --menu main|pause|new|load|save|settings shows one, and --ledger NAME opens a ledger.
//!
//! Controls: drag or two-finger scroll to pan, pinch or wheel to zoom, WASD/arrows, Z/X zoom,
//! Space pause, 1/2/3 speed, T +1 hour, Y +1 day, F full screen, F3 hide the UI, L next lens,
//! Esc close (or the pause menu).

mod drive;
mod gpu;
mod hud;
mod saves;
mod view;

use glam::{Vec2, Vec3};
use render::{Camera, Frame, Lighting, Renderer};
use sim::Sim;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

pub struct Args {
    opts: BTreeMap<String, String>,
}

impl Args {
    fn parse() -> Args {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let mut opts = BTreeMap::new();
        let mut i = 0;
        while i < raw.len() {
            let k = raw[i].trim_start_matches("--").to_string();
            let v = raw.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
            i += if v.is_some() { 2 } else { 1 };
            opts.insert(k, v.unwrap_or_else(|| "true".into()));
        }
        Args { opts }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.opts.get(k).map(|s| s.as_str())
    }
    fn has(&self, k: &str) -> bool {
        self.opts.contains_key(k)
    }
    fn num<T: std::str::FromStr>(&self, k: &str, d: T) -> T {
        self.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
    fn size(&self, k: &str, d: (u32, u32)) -> (u32, u32) {
        self.get(k)
            .and_then(|v| v.split_once('x'))
            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
            .unwrap_or(d)
    }
}

fn assets_dir() -> PathBuf {
    // Run from the repository root, or from anywhere via the build-time path.
    let here = PathBuf::from("assets");
    if here.join("models").exists() { here } else { PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")) }
}

/// Where each person's head is on screen (last frame's view), for speech bubbles.
fn heads(r: &render::Renderer, sim: &Sim, people: &[render::people::PersonDraw], win: (u32, u32)) -> Vec<(sim::PersonId, (f32, f32))> {
    let ids: std::collections::BTreeMap<u64, sim::PersonId> = sim.people.iter().map(|(id, _)| (view::key(id), id)).collect();
    people
        .iter()
        .filter_map(|d| {
            let id = *ids.get(&d.key)?;
            let s = r.to_screen(d.pos + Vec3::new(0.0, 1.65 * d.size * d.height, 0.0), win)?;
            Some((id, (s.x, s.y)))
        })
        .collect()
}

/// What's drawn of the sim in a frame.
struct Scene {
    people: Vec<render::people::PersonDraw>,
    camp: render::CampDraw,
    things: Vec<render::ThingDraw>,
    animals: Vec<render::AnimalDraw>,
}

/// The sim, plus the clock that drives it in real time.
pub struct World {
    pub sim: Sim,
    paused: bool,
    speed: f32,
    carry: f32,
    /// Everyone's pose a tick ago, to smooth movement.
    prev: view::Poses,
    /// The smooth ground surface (what people and things stand on).
    pub ground: render::terrain::Ground,
    /// Pause as soon as this person makes their next decision (decisions they'd made when asked).
    pub pause_on: Option<(sim::PersonId, u64)>,
    /// Set when a watched decision paused the game: open that trace.
    pub caught: Option<sim::PersonId>,
    /// The AI budget (M8): which brain makes everyday decisions.
    pub ai: hud::Ai,
    /// On Max: the Haiku driver answering the bridge (a child process), and its folder.
    driver: Option<(std::process::Child, PathBuf)>,
}

impl Drop for World {
    fn drop(&mut self) {
        self.stop_driver();
    }
}

/// Game minutes per real second at 1x.
const MINUTES_PER_SECOND: f32 = 3.0;

impl World {
    fn new(a: &Args) -> World {
        let mut sim = Sim::new(a.num("seed", 2u64), a.num("size", 256u32));
        // `--give house,berries`: gifts at the start (for screenshots of them).
        if let Some(list) = a.get("give") {
            for what in list.split(',') {
                let g = if sim.content.buildings.iter().any(|b| b.id == what) {
                    sim::commands::Gift::Building { kind: what.into() }
                } else {
                    sim::commands::Gift::Items { item: what.into(), amount: 30 }
                };
                sim.command(sim::Command::Gift(g));
            }
        }
        let cal = &sim.content.calendar;
        let day = a.num("day", 1u64).max(1) - 1;
        let hour: f32 = a.num("time", 10.0);
        let target = day * cal.ticks_per_day() + (hour * 60.0) as u64 / cal.minutes_per_tick as u64;
        if target > sim.tick {
            sim.run_ticks(target - sim.tick);
        }
        World::from_sim(sim)
    }

    /// A world for a sim (new, or loaded from a save).
    fn from_sim(sim: Sim) -> World {
        let prev = view::poses(&sim);
        let ground = render::terrain::Ground::new(&sim.map);
        World { sim, paused: false, speed: 1.0, carry: 0.0, prev, ground, pause_on: None, caught: None, ai: hud::Ai::Low, driver: None }
    }

    /// Change the AI budget. Max sends everyday decisions to Claude Haiku through the bridge, with
    /// `tools/haiku_driver` answering (it uses your Claude plan); Low is the rules brain, no servers.
    fn set_ai(&mut self, ai: hud::Ai) {
        if ai == self.ai {
            return;
        }
        match ai {
            hud::Ai::Max => {
                // The repository root (where assets/, tools/ and runs/ are), wherever we run from.
                let root = assets_dir().parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let dir = root.join(format!("runs/live-{}/bridge", self.sim.seed));
                let brain = match brains::SubagentBrain::new(&dir) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("colony: can't start the bridge in {}: {e}", dir.display());
                        return;
                    }
                };
                let driver = std::process::Command::new(root.join("tools/haiku_driver")).arg(&dir).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                match driver {
                    Ok(child) => self.driver = Some((child, dir)),
                    Err(e) => {
                        eprintln!("colony: can't start tools/haiku_driver: {e}");
                        return;
                    }
                }
                self.sim.brains.map.insert("subagent".into(), Box::new(brain));
                self.ai = ai;
            }
            _ => {
                self.stop_driver();
                self.ai = ai;
            }
        }
        self.assign_brains();
    }

    /// Everyone's brain follows the AI budget (newborns too).
    fn assign_brains(&mut self) {
        let want = if self.ai == hud::Ai::Max && self.driver.is_some() { "subagent" } else { "rules" };
        let ids: Vec<sim::PersonId> = self.sim.people.iter().filter(|(_, p)| p.alive && p.brain != want).map(|(id, _)| id).collect();
        for id in ids {
            self.sim.set_brain(id, want);
        }
    }

    fn stop_driver(&mut self) {
        if let Some((mut child, _)) = self.driver.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn advance(&mut self, dt: f32) {
        if self.paused {
            // God tools still land while paused (at this tick, as they would on the next).
            self.sim.apply_commands();
            return;
        }
        // On Max, time waits while Haiku decides (it answers in a few seconds).
        if self.sim.brains_in_flight() > 0 {
            self.sim.poll_brains();
            self.carry = 0.0;
            return;
        }
        if self.ai == hud::Ai::Max {
            self.assign_brains();
            if let Some((why, _)) = self.sim.brains_stopped().map(|(_, w)| (w, ())) {
                eprintln!("colony: Haiku stopped ({why}); back to the rules brain");
                self.set_ai(hud::Ai::Low);
            }
        }
        self.carry += dt * MINUTES_PER_SECOND * self.speed / self.sim.content.calendar.minutes_per_tick as f32;
        let n = self.carry.floor();
        self.carry -= n;
        if n > 0.0 {
            if let Some((who, made)) = self.pause_on {
                // Step one tick at a time so the game stops right on the decision.
                for _ in 0..n as u64 {
                    self.prev = view::poses(&self.sim);
                    self.sim.step();
                    let now = self.sim.people.get(who).map_or(made, |p| p.tally.decisions);
                    if now != made {
                        self.paused = true;
                        self.carry = 0.0;
                        self.pause_on = None;
                        self.caught = Some(who);
                        break;
                    }
                }
                return;
            }
            self.sim.run_ticks(n as u64 - 1);
            self.prev = view::poses(&self.sim);
            self.sim.step();
        }
    }

    /// Watch someone: pause on their next decision (or stop watching).
    fn toggle_watch(&mut self, who: sim::PersonId) {
        self.pause_on = match self.pause_on {
            Some((w, _)) if w == who => None,
            _ => self.sim.people.get(who).map(|p| (who, p.tally.decisions)),
        };
        if self.pause_on.is_some() {
            self.paused = false;
        }
    }

    fn hour(&self) -> f32 {
        let cal = &self.sim.content.calendar;
        (cal.minute_of_day(self.sim.tick) as f32 + self.carry * cal.minutes_per_tick as f32) / 60.0
    }

    pub fn lighting(&self) -> Lighting {
        let cal = &self.sim.content.calendar;
        let t = self.sim.tick;
        let season = cal.season_index(t);
        let hour = self.hour();
        // Snow cover comes from the weather (it builds on snowy days and melts on warm ones).
        let snow = self.sim.map.weather.snow.clamp(0.0, 1.0);
        Lighting::at(hour, season, snow)
    }

    fn describe(&self) -> String {
        self.sim.describe_time()
    }

    /// Everything the renderer needs from the sim this frame.
    fn scene(&self, cam: &Camera, selected: Option<sim::PersonId>, r: &render::Renderer, lens: hud::Lens) -> Scene {
        let alpha = if self.paused { 1.0 } else { self.carry.clamp(0.0, 1.0) };
        let tps = if self.paused { 0.0 } else { MINUTES_PER_SECOND * self.speed / self.sim.content.calendar.minutes_per_tick as f32 };
        let side = cam.right();
        let light = self.lighting();
        Scene {
            people: view::people(&self.sim, &self.ground, &self.prev, alpha, selected, tps, r.walk_speed(), lens, (side.x, side.z)),
            camp: view::camp(&self.sim, &self.ground),
            things: view::things(&self.sim, &self.ground, r, self.hour(), light.night),
            animals: view::animals(&self.sim, &self.ground, r, &self.prev, alpha, tps),
        }
    }

    fn status(&self, fps: f32, frame_ms: f32, selected: Option<view::Pick>, hovered: Option<view::Pick>) -> hud::Status {
        hud::Status { paused: self.paused, speed: self.speed, fps, frame_ms, selected, hovered, pause_on: self.pause_on.map(|w| w.0), ai: self.ai, deciding: self.sim.brains_in_flight(), heads: Vec::new(), in_game: true, following: None, slots: Vec::new(), app: saves::AppSettings::default() }
    }

    fn nodes(&self) -> Vec<sim::Node> {
        self.sim.nodes.iter().map(|(_, n)| n.clone()).collect()
    }
}

/// The camera on the start site, at the usual zoom.
fn home_camera(sim: &Sim) -> Camera {
    let (sx, sy) = sim.map.start;
    let (x, z) = (sx as f32 + 0.5, sy as f32 + 0.5);
    let mut cam = Camera::new(Vec3::new(x, render::terrain::Ground::new(&sim.map).height(x, z), z));
    cam.size = 16.0;
    cam
}

fn start_camera(sim: &Sim, a: &Args) -> Camera {
    let (sx, sy) = sim.map.start;
    let (x, z) = (a.num("x", sx as f32 + 0.5), a.num("z", sy as f32 + 0.5));
    let mut cam = Camera::new(Vec3::new(x, render::terrain::Ground::new(&sim.map).height(x, z), z));
    cam.size = a.num("zoom", 16.0);
    cam
}

fn main() {
    let a = Args::parse();
    if let Some(path) = a.get("shot") {
        shot(&a, PathBuf::from(path));
    } else if a.has("bench") {
        bench(&a);
    } else if a.has("pan-test") {
        pan_test(&a);
    } else if let Some(out) = a.get("qa-sheet") {
        qa_sheet(&a, PathBuf::from(out));
    } else {
        let event_loop = EventLoop::new().expect("event loop");
        event_loop.set_control_flow(ControlFlow::Poll);
        let mut app = App::new(a);
        event_loop.run_app(&mut app).expect("event loop ran");
        if let Some(report) = app.perf_report() {
            println!("{report}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Headless: a screenshot or a GPU benchmark, with no window at all.

fn headless_renderer(world: &World) -> Renderer {
    let (device, queue) = gpu::headless().unwrap_or_else(|e| panic!("no GPU: {e}"));
    let nodes: Vec<sim::Node> = world.sim.nodes.iter().map(|(_, n)| n.clone()).collect();
    Renderer::new(device, queue, wgpu::TextureFormat::Rgba8UnormSrgb, &world.sim.map, &nodes, &world.sim.content, &assets_dir())
        .unwrap_or_else(|e| panic!("renderer: {e}"))
}

/// The HUD for a headless shot or bench, set up from the UI options (see the top of this file).
struct Staged {
    hud: hud::Hud,
    selected: Option<view::Pick>,
    hovered: Option<view::Pick>,
    cursor: (f32, f32),
}

fn stage_ui(a: &Args, world: &mut World, size: (u32, u32)) -> Option<Staged> {
    if a.has("no-ui") {
        return None;
    }
    let mut hud = hud::Hud::new(&assets_dir()).unwrap_or_else(|e| panic!("fonts: {e}"));
    hud.take_events(&mut world.sim);
    let living: Vec<sim::PersonId> = world.sim.people.iter().filter(|(_, p)| p.alive).map(|(id, _)| id).collect();
    let person = a.get("inspect").and_then(|k| k.parse::<usize>().ok()).and_then(|k| living.get(k).copied());
    let thing = a.get("pick").and_then(|what| match what {
        "fire" => Some(view::Pick::Fire),
        "stores" => Some(view::Pick::Stores),
        // A building or field kind, or an animal kind: the nearest to the camp.
        kind if world.sim.content.buildings.iter().any(|b| b.id == kind) => {
            let k = world.sim.content.building(kind);
            let (sx, sy) = world.sim.camp.stores_pos();
            let c = &world.sim.content;
            world
                .sim
                .structures
                .iter()
                .filter(|(_, s)| s.kind == k)
                .min_by(|a, b| {
                    let d = |s: &sim::buildings::Structure| (s.centre(c).0 - sx).powi(2) + (s.centre(c).1 - sy).powi(2);
                    d(a.1).total_cmp(&d(b.1))
                })
                .map(|(id, _)| view::Pick::Structure(id))
        }
        kind if world.sim.content.animals.iter().any(|b| b.id == kind) => {
            let k = world.sim.content.animal(kind);
            let (sx, sy) = world.sim.camp.stores_pos();
            world
                .sim
                .animals
                .iter()
                .filter(|(_, a)| a.kind == k)
                .min_by(|a, b| {
                    let d = |a: &sim::animals::Animal| (a.pos.0 - sx).powi(2) + (a.pos.1 - sy).powi(2);
                    d(a.1).total_cmp(&d(b.1))
                })
                .map(|(id, _)| view::Pick::Animal(id))
        }
        kind => {
            let k = world.sim.content.nodes.iter().position(|n| n.id == kind)? as u16;
            let (sx, sy) = world.sim.camp.stores_pos();
            world
                .sim
                .nodes
                .iter()
                .filter(|(_, n)| n.kind == k)
                .min_by(|a, b| {
                    let d = |n: &sim::Node| (n.x as f32 - sx).powi(2) + (n.y as f32 - sy).powi(2);
                    d(a.1).total_cmp(&d(b.1))
                })
                .map(|(id, _)| view::Pick::Node(id))
        }
    });
    let selected = thing.or(person.map(view::Pick::Person));
    hud.settings = a.has("settings");
    hud.gifting = a.has("gifting");
    if let Some(l) = a.get("ledger").and_then(hud::Ledger::from_name) {
        hud.open_ledger(Some(l));
    }
    hud.menu = match a.get("menu") {
        Some("main") => Some(hud::Menu::Main),
        Some("pause") => Some(hud::Menu::Pause),
        Some("new") => Some(hud::Menu::new_world(world.sim.seed)),
        Some("load") => Some(hud::Menu::Slots { saving: false }),
        Some("save") => Some(hud::Menu::Slots { saving: true }),
        Some("settings") => Some(hud::Menu::Settings),
        _ => None,
    };
    if matches!(hud.menu, Some(hud::Menu::Pause)) {
        hud.menu_home = hud::Menu::Pause;
    }
    if a.has("arena") {
        hud.open_ledger(Some(hud::Ledger::Arena));
    }
    if let (Some(words), Some(p)) = (a.get("speak"), person) {
        hud.speaking = Some((p, words.to_string()));
    }
    hud.tab = match a.get("tab").unwrap_or("mind") {
        "life" => hud::Tab::Life,
        "body" => hud::Tab::Body,
        "skills" => hud::Tab::Skills,
        "log" => hud::Tab::Log,
        _ => hud::Tab::Mind,
    };
    hud.lens = match a.get("lens").unwrap_or("off") {
        "traffic" => hud::Lens::Traffic,
        "mood" => hud::Lens::Mood,
        "needs" => hud::Lens::Needs,
        "soil" => hud::Lens::Soil,
        "beauty" => hud::Lens::Beauty,
        _ => hud::Lens::Off,
    };
    hud.detail = match a.get("detail").unwrap_or("normal") {
        "minimal" => hud::Detail::Minimal,
        "analyst" => hud::Detail::Analyst,
        _ => hud::Detail::Normal,
    };
    if let (Some(s), Some(k)) = (person, a.get("trace").and_then(|k| k.parse::<usize>().ok())) {
        hud.trace = Some((s, k));
    }
    if a.has("history") {
        hud.history = Some(a.get("history").and_then(|v| v.parse().ok()).unwrap_or(0));
        hud.ledger = Some(hud::Ledger::History);
    }
    let cursor = a
        .get("cursor")
        .and_then(|c| c.split_once(','))
        .and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))
        .unwrap_or((-1.0, -1.0));
    // Hover whoever is nearest the middle of the screen, apart from the selected person.
    let hovered = if a.has("cursor") || a.has("no-hover") { None } else { living.iter().find(|h| Some(**h) != person).copied().map(view::Pick::Person) };
    let _ = size;
    Some(Staged { hud, selected, hovered, cursor })
}

/// Scale for the UI: 2 window pixels per logical pixel on a retina-sized image, 1 on a small one.
fn headless_scale(size: (u32, u32)) -> f32 {
    if size.1 >= 1400 { 2.0 } else { 1.0 }
}

fn shot(a: &Args, path: PathBuf) {
    let mut world = World::new(a);
    // `--make-save N`: save this world into slot N first (for shots of the save menus; point
    // COLONY_DATA at a scratch folder).
    if let Some(slot) = a.get("make-save").and_then(|v| v.parse().ok()) {
        saves::save(&world.sim, slot).and_then(|j| j.wait()).unwrap_or_else(|e| panic!("save: {e}"));
    }
    let size = a.size("shot-size", (1600, 1000));
    let mut r = headless_renderer(&world);
    let mut cam = start_camera(&world.sim, a);
    let light = world.lighting();
    let mut staged = stage_ui(a, &mut world, size);
    let lens = staged.as_ref().map_or(hud::Lens::Off, |s| s.hud.lens);
    let selected = staged.as_ref().and_then(|s| s.selected);
    // `--look`: centre the camera on the picked thing.
    if a.has("look")
        && let Some(m) = selected.and_then(|p| view::marker(&world.sim, &world.ground, p))
    {
        let at = match m {
            render::Marker::Ring(at, _) => at,
            render::Marker::Area([x0, z0, x1, z1]) => {
                let (x, z) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
                Vec3::new(x, world.ground.height(x, z), z)
            }
        };
        cam.focus = at;
    }
    let Scene { people, camp, things, animals } = world.scene(&cam, selected.and_then(|s| s.person()), &r, lens);
    let marker = selected.and_then(|p| view::marker(&world.sim, &world.ground, p));
    // Put the hover cursor on the hovered person, as if the mouse were there.
    if let Some(st) = staged.as_mut()
        && let Some(h) = st.hovered.and_then(|h| h.person())
        && let Some(d) = people.iter().find(|d| d.key == view::key(h))
    {
        r.render(&gpu::offscreen_target(r.device(), size, r.target_format()).create_view(&Default::default()), size, &Frame { camera: &cam, light: &light, time: 0.0, ui: None, ground: render::GroundLens::Off, people: &people, camp: Some(camp),
            things: &things,
            beauty: Some(&world.sim.beauty),
            animals: &animals, marker: None });
        if let Some(p) = r.to_screen(d.pos + Vec3::new(0.0, 0.55, 0.0), size) {
            st.cursor = (p.x, p.y);
        }
    }
    if r.last_view.is_none() {
        r.render(&gpu::offscreen_target(r.device(), size, r.target_format()).create_view(&Default::default()), size, &Frame { camera: &cam, light: &light, time: 0.0, ui: None, ground: render::GroundLens::Off, people: &people, camp: Some(camp), things: &things, beauty: None, animals: &animals, marker: None });
    }
    let shot_heads = heads(&r, &world.sim, &people, size);
    let ui = staged.as_mut().map(|st| {
        let mut status = world.status(60.0, 16.7, st.selected, st.hovered);
        status.slots = saves::list();
        status.in_game = a.get("menu") != Some("main");
        status.heads = shot_heads.clone();
        st.hud.ui.begin((size.0 as f32, size.1 as f32), headless_scale(size), ui::Input { cursor: st.cursor, clicked: false, scroll: 0.0 });
        st.hud.frame(&world.sim, &status);
        st.hud.ui.finish()
    });
    let frame = Frame { camera: &cam, light: &light, time: 0.0, ui, ground: lens.ground(), people: &people, camp: Some(camp),
            things: &things,
            beauty: Some(&world.sim.beauty),
            animals: &animals, marker };
    let img = gpu::render_to_image(&mut r, size, &frame);
    gpu::write_png(&path, size, &img);
    println!("wrote {} ({}x{}, {})", path.display(), size.0, size.1, world.describe());
}

fn bench(a: &Args) {
    let mut world = World::new(a);
    let size = a.size("bench", (3024, 1964));
    let frames = a.num("frames", 300u32);
    let mut r = headless_renderer(&world);
    let mut cam = start_camera(&world.sim, a);
    let light = world.lighting();
    let target = gpu::offscreen_target(r.device(), size, r.target_format());
    let view = target.create_view(&Default::default());
    let mut staged = stage_ui(a, &mut world, size);
    let lens = staged.as_ref().map_or(hud::Lens::Off, |s| s.hud.lens);
    let selected = staged.as_ref().and_then(|s| s.selected.and_then(|p| p.person()));
    let mut times = Vec::new();
    for f in 0..frames + 20 {
        // Pan slowly so culling and snapping do real work.
        cam.focus.x += 0.05;
        let t = Instant::now();
        let Scene { people, camp, things, animals } = world.scene(&cam, selected, &r, lens);
        let ui = staged.as_mut().map(|st| {
            let status = world.status(60.0, 16.7, st.selected, st.hovered);
            st.hud.ui.begin((size.0 as f32, size.1 as f32), 2.0, ui::Input { cursor: (size.0 as f32 * 0.5, size.1 as f32 * 0.5), clicked: false, scroll: 0.0 });
            st.hud.frame(&world.sim, &status);
            st.hud.ui.finish()
        });
        r.render(&view, size, &Frame { camera: &cam, light: &light, time: f as f32 / 60.0, ui, ground: lens.ground(), people: &people, camp: Some(camp),
            things: &things,
            beauty: Some(&world.sim.beauty),
            animals: &animals, marker: None });
        r.device().poll(wgpu::PollType::wait_indefinitely()).expect("gpu finished");
        if f >= 20 {
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    println!("{}", perf_json("bench", size, &times));
}

/// A contact sheet of views for QA: `--qa-sheet out.png --spots "2:196,169 5:93,9 ..."` renders
/// each seed:x,z at `--zoom` (default 12) and lays them out three across, labelled in order.
fn qa_sheet(a: &Args, out: PathBuf) {
    let spots: Vec<(u64, f32, f32)> = a
        .get("spots")
        .unwrap_or("")
        .split_whitespace()
        .filter_map(|s| {
            let (seed, xy) = s.split_once(':')?;
            let (x, z) = xy.split_once(',')?;
            Some((seed.parse().ok()?, x.parse().ok()?, z.parse().ok()?))
        })
        .collect();
    let tile = a.size("tile", (600, 380));
    let cols = 3u32;
    let rows = (spots.len() as u32).div_ceil(cols).max(1);
    let (w, h) = (cols * (tile.0 + 6) + 6, rows * (tile.1 + 6) + 6);
    let mut sheet = vec![0u8; (w * h * 4) as usize];
    for px in sheet.chunks_mut(4) {
        px.copy_from_slice(&[30, 26, 31, 255]);
    }
    let mut cache: Option<(u64, World, Renderer)> = None;
    let mut labeller: Option<hud::Hud> = None;
    for (k, &(seed, x, z)) in spots.iter().enumerate() {
        if cache.as_ref().is_none_or(|c| c.0 != seed) {
            let mut args = Args { opts: a.opts.clone() };
            args.opts.insert("seed".into(), seed.to_string());
            let world = World::new(&args);
            let r = headless_renderer(&world);
            cache = Some((seed, world, r));
        }
        let (_, world, r) = cache.as_mut().unwrap();
        let mut cam = Camera::new(Vec3::new(x + 0.5, world.ground.height(x + 0.5, z + 0.5), z + 0.5));
        cam.size = a.num("zoom", 12.0);
        let light = world.lighting();
        let Scene { people, camp, things, animals } = world.scene(&cam, None, r, hud::Lens::Off);
        let label = labeller.get_or_insert_with(|| hud::Hud::new(&assets_dir()).unwrap_or_else(|e| panic!("fonts: {e}")));
        label.ui.begin((tile.0 as f32, tile.1 as f32), 1.0, ui::Input::default());
        let tt = ui::Text { font: ui::Font::Sans, size: 13.0, colour: ui::style::TEXT };
        let s = format!("{} seed {seed} {x},{z}", k + 1);
        let lw = label.ui.measure(&s, tt);
        label.ui.rect(ui::Rect::new(4.0, 4.0, lw + 12.0, 22.0), ui::style::PANEL, 4.0);
        label.ui.text(10.0, 6.0, &s, tt);
        let img = gpu::render_to_image(r, tile, &Frame { camera: &cam, light: &light, time: 3.0, ui: Some(label.ui.finish()), ground: render::GroundLens::Off, people: &people, camp: Some(camp),
            things: &things,
            beauty: Some(&world.sim.beauty),
            animals: &animals, marker: None });
        let (ox, oy) = (6 + (k as u32 % cols) * (tile.0 + 6), 6 + (k as u32 / cols) * (tile.1 + 6));
        for y in 0..tile.1 {
            let src = (y * tile.0 * 4) as usize;
            let dst = (((oy + y) * w + ox) * 4) as usize;
            sheet[dst..dst + (tile.0 * 4) as usize].copy_from_slice(&img[src..src + (tile.0 * 4) as usize]);
        }
    }
    gpu::write_png(&out, (w, h), &sheet);
    println!("wrote {} ({} views)", out.display(), spots.len());
}

/// Nudge the camera a quarter of an art pixel at a time and count changed pixels between frames.
/// With the camera snapped to the art-pixel grid, three nudges in four change nothing and the
/// fourth moves the whole picture by exactly one art pixel (no crawling or shimmer).
fn pan_test(a: &Args) {
    let world = World::new(a);
    let size = a.size("shot-size", (800, 500));
    let mut r = headless_renderer(&world);
    let mut cam = start_camera(&world.sim, a);
    let light = world.lighting();
    let step = render::View::new(&cam, size.0, size.1).t / 4.0;
    let right = cam.right();
    let mut prev: Option<Vec<u8>> = None;
    let mut changed = Vec::new();
    for _ in 0..9 {
        let Scene { people, camp, things, animals } = world.scene(&cam, None, &r, hud::Lens::Off);
        let img = gpu::render_to_image(&mut r, size, &Frame { camera: &cam, light: &light, time: 0.0, ui: None, ground: render::GroundLens::Off, people: &people, camp: Some(camp),
            things: &things,
            beauty: Some(&world.sim.beauty),
            animals: &animals, marker: None });
        if let Some(p) = &prev {
            let n = img.chunks(4).zip(p.chunks(4)).filter(|(x, y)| x != y).count();
            changed.push(n);
        }
        prev = Some(img);
        cam.focus += right * step;
    }
    let total = (size.0 * size.1) as usize;
    let still = changed.iter().filter(|&&n| n * 200 < total).count();
    println!("PAN changed pixels per quarter-pixel nudge: {changed:?} (of {total}); {still} of {} nudges left the picture still", changed.len());
}

fn perf_json(mode: &str, size: (u32, u32), times: &[f64]) -> String {
    let mut s = times.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let pct = |p: f64| s[((s.len() as f64 - 1.0) * p).round() as usize];
    let avg = s.iter().sum::<f64>() / s.len() as f64;
    format!(
        "PERF {{\"mode\":\"{mode}\",\"size\":\"{}x{}\",\"frames\":{},\"fps_avg\":{:.0},\"ms_p50\":{:.2},\"ms_p95\":{:.2},\"ms_p99\":{:.2},\"ms_worst\":{:.2}}}",
        size.0,
        size.1,
        s.len(),
        1000.0 / avg,
        pct(0.5),
        pct(0.95),
        pct(0.99),
        s[s.len() - 1]
    )
}

// ---------------------------------------------------------------------------------------------
// The window

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

struct Drag {
    start: Vec2,
    last: Vec2,
    moving: bool,
}

struct App {
    args: Args,
    world: World,
    camera: Camera,
    size_target: f32,
    gfx: Option<Gfx>,
    keys: std::collections::BTreeSet<KeyCode>,
    cursor: Vec2,
    drag: Option<Drag>,
    last: Instant,
    started: Instant,
    frame_ms: Vec<f64>,
    /// (waiting for the window's next image, building and submitting the frame), ms
    split_ms: Vec<(f64, f64)>,
    /// (fps, ms per frame) over the last half second.
    fps: (f32, f32),
    show_ui: bool,
    fps_window: (Instant, u32, f64),
    hud: Option<hud::Hud>,
    /// A click (press and release without dragging) waiting for the next frame.
    click: bool,
    scroll: f32,
    perf_secs: Option<f64>,
    hovered: Option<view::Pick>,
    selected: Option<view::Pick>,
    // Menus, saves and settings (M10)
    /// A world is being played (false while the launch menu shows over the backdrop world).
    in_game: bool,
    slots: Vec<saves::SlotInfo>,
    app: saves::AppSettings,
    save_job: Option<(sim::save::SaveJob, u32)>,
    last_autosave: Instant,
    /// Whether the world was paused when the pause menu opened.
    was_paused: bool,
    quit: bool,
    /// Driven from outside (`--drive DIR`), and a screenshot it asked for.
    drive: Option<drive::Drive>,
    shot_next: Option<PathBuf>,
    /// The camera keeping someone in view, or gliding to a spot.
    follow: Option<sim::PersonId>,
    glide: Option<Vec2>,
}

/// Autosave at most this often (real seconds), on the next morning in the game.
const AUTOSAVE_SECS: u64 = 180;

impl App {
    fn new(args: Args) -> App {
        let world = World::new(&args);
        let camera = start_camera(&world.sim, &args);
        let perf_secs = args.get("perf").and_then(|v| v.parse().ok());
        App {
            size_target: camera.size,
            camera,
            world,
            gfx: None,
            keys: Default::default(),
            cursor: Vec2::ZERO,
            drag: None,
            last: Instant::now(),
            started: Instant::now(),
            frame_ms: Vec::new(),
            split_ms: Vec::new(),
            fps: (0.0, 0.0),
            show_ui: true,
            hud: None,
            click: false,
            scroll: 0.0,
            fps_window: (Instant::now(), 0, 0.0),
            perf_secs,
            hovered: None,
            selected: None,
            in_game: false,
            slots: saves::list(),
            app: saves::AppSettings::load(),
            save_job: None,
            last_autosave: Instant::now(),
            was_paused: false,
            quit: false,
            drive: args.get("drive").map(|d| drive::Drive::new(PathBuf::from(d))),
            shot_next: None,
            follow: None,
            glide: None,
            args,
        }
    }

    /// Play another world (new, or loaded): a fresh renderer for its map, the camera at its camp.
    /// The world being played is kept first ("Your previous world"), so nothing is lost.
    fn replace_world(&mut self, sim: Sim) {
        if self.in_game && self.args.get("perf").is_none() {
            if let Some((job, _)) = self.save_job.take() {
                let _ = job.wait();
            }
            if let Ok(job) = saves::save(&self.world.sim, saves::BEFORE) {
                let _ = job.wait();
            }
        }
        let paused = self.in_game && (self.world.paused || self.was_paused);
        self.world = World::from_sim(sim);
        self.world.paused = paused;
        self.was_paused = paused;
        self.follow = None;
        self.glide = None;
        self.camera = home_camera(&self.world.sim);
        self.size_target = self.camera.size;
        self.selected = None;
        self.hovered = None;
        if let Some(g) = self.gfx.as_mut() {
            let nodes = self.world.nodes();
            let (d, q) = (g.renderer.device().clone(), g.renderer.queue().clone());
            match Renderer::new(d, q, g.config.format, &self.world.sim.map, &nodes, &self.world.sim.content, &assets_dir()) {
                Ok(r) => g.renderer = r,
                Err(e) => eprintln!("colony: renderer for the new world: {e}"),
            }
        }
        if let Some(h) = self.hud.as_mut() {
            h.reset_world(&self.world.sim);
        }
        self.slots = saves::list();
        self.in_game = true;
        self.last_autosave = Instant::now();
        self.apply_settings();
    }

    fn new_world(&mut self, seed: u64, size: u32, beauty: bool, packs: Vec<String>) {
        let mut sim = Sim::new(seed, size);
        // Packs are chosen when a world is made, and kept with it.
        sim.settings.packs = packs;
        if !beauty {
            sim.command(sim::Command::SetBeautyMatters { on: false });
        }
        self.replace_world(sim);
        // A new world starts running (a loaded one keeps the pause it was loaded in).
        self.world.paused = false;
        self.was_paused = false;
    }

    fn save(&mut self, slot: u32, quiet: bool) {
        // One save at a time: finish the last first.
        if let Some((job, _)) = self.save_job.take() {
            let _ = job.wait();
        }
        match saves::save(&self.world.sim, slot) {
            Ok(job) => {
                self.save_job = Some((job, slot));
                if !quiet && let Some(h) = self.hud.as_mut() {
                    let name = if slot == saves::AUTOSAVE { "the autosave".to_string() } else { format!("slot {slot}") };
                    h.note(format!("Saved to {name}: {}", saves::describe(&self.world.sim)), self.world.sim.tick);
                }
            }
            Err(e) => {
                if let Some(h) = self.hud.as_mut() {
                    h.note(format!("Couldn't save: {e}"), self.world.sim.tick);
                }
            }
        }
    }

    fn load(&mut self, slot: u32) {
        if let Some((job, _)) = self.save_job.take() {
            let _ = job.wait();
        }
        match saves::load(slot) {
            Ok(sim) => {
                self.replace_world(sim);
                if let Some(h) = self.hud.as_mut() {
                    h.menu = None;
                    h.note(format!("Loaded: {}", saves::describe(&self.world.sim)), self.world.sim.tick);
                }
            }
            Err(e) => {
                if let Some(h) = self.hud.as_mut() {
                    h.note(format!("Couldn't load that save: {e}"), self.world.sim.tick);
                    h.menu = Some(h.menu_home.clone());
                }
            }
        }
    }

    /// Put the app settings into effect.
    fn apply_settings(&mut self) {
        let lower = self.app.power == saves::Power::Lower;
        if let Some(g) = self.gfx.as_mut() {
            g.renderer.shadows = !lower;
            let full = g.window.fullscreen().is_some();
            if full != self.app.fullscreen {
                g.window.set_fullscreen(if self.app.fullscreen { Some(Fullscreen::Borderless(None)) } else { None });
            }
        }
        if lower {
            self.world.set_ai(hud::Ai::Low);
        }
    }

    /// A key, from the keyboard or the driver: the menus first, then typing, then the shortcuts.
    fn key(&mut self, code: KeyCode, text: Option<&str>, pressed: bool, repeat: bool) {
        // The menus (M10): Esc goes back; the new-world menu takes the seed's digits.
        if let Some(h) = self.hud.as_mut()
            && h.menu.is_some()
        {
            if pressed {
                match code {
                    KeyCode::Escape => {
                        if h.menu_back(self.in_game) && h.menu.is_none() {
                            self.close_menu();
                        }
                    }
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        if let Some(hud::Menu::NewWorld { seed, size, beauty, packs }) = h.menu.clone()
                            && let Ok(seed) = seed.trim().parse::<u64>()
                            && seed > 0
                        {
                            self.new_world(seed, size, beauty, packs);
                            if let Some(h) = self.hud.as_mut() {
                                h.menu = None;
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        if let Some(seed) = h.typing_seed() {
                            seed.pop();
                        }
                    }
                    KeyCode::KeyF => {
                        self.app.fullscreen = !self.app.fullscreen;
                        self.app.store();
                        self.apply_settings();
                    }
                    _ => {
                        if let (Some(seed), Some(text)) = (h.typing_seed(), text) {
                            for ch in text.chars().filter(|c| c.is_ascii_digit()) {
                                if seed.len() < 12 {
                                    seed.push(ch);
                                }
                            }
                        }
                    }
                }
            }
            return;
        }
        // Typing words to someone (M8): keys go to the text, not the game's shortcuts.
        if let Some(h) = self.hud.as_mut()
            && let Some((who, words)) = h.speaking.as_mut()
        {
            if pressed {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        let (who, said) = (*who, words.trim().to_string());
                        h.speaking = None;
                        if !said.is_empty() {
                            self.world.sim.command(sim::Command::Speak { person: who, words: said });
                        }
                    }
                    KeyCode::Escape => h.speaking = None,
                    KeyCode::Backspace => {
                        words.pop();
                    }
                    _ => {
                        if let Some(text) = text {
                            for ch in text.chars().filter(|c| !c.is_control()) {
                                if words.chars().count() < 120 {
                                    words.push(ch);
                                }
                            }
                        }
                    }
                }
            }
            return;
        }
        if pressed {
            if !repeat {
                match code {
                    KeyCode::Space => self.world.paused = !self.world.paused,
                    KeyCode::Digit1 => self.world.speed = 1.0,
                    KeyCode::Digit2 => self.world.speed = 3.0,
                    KeyCode::Digit3 => self.world.speed = 10.0,
                    KeyCode::KeyT => self.world.sim.run_ticks(60 / self.world.sim.content.calendar.minutes_per_tick as u64),
                    KeyCode::KeyY => {
                        let d = self.world.sim.content.calendar.ticks_per_day();
                        self.world.sim.run_ticks(d);
                    }
                    KeyCode::F3 => self.show_ui = !self.show_ui,
                    KeyCode::KeyL => {
                        if let Some(h) = self.hud.as_mut() {
                            h.lens = match h.lens {
                                hud::Lens::Off => hud::Lens::Traffic,
                                hud::Lens::Traffic => hud::Lens::Mood,
                                hud::Lens::Mood => hud::Lens::Needs,
                                hud::Lens::Needs => hud::Lens::Soil,
                                hud::Lens::Soil => hud::Lens::Beauty,
                                hud::Lens::Beauty => hud::Lens::Off,
                            };
                        }
                    }
                    KeyCode::Escape => {
                        // Close the nearest thing open; with nothing open, the pause menu.
                        let h = self.hud.as_mut();
                        match h {
                            Some(h) if h.trace.is_some() => h.trace = None,
                            Some(h) if h.ledger.is_some() => h.open_ledger(None),
                            Some(h) if h.gifting || h.settings => {
                                h.gifting = false;
                                h.settings = false;
                            }
                            _ if self.follow.is_some() => self.take_camera(),
                            _ if self.selected.is_some() => self.selected = None,
                            Some(_) => self.open_pause(),
                            None => {}
                        }
                    }
                    KeyCode::KeyF => {
                        self.app.fullscreen = !self.app.fullscreen;
                        self.app.store();
                        self.apply_settings();
                    }
                    _ => {}
                }
            }
            self.keys.insert(code);
        } else {
            self.keys.remove(&code);
        }
    }

    fn open_pause(&mut self) {
        if let Some(h) = self.hud.as_mut() {
            self.was_paused = self.world.paused;
            self.world.paused = true;
            h.menu_home = hud::Menu::Pause;
            h.menu = Some(hud::Menu::Pause);
            self.slots = saves::list();
        }
    }

    fn close_menu(&mut self) {
        if let Some(h) = self.hud.as_mut() {
            h.menu = None;
        }
        if self.in_game {
            self.world.paused = self.was_paused;
        }
        self.in_game = true;
        self.was_paused = false;
    }

    /// Saves finishing, and the autosave each morning (at most every few minutes).
    fn tend_saves(&mut self) {
        if self.save_job.as_ref().is_some_and(|(j, _)| j.is_done()) {
            if let Some((job, _)) = self.save_job.take()
                && let Err(e) = job.wait()
                && let Some(h) = self.hud.as_mut()
            {
                h.note(format!("Saving failed: {e}"), self.world.sim.tick);
            }
            self.slots = saves::list();
        }
        let cal = &self.world.sim.content.calendar;
        let morning = cal.minute_of_day(self.world.sim.tick) < 60 * 7;
        if self.in_game && self.save_job.is_none() && morning && self.last_autosave.elapsed().as_secs() >= AUTOSAVE_SECS && self.args.get("perf").is_none() {
            self.last_autosave = Instant::now();
            self.save(saves::AUTOSAVE, true);
        }
    }

    fn perf_report(&self) -> Option<String> {
        let g = self.gfx.as_ref()?;
        self.perf_secs?;
        if self.frame_ms.is_empty() {
            return None;
        }
        let mut a: Vec<f64> = self.split_ms.iter().map(|s| s.0).collect();
        let mut d: Vec<f64> = self.split_ms.iter().map(|s| s.1).collect();
        a.sort_by(|x, y| x.total_cmp(y));
        d.sort_by(|x, y| x.total_cmp(y));
        let mid = |v: &[f64], p: f64| v.get(((v.len() as f64 - 1.0) * p) as usize).copied().unwrap_or(0.0);
        Some(format!(
            "{}\nSPLIT acquire p50 {:.2} p99 {:.2} ms; draw p50 {:.2} p99 {:.2} ms; vsync {}",
            perf_json("window", (g.config.width, g.config.height), &self.frame_ms),
            mid(&a, 0.5),
            mid(&a, 0.99),
            mid(&d, 0.5),
            mid(&d, 0.99),
            !(self.args.has("novsync") || !self.args.has("vsync"))
        ))
    }

    fn metres_per_pixel(&self) -> f32 {
        let h = self.gfx.as_ref().map_or(800, |g| g.config.height) as f32;
        self.camera.size / h
    }

    /// Move the world with the cursor: a drag of `d` window pixels (the ground follows the cursor).
    fn drag_pixels(&mut self, d: Vec2) {
        self.pan_pixels(Vec2::new(d.x, -d.y));
    }

    /// The player moved the camera themselves: stop following or gliding.
    fn take_camera(&mut self) {
        self.follow = None;
        self.glide = None;
    }

    /// Pan by `d` window pixels (trackpad scrolling).
    fn pan_pixels(&mut self, d: Vec2) {
        self.take_camera();
        let mpp = self.metres_per_pixel();
        let right = self.camera.right();
        let up = self.camera.ground_up();
        let along = d.y * mpp / self.camera.pitch.sin();
        self.camera.focus.x -= right.x * d.x * mpp;
        self.camera.focus.z -= right.z * d.x * mpp;
        self.camera.focus.x -= up.x * along;
        self.camera.focus.z -= up.y * along;
        self.clamp_focus();
    }

    /// Keep the focus on the ground by sliding it along the view direction. In an orthographic view
    /// that doesn't move the picture at all (no jitter), but zooming and the shadows stay centred
    /// on what's actually under the middle of the screen.
    fn settle_focus(&mut self) {
        let f = self.camera.forward();
        for _ in 0..4 {
            let c = self.camera.focus;
            let g = self.world.ground.height(c.x, c.z);
            self.camera.focus += f * ((g - c.y) / f.y);
        }
    }

    fn clamp_focus(&mut self) {
        let n = self.world.sim.map.size as f32;
        self.camera.focus.x = self.camera.focus.x.clamp(0.0, n);
        self.camera.focus.z = self.camera.focus.z.clamp(0.0, n);
    }

    fn zoom_by(&mut self, factor: f32) {
        self.size_target = (self.size_target * factor).clamp(6.0, 220.0);
    }

    fn update(&mut self, dt: f32) {
        // Keyboard pan and zoom
        let speed = self.camera.size * 0.9 * dt;
        let mut pan = Vec2::ZERO;
        let keys = self.keys.clone();
        let held = |k: &[KeyCode]| k.iter().any(|c| keys.contains(c));
        if held(&[KeyCode::KeyA, KeyCode::ArrowLeft]) {
            pan.x -= 1.0;
        }
        if held(&[KeyCode::KeyD, KeyCode::ArrowRight]) {
            pan.x += 1.0;
        }
        if held(&[KeyCode::KeyW, KeyCode::ArrowUp]) {
            pan.y += 1.0;
        }
        if held(&[KeyCode::KeyS, KeyCode::ArrowDown]) {
            pan.y -= 1.0;
        }
        if pan != Vec2::ZERO {
            self.take_camera();
            let r = self.camera.right();
            let u = self.camera.ground_up();
            let p = pan.normalize() * speed;
            self.camera.focus.x += r.x * p.x + u.x * p.y;
            self.camera.focus.z += r.z * p.x + u.y * p.y;
            self.clamp_focus();
        }
        if held(&[KeyCode::KeyZ, KeyCode::Equal]) {
            self.zoom_by(1.0 - dt * 1.5);
        }
        if held(&[KeyCode::KeyX, KeyCode::Minus]) {
            self.zoom_by(1.0 + dt * 1.5);
        }
        // Glide towards the zoom target; keep the focus on the ground.
        let k = 1.0 - (-dt * 12.0).exp();
        self.camera.size += (self.size_target - self.camera.size) * k;
        // Finish the glide cleanly: endless tiny size changes would keep nudging the pixel grid.
        if (self.size_target - self.camera.size).abs() < self.size_target * 0.003 {
            self.camera.size = self.size_target;
        }
        self.settle_focus();
        // Following someone, or gliding to a spot picked from a link.
        if let Some(id) = self.follow {
            match self.world.sim.people.get(id).filter(|p| p.alive) {
                Some(p) => self.glide = Some(Vec2::new(p.pos.0, p.pos.1)),
                None => self.follow = None,
            }
        }
        if let Some(to) = self.glide {
            let k = 1.0 - (-dt * 5.0).exp();
            self.camera.focus.x += (to.x - self.camera.focus.x) * k;
            self.camera.focus.z += (to.y - self.camera.focus.z) * k;
            if self.follow.is_none() && (Vec2::new(self.camera.focus.x, self.camera.focus.z) - to).length() < 0.05 {
                self.glide = None;
            }
            self.clamp_focus();
        }
        self.world.advance(dt);
    }

    /// Run the driver's commands that are due (`--drive`), through the same paths as real input.
    fn run_drive(&mut self) {
        let Some(d) = self.drive.as_mut() else { return };
        if let Some((k, until)) = d.held
            && Instant::now() >= until
        {
            d.held = None;
            d.report("ok");
            self.key(k, None, false, false);
        }
        let scale = self.gfx.as_ref().map_or(1.0, |g| g.window.scale_factor() as f32);
        while let Some(step) = self.drive.as_mut().and_then(|d| d.step()) {
            let mut said = "ok".to_string();
            let mut stop = false;
            match step {
                drive::Step::Click(x, y) => {
                    self.cursor = Vec2::new(x * scale, y * scale);
                    self.click = true;
                    stop = true;
                }
                drive::Step::Move(x, y) => {
                    self.cursor = Vec2::new(x * scale, y * scale);
                    stop = true;
                }
                drive::Step::Drag(x1, y1, x2, y2) => {
                    self.drag_pixels(Vec2::new((x2 - x1) * scale, (y2 - y1) * scale));
                    self.cursor = Vec2::new(x2 * scale, y2 * scale);
                }
                drive::Step::Scroll(dy) => {
                    if self.hud.as_ref().is_some_and(|h| h.ui.over_ui()) {
                        self.scroll += dy;
                    } else {
                        self.zoom_by(0.88f32.powf(dy));
                    }
                    stop = true;
                }
                drive::Step::Key(k) => {
                    self.key(k, None, true, false);
                    self.key(k, None, false, false);
                    stop = true;
                }
                drive::Step::Hold(k, secs) => {
                    self.key(k, None, true, false);
                    if let Some(d) = self.drive.as_mut() {
                        d.held = Some((k, Instant::now() + std::time::Duration::from_secs_f32(secs)));
                    }
                    break;
                }
                drive::Step::Type(text) => {
                    let taking = self.hud.as_mut().is_some_and(|h| h.speaking.is_some() || h.typing_seed().is_some());
                    if taking {
                        for ch in text.chars() {
                            let one = ch.to_string();
                            self.key(KeyCode::F24, Some(&one), true, false);
                            self.key(KeyCode::F24, None, false, false);
                        }
                    } else {
                        said = "nothing is taking text right now (open the seed box or speak to someone first)".into();
                    }
                }
                drive::Step::Shot(path) => {
                    self.shot_next = Some(path);
                    break;
                }
                drive::Step::Info => {
                    let (w, h) = self.gfx.as_ref().map_or((0.0, 0.0), |g| (g.config.width as f32 / scale, g.config.height as f32 / scale));
                    let who = match self.selected {
                        Some(view::Pick::Person(id)) => self.world.sim.people.get(id).map_or("someone".into(), |p| p.name.clone()),
                        Some(_) => "a thing".into(),
                        None => "nobody".into(),
                    };
                    let menu = self.hud.as_ref().and_then(|h| h.menu.as_ref()).map_or("none".to_string(), |m| format!("{m:?}").split([' ', '{']).next().unwrap_or("").to_string());
                    let ledger = self.hud.as_ref().and_then(|h| h.ledger).map_or("none", |l| l.name());
                    said = format!(
                        "window {w:.0}x{h:.0} · {} · {} · speed {}x · selected {who} · menu {menu} · ledger {ledger}",
                        self.world.sim.describe_time(),
                        if self.world.paused { "paused" } else { "running" },
                        self.world.speed
                    );
                }
                drive::Step::Quit => self.quit = true,
            }
            if let Some(d) = self.drive.as_mut() {
                d.report(said);
                if stop {
                    // Let the input land (the UI sees it on the next frame) before going on.
                    d.frames = 2;
                }
            }
            if stop {
                break;
            }
        }
    }

    fn redraw(&mut self) {
        self.run_drive();
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;
        self.update(dt as f32);
        // fps over the last half second.
        let w = &mut self.fps_window;
        w.1 += 1;
        w.2 += dt * 1000.0;
        if w.0.elapsed().as_secs_f64() >= 0.5 {
            self.fps = ((w.1 as f64 / w.0.elapsed().as_secs_f64()) as f32, (w.2 / w.1 as f64) as f32);
            *w = (Instant::now(), 0, 0.0);
        }
        let elapsed = self.started.elapsed().as_secs_f64();
        if let Some(secs) = self.perf_secs {
            if elapsed > 2.0 {
                self.frame_ms.push(dt * 1000.0);
            }
            if elapsed > secs + 2.0 {
                if let Some(g) = self.gfx.as_ref() {
                    g.window.set_visible(false);
                }
                std::process::exit({
                    if let Some(r) = self.perf_report() {
                        println!("{r}");
                    }
                    0
                });
            }
        }
        let light = self.world.lighting();
        let Some(g) = self.gfx.as_mut() else { return };
        let t_acquire = Instant::now();
        // No image from the window (it's covered, or just resized): carry on without one, so the
        // UI still runs and a driver's screenshot is still taken offscreen.
        let frame = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => Some(f),
            _ => {
                g.surface.configure(g.renderer.device(), &g.config);
                None
            }
        };
        let acquire_ms = t_acquire.elapsed().as_secs_f64() * 1000.0;
        let t_draw = Instant::now();
        let view = frame.as_ref().map(|f| f.texture.create_view(&Default::default()));
        if self.world.sim.map.version != g.renderer.world_version {
            let nodes = self.world.nodes();
            g.renderer.update_world(&self.world.sim.map, &nodes, &self.world.sim.content);
        }
        let win = (g.config.width, g.config.height);
        let scale = g.window.scale_factor() as f32;
        let lens = self.hud.as_ref().map_or(hud::Lens::Off, |h| h.lens);
        let Scene { people, camp, things, animals } = self.world.scene(&self.camera, self.selected.and_then(|s| s.person()), &g.renderer, lens);
        let dragging = self.drag.as_ref().is_some_and(|d| d.moving);
        let picked = if dragging { None } else { view::pick(&g.renderer, &self.world.sim, &self.world.ground, &self.camera, &people, self.cursor, win, scale) };
        let click = std::mem::take(&mut self.click);
        let scroll = std::mem::take(&mut self.scroll);
        // A watched decision paused the game: show its trace.
        if let Some(who) = self.world.caught.take()
            && let Some(h) = self.hud.as_mut()
        {
            h.trace = Some((who, 0));
            self.selected = Some(view::Pick::Person(who));
        }
        let mut ui_out = None;
        // Menu actions that replace the world or touch the app wait until the frame is drawn.
        let mut later: Vec<hud::Action> = Vec::new();
        if self.show_ui
            && let Some(h) = self.hud.as_mut()
        {
            h.take_events(&mut self.world.sim);
            // The UI drawn last frame decides whether the cursor is over it.
            let over = h.ui.over_ui();
            self.hovered = if over { None } else { picked };
            let mut status = self.world.status(self.fps.0, self.fps.1, self.selected, self.hovered);
            status.heads = heads(&g.renderer, &self.world.sim, &people, win);
            status.in_game = self.in_game;
            status.following = self.follow;
            status.slots = self.slots.clone();
            status.app = self.app.clone();
            h.ui.begin((win.0 as f32, win.1 as f32), scale * self.app.ui_scale, ui::Input { cursor: (self.cursor.x, self.cursor.y), clicked: click && !dragging, scroll });
            for act in h.frame(&self.world.sim, &status) {
                match act {
                    hud::Action::Select(s) => {
                        // Picked from a link, a list or the feed: bring them into view.
                        if let Some(view::Pick::Person(id)) = s
                            && let Some(p) = self.world.sim.people.get(id)
                        {
                            self.glide = Some(Vec2::new(p.pos.0, p.pos.1));
                        }
                        self.selected = s;
                    }
                    hud::Action::Follow(f) => {
                        self.follow = f;
                        self.glide = None;
                    }
                    hud::Action::Speed(v) => {
                        self.world.speed = v;
                        self.world.paused = false;
                    }
                    hud::Action::TogglePause => self.world.paused = !self.world.paused,
                    hud::Action::PauseOnNextDecision(who) => self.world.toggle_watch(who),
                    hud::Action::Command(c) => self.world.sim.command(c),
                    hud::Action::SetAi(ai) => self.world.set_ai(ai),
                    hud::Action::Look((x, z)) => {
                        let n = self.world.sim.map.size as f32;
                        self.camera.focus.x = x.clamp(0.0, n);
                        self.camera.focus.z = z.clamp(0.0, n);
                    }
                    hud::Action::OpenMenu(m) => h.menu = Some(m),
                    other => later.push(other),
                }
            }
            if click && !over && !h.ui.over_ui() && h.trace.is_none() {
                // A click on the world: select whoever is under the cursor, or nobody.
                self.selected = self.hovered;
            }
            ui_out = Some(h.ui.finish());
        } else {
            self.hovered = picked;
            if click {
                self.selected = self.hovered;
            }
        }
        let fr = Frame {
            camera: &self.camera,
            light: &light,
            time: elapsed as f32,
            ui: ui_out,
            ground: lens.ground(),
            people: &people,
            camp: Some(camp),
            things: &things,
            beauty: Some(&self.world.sim.beauty),
            animals: &animals,
            marker: self.selected.and_then(|p| view::marker(&self.world.sim, &self.world.ground, p)),
        };
        let atlas_sent = fr.ui.as_ref().is_some_and(|u| u.atlas.is_some());
        let mut drawn = frame.is_some();
        // A screenshot the driver asked for: the same frame, drawn offscreen and written as a PNG
        // (before the done file, so whoever asked never reads half an image).
        if let Some(path) = self.shot_next.take() {
            // At window-point size (what `click` takes), unless the path ends "@2x.png".
            let img = gpu::render_to_image(&mut g.renderer, win, &fr);
            drawn = true;
            let k = if path.to_string_lossy().ends_with("@2x.png") { 1 } else { scale.round().max(1.0) as u32 };
            let (small, size) = gpu::shrink(&img, win, k);
            gpu::write_png(&path, size, &small);
            if let Some(d) = self.drive.as_mut() {
                d.report(format!("ok {}", path.display()));
                d.frames = 1;
            }
        }
        if let (Some(view), Some(frame)) = (view, frame) {
            g.renderer.render(&view, win, &fr);
            g.window.pre_present_notify();
            g.renderer.queue().present(frame);
        }
        // Nothing was drawn (no image from the window): new glyphs didn't reach the GPU yet.
        if atlas_sent
            && !drawn
            && let Some(h) = self.hud.as_mut()
        {
            h.ui.resend_atlas();
        }
        if self.perf_secs.is_some() && elapsed > 2.0 {
            self.split_ms.push((acquire_ms, t_draw.elapsed().as_secs_f64() * 1000.0));
        }
        for act in later {
            match act {
                hud::Action::CloseMenu => self.close_menu(),
                hud::Action::NewWorld { seed, size, beauty, packs } => self.new_world(seed, size, beauty, packs),
                hud::Action::Save(slot) => {
                    self.save(slot, false);
                    if let Some(h) = self.hud.as_mut() {
                        h.menu = Some(h.menu_home.clone());
                    }
                }
                hud::Action::Load(slot) => self.load(slot),
                hud::Action::Settings(s) => {
                    self.app = s;
                    self.app.store();
                    self.apply_settings();
                }
                hud::Action::Quit => self.quit = true,
                _ => {}
            }
        }
        self.tend_saves();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let (w, h) = self.args.size("window", (1280, 800));
        let attrs = Window::default_attributes()
            .with_title("Colony")
            .with_inner_size(winit::dpi::LogicalSize::new(w, h))
            .with_active(!self.args.has("background"));
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let vsync = self.args.has("vsync") || !(self.args.has("novsync") || self.perf_secs.is_some());
        let (surface, device, queue, config) = gpu::for_window(window.clone(), vsync).unwrap_or_else(|e| panic!("no GPU: {e}"));
        let nodes: Vec<sim::Node> = self.world.sim.nodes.iter().map(|(_, n)| n.clone()).collect();
        let renderer = Renderer::new(device, queue, config.format, &self.world.sim.map, &nodes, &self.world.sim.content, &assets_dir())
            .unwrap_or_else(|e| panic!("renderer: {e}"));
        self.gfx = Some(Gfx { window, surface, config, renderer });
        // Start with the UI options too (--inspect, --tab, --lens, --detail), for perf runs.
        let size = (self.gfx.as_ref().map_or(1, |g| g.config.width), self.gfx.as_ref().map_or(1, |g| g.config.height));
        if let Some(st) = stage_ui(&self.args, &mut self.world, size) {
            self.selected = st.selected;
            self.hud = Some(st.hud);
        }
        // The main menu, over the world from --seed (playing on behind it).
        let menu = !(self.args.has("no-menu") || self.perf_secs.is_some());
        match self.hud.as_mut() {
            Some(h) if menu => {
                h.menu_home = hud::Menu::Main;
                h.menu = Some(hud::Menu::Main);
            }
            _ => self.in_game = true,
        }
        self.apply_settings();
        self.last = Instant::now();
        self.started = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.quit = true;
                self.about_to_wait(el);
            }
            WindowEvent::Resized(size) => {
                if let Some(g) = self.gfx.as_mut()
                    && size.width > 0
                    && size.height > 0
                {
                    g.config.width = size.width;
                    g.config.height = size.height;
                    g.surface.configure(g.renderer.device(), &g.config);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                self.key(code, event.text.as_deref(), event.state == ElementState::Pressed, event.repeat);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = Vec2::new(position.x as f32, position.y as f32);
                let d = p - self.cursor;
                self.cursor = p;
                if let Some(drag) = self.drag.as_mut() {
                    if !drag.moving && (p - drag.start).length() > 6.0 {
                        drag.moving = true;
                    }
                    let moving = drag.moving;
                    drag.last = p;
                    if moving {
                        self.drag_pixels(d);
                    }
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if state == ElementState::Released && self.drag.as_ref().is_some_and(|d| !d.moving) {
                    // A click (not a drag): the UI or the world takes it next frame.
                    self.click = true;
                }
                self.drag = (state == ElementState::Pressed).then_some(Drag { start: self.cursor, last: self.cursor, moving: false });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Over the UI, scrolling scrolls lists; over the world it pans or zooms.
                let over = self.hud.as_ref().is_some_and(|h| h.ui.over_ui());
                match delta {
                    MouseScrollDelta::LineDelta(_, y) if over => self.scroll += y,
                    MouseScrollDelta::PixelDelta(p) if over => self.scroll += p.y as f32 / 40.0,
                    MouseScrollDelta::LineDelta(_, y) => self.zoom_by(0.88f32.powf(y)),
                    MouseScrollDelta::PixelDelta(p) => self.pan_pixels(Vec2::new(p.x as f32, p.y as f32)),
                }
            }
            WindowEvent::PinchGesture { delta, .. } => self.zoom_by(1.0 / (1.0 + delta as f32).max(0.2)),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.quit {
            // Keep the world: the autosave first (a quick save, waited for).
            if self.in_game && self.args.get("perf").is_none() {
                self.save(saves::AUTOSAVE, true);
            }
            if let Some((job, _)) = self.save_job.take() {
                let _ = job.wait();
            }
            if let Some(d) = self.drive.as_mut() {
                d.report("quit (the autosave written first)");
                d.flush();
            }
            el.exit();
            return;
        }
        // Lower power: at most 60 frames a second.
        if self.app.power == saves::Power::Lower {
            let next = self.last + std::time::Duration::from_micros(16_667);
            if Instant::now() < next {
                el.set_control_flow(ControlFlow::WaitUntil(next));
                return;
            }
        }
        el.set_control_flow(ControlFlow::Poll);
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}
