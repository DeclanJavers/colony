// Sharp overlay drawn after the pixel pass at full resolution: one textured, alpha-blended
// rectangle in window pixels (for now, the FPS readout).

struct Rect {
    // x, y, w, h in window pixels
    r: vec4<f32>,
    // window width, height
    win: vec4<f32>,
};

@group(0) @binding(0) var<uniform> rect: Rect;
@group(0) @binding(1) var tex: texture_2d<f32>;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_rect(@builtin(vertex_index) i: u32) -> Out {
    let corner = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u));
    let px = rect.r.xy + corner * rect.r.zw;
    var o: Out;
    o.clip = vec4<f32>(px.x / rect.win.x * 2.0 - 1.0, 1.0 - px.y / rect.win.y * 2.0, 0.0, 1.0);
    o.uv = corner;
    return o;
}

@fragment
fn fs_rect(i: Out) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    return textureLoad(tex, vec2<i32>(min(i.uv * size, size - 1.0)), 0);
}
