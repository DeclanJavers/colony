// The pixel pass: the low-resolution scene scaled up with whole-pixel blocks, dark outlines where a
// shape ends against something further away, and the night grade.

struct Post {
    // x: rendered pixels per art pixel; y: depth jump that counts as an edge (in depth units);
    // z: night 0..1; w: unused
    p: vec4<f32>,
    outline: vec4<f32>,
    night_tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> post: Post;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var scene_depth: texture_depth_2d;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn depth_at(t: vec2<i32>) -> f32 {
    let size = vec2<i32>(textureDimensions(scene_depth));
    return textureLoad(scene_depth, clamp(t, vec2<i32>(0), size - 1), 0);
}

@fragment
fn fs_post(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let t = vec2<i32>(floor(frag.xy / post.p.x));
    var c = textureLoad(scene, t, 0).rgb;
    let d = depth_at(t);
    // Outline the near side of an edge: any neighbour much further away.
    let far = max(max(depth_at(t + vec2<i32>(1, 0)), depth_at(t - vec2<i32>(1, 0))),
                  max(depth_at(t + vec2<i32>(0, 1)), depth_at(t - vec2<i32>(0, 1))));
    if (d < 1.0 && far - d > post.p.y) {
        c = mix(c, post.outline.rgb, post.outline.a);
    }
    // Night: a cool blue grade, a little desaturated, never too dark to read.
    let night = post.p.z;
    let lum = dot(c, vec3<f32>(0.3, 0.59, 0.11));
    let graded = mix(c, vec3<f32>(lum), 0.45) * post.night_tint.rgb;
    c = mix(c, graded, night);
    return vec4<f32>(c, 1.0);
}
