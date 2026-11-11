//! Colony's renderer (ARCHITECTURE.md section 11): terrain chunks, water and instanced models,
//! lit by the sun with a shadow map, drawn into a low-resolution target whose pixels belong to the
//! world (48 per tile), then scaled up by the pixel pass with outlines and the night grade.
//! The camera snaps to the art-pixel grid so panning never crawls. The renderer only reads the sim.

pub mod models;
pub mod people;
pub mod terrain;

use glam::{Mat4, Vec2, Vec3, Vec4};
use models::{Instance, ModelMesh, ModelVertex};
use people::{AnimState, MAX_PEOPLE, PersonDraw, PersonInstance, PersonVertex, Rig};
use std::collections::BTreeMap;
use sim::content::{Content, hex_colour};
use sim::world::{Map, Node};
use std::path::Path;
use terrain::{CHUNK, TerrainVertex, WaterVertex};
use wgpu::util::DeviceExt;

/// Art pixels per tile (metre) of ground.
pub const PER_TILE: f32 = 48.0;

/// Art pixels per tile: PER_TILE, or COLONY_ART_PER_TILE for comparing densities.
fn per_tile() -> f32 {
    static V: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("COLONY_ART_PER_TILE").ok().and_then(|v| v.parse().ok()).unwrap_or(PER_TILE))
}
/// The format the scene is drawn in before the pixel pass.
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADOW_SIZE: u32 = 2048;
const CAM_DIST: f32 = 400.0;
const NEAR: f32 = 1.0;
const FAR: f32 = 900.0;
/// Node kinds whose leaves change colour with the seasons. (Pines stay green.)
const SEASONAL: &[&str] = &["tree"];

/// sRGB hex to linear RGB.
pub fn linear(hex: &str) -> [f32; 3] {
    hex_colour(hex).map(|c| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    })
}

fn v4(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

// ---------------------------------------------------------------------------------------------
// Camera and lighting

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// The point looked at: x and z on the ground, y its (smoothed) height.
    pub focus: Vec3,
    /// How many metres of world the view is tall.
    pub size: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Camera {
    pub fn new(focus: Vec3) -> Self {
        Camera { focus, size: 16.0, yaw: 45f32.to_radians(), pitch: 35f32.to_radians() }
    }

    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sy * cp, -sp, -cy * cp).normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// Unit vector on the ground pointing "up the screen".
    pub fn ground_up(&self) -> Vec2 {
        let f = self.forward();
        Vec2::new(f.x, f.z).normalize()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Lighting {
    /// Towards the sun (or the moon at night).
    pub sun_dir: Vec3,
    pub sun_strength: f32,
    pub sun_col: Vec3,
    pub ambient: Vec3,
    /// 0 by day .. about 0.85 at night
    pub night: f32,
    /// 0 spring, 1 summer, 2 autumn, 3 winter
    pub season: usize,
    /// Snow on the ground, 0..1
    pub snow: f32,
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Lighting {
    /// Light for an hour of the day (0..24) in a season. The light always comes from the same
    /// direction (the back left of the view, docs/STYLE.md), so shadows never crawl; the day shows
    /// in brightness and colour: warm low light at the ends of the day, cool moonlight at night.
    pub fn at(hour: f32, season: usize, snow: f32) -> Self {
        let sun = Vec3::new(-0.92, 1.25, -0.38).normalize();
        let day = ((hour - 6.0) / 14.0).clamp(0.0, 1.0);
        let height = (day * std::f32::consts::PI).sin();
        let night = ((1.0 - smoothstep(5.0, 7.0, hour)) + smoothstep(19.0, 21.0, hour)).min(1.0) * 0.85;
        let low = 1.0 - smoothstep(0.15, 0.55, height);
        let warm = Vec3::new(1.0, 0.93, 0.82).lerp(Vec3::new(1.0, 0.72, 0.48), low);
        Lighting {
            sun_dir: sun,
            sun_strength: (0.62 + 0.2 * height) * (1.0 - night) + 0.35 * night,
            sun_col: warm.lerp(Vec3::new(0.55, 0.62, 0.9), night),
            ambient: Vec3::new(0.30, 0.32, 0.38).lerp(Vec3::new(0.30, 0.33, 0.45), night),
            night,
            season,
            snow,
        }
    }
}

/// The camp's things to draw: the stores pile and the fire pit.
#[derive(Clone, Copy, Debug)]
pub struct CampDraw {
    pub stores: Vec3,
    pub fire: Vec3,
    pub lit: bool,
}

/// What to draw this frame.
pub struct Frame<'a> {
    pub camera: &'a Camera,
    pub light: &'a Lighting,
    /// Seconds, for animation (water ripples, walking).
    pub time: f32,
    /// The UI, laid out by the ui crate (None draws no UI).
    pub ui: Option<ui::Output<'a>>,
    /// The traffic lens: footsteps shown as heat on the ground.
    pub traffic: bool,
    pub people: &'a [PersonDraw],
    pub camp: Option<CampDraw>,
    /// A gold ring around a selected thing (not a person): where, and how wide (metres).
    pub marker: Option<(Vec3, f32)>,
}

/// How the camera maps the world onto the window this frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Window pixels per art pixel.
    pub px: u32,
    /// Metres per art pixel.
    pub t: f32,
    /// Size of the low-resolution scene target.
    pub low: (u32, u32),
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub forward: Vec3,
    pub half: Vec2,
}

impl View {
    pub fn new(cam: &Camera, w: u32, h: u32) -> View {
        // Zoom snaps to fixed levels so pixels never twitch: close up, an art pixel is exactly
        // 1/48 m and a whole number of screen pixels; far out, a screen pixel shows a whole number
        // of art pixels. Between levels nothing changes.
        let wanted = cam.size.max(0.5) / h.max(1) as f32; // metres per screen pixel
        let art = 1.0 / per_tile();
        let (px, t) = if wanted <= art {
            (((art / wanted).round() as u32).max(1), art)
        } else {
            (1, art * (wanted / art).round().max(1.0))
        };
        let ppm = px as f32 / t;
        let low = (w.max(1).div_ceil(px), h.max(1).div_ceil(px));
        let fwd = cam.forward();
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd);
        let mut eye = cam.focus - fwd * CAM_DIST;
        // Snap to the art-pixel grid: the picture moves a whole art pixel at a time.
        let (r, u) = (eye.dot(right), eye.dot(up));
        eye += right * ((r / t).round() * t - r) + up * ((u / t).round() * t - u);
        let half = Vec2::new(low.0 as f32 * t / 2.0, low.1 as f32 * t / 2.0);
        // The low-res image is anchored to the window's top left, so shift the projection to
        // keep the focus at the window's centre.
        let off = Vec2::new((low.0 * px) as f32 - w as f32, (low.1 * px) as f32 - h as f32) / ppm / 2.0;
        let proj = ortho_rh(-half.x + off.x, half.x + off.x, -half.y - off.y, half.y - off.y, NEAR, FAR);
        let view = look_at_rh(eye, eye + fwd, Vec3::Y);
        View { px, t, low, view_proj: proj * view, eye, forward: fwd, half }
    }
}

/// Right-handed view matrix (camera looks down -Z).
fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        Vec4::new(s.x, u.x, -f.x, 0.0),
        Vec4::new(s.y, u.y, -f.y, 0.0),
        Vec4::new(s.z, u.z, -f.z, 0.0),
        Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

/// Right-handed orthographic projection with depth 0 (near) .. 1 (far), as wgpu expects.
fn ortho_rh(l: f32, r: f32, b: f32, t: f32, near: f32, far: f32) -> Mat4 {
    let (w, h, d) = (r - l, t - b, far - near);
    Mat4::from_cols(
        Vec4::new(2.0 / w, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 2.0 / h, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -1.0 / d, 0.0),
        Vec4::new(-(r + l) / w, -(t + b) / h, -near / d, 1.0),
    )
}

/// Per-tile data for the ground shader: biome, wear (footsteps as a share of a full trail), moisture.
fn tile_data(map: &Map) -> Vec<u8> {
    let n = (map.size * map.size) as usize;
    let mut tiles = Vec::with_capacity(n * 4);
    for i in 0..n {
        let wear = (map.footfall[i] / sim::path::TRAIL_STEPS * 255.0).clamp(0.0, 255.0) as u8;
        tiles.extend_from_slice(&[map.biome[i], wear, (map.moisture[i] * 255.0) as u8, 0]);
    }
    tiles
}

fn visible(m: &Mat4, min: [f32; 3], max: [f32; 3]) -> bool {
    let mut out = [true; 6];
    for k in 0..8 {
        let p = Vec4::new(
            if k & 1 == 0 { min[0] } else { max[0] },
            if k & 2 == 0 { min[1] } else { max[1] },
            if k & 4 == 0 { min[2] } else { max[2] },
            1.0,
        );
        let c = *m * p;
        out[0] &= c.x < -c.w;
        out[1] &= c.x > c.w;
        out[2] &= c.y < -c.w;
        out[3] &= c.y > c.w;
        out[4] &= c.z < 0.0;
        out[5] &= c.z > c.w;
    }
    !out.iter().any(|&o| o)
}

// ---------------------------------------------------------------------------------------------
// GPU data

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    sun_view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sun_col: [f32; 4],
    ambient: [f32; 4],
    cam_fwd: [f32; 4],
    season: [f32; 4],
    biome: [[f32; 4]; 16],
    pal: [[f32; 4]; 8],
    fire: [f32; 4],
    /// x: 1 when the traffic lens is on
    lens: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PostUniform {
    p: [f32; 4],
    outline: [f32; 4],
    night_tint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectUniform {
    r: [f32; 4],
    win: [f32; 4],
}

struct Mesh {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    count: u32,
    min: [f32; 3],
    max: [f32; 3],
}

struct Model {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    count: u32,
    height: f32,
    instances: Option<wgpu::Buffer>,
    /// (chunk bounds min, max, first instance, instance count), instances sorted by chunk
    groups: Vec<([f32; 3], [f32; 3], u32, u32)>,
}

/// One node to draw: which model and where (built once; tints change with the seasons).
struct Placed {
    model: usize,
    chunk: u32,
    transform: Mat4,
    seasonal: bool,
    hash: u32,
}

struct LowRes {
    size: (u32, u32),
    colour: wgpu::TextureView,
    depth: wgpu::TextureView,
    post_bg: wgpu::BindGroup,
}

struct PeopleGpu {
    rig: Rig,
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    count: u32,
    instances: wgpu::Buffer,
    joints: wgpu::Buffer,
    bg: wgpu::BindGroup,
    pipe: wgpu::RenderPipeline,
    shadow_pipe: wgpu::RenderPipeline,
    anim: BTreeMap<u64, AnimState>,
    drawn: u32,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target_format: wgpu::TextureFormat,
    globals: Globals,
    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    scene_bg: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    terrain_pipe: wgpu::RenderPipeline,
    water_pipe: wgpu::RenderPipeline,
    model_pipe: wgpu::RenderPipeline,
    shadow_terrain_pipe: wgpu::RenderPipeline,
    shadow_model_pipe: wgpu::RenderPipeline,
    post_pipe: wgpu::RenderPipeline,
    post_bgl: wgpu::BindGroupLayout,
    post_buf: wgpu::Buffer,
    ui: UiGpu,
    chunks: Vec<Mesh>,
    water: Option<Mesh>,
    models: Vec<Model>,
    placed: Vec<Placed>,
    tint_season: Option<usize>,
    low: Option<LowRes>,
    map_size: u32,
    pub last_view: Option<View>,
    tile_tex: wgpu::Texture,
    /// Model index by file name (for the camp's things and what people carry).
    names: BTreeMap<String, usize>,
    /// Node kind -> model indices (variants).
    kind_models: Vec<Vec<usize>>,
    /// Instances drawn fresh each frame (camp objects, carried things): (model, first, count).
    dynamic: wgpu::Buffer,
    dynamic_groups: Vec<(usize, u32, u32)>,
    people: Option<PeopleGpu>,
    /// The map version the nodes and tiles were last built from.
    pub world_version: u64,
    ground: terrain::Ground,
}

const MAX_DYNAMIC: usize = 512;

impl Renderer {
    /// `target_format` is what `render` draws into (the window's surface, or an image).
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        target_format: wgpu::TextureFormat,
        map: &Map,
        nodes: &[Node],
        content: &Content,
        assets: &Path,
    ) -> Result<Renderer, String> {
        let d = &device;
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene.wgsl").into()),
        });
        let post_shader = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });
        let ui_shader = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui.wgsl").into()),
        });

        let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let tex_entry = |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let nofilter = wgpu::TextureSampleType::Float { filterable: false };

        let globals_bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("globals"), entries: &[uniform_entry(0)] });
        let scene_bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene"),
            entries: &[
                tex_entry(0, nofilter),
                tex_entry(1, wgpu::TextureSampleType::Depth),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                tex_entry(3, wgpu::TextureSampleType::Float { filterable: true }),
                tex_entry(5, nofilter),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let post_bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[uniform_entry(0), tex_entry(1, nofilter), tex_entry(2, wgpu::TextureSampleType::Depth)],
        });
        let globals = Globals::new(content);
        let globals_buf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_bg = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }],
        });

        // Tile data: biome, footfall, moisture.
        let n = map.size;
        let tiles = tile_data(map);
        let tile_tex = d.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("tiles"),
                size: wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &tiles,
        );
        let shadow_tex = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"),
            size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        // How the water moves (terrain::flow_field), sampled smoothly so it blends across tiles.
        let flow_tex = d.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("water flow"),
                size: wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &terrain::flow_field(map),
        );
        let flow_sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("water flow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Ground height at every tile corner, so the water shader knows the depth at each pixel.
        let corners = terrain::corner_heights(map);
        let height_tex = d.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("ground height"),
                size: wgpu::Extent3d { width: n + 1, height: n + 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&corners),
        );
        let scene_bg = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene"),
            layout: &scene_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&tile_tex.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&shadow_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&flow_tex.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&flow_sampler) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&height_tex.create_view(&Default::default())) },
            ],
        });

        let colour_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("colour"),
            bind_group_layouts: &[Some(&globals_bgl), Some(&scene_bgl)],
            immediate_size: 0,
        });
        let shadow_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow"),
            bind_group_layouts: &[Some(&globals_bgl)],
            immediate_size: 0,
        });
        let terrain_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32],
        };
        let water_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WaterVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32],
        };
        let model_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ModelVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32],
        };
        let instance_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4],
        };

        let depth = |bias: wgpu::DepthBiasState| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias,
        };
        let colour_target = [Some(wgpu::ColorTargetState { format: SCENE_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })];
        let pipe = |label: &str, layout: &wgpu::PipelineLayout, vs: &str, fs: Option<&str>, buffers: &[Option<wgpu::VertexBufferLayout>], bias: wgpu::DepthBiasState| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), compilation_options: Default::default(), buffers },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(depth(bias)),
                multisample: Default::default(),
                fragment: fs.map(|fs| wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &colour_target,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let no_bias = wgpu::DepthBiasState::default();
        let shadow_bias = wgpu::DepthBiasState { constant: 1, slope_scale: 1.2, clamp: 0.0 };
        let terrain_pipe = pipe("terrain", &colour_layout, "vs_terrain", Some("fs_terrain"), &[Some(terrain_vb.clone())], no_bias);
        let water_pipe = pipe("water", &colour_layout, "vs_water", Some("fs_water"), &[Some(water_vb)], no_bias);
        let model_pipe = pipe("models", &colour_layout, "vs_model", Some("fs_model"), &[Some(model_vb.clone()), Some(instance_vb.clone())], no_bias);
        let shadow_terrain_pipe = pipe("shadow terrain", &shadow_layout, "vs_shadow_terrain", None, &[Some(terrain_vb.clone())], shadow_bias);
        let shadow_model_pipe = pipe("shadow models", &shadow_layout, "vs_shadow_model", None, &[Some(model_vb), Some(instance_vb)], shadow_bias);

        let post_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post"),
            bind_group_layouts: &[Some(&post_bgl)],
            immediate_size: 0,
        });
        let post_pipe = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post"),
            layout: Some(&post_layout),
            vertex: wgpu::VertexState { module: &post_shader, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs_post"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: target_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let post_buf = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post"),
            size: std::mem::size_of::<PostUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui = UiGpu::new(d, &queue, &ui_shader, target_format);
        let dynamic = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dynamic instances"),
            size: (MAX_DYNAMIC * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut r = Renderer {
            device,
            queue,
            target_format,
            globals,
            globals_buf,
            globals_bg,
            scene_bg,
            shadow_view,
            terrain_pipe,
            water_pipe,
            model_pipe,
            shadow_terrain_pipe,
            shadow_model_pipe,
            post_pipe,
            post_bgl,
            post_buf,
            ui,
            chunks: Vec::new(),
            water: None,
            models: Vec::new(),
            placed: Vec::new(),
            tint_season: None,
            low: None,
            map_size: n,
            last_view: None,
            tile_tex,
            names: BTreeMap::new(),
            kind_models: Vec::new(),
            dynamic,
            dynamic_groups: Vec::new(),
            people: None,
            world_version: map.version,
            ground: terrain::Ground::new(map),
        };
        r.build_terrain(map);
        r.load_models(content, assets)?;
        r.place_nodes(map, nodes, content);
        r.people = Some(r.build_people(&shader, &globals_bgl, &scene_bgl, &assets.join("models/villager.glb"))?);
        Ok(r)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Ground speed (m/s, full-size person) at which the walk cycle's feet don't slide.
    pub fn walk_speed(&self) -> f32 {
        self.people.as_ref().map_or(1.0, |p| p.rig.natural_speed("walk"))
    }

    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    fn mesh<V: bytemuck::Pod>(&self, label: &str, m: &terrain::MeshData<V>) -> Option<Mesh> {
        if m.indices.is_empty() {
            return None;
        }
        let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(&m.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(&m.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Some(Mesh { vb, ib, count: m.indices.len() as u32, min: m.min, max: m.max })
    }

    fn build_terrain(&mut self, map: &Map) {
        let corners = terrain::corner_heights(map);
        let chunks = map.size.div_ceil(CHUNK);
        self.chunks.clear();
        for cz in 0..chunks {
            for cx in 0..chunks {
                let m = terrain::chunk_mesh(map, &self.ground, cx, cz);
                if let Some(mesh) = self.mesh("terrain chunk", &m) {
                    self.chunks.push(mesh);
                }
            }
        }
        self.water = self.mesh("water", &terrain::water_mesh(map, &corners));
    }

    fn load_models(&mut self, content: &Content, assets: &Path) -> Result<(), String> {
        // Each node kind's models: <id>.glb, <id>_2.glb, ...
        for def in &content.nodes {
            let mut list = Vec::new();
            for v in 1..=9 {
                let name = if v == 1 { def.id.clone() } else { format!("{}_{v}", def.id) };
                let path = assets.join("models").join(format!("{name}.glb"));
                if !path.exists() {
                    if v == 1 {
                        eprintln!("render: no model for '{}' ({})", def.id, path.display());
                    }
                    break;
                }
                list.push(self.models.len());
                self.upload_model(&models::load(&path)?);
            }
            self.kind_models.push(list);
        }
        for name in ["campfire", "stockpile", "prop_barrel"] {
            let path = assets.join("models").join(format!("{name}.glb"));
            if path.exists() {
                self.names.insert(name.into(), self.models.len());
                self.upload_model(&models::load(&path)?);
            }
        }
        // The selection ring: a flat gold ring that glows (drawn like a flame).
        let gold = linear("#E8C468");
        let mut ring = ModelMesh { name: "ring".into(), vertices: Vec::new(), indices: Vec::new(), height: 0.1 };
        let seg = 28;
        for k in 0..=seg {
            let a = k as f32 / seg as f32 * std::f32::consts::TAU;
            for r in [0.34f32, 0.44] {
                ring.vertices.push(ModelVertex { pos: [a.cos() * r, 0.04, a.sin() * r], normal: [0.0, 1.0, 0.0], colour: gold, flag: 3.0 });
            }
            if k < seg {
                let b = (k * 2) as u32;
                ring.indices.extend_from_slice(&[b, b + 1, b + 2, b + 1, b + 3, b + 2]);
            }
        }
        self.names.insert("ring".into(), self.models.len());
        self.upload_model(&ring);
        for (i, def) in content.nodes.iter().enumerate() {
            if let Some(&m) = self.kind_models[i].first() {
                self.names.entry(def.id.clone()).or_insert(m);
            }
        }
        Ok(())
    }

    /// Where every resource node sits (rebuilt when nodes come and go).
    fn place_nodes(&mut self, map: &Map, nodes: &[Node], content: &Content) {
        let chunks_x = map.size.div_ceil(CHUNK);
        self.placed.clear();
        for nd in nodes {
            let list = &self.kind_models[nd.kind as usize];
            if list.is_empty() {
                continue;
            }
            let model = list[nd.variant as usize % list.len()];
            let h = sim::hash::hash2(0xC01, nd.x as i32, nd.y as i32);
            let jitter = ((h & 0xFF) as f32 / 255.0 - 0.5) * 0.8;
            let def = &content.nodes[nd.kind as usize];
            // Things that get used up shrink a little as they do (berries, sticks...).
            let left = if def.amount > 0.0 { (nd.amount / def.amount).clamp(0.0, 1.0) } else { 1.0 };
            let shrink = if def.blocks && def.id != "berry_bush" { 1.0 } else { 0.55 + 0.45 * left };
            let scale = (0.88 + ((h >> 8) & 0xFF) as f32 / 255.0 * 0.26) * shrink;
            let pos = Vec3::new(nd.x as f32 + 0.5, self.ground.height(nd.x as f32 + 0.5, nd.y as f32 + 0.5), nd.y as f32 + 0.5);
            let transform = Mat4::from_translation(pos)
                * Mat4::from_rotation_y(nd.turn as f32 * std::f32::consts::FRAC_PI_2 + jitter)
                * Mat4::from_scale(Vec3::splat(scale));
            self.placed.push(Placed {
                model,
                chunk: (nd.y as u32 / CHUNK) * chunks_x + nd.x as u32 / CHUNK,
                transform,
                seasonal: SEASONAL.contains(&def.id.as_str()),
                hash: (h >> 16) as u32,
            });
        }
        self.placed.sort_by_key(|p| (p.model, p.chunk));
        self.tint_season = None;
    }

    /// The world changed (nodes used up or regrown, paths worn in): redo nodes and ground data.
    pub fn update_world(&mut self, map: &Map, nodes: &[Node], content: &Content) {
        self.world_version = map.version;
        self.place_nodes(map, nodes, content);
        for m in &mut self.models {
            m.instances = None;
        }
        let n = map.size;
        let tiles = tile_data(map);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.tile_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &tiles,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(n * 4), rows_per_image: Some(n) },
            wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        );
    }

    fn build_people(&self, shader: &wgpu::ShaderModule, globals_bgl: &wgpu::BindGroupLayout, scene_bgl: &wgpu::BindGroupLayout, path: &Path) -> Result<PeopleGpu, String> {
        let d = &self.device;
        let rig = Rig::load(path)?;
        let vb = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("villager"), contents: bytemuck::cast_slice(&rig.vertices), usage: wgpu::BufferUsages::VERTEX });
        let ib = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("villager"), contents: bytemuck::cast_slice(&rig.indices), usage: wgpu::BufferUsages::INDEX });
        let instances = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("people"),
            size: (MAX_PEOPLE * std::mem::size_of::<PersonInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let joints = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("joints"),
            size: (MAX_PEOPLE * rig.joint_count() * 64) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("joints"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let bg = d.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("joints"), layout: &bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: joints.as_entire_binding() }] });
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PersonVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32, 4 => Float32, 5 => Uint32],
        };
        let ibl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PersonInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4, 13 => Float32x4, 14 => Float32x4],
        };
        let colour_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("people"),
            bind_group_layouts: &[Some(globals_bgl), Some(scene_bgl), Some(&bgl)],
            immediate_size: 0,
        });
        let shadow_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("people shadow"),
            bind_group_layouts: &[Some(globals_bgl), None, Some(&bgl)],
            immediate_size: 0,
        });
        let depth = |bias| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias,
        };
        let targets = [Some(wgpu::ColorTargetState { format: SCENE_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })];
        let pipe = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("people"),
            layout: Some(&colour_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_person"), compilation_options: Default::default(), buffers: &[Some(vbl.clone()), Some(ibl.clone())] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth(wgpu::DepthBiasState::default())),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_model"), compilation_options: Default::default(), targets: &targets }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_pipe = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("people shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_shadow_person"), compilation_options: Default::default(), buffers: &[Some(vbl), Some(ibl)] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth(wgpu::DepthBiasState { constant: 1, slope_scale: 1.2, clamp: 0.0 })),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let count = rig.indices.len() as u32;
        Ok(PeopleGpu { rig, vb, ib, count, instances, joints, bg, pipe, shadow_pipe, anim: BTreeMap::new(), drawn: 0 })
    }

    /// Pose everyone, and gather this frame's dynamic models (camp things, carried things).
    fn prepare_dynamic(&mut self, frame: &Frame) {
        let mut dyn_list: Vec<(usize, Instance)> = Vec::new();
        if let Some(camp) = frame.camp {
            if let Some(&m) = self.names.get("stockpile") {
                dyn_list.push((m, Instance::new(Mat4::from_translation(camp.stores), None)));
            }
            if let Some(&m) = self.names.get("campfire") {
                let t = Mat4::from_translation(camp.fire);
                dyn_list.push((m, if camp.lit { Instance::new(t, None) } else { Instance::unlit(t) }));
            }
        }
        if let Some((at, width)) = frame.marker
            && let Some(&m) = self.names.get("ring")
        {
            let w = width / 0.78;
            dyn_list.push((m, Instance::new(Mat4::from_translation(at) * Mat4::from_scale(Vec3::new(w, 1.0, w)), None)));
        }
        if let Some(pg) = self.people.as_mut() {
            let jn = pg.rig.joint_count();
            let mut joints: Vec<Mat4> = Vec::with_capacity(frame.people.len() * jn);
            let mut inst: Vec<PersonInstance> = Vec::with_capacity(frame.people.len());
            for p in frame.people.iter().take(MAX_PEOPLE) {
                // Someone first seen mid-hold (already sitting or lying) starts settled, not standing.
                let settled = if Rig::holds(p.clip) { 1.0e3 } else { 0.0 };
                let st = pg.anim.entry(p.key).or_insert(AnimState { clip: p.clip, since: frame.time - 1.0, prev: p.clip, phase: settled, prev_phase: settled, last: frame.time });
                if st.clip != p.clip {
                    st.prev = st.clip;
                    st.prev_phase = st.phase;
                    st.clip = p.clip;
                    st.since = frame.time;
                    st.phase = 0.0;
                }
                // Clip time advances at the person's own rate (walking: their real ground speed).
                st.phase += (frame.time - st.last).clamp(0.0, 0.25) * p.rate;
                st.last = frame.time;
                let offset = (p.key % 97) as f32 * 0.137;
                let local = |clip: &str, phase: f32| {
                    if Rig::holds(clip) { phase.min(pg.rig.clips.get(clip).map_or(0.0, |c| c.duration - 0.01)) } else { phase + offset }
                };
                let blend = ((frame.time - st.since) / 0.25).clamp(0.0, 1.0);
                let base = joints.len();
                let hand = if blend < 1.0 {
                    pg.rig.pose((st.prev, local(st.prev, st.prev_phase)), Some((st.clip, local(st.clip, st.phase), blend)), &mut joints)
                } else {
                    pg.rig.pose((st.clip, local(st.clip, st.phase)), None, &mut joints)
                };
                let scale = Vec3::new(0.9 + 0.2 * p.build, p.height, 0.9 + 0.2 * p.build) * p.size;
                let model = Mat4::from_translation(p.pos) * Mat4::from_rotation_y(p.facing) * Mat4::from_scale(scale);
                inst.push(PersonInstance {
                    model: model.to_cols_array_2d(),
                    skin: v4(p.skin, 1.0),
                    hair: v4(p.hair, 1.0),
                    clothes: v4(p.clothes, 1.0),
                    eyes: v4(p.eyes, 1.0),
                    extra: [(p.parts | 1) as f32, base as f32, p.selected as u8 as f32, 0.0],
                });
                if let Some(&m) = self.names.get("ring") {
                    // Gold for the selected person; a lens colour (mood, needs) for everyone else.
                    let ring = Mat4::from_translation(p.pos) * Mat4::from_scale(Vec3::new(p.size.max(0.5), 1.0, p.size.max(0.5)));
                    if p.selected {
                        dyn_list.push((m, Instance::new(ring, None)));
                    } else if let Some(c) = p.ring {
                        dyn_list.push((m, Instance::new(ring * Mat4::from_scale(Vec3::new(0.8, 1.0, 0.8)), Some(c))));
                    }
                }
                if let Some(item) = p.carry
                    && let Some(&m) = self.names.get(item)
                {
                    let held = model * hand * Mat4::from_translation(Vec3::new(0.0, 0.06, 0.04)) * Mat4::from_scale(Vec3::splat(0.45));
                    dyn_list.push((m, Instance::new(held, None)));
                }
            }
            if !inst.is_empty() {
                self.queue.write_buffer(&pg.instances, 0, bytemuck::cast_slice(&inst));
                let flat: Vec<[[f32; 4]; 4]> = joints.iter().map(|m| m.to_cols_array_2d()).collect();
                self.queue.write_buffer(&pg.joints, 0, bytemuck::cast_slice(&flat));
            }
            pg.drawn = inst.len() as u32;
            let keep: std::collections::BTreeSet<u64> = frame.people.iter().map(|p| p.key).collect();
            pg.anim.retain(|k, _| keep.contains(k));
        }
        dyn_list.sort_by_key(|(m, _)| *m);
        dyn_list.truncate(MAX_DYNAMIC);
        self.dynamic_groups.clear();
        let mut start = 0;
        while start < dyn_list.len() {
            let m = dyn_list[start].0;
            let mut end = start;
            while end < dyn_list.len() && dyn_list[end].0 == m {
                end += 1;
            }
            self.dynamic_groups.push((m, start as u32, (end - start) as u32));
            start = end;
        }
        let data: Vec<Instance> = dyn_list.into_iter().map(|(_, i)| i).collect();
        if !data.is_empty() {
            self.queue.write_buffer(&self.dynamic, 0, bytemuck::cast_slice(&data));
        }
    }

    fn draw_dynamic(&self, pass: &mut wgpu::RenderPass) {
        for &(m, first, count) in &self.dynamic_groups {
            let model = &self.models[m];
            pass.set_vertex_buffer(0, model.vb.slice(..));
            pass.set_vertex_buffer(1, self.dynamic.slice(..));
            pass.set_index_buffer(model.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..model.count, 0, first..first + count);
        }
    }

    fn draw_people(&self, pass: &mut wgpu::RenderPass, shadow: bool) {
        let Some(pg) = &self.people else { return };
        if pg.drawn == 0 {
            return;
        }
        pass.set_pipeline(if shadow { &pg.shadow_pipe } else { &pg.pipe });
        pass.set_bind_group(2, &pg.bg, &[]);
        pass.set_vertex_buffer(0, pg.vb.slice(..));
        pass.set_vertex_buffer(1, pg.instances.slice(..));
        pass.set_index_buffer(pg.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..pg.count, 0, 0..pg.drawn);
    }

    fn upload_model(&mut self, m: &ModelMesh) {
        let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&m.name),
            contents: bytemuck::cast_slice(&m.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&m.name),
            contents: bytemuck::cast_slice(&m.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.models.push(Model { vb, ib, count: m.indices.len() as u32, height: m.height, instances: None, groups: Vec::new() });
    }

    /// (Re)build instance buffers with leaf colours for a season.
    fn update_instances(&mut self, season: usize) {
        if self.tint_season == Some(season) {
            return;
        }
        self.tint_season = Some(season);
        let autumn = [linear("#D9822B"), linear("#B9472A"), linear("#E3B23C")];
        let frost = linear("#A7B39C");
        let chunks_x = self.map_size.div_ceil(CHUNK);
        for (mi, model) in self.models.iter_mut().enumerate() {
            let mine: Vec<&Placed> = self.placed.iter().filter(|p| p.model == mi).collect();
            if mine.is_empty() {
                continue;
            }
            let data: Vec<Instance> = mine
                .iter()
                .map(|p| {
                    let tint = match (p.seasonal, season) {
                        (true, 2) => Some(autumn[p.hash as usize % 3]),
                        (true, 3) => Some(frost),
                        _ => None,
                    };
                    Instance::new(p.transform, tint)
                })
                .collect();
            match &model.instances {
                Some(buf) => self.queue.write_buffer(buf, 0, bytemuck::cast_slice(&data)),
                None => {
                    model.instances = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("instances"),
                        contents: bytemuck::cast_slice(&data),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    }))
                }
            }
            model.groups.clear();
            let mut start = 0;
            while start < mine.len() {
                let c = mine[start].chunk;
                let mut end = start;
                while end < mine.len() && mine[end].chunk == c {
                    end += 1;
                }
                let (cx, cz) = ((c % chunks_x) * CHUNK, (c / chunks_x) * CHUNK);
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for p in &mine[start..end] {
                    let y = p.transform.w_axis.y;
                    lo = lo.min(y);
                    hi = hi.max(y);
                }
                model.groups.push((
                    [cx as f32 - 1.0, lo, cz as f32 - 1.0],
                    [(cx + CHUNK) as f32 + 1.0, hi + model.height * 1.2 + 0.5, (cz + CHUNK) as f32 + 1.0],
                    start as u32,
                    (end - start) as u32,
                ));
                start = end;
            }
        }
    }

    fn ensure_low(&mut self, size: (u32, u32)) {
        if self.low.as_ref().is_some_and(|l| l.size == size) {
            return;
        }
        let tex = |label: &str, format: wgpu::TextureFormat| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let colour = tex("scene colour", SCENE_FORMAT);
        let depth = tex("scene depth", DEPTH_FORMAT);
        let post_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: &self.post_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.post_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&colour) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&depth) },
            ],
        });
        self.low = Some(LowRes { size, colour, depth, post_bg });
    }

    /// Where a world point lands in the window last frame (for picking people).
    /// How tall a node kind's model is (metres), for picking it under the cursor.
    pub fn kind_height(&self, kind: u16) -> f32 {
        self.kind_models.get(kind as usize).and_then(|l| l.first()).map_or(0.5, |&m| self.models[m].height)
    }

    pub fn to_screen(&self, p: Vec3, win: (u32, u32)) -> Option<Vec2> {
        let v = self.last_view?;
        let c = v.view_proj * p.extend(1.0);
        if c.w <= 0.0 {
            return None;
        }
        Some(Vec2::new((c.x / c.w * 0.5 + 0.5) * win.0 as f32, (0.5 - c.y / c.w * 0.5) * win.1 as f32))
    }

    /// Draw a frame into `target` (`size` pixels, in the renderer's target format).
    pub fn render(&mut self, target: &wgpu::TextureView, size: (u32, u32), frame: &Frame) {
        let view = View::new(frame.camera, size.0, size.1);
        self.last_view = Some(view);
        self.ensure_low(view.low);
        self.update_instances(frame.light.season);
        self.prepare_dynamic(frame);
        let light = frame.light;

        // Sun matrix: a box around the view, snapped to shadow texels so shadows hold still.
        let l = light.sun_dir.normalize();
        let reach = view.half.x.max(view.half.y) * 1.5 + 20.0;
        let lf = -l;
        let up = if l.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let lr = lf.cross(up).normalize();
        let lu = lr.cross(lf);
        let texel = 2.0 * reach / SHADOW_SIZE as f32;
        let mut leye = frame.camera.focus + l * 350.0;
        let (a, b) = (leye.dot(lr), leye.dot(lu));
        leye += lr * ((a / texel).round() * texel - a) + lu * ((b / texel).round() * texel - b);
        let sun_vp = ortho_rh(-reach, reach, -reach, reach, 1.0, 800.0) * look_at_rh(leye, leye + lf, up);

        let g = &mut self.globals;
        g.view_proj = view.view_proj.to_cols_array_2d();
        g.sun_view_proj = sun_vp.to_cols_array_2d();
        g.sun_dir = [l.x, l.y, l.z, light.sun_strength];
        g.sun_col = [light.sun_col.x, light.sun_col.y, light.sun_col.z, 1.0];
        g.ambient = [light.ambient.x, light.ambient.y, light.ambient.z, 1.0];
        g.cam_fwd = [view.forward.x, view.forward.y, view.forward.z, self.map_size as f32];
        g.season = [light.season as f32, light.snow, frame.time, 0.00025];
        g.lens = [frame.traffic as u8 as f32, 0.0, 0.0, 0.0];
        g.fire = match frame.camp {
            Some(c) if c.lit => {
                let flicker = 0.92 + 0.08 * (frame.time * 7.3).sin() * (frame.time * 3.1).cos();
                [c.fire.x, c.fire.y, c.fire.z, (0.35 + 0.65 * light.night / 0.85) * flicker]
            }
            _ => [0.0, -100.0, 0.0, 0.0],
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&self.globals));
        let edge_m = (view.t * 4.0).max(0.3);
        let post = PostUniform {
            p: [view.px as f32, edge_m / (FAR - NEAR), light.night, 0.0],
            outline: v4(linear("#2A1E1A"), 0.85),
            night_tint: [0.50, 0.60, 1.05, 1.0],
        };
        self.queue.write_buffer(&self.post_buf, 0, bytemuck::bytes_of(&post));
        if let Some(out) = &frame.ui {
            self.ui.upload(&self.device, &self.queue, out, size);
        }

        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        // 1. Shadows
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_pipeline(&self.shadow_terrain_pipe);
            for c in &self.chunks {
                if visible(&sun_vp, c.min, c.max) {
                    pass.set_vertex_buffer(0, c.vb.slice(..));
                    pass.set_index_buffer(c.ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..c.count, 0, 0..1);
                }
            }
            pass.set_pipeline(&self.shadow_model_pipe);
            Self::draw_models(&mut pass, &self.models, &sun_vp);
            self.draw_dynamic(&mut pass);
            self.draw_people(&mut pass, true);
        }
        // 2. The scene, at low resolution
        let low = self.low.as_ref().unwrap();
        {
            let void = linear("#1E1A1F");
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &low.colour,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: void[0] as f64, g: void[1] as f64, b: void[2] as f64, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &low.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.scene_bg, &[]);
            pass.set_pipeline(&self.terrain_pipe);
            for c in &self.chunks {
                if visible(&view.view_proj, c.min, c.max) {
                    pass.set_vertex_buffer(0, c.vb.slice(..));
                    pass.set_index_buffer(c.ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..c.count, 0, 0..1);
                }
            }
            if let Some(w) = &self.water {
                pass.set_pipeline(&self.water_pipe);
                pass.set_vertex_buffer(0, w.vb.slice(..));
                pass.set_index_buffer(w.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..w.count, 0, 0..1);
            }
            pass.set_pipeline(&self.model_pipe);
            Self::draw_models(&mut pass, &self.models, &view.view_proj);
            self.draw_dynamic(&mut pass);
            self.draw_people(&mut pass, false);
        }
        // 3. Pixel pass into the target, then the UI on top
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.post_pipe);
            pass.set_bind_group(0, &low.post_bg, &[]);
            pass.draw(0..3, 0..1);
            if frame.ui.is_some() {
                self.ui.draw(&mut pass);
            }
        }
        self.queue.submit([enc.finish()]);
    }

    fn draw_models(pass: &mut wgpu::RenderPass, models: &[Model], vp: &Mat4) {
        for m in models {
            let Some(inst) = &m.instances else { continue };
            pass.set_vertex_buffer(0, m.vb.slice(..));
            pass.set_vertex_buffer(1, inst.slice(..));
            pass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
            for &(min, max, first, count) in &m.groups {
                if visible(vp, min, max) {
                    pass.draw_indexed(0..m.count, 0, first..first + count);
                }
            }
        }
    }
}

impl Globals {
    fn new(content: &Content) -> Globals {
        let mut biome = [[0.0; 4]; 16];
        for (i, b) in content.biomes.iter().enumerate().take(16) {
            let kind = match b.id.as_str() {
                "sea" | "lake" | "river" => 0.0,
                "beach" => 2.0,
                "mountain" => 3.0,
                "snow" => 4.0,
                "marsh" => 5.0,
                _ => 1.0,
            };
            biome[i] = v4(linear(&b.colour), kind);
        }
        let pal = ["#5C9439", "#86913D", "#BDC9DB", "#D6E0ED", "#8A5A35", "#5C3A22", "#9A9590", "#6E6A68"].map(|h| v4(linear(h), 1.0));
        Globals {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            sun_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            sun_dir: [0.0, 1.0, 0.0, 1.0],
            sun_col: [1.0; 4],
            ambient: [0.4, 0.4, 0.5, 1.0],
            cam_fwd: [0.0, -1.0, 0.0, 0.0],
            season: [0.0; 4],
            biome,
            pal,
            fire: [0.0, -100.0, 0.0, 0.0],
            lens: [0.0; 4],
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The UI layer

struct UiGpu {
    pipe: wgpu::RenderPipeline,
    bg: wgpu::BindGroup,
    win: wgpu::Buffer,
    atlas: wgpu::Texture,
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    vcap: usize,
    icap: usize,
    count: u32,
}

impl UiGpu {
    fn new(d: &wgpu::Device, _q: &wgpu::Queue, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat) -> UiGpu {
        let atlas = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui atlas"),
            size: wgpu::Extent3d { width: 1024, height: 1024, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let win = d.create_buffer(&wgpu::BufferDescriptor { label: Some("ui window"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let smp = d.create_sampler(&wgpu::SamplerDescriptor { label: Some("ui atlas"), ..Default::default() });
        let bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let bg = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: win.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&smp) },
            ],
        });
        let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ui"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let pipe = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_ui"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ui::Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2, 4 => Float32x4],
                })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_ui"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mk = |label: &str, size: usize, usage: wgpu::BufferUsages| {
            d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size as u64, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
        };
        let (vcap, icap) = (16_384, 24_576);
        UiGpu {
            pipe,
            bg,
            win,
            atlas,
            vb: mk("ui vertices", vcap * std::mem::size_of::<ui::Vertex>(), wgpu::BufferUsages::VERTEX),
            ib: mk("ui indices", icap * 4, wgpu::BufferUsages::INDEX),
            vcap,
            icap,
            count: 0,
        }
    }

    fn upload(&mut self, d: &wgpu::Device, q: &wgpu::Queue, out: &ui::Output, win: (u32, u32)) {
        q.write_buffer(&self.win, 0, bytemuck::cast_slice(&[win.0 as f32, win.1 as f32, 0.0, 0.0]));
        if let Some(atlas) = out.atlas {
            q.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.atlas, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                atlas,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(out.atlas_size), rows_per_image: Some(out.atlas_size) },
                wgpu::Extent3d { width: out.atlas_size, height: out.atlas_size, depth_or_array_layers: 1 },
            );
        }
        if out.vertices.len() > self.vcap {
            self.vcap = out.vertices.len().next_power_of_two();
            self.vb = d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ui vertices"),
                size: (self.vcap * std::mem::size_of::<ui::Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if out.indices.len() > self.icap {
            self.icap = out.indices.len().next_power_of_two();
            self.ib = d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ui indices"),
                size: (self.icap * 4) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !out.vertices.is_empty() {
            q.write_buffer(&self.vb, 0, bytemuck::cast_slice(out.vertices));
            q.write_buffer(&self.ib, 0, bytemuck::cast_slice(out.indices));
        }
        self.count = out.indices.len() as u32;
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipe);
        pass.set_bind_group(0, &self.bg, &[]);
        pass.set_vertex_buffer(0, self.vb.slice(..));
        pass.set_index_buffer(self.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.count, 0, 0..1);
    }
}
