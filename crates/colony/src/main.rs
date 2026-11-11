//! Colony, the game: a window onto the sim, drawn by the renderer.
//!
//!   colony [--seed N] [--size 256] [--time 10] [--day 1] [--zoom 26]
//!          [--window 1280x800] [--background] [--novsync]
//!   colony --shot out.png [--shot-size 1600x1000] ...     render one frame with no window, save it
//!   colony --bench 3024x1964 [--frames 300] ...           time frames with no window (GPU cost)
//!   colony --perf 10 ...                                  run windowed for N seconds, print fps
//!
//! Controls: drag or two-finger scroll to pan, pinch or wheel to zoom, WASD/arrows, Z/X zoom,
//! Space pause, 1/2/3 speed, T +1 hour, Y +1 day, F full screen, F3 hide the readout.

mod gpu;
mod view;

use glam::{Vec2, Vec3};
use render::{Camera, Frame, Lighting, Panel, Renderer};
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
}

/// Game minutes per real second at 1x.
const MINUTES_PER_SECOND: f32 = 3.0;

impl World {
    fn new(a: &Args) -> World {
        let mut sim = Sim::new(a.num("seed", 2u64), a.num("size", 256u32));
        let cal = &sim.content.calendar;
        let day = a.num("day", 1u64).max(1) - 1;
        let hour: f32 = a.num("time", 10.0);
        let target = day * cal.ticks_per_day() + (hour * 60.0) as u64 / cal.minutes_per_tick as u64;
        if target > sim.tick {
            sim.run_ticks(target - sim.tick);
        }
        let prev = view::poses(&sim);
        let ground = render::terrain::Ground::new(&sim.map);
        World { sim, paused: false, speed: 1.0, carry: 0.0, prev, ground }
    }

    fn advance(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        self.carry += dt * MINUTES_PER_SECOND * self.speed / self.sim.content.calendar.minutes_per_tick as f32;
        let n = self.carry.floor();
        self.carry -= n;
        if n > 0.0 {
            self.sim.run_ticks(n as u64 - 1);
            self.prev = view::poses(&self.sim);
            self.sim.step();
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
        let day_in_season = cal.day_of_season(t) - 1;
        let hour = self.hour();
        // Snow settles over the first half-day of winter and melts over the first of spring.
        let snow = match (season, day_in_season) {
            (3, 0) => (hour / 12.0).min(1.0),
            (3, _) => 1.0,
            (0, 0) => (1.0 - hour / 12.0).max(0.0),
            _ => 0.0,
        };
        Lighting::at(hour, season, snow)
    }

    fn describe(&self) -> String {
        self.sim.describe_time()
    }

    /// Everything the renderer needs from the sim this frame.
    fn scene(&self, selected: Option<sim::PersonId>, walk_ref: f32) -> (Vec<render::people::PersonDraw>, render::CampDraw) {
        let alpha = if self.paused { 1.0 } else { self.carry.clamp(0.0, 1.0) };
        let tps = if self.paused { 0.0 } else { MINUTES_PER_SECOND * self.speed / self.sim.content.calendar.minutes_per_tick as f32 };
        (view::people(&self.sim, &self.ground, &self.prev, alpha, selected, tps, walk_ref), view::camp(&self.sim, &self.ground))
    }

    fn nodes(&self) -> Vec<sim::Node> {
        self.sim.nodes.iter().map(|(_, n)| n.clone()).collect()
    }
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

fn shot(a: &Args, path: PathBuf) {
    let world = World::new(a);
    let size = a.size("shot-size", (1600, 1000));
    let mut r = headless_renderer(&world);
    let cam = start_camera(&world.sim, a);
    let light = world.lighting();
    let mut panels: Vec<Panel> = if a.has("readout") { vec![Panel { x: 12.0, y: 12.0, lines: vec![world.describe()] }] } else { Vec::new() };
    // --inspect K: select the K-th person and show the inspector (and the next one's hover card).
    let living: Vec<sim::PersonId> = world.sim.people.iter().filter(|(_, p)| p.alive).map(|(id, _)| id).collect();
    let selected = a.get("inspect").and_then(|k| k.parse::<usize>().ok()).and_then(|k| living.get(k).copied());
    if let Some(s) = selected {
        let lines = view::inspector(&world.sim, s);
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 8.0;
        panels.push(Panel { x: size.0 as f32 - width - 24.0, y: 12.0, lines });
        if let Some(h) = living.iter().find(|h| **h != s) {
            panels.push(Panel { x: 40.0, y: size.1 as f32 - 140.0, lines: view::hover(&world.sim, *h) });
        }
    }
    let (people, camp) = world.scene(selected, r.walk_speed());
    let frame = Frame { camera: &cam, light: &light, time: 0.0, panels: &panels, ui_scale: 2, people: &people, camp: Some(camp) };
    let img = gpu::render_to_image(&mut r, size, &frame);
    gpu::write_png(&path, size, &img);
    println!("wrote {} ({}x{}, {}); walk cycle no-slide speed {:.2} m/s, people walk {:.2} m/s at 1x", path.display(), size.0, size.1, world.describe(), r.walk_speed(), world.sim.content.people.walk_speed * MINUTES_PER_SECOND);
}

fn bench(a: &Args) {
    let world = World::new(a);
    let size = a.size("bench", (3024, 1964));
    let frames = a.num("frames", 300u32);
    let mut r = headless_renderer(&world);
    let mut cam = start_camera(&world.sim, a);
    let light = world.lighting();
    let target = gpu::offscreen_target(r.device(), size, r.target_format());
    let view = target.create_view(&Default::default());
    let mut times = Vec::new();
    for f in 0..frames + 20 {
        // Pan slowly so culling and snapping do real work.
        cam.focus.x += 0.05;
        let t = Instant::now();
        let (people, camp) = world.scene(None, r.walk_speed());
        r.render(&view, size, &Frame { camera: &cam, light: &light, time: f as f32 / 60.0, panels: &[], ui_scale: 2, people: &people, camp: Some(camp) });
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
        let (people, camp) = world.scene(None, r.walk_speed());
        let panels = vec![Panel { x: 6.0, y: 6.0, lines: vec![format!("{} seed {seed} {x},{z}", k + 1)] }];
        let img = gpu::render_to_image(r, tile, &Frame { camera: &cam, light: &light, time: 3.0, panels: &panels, ui_scale: 2, people: &people, camp: Some(camp) });
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
        let (people, camp) = world.scene(None, r.walk_speed());
        let img = gpu::render_to_image(&mut r, size, &Frame { camera: &cam, light: &light, time: 0.0, panels: &[], ui_scale: 2, people: &people, camp: Some(camp) });
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
    readout: Vec<String>,
    show_readout: bool,
    fps_window: (Instant, u32, f64),
    perf_secs: Option<f64>,
    hovered: Option<sim::PersonId>,
    selected: Option<sim::PersonId>,
}

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
            readout: vec!["-- fps".into()],
            show_readout: true,
            fps_window: (Instant::now(), 0, 0.0),
            perf_secs,
            hovered: None,
            selected: None,
            args,
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

    /// Pan by `d` window pixels (trackpad scrolling).
    fn pan_pixels(&mut self, d: Vec2) {
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
        self.world.advance(dt);
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;
        self.update(dt as f32);
        // Readout: fps over the last half second.
        let w = &mut self.fps_window;
        w.1 += 1;
        w.2 += dt * 1000.0;
        if w.0.elapsed().as_secs_f64() >= 0.5 {
            let fps = w.1 as f64 / w.0.elapsed().as_secs_f64();
            let ms = w.2 / w.1 as f64;
            let speed = if self.world.paused { "paused".to_string() } else { format!("{}x", self.world.speed) };
            self.readout = vec![format!("{fps:.0} fps  {ms:.1} ms"), self.world.describe(), format!("{} people  {speed}", self.world.sim.alive())];
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
        let frame = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                g.surface.configure(g.renderer.device(), &g.config);
                return;
            }
        };
        let acquire_ms = t_acquire.elapsed().as_secs_f64() * 1000.0;
        let t_draw = Instant::now();
        let view = frame.texture.create_view(&Default::default());
        if self.world.sim.map.version != g.renderer.world_version {
            let nodes = self.world.nodes();
            g.renderer.update_world(&self.world.sim.map, &nodes, &self.world.sim.content);
        }
        let win = (g.config.width, g.config.height);
        let scale = g.window.scale_factor() as f32;
        let ui = (2.0 * scale).round().max(1.0) as u32;
        let (people, camp) = self.world.scene(self.selected, g.renderer.walk_speed());
        self.hovered = if self.drag.as_ref().is_some_and(|d| d.moving) { None } else { view::pick(&g.renderer, &self.world.sim, &people, self.cursor, win, scale) };
        let mut panels = Vec::new();
        if self.show_readout {
            panels.push(Panel { x: 12.0 * scale, y: 12.0 * scale, lines: self.readout.clone() });
        }
        if let Some(h) = self.hovered.filter(|h| Some(*h) != self.selected) {
            panels.push(Panel { x: self.cursor.x + 18.0 * scale, y: self.cursor.y + 18.0 * scale, lines: view::hover(&self.world.sim, h) });
        }
        if let Some(s) = self.selected {
            let lines = view::inspector(&self.world.sim, s);
            let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 4.0 * ui as f32;
            panels.push(Panel { x: win.0 as f32 - width - 24.0 * scale, y: 12.0 * scale, lines });
        }
        g.renderer.render(&view, win, &Frame {
            camera: &self.camera,
            light: &light,
            time: elapsed as f32,
            panels: &panels,
            ui_scale: ui,
            people: &people,
            camp: Some(camp),
        });
        g.window.pre_present_notify();
        g.renderer.queue().present(frame);
        if self.perf_secs.is_some() && elapsed > 2.0 {
            self.split_ms.push((acquire_ms, t_draw.elapsed().as_secs_f64() * 1000.0));
        }
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
        self.last = Instant::now();
        self.started = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
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
                if event.state == ElementState::Pressed {
                    if !event.repeat {
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
                            KeyCode::F3 => self.show_readout = !self.show_readout,
                            KeyCode::Escape => self.selected = None,
                            KeyCode::KeyF => {
                                if let Some(g) = &self.gfx {
                                    let full = g.window.fullscreen().is_some();
                                    g.window.set_fullscreen(if full { None } else { Some(Fullscreen::Borderless(None)) });
                                }
                            }
                            _ => {}
                        }
                    }
                    self.keys.insert(code);
                } else {
                    self.keys.remove(&code);
                }
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
                    // A click (not a drag): select whoever is under the cursor, or nobody.
                    self.selected = self.hovered;
                }
                self.drag = (state == ElementState::Pressed).then_some(Drag { start: self.cursor, last: self.cursor, moving: false });
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(_, y) => self.zoom_by(0.88f32.powf(y)),
                MouseScrollDelta::PixelDelta(p) => self.pan_pixels(Vec2::new(p.x as f32, p.y as f32)),
            },
            WindowEvent::PinchGesture { delta, .. } => self.zoom_by(1.0 / (1.0 + delta as f32).max(0.2)),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}
