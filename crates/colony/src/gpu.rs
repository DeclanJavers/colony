//! GPU setup: a device for a window, or one with no window at all (screenshots, benchmarks), and
//! reading a rendered frame back into an image.

use render::{Frame, Renderer};
use std::path::Path;
use std::sync::Arc;
use winit::window::Window;

fn limits() -> wgpu::Limits {
    wgpu::Limits::default()
}

async fn device(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface,
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor { label: Some("colony"), required_limits: limits(), ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    Ok((adapter, device, queue))
}

pub fn headless() -> Result<(wgpu::Device, wgpu::Queue), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let (_, d, q) = pollster::block_on(device(&instance, None))?;
    Ok((d, q))
}

pub fn for_window(window: Arc<Window>, vsync: bool) -> Result<(wgpu::Surface<'static>, wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let size = window.inner_size();
    let surface = instance.create_surface(window).map_err(|e| e.to_string())?;
    let (adapter, device, queue) = pollster::block_on(device(&instance, Some(&surface)))?;
    let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).ok_or("surface not supported")?;
    let caps = surface.get_capabilities(&adapter);
    if let Some(f) = caps.formats.iter().find(|f| f.is_srgb()) {
        config.format = *f;
    }
    config.present_mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
    config.desired_maximum_frame_latency = 2;
    surface.configure(&device, &config);
    Ok((surface, device, queue, config))
}

pub fn offscreen_target(device: &wgpu::Device, size: (u32, u32), format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Render one frame and read it back as tightly packed sRGB RGBA.
pub fn render_to_image(r: &mut Renderer, size: (u32, u32), frame: &Frame) -> Vec<u8> {
    let tex = offscreen_target(r.device(), size, r.target_format());
    let view = tex.create_view(&Default::default());
    r.render(&view, size, frame);
    let row = size.0 * 4;
    let padded = row.div_ceil(256) * 256;
    let buf = r.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * size.1) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = r.device().create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(size.1) },
        },
        wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
    );
    r.queue().submit([enc.finish()]);
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    r.device().poll(wgpu::PollType::wait_indefinitely()).expect("gpu finished");
    let data = slice.get_mapped_range().expect("readback mapped");
    let mut out = Vec::with_capacity((row * size.1) as usize);
    for y in 0..size.1 {
        let start = (y * padded) as usize;
        out.extend_from_slice(&data[start..start + row as usize]);
    }
    out
}

pub fn write_png(path: &Path, size: (u32, u32), rgba: &[u8]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("can't write {}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size.0, size.1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("png header").write_image_data(rgba).expect("png data");
}
