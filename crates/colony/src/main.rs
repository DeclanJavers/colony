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

/// The sim, plus the clock that drives it in real time.
pub struct World {
    pub sim: Sim,
    paused: bool,
    speed: f32,
    carry: f32,
}

/// Game minutes per real second at 1x.
const MINUTES_PER_SECOND: f32 = 4.0;

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
        World { sim, paused: false, speed: 1.0, carry: 0.0 }
    }

    fn advance(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        self.carry += dt * MINUTES_PER_SECOND * self.speed / self.sim.content.calendar.minutes_per_tick as f32;
        let n = self.carry.floor();
        self.carry -= n;
        self.sim.run_ticks(n as u64);
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
}

fn start_camera(sim: &Sim, a: &Args) -> Camera {
    let (sx, sy) = sim.map.start;
    let (x, z) = (a.num("x", sx as f32 + 0.5), a.num("z", sy as f32 + 0.5));
    let mut cam = Camera::new(Vec3::new(x, render::terrain::surface_at(&sim.map, x, z), z));
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
    let lines = vec![world.describe()];
    let overlay = a.has("readout").then_some(lines.as_slice());
    let img = gpu::render_to_image(&mut r, size, &Frame { camera: &cam, light: &light, time: 0.0, overlay });
    gpu::write_png(&path, size, &img);
    println!("wrote {} ({}x{}, {})", path.display(), size.0, size.1, world.describe());
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
        r.render(&view, size, &Frame { camera: &cam, light: &light, time: f as f32 / 60.0, overlay: None });
        r.device().poll(wgpu::PollType::wait_indefinitely()).expect("gpu finished");
        if f >= 20 {
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    println!("{}", perf_json("bench", size, &times));
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
        let img = gpu::render_to_image(&mut r, size, &Frame { camera: &cam, light: &light, time: 0.0, overlay: None });
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

    /// Move the world with the cursor: a drag of `d` window pixels.
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
        let ground = render::terrain::surface_at(&self.world.sim.map, self.camera.focus.x, self.camera.focus.z);
        self.camera.focus.y += (ground - self.camera.focus.y) * (1.0 - (-dt * 4.0).exp());
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
            self.readout = vec![format!("{fps:.0} fps  {ms:.1} ms"), self.world.describe()];
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
        let overlay = self.show_readout.then_some(self.readout.as_slice());
        g.renderer.render(&view, (g.config.width, g.config.height), &Frame {
            camera: &self.camera,
            light: &light,
            time: elapsed as f32,
            overlay,
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
                        self.pan_pixels(d);
                    }
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
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
