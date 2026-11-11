// The UI (crates/ui), drawn last at full resolution: rounded shapes from a signed distance, text
// from the glyph atlas.

struct Win {
    // x, y: window size in pixels
    size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> win: Win;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

struct In {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) colour: vec4<f32>,
    @location(3) local: vec2<f32>,
    // x: mode (0 fill, 1 text, 2 outline), y: radius, zw: half size
    @location(4) shape: vec4<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) colour: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) shape: vec4<f32>,
};

@vertex
fn vs_ui(v: In) -> Out {
    var o: Out;
    o.clip = vec4<f32>(v.pos.x / win.size.x * 2.0 - 1.0, 1.0 - v.pos.y / win.size.y * 2.0, 0.0, 1.0);
    o.uv = v.uv;
    o.colour = v.colour;
    o.local = v.local;
    o.shape = v.shape;
    return o;
}

fn round_rect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_ui(i: Out) -> @location(0) vec4<f32> {
    let mode = i.shape.x;
    var a = 1.0;
    if (mode > 0.5 && mode < 1.5) {
        a = textureSampleLevel(atlas, smp, i.uv, 0.0).r;
    } else {
        let d = round_rect(i.local, i.shape.zw, i.shape.y);
        if (mode > 1.5) {
            a = clamp(1.0 - abs(d + 0.5), 0.0, 1.0);
        } else {
            a = clamp(0.5 - d, 0.0, 1.0);
        }
    }
    return vec4<f32>(i.colour.rgb, i.colour.a * a);
}
