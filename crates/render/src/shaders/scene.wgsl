// The 3D scene, drawn into the low-resolution pixel-art target (see post.wgsl).
// Group 0: globals (every pass). Group 1: tile data and the shadow map (colour passes only).

struct Globals {
    view_proj: mat4x4<f32>,
    sun_view_proj: mat4x4<f32>,
    // xyz: direction towards the sun (or moon); w: its strength
    sun_dir: vec4<f32>,
    sun_col: vec4<f32>,
    ambient: vec4<f32>,
    // xyz: camera forward; w: map size in tiles
    cam_fwd: vec4<f32>,
    // x: season 0..3, y: snow cover 0..1, z: seconds (animation), w: shadow bias
    season: vec4<f32>,
    // rgb: summer colour (linear); a: ground kind (0 water, 1 grass, 2 sand, 3 stone, 4 snow, 5 marsh)
    biome: array<vec4<f32>, 16>,
    // spring, autumn grass tints and winter snow (dark, light); soil; soil dark; stone; stone dark
    pal: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tiles: texture_2d<f32>;
@group(1) @binding(1) var shadow_map: texture_depth_2d;
@group(1) @binding(2) var shadow_cmp: sampler_comparison;

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn shadow_at(world: vec3<f32>) -> f32 {
    let p = g.sun_view_proj * vec4<f32>(world, 1.0);
    let uv = p.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 || p.z > 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_map, shadow_cmp, uv, p.z - g.season.w);
}

fn light(col: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let d = max(dot(n, g.sun_dir.xyz), 0.0);
    let sh = select(1.0, shadow_at(world), d > 0.0);
    return col * (g.ambient.rgb + g.sun_col.rgb * g.sun_dir.w * d * sh);
}

/// Flat (faceted) normal from screen-space derivatives, turned to face the camera.
fn facet_normal(world: vec3<f32>) -> vec3<f32> {
    var n = normalize(cross(dpdx(world), dpdy(world)));
    if (dot(n, g.cam_fwd.xyz) > 0.0) {
        n = -n;
    }
    return n;
}

// ---------------------------------------------------------------------------------------------
// Terrain

struct TerrainIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // 0 = ground, 1 = soil side of the diorama, 2 = water side
    @location(2) side: f32,
};

struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) side: f32,
    @location(2) normal: vec3<f32>,
};

@vertex
fn vs_terrain(v: TerrainIn) -> TerrainOut {
    var o: TerrainOut;
    o.world = v.pos;
    o.side = v.side;
    o.normal = v.normal;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    return o;
}

fn tile_data(t: vec2<i32>) -> vec4<f32> {
    let size = i32(g.cam_fwd.w);
    return textureLoad(tiles, clamp(t, vec2<i32>(0), vec2<i32>(size - 1)), 0);
}

fn biome_of(d: vec4<f32>) -> vec4<f32> {
    return g.biome[u32(d.r * 255.0 + 0.5)];
}

fn ground_colour(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let tile = vec2<i32>(floor(world.xz));
    let data = tile_data(tile);
    let b = biome_of(data);
    let kind = i32(b.a + 0.5);
    // 16 texels per tile, so the pattern holds still under the pixel pass
    let t = floor(world.xz * 16.0) / 16.0;
    let clump = vnoise(t * 0.55) * 0.6 + vnoise(t * 1.6) * 0.4;
    let speck = hash2(floor(world.xz * 16.0));
    // Blend biome colours across tile edges (dithered, so edges stay pixel art). Water
    // neighbours don't bleed onto the shore.
    let p = t + vec2<f32>(1.0 / 32.0) - 0.5;
    let base = vec2<i32>(floor(p));
    let f = clamp(fract(p) + (speck - 0.5) * 0.6, vec2<f32>(0.0), vec2<f32>(1.0));
    var cols: array<vec3<f32>, 4>;
    for (var k = 0; k < 4; k++) {
        let nb = biome_of(tile_data(base + vec2<i32>(k & 1, k >> 1)));
        cols[k] = select(nb.rgb, b.rgb, nb.a < 0.5);
    }
    var c = mix(mix(cols[0], cols[1], f.x), mix(cols[2], cols[3], f.x), f.y);
    let steep = n.y < 0.72;
    if (kind == 1 || kind == 5) {
        let season = g.season.x;
        if (season < 0.5) {
            c = mix(c, g.pal[0].rgb, 0.45);
        } else if (season > 1.5 && season < 2.5) {
            c = mix(c, g.pal[1].rgb, 0.65);
        }
        c = c * mix(0.86, 1.12, smoothstep(0.3, 0.75, clump));
        if (speck > 0.985) {
            c = c * 1.25;
        }
        // Worn ground: footfall (green channel) turns grass to dirt.
        c = mix(c, g.pal[4].rgb, smoothstep(0.15, 0.6, data.g));
    } else if (kind == 2) {
        c = c * mix(0.93, 1.06, clump);
    } else if (kind == 3 || kind == 4) {
        c = c * mix(0.85, 1.1, clump);
        if (speck > 0.97) {
            c = g.pal[7].rgb;
        }
    }
    if (steep && kind != 4) {
        // Cliffs and banks show earth (stone up in the mountains).
        let rock = select(g.pal[4].rgb, g.pal[6].rgb, kind == 3 || world.y > 14.0);
        c = mix(rock, rock * 0.8, step(0.5, clump));
    }
    let snow = g.season.y;
    if (snow > 0.0 && !steep && kind != 0) {
        let s = mix(g.pal[2].rgb, g.pal[3].rgb, smoothstep(0.3, 0.8, clump));
        c = mix(c, s, snow * smoothstep(0.2, 0.6, n.y + clump * 0.3));
    }
    return c;
}

@fragment
fn fs_terrain(i: TerrainOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    var c: vec3<f32>;
    if (i.side > 1.5) {
        c = g.biome[0].rgb * 0.7;
    } else if (i.side > 0.5) {
        // The diorama's sides: layered soil with a few stones.
        let band = floor(i.world.y * 2.0 + hash2(floor(i.world.xz * 0.5)) * 0.8);
        c = select(g.pal[4].rgb, g.pal[5].rgb, fract(band * 0.5) > 0.25);
        if (hash2(floor(vec2<f32>(i.world.x + i.world.z, i.world.y) * 6.0)) > 0.96) {
            c = g.pal[6].rgb;
        }
    } else {
        c = ground_colour(i.world, n);
    }
    return vec4<f32>(light(c, n, i.world), 1.0);
}

// ---------------------------------------------------------------------------------------------
// Water

struct WaterIn {
    @location(0) pos: vec3<f32>,
    // metres of water under the surface
    @location(1) depth: f32,
};

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) depth: f32,
};

@vertex
fn vs_water(v: WaterIn) -> WaterOut {
    var o: WaterOut;
    o.world = v.pos;
    o.depth = v.depth;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    return o;
}

@fragment
fn fs_water(i: WaterOut) -> @location(0) vec4<f32> {
    let shallow = g.biome[2].rgb;
    let deep = g.biome[0].rgb * 0.62;
    var c = mix(shallow, deep, smoothstep(0.0, 7.0, i.depth));
    let cell = floor(i.world.xz * 8.0);
    let ripple = hash2(cell + floor(g.season.z * 1.3) * vec2<f32>(7.0, 3.0));
    if (ripple > 0.993) {
        c = mix(c, vec3<f32>(0.21, 0.48, 0.79), 0.8);
    }
    let snow = g.season.y;
    return vec4<f32>(light(c, vec3<f32>(0.0, 1.0, 0.0), i.world) * mix(1.0, 0.92, snow), 1.0);
}

// ---------------------------------------------------------------------------------------------
// Models (instanced)

struct ModelIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec3<f32>,
    // 0 = fixed colour, 1 = leaves, 2 = lighter top leaves
    @location(3) flag: f32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    // rgb: leaf colour for this instance; a: 1 = use it
    @location(8) tint: vec4<f32>,
};

struct ModelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec3<f32>,
};

@vertex
fn vs_model(v: ModelIn) -> ModelOut {
    let m = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var o: ModelOut;
    let w = m * vec4<f32>(v.pos, 1.0);
    o.world = w.xyz;
    o.normal = normalize((m * vec4<f32>(v.normal, 0.0)).xyz);
    var c = v.colour;
    if (v.flag > 0.5 && v.tint.a > 0.5) {
        c = v.tint.rgb * select(1.0, 1.22, v.flag > 1.5);
    }
    o.colour = c;
    o.clip = g.view_proj * w;
    return o;
}

@fragment
fn fs_model(i: ModelOut) -> @location(0) vec4<f32> {
    return vec4<f32>(light(i.colour, normalize(i.normal), i.world), 1.0);
}

// ---------------------------------------------------------------------------------------------
// Shadow pass (depth only, from the sun)

@vertex
fn vs_shadow_terrain(v: TerrainIn) -> @builtin(position) vec4<f32> {
    return g.sun_view_proj * vec4<f32>(v.pos, 1.0);
}

@vertex
fn vs_shadow_model(v: ModelIn) -> @builtin(position) vec4<f32> {
    let m = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    return g.sun_view_proj * (m * vec4<f32>(v.pos, 1.0));
}
