//! Colony's renderer (ARCHITECTURE.md section 11): terrain chunks, water and instanced models,
//! lit by the sun with a shadow map, drawn into a low-resolution target whose pixels belong to the
//! world (about 24 per tile), then scaled up by the pixel pass with outlines and the night grade.
//! The camera snaps to the art-pixel grid so panning never crawls. The renderer only reads the sim.

pub mod font;
pub mod models;
pub mod terrain;

use glam::{Mat4, Vec2, Vec3, Vec4};
use models::{Instance, ModelMesh, ModelVertex};
use sim::content::{Content, hex_colour};
use sim::world::{Map, Node};
use std::path::Path;
use terrain::{CHUNK, TerrainVertex, WaterVertex};
use wgpu::util::DeviceExt;

/// Art pixels per tile (metre) of ground.
pub const PER_TILE: f32 = 24.0;
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
    /// Light for an hour of the day (0..24) in a season. The sun comes from the back left of the
    /// view (docs/STYLE.md), swinging a little from morning to evening.
    pub fn at(hour: f32, season: usize, snow: f32) -> Self {
        let day = ((hour - 6.0) / 14.0).clamp(0.0, 1.0);
        let az = -0.7 + day * 1.4;
        let el = (day * std::f32::consts::PI).sin() * 1.1 + 0.2;
        let sun = Vec3::new(-az.cos(), el, -az.sin()).normalize();
        let night = ((1.0 - smoothstep(5.0, 7.0, hour)) + smoothstep(19.0, 21.0, hour)).min(1.0) * 0.85;
        let low = 1.0 - smoothstep(0.15, 0.5, sun.y);
        let warm = Vec3::new(1.0, 0.93, 0.82).lerp(Vec3::new(1.0, 0.72, 0.48), low);
        let moon = Vec3::new(-0.5, 0.8, 0.3).normalize();
        Lighting {
            sun_dir: sun.lerp(moon, night).normalize(),
            sun_strength: 0.82 * (1.0 - night) + 0.35 * night,
            sun_col: warm.lerp(Vec3::new(0.55, 0.62, 0.9), night),
            ambient: Vec3::new(0.30, 0.32, 0.38).lerp(Vec3::new(0.30, 0.33, 0.45), night),
            night,
            season,
            snow,
        }
    }
}

/// What to draw this frame.
pub struct Frame<'a> {
    pub camera: &'a Camera,
    pub light: &'a Lighting,
    /// Seconds, for animation (water ripples).
    pub time: f32,
    /// Lines for the debug readout (FPS...). None hides it.
    pub overlay: Option<&'a [String]>,
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
        let ppm = h.max(1) as f32 / cam.size.max(0.5);
        let px = ((ppm / PER_TILE).round() as u32).max(1);
        let t = px as f32 / ppm;
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

struct Overlay {
    tex: wgpu::Texture,
    bg: wgpu::BindGroup,
    size: (u32, u32),
    text: Vec<String>,
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
    overlay_pipe: wgpu::RenderPipeline,
    overlay_bgl: wgpu::BindGroupLayout,
    overlay_buf: wgpu::Buffer,
    chunks: Vec<Mesh>,
    water: Option<Mesh>,
    models: Vec<Model>,
    placed: Vec<Placed>,
    tint_season: Option<usize>,
    low: Option<LowRes>,
    overlay: Option<Overlay>,
    map_size: u32,
    pub last_view: Option<View>,
}

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
        let overlay_shader = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/overlay.wgsl").into()),
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
            ],
        });
        let post_bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[uniform_entry(0), tex_entry(1, nofilter), tex_entry(2, wgpu::TextureSampleType::Depth)],
        });
        let overlay_bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay"),
            entries: &[uniform_entry(0), tex_entry(1, nofilter)],
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
        let mut tiles = Vec::with_capacity((n * n * 4) as usize);
        for i in 0..(n * n) as usize {
            tiles.extend_from_slice(&[map.biome[i], (map.footfall[i] * 255.0).clamp(0.0, 255.0) as u8, (map.moisture[i] * 255.0) as u8, 0]);
        }
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
        let scene_bg = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene"),
            layout: &scene_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&tile_tex.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&shadow_sampler) },
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
        let shadow_bias = wgpu::DepthBiasState { constant: 2, slope_scale: 2.5, clamp: 0.0 };
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
        let overlay_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay"),
            bind_group_layouts: &[Some(&overlay_bgl)],
            immediate_size: 0,
        });
        let overlay_pipe = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay"),
            layout: Some(&overlay_layout),
            vertex: wgpu::VertexState { module: &overlay_shader, entry_point: Some("vs_rect"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_rect"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let overlay_buf = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay"),
            size: std::mem::size_of::<RectUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
            overlay_pipe,
            overlay_bgl,
            overlay_buf,
            chunks: Vec::new(),
            water: None,
            models: Vec::new(),
            placed: Vec::new(),
            tint_season: None,
            low: None,
            overlay: None,
            map_size: n,
            last_view: None,
        };
        r.build_terrain(map);
        r.build_models(map, nodes, content, assets)?;
        Ok(r)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
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
                let m = terrain::chunk_mesh(map, &corners, cx, cz);
                if let Some(mesh) = self.mesh("terrain chunk", &m) {
                    self.chunks.push(mesh);
                }
            }
        }
        self.water = self.mesh("water", &terrain::water_mesh(map, &corners));
    }

    fn build_models(&mut self, map: &Map, nodes: &[Node], content: &Content, assets: &Path) -> Result<(), String> {
        // Each node kind's models: <id>.glb, <id>_2.glb, ...
        let mut files: Vec<Vec<usize>> = Vec::new();
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
                let mesh: ModelMesh = models::load(&path)?;
                list.push(self.models.len());
                self.upload_model(&mesh);
            }
            files.push(list);
        }
        let chunks_x = map.size.div_ceil(CHUNK);
        for nd in nodes {
            let list = &files[nd.kind as usize];
            if list.is_empty() {
                continue;
            }
            let model = list[nd.variant as usize % list.len()];
            let i = map.idx(nd.x as u32, nd.y as u32);
            let h = sim::hash::hash2(0xC01, nd.x as i32, nd.y as i32);
            let jitter = ((h & 0xFF) as f32 / 255.0 - 0.5) * 0.8;
            let scale = 0.88 + ((h >> 8) & 0xFF) as f32 / 255.0 * 0.26;
            let pos = Vec3::new(nd.x as f32 + 0.5, map.elevation[i], nd.y as f32 + 0.5);
            let transform = Mat4::from_translation(pos)
                * Mat4::from_rotation_y(nd.turn as f32 * std::f32::consts::FRAC_PI_2 + jitter)
                * Mat4::from_scale(Vec3::splat(scale));
            self.placed.push(Placed {
                model,
                chunk: (nd.y as u32 / CHUNK) * chunks_x + nd.x as u32 / CHUNK,
                transform,
                seasonal: SEASONAL.contains(&content.nodes[nd.kind as usize].id.as_str()),
                hash: (h >> 16) as u32,
            });
        }
        self.placed.sort_by_key(|p| (p.model, p.chunk));
        Ok(())
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

    fn update_overlay(&mut self, lines: &[String]) {
        if self.overlay.as_ref().is_some_and(|o| o.text == lines) {
            return;
        }
        let img = font::render(lines, 2);
        let reuse = self.overlay.as_ref().is_some_and(|o| o.size == (img.w, img.h));
        if !reuse {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("overlay"),
                size: wgpu::Extent3d { width: img.w, height: img.h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("overlay"),
                layout: &self.overlay_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.overlay_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&tex.create_view(&Default::default())) },
                ],
            });
            self.overlay = Some(Overlay { tex, bg, size: (img.w, img.h), text: Vec::new() });
        }
        let o = self.overlay.as_mut().unwrap();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &o.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &img.px,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(img.w * 4), rows_per_image: Some(img.h) },
            wgpu::Extent3d { width: img.w, height: img.h, depth_or_array_layers: 1 },
        );
        o.text = lines.to_vec();
    }

    /// Draw a frame into `target` (`size` pixels, in the renderer's target format).
    pub fn render(&mut self, target: &wgpu::TextureView, size: (u32, u32), frame: &Frame) {
        let view = View::new(frame.camera, size.0, size.1);
        self.last_view = Some(view);
        self.ensure_low(view.low);
        self.update_instances(frame.light.season);
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
        g.season = [light.season as f32, light.snow, frame.time, 0.0006];
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&self.globals));
        let edge_m = (view.t * 4.0).max(0.3);
        let post = PostUniform {
            p: [view.px as f32, edge_m / (FAR - NEAR), light.night, 0.0],
            outline: v4(linear("#2A1E1A"), 0.85),
            night_tint: [0.50, 0.60, 1.05, 1.0],
        };
        self.queue.write_buffer(&self.post_buf, 0, bytemuck::bytes_of(&post));
        if let Some(lines) = frame.overlay {
            self.update_overlay(lines);
            if let Some(o) = &self.overlay {
                let r = RectUniform { r: [12.0, 12.0, o.size.0 as f32, o.size.1 as f32], win: [size.0 as f32, size.1 as f32, 0.0, 0.0] };
                self.queue.write_buffer(&self.overlay_buf, 0, bytemuck::bytes_of(&r));
            }
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
        }
        // 3. Pixel pass into the target, then the overlay
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
            if frame.overlay.is_some()
                && let Some(o) = &self.overlay
            {
                pass.set_pipeline(&self.overlay_pipe);
                pass.set_bind_group(0, &o.bg, &[]);
                pass.draw(0..4, 0..1);
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
        }
    }
}
