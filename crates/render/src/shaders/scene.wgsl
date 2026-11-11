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
    // xyz: the camp fire; w: its light (0 when out)
    fire: vec4<f32>,
    // x: 1 when the traffic lens is on
    lens: vec4<f32>,
    // A selected area outlined in gold on the ground (x0, z0, x1, z1); none when x0 > x1.
    area: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tiles: texture_2d<f32>;
@group(1) @binding(1) var shadow_map: texture_depth_2d;
@group(1) @binding(2) var shadow_cmp: sampler_comparison;
@group(1) @binding(3) var flow_tex: texture_2d<f32>;
@group(1) @binding(4) var flow_smp: sampler;
@group(1) @binding(5) var height_tex: texture_2d<f32>;
@group(2) @binding(0) var<storage, read> joints: array<mat4x4<f32>>;

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

/// Warm light from the camp fire: strongest close by, gone by about 9 m.
fn fire_light(n: vec3<f32>, world: vec3<f32>) -> f32 {
    let d = g.fire.xyz + vec3<f32>(0.0, 0.4, 0.0) - world;
    let k = g.fire.w * max(0.0, 1.0 - length(d) / 9.0);
    return k * k * (max(dot(n, normalize(d)), 0.0) * 0.6 + 0.4);
}

/// Lit colour, and in alpha how much the night grade should apply (fire light keeps its warmth).
fn shade(col: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec4<f32> {
    let d = max(dot(n, g.sun_dir.xyz), 0.0);
    let sh = select(1.0, shadow_at(world), d > 0.0);
    let f = fire_light(n, world);
    let c = col * (g.ambient.rgb + g.sun_col.rgb * g.sun_dir.w * d * sh) + col * vec3<f32>(1.0, 0.62, 0.3) * f * 1.8;
    return vec4<f32>(c, 1.0 - clamp(f * 1.5, 0.0, 0.85));
}

fn light(col: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    return shade(col, n, world).rgb;
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
    // Warp where the biomes are sampled, so their edges wander instead of following the grid.
    let warp = vec2<f32>(vnoise(t * 0.7 + vec2<f32>(11.0, 3.0)), vnoise(t * 0.7 + vec2<f32>(47.0, 29.0))) - 0.5;
    let p = t + vec2<f32>(1.0 / 32.0) - 0.5 + warp * 1.4;
    let base = vec2<i32>(floor(p));
    let f = clamp(fract(p) + (speck - 0.5) * 0.6, vec2<f32>(0.0), vec2<f32>(1.0));
    // Ground on a water tile that pokes above the water (banks, beaches) is wet sand or mud, not
    // the water's map colour.
    let wet = mix(g.biome[3].rgb, g.pal[4].rgb, 0.45) * 0.85;
    var cols: array<vec3<f32>, 4>;
    for (var k = 0; k < 4; k++) {
        let nb = biome_of(tile_data(base + vec2<i32>(k & 1, k >> 1)));
        cols[k] = select(nb.rgb, wet, nb.a < 0.5);
    }
    var c = mix(mix(cols[0], cols[1], f.x), mix(cols[2], cols[3], f.x), f.y);
    // Wear is sampled where people actually walked (no warp).
    let pw = t + vec2<f32>(1.0 / 32.0) - 0.5;
    let bw = vec2<i32>(floor(pw));
    let fw = clamp(fract(pw) + (speck - 0.5) * 0.6, vec2<f32>(0.0), vec2<f32>(1.0));
    var wear: array<f32, 4>;
    for (var k = 0; k < 4; k++) {
        wear[k] = tile_data(bw + vec2<i32>(k & 1, k >> 1)).g;
    }
    let worn = mix(mix(wear[0], wear[1], fw.x), mix(wear[2], wear[3], fw.x), fw.y);
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
        // Worn ground: footsteps (green channel, blended across tiles) wear grass to dirt.
        c = mix(c, mix(c, g.pal[4].rgb, 0.5), smoothstep(0.2, 0.45, worn));
        c = mix(c, g.pal[4].rgb * mix(0.9, 1.05, clump), smoothstep(0.45, 0.8, worn));
    } else if (kind == 2) {
        c = c * mix(0.93, 1.06, clump);
    } else if (kind == 3 || kind == 4) {
        c = c * mix(0.85, 1.1, clump);
        if (speck > 0.97) {
            c = g.pal[7].rgb;
        }
    }
    let field = (u32(data.a * 255.0 + 0.5) & 1u) == 1u;
    if (field && !steep) {
        // A field: dug soil in raised rows (three to a tile, along x), the furrows darker.
        let row = fract(t.y * 3.0);
        let ridge = smoothstep(0.12, 0.3, row) * (1.0 - smoothstep(0.62, 0.8, row));
        c = mix(g.pal[5].rgb, g.pal[4].rgb, ridge) * mix(0.92, 1.06, clump);
        if (speck > 0.97) {
            c = g.pal[5].rgb * 0.9;
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
    let c = terrain_colour(i);
    if (g.area.x <= g.area.z && i.side < 0.5) {
        // A selected field: a gold band along the inside of its edge, drawn on the ground so
        // it follows the slope (a ring at one height would cut into the hill).
        let p = i.world.xz;
        let d = min(min(p.x - g.area.x, g.area.z - p.x), min(p.y - g.area.y, g.area.w - p.y));
        if (d > -0.03 && d < 0.11) {
            return vec4<f32>(vec3<f32>(0.807, 0.552, 0.138) * 1.15, c.a);
        }
    }
    return c;
}

fn terrain_colour(i: TerrainOut) -> vec4<f32> {
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
    let lit = shade(c, n, i.world);
    if (g.lens.x > 2.5 && i.side < 0.5) {
        // Beauty lens: the world greyed, beautiful places glowing from dusky violet to warm
        // white, in steps. (The tile texture's moisture channel holds beauty while it's on.)
        let grey = vec3<f32>(dot(lit.rgb, vec3<f32>(0.3, 0.55, 0.15))) * 0.5;
        let q = i.world.xz - vec2<f32>(0.5);
        let b0 = vec2<i32>(floor(q));
        let fq = fract(q);
        let a0 = tile_data(b0).b;
        let a1 = tile_data(b0 + vec2<i32>(1, 0)).b;
        let a2 = tile_data(b0 + vec2<i32>(0, 1)).b;
        let a3 = tile_data(b0 + vec2<i32>(1, 1)).b;
        let beauty = mix(mix(a0, a1, fq.x), mix(a2, a3, fq.x), fq.y);
        let level = floor(clamp(beauty, 0.0, 1.0) * 4.99) / 4.0;
        let dusk = vec3<f32>(0.147, 0.102, 0.270);
        let violet = vec3<f32>(0.456, 0.330, 0.870);
        let warm = vec3<f32>(0.896, 0.823, 0.701);
        let col = select(mix(violet, warm, (level - 0.5) * 2.0), mix(dusk, violet, level * 2.0), level < 0.5);
        return vec4<f32>(mix(grey, col, smoothstep(0.0, 0.1, beauty) * 0.9), lit.a);
    }
    if (g.lens.x > 1.5 && i.side < 0.5) {
        // Soil lens: the world greyed; richness from pale sand (poor) to deep green (rich), and
        // wetter soil darker and bluer (as wet earth looks).
        let soil = soil_at(i.world.xz);
        let grey = vec3<f32>(dot(lit.rgb, vec3<f32>(0.3, 0.55, 0.15))) * 0.5;
        let poor = vec3<f32>(0.604, 0.479, 0.216);
        let mid = vec3<f32>(0.497, 0.540, 0.080);
        let rich = vec3<f32>(0.052, 0.250, 0.036);
        // Stepped (five bands), so it reads as pixel art and as a map.
        let f = floor(clamp((soil.x - 0.15) / 0.75, 0.0, 1.0) * 4.99) / 4.0;
        var col = select(mix(mid, rich, (f - 0.5) * 2.0), mix(poor, mid, f * 2.0), f < 0.5);
        let wet = floor(clamp(soil.y, 0.0, 1.0) * 3.99) / 3.0;
        col = mix(col, col * vec3<f32>(0.45, 0.6, 1.1), wet * 0.8);
        let water = tile_data(vec2<i32>(floor(i.world.xz))).r * 255.0 < 2.5;
        return vec4<f32>(select(mix(grey, col * 0.9, 0.85), grey, water), lit.a);
    }
    if (g.lens.x > 0.5 && i.side < 0.5) {
        // Traffic lens: the world greyed and dimmed, footsteps glowing from brown to amber.
        let worn = worn_at(i.world.xz);
        let grey = vec3<f32>(dot(lit.rgb, vec3<f32>(0.3, 0.55, 0.15))) * 0.55;
        let heat = mix(vec3<f32>(0.107, 0.042, 0.016), vec3<f32>(1.0, 0.434, 0.051), smoothstep(0.05, 0.7, worn));
        return vec4<f32>(mix(grey, heat, smoothstep(0.0, 0.12, worn) * 0.9), lit.a);
    }
    return lit;
}

/// Soil richness (x) and moisture (y) here, blended across tiles.
fn soil_at(p: vec2<f32>) -> vec2<f32> {
    let q = p - vec2<f32>(0.5);
    let b = vec2<i32>(floor(q));
    let f = fract(q);
    var v: array<vec2<f32>, 4>;
    for (var k = 0; k < 4; k++) {
        let d = tile_data(b + vec2<i32>(k & 1, k >> 1));
        v[k] = vec2<f32>(f32(u32(d.a * 255.0 + 0.5) >> 1u) / 127.0, d.b);
    }
    return mix(mix(v[0], v[1], f.x), mix(v[2], v[3], f.x), f.y);
}

/// Footsteps worn into the ground here, as a share of a full trail (blended across tiles).
fn worn_at(p: vec2<f32>) -> f32 {
    let q = p - vec2<f32>(0.5);
    let b = vec2<i32>(floor(q));
    let f = fract(q);
    let a = tile_data(b).g;
    let bb = tile_data(b + vec2<i32>(1, 0)).g;
    let c = tile_data(b + vec2<i32>(0, 1)).g;
    let d = tile_data(b + vec2<i32>(1, 1)).g;
    return mix(mix(a, bb, f.x), mix(c, d, f.x), f.y);
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

fn corner_h(x: i32, z: i32) -> f32 {
    let w = i32(g.cam_fwd.w);
    return textureLoad(height_tex, clamp(vec2<i32>(x, z), vec2<i32>(0), vec2<i32>(w)), 0).r;
}

fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    return 0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t2 * t);
}

/// The same smooth ground surface the terrain mesh follows (terrain::Ground).
fn ground_height(p: vec2<f32>) -> f32 {
    let b = vec2<i32>(floor(p));
    let f = p - floor(p);
    var rows: array<f32, 4>;
    for (var k = 0; k < 4; k++) {
        let z = b.y - 1 + k;
        rows[k] = catmull(corner_h(b.x - 1, z), corner_h(b.x, z), corner_h(b.x + 1, z), corner_h(b.x + 2, z), f.x);
    }
    return catmull(rows[0], rows[1], rows[2], rows[3], f.y);
}

/// Ripples: a plain two-octave pattern (carried along by the flow map below).
fn ripples(p: vec2<f32>) -> f32 {
    return vnoise(p * 2.2) * 0.65 + vnoise(p * 5.1 + vec2<f32>(7.1, 3.7)) * 0.35;
}

@fragment
fn fs_water(i: WaterOut) -> @location(0) vec4<f32> {
    let p = i.world.xz;
    // The flow field (terrain::flow_field): direction and speed.
    let f = textureSampleLevel(flow_tex, flow_smp, p / g.cam_fwd.w, 0.0);
    let raw = f.rg * 2.0 - 1.0;
    let dl = length(raw);
    let speed = f.b * smoothstep(0.05, 0.4, dl);
    // How much this water is current (rivers and their plumes) rather than calm (sea, lakes).
    let current = smoothstep(0.03, 0.3, speed);
    // Calm water drifts with a light breeze; the current takes over as it strengthens, so the
    // sea and a river mouth are one continuous pattern.
    let breeze = vec2<f32>(0.87, 0.49);
    let flow_dir = select(breeze, raw / max(dl, 1e-4), dl > 0.05);
    let dir = normalize(mix(breeze, flow_dir, current) + vec2<f32>(1e-4, 0.0));
    let t = g.season.z;
    // Real depth at this pixel (the water surface minus the smooth ground under it). Only needed
    // near the shore; open water uses the depth carried by the mesh.
    var depth = i.depth;
    if (i.depth < 1.5) {
        depth = max(i.world.y - ground_height(p), 0.0);
    }

    let shallow = g.biome[2].rgb;
    let deep = g.biome[0].rgb * 0.62;
    // Shallows lighten towards the sand; deep water darkens.
    let sandy = mix(shallow, g.biome[3].rgb, 0.18);
    var c = mix(sandy, shallow, smoothstep(0.0, 0.18, depth));
    c = mix(c, deep, smoothstep(0.5, 7.0, depth));
    // Fast water is a touch lighter (stirred up); still water darker and calmer.
    c = c * mix(0.95, 1.08, speed);
    let light = vec3<f32>(0.24, 0.52, 0.82);
    let foam = vec3<f32>(0.80, 0.88, 0.94);

    // A flow map: the pattern is carried along the current (or the breeze) by two copies half a
    // cycle apart, cross-faded so neither ever stretches too far or jumps.
    let cycle = t * 0.35;
    let ph0 = fract(cycle);
    let ph1 = fract(cycle + 0.5);
    let mixw = abs(1.0 - 2.0 * ph0);
    let travel = dir * mix(0.35, 0.5 + speed * 2.5, current);
    let n = mix(ripples(p - travel * ph0 + vec2<f32>(13.0, 7.0)), ripples(p - travel * ph1), mixw);
    // Stepped shades so it stays pixel art. Calm water: few, soft crests. Current: more, brighter
    // crests and troughs, white flecks where fast.
    let crest = mix(0.8, 0.72 - speed * 0.1, current);
    if (n > crest) {
        c = mix(c, light, mix(0.14, 0.22 + 0.25 * speed, current));
    } else if (n < 0.28 && current > 0.3) {
        c = c * mix(1.0, 0.94, current);
    }
    if (speed > 0.72 && n > crest + 0.08 && hash2(floor(p * 10.0) + floor(t * 4.0)) > 0.7) {
        c = mix(c, foam, 0.5);
    }
    // Glints on calm water, fading out where the current runs.
    if (hash2(floor(p * 8.0) + floor(t * 1.3) * vec2<f32>(7.0, 3.0)) > 0.993 + 0.006 * current) {
        c = mix(c, light, 0.8);
    }
    // Foam on the real waterline: a thin band where the water is only a few centimetres deep,
    // broken up a little, carried by the current and lapping in and out where it's still.
    let lap = 0.025 * sin(t * 1.1 + p.x * 0.4 + p.y * 0.3) * (1.0 - speed);
    let broken = vnoise(p * 5.0 - dir * speed * t * 1.2 + vec2<f32>(t * 0.2, 0.0));
    if (depth < 0.03 + lap * 0.6 + broken * 0.02) {
        c = mix(c, foam, 0.6);
    }
    let snow = g.season.y;
    let lit = shade(c, vec3<f32>(0.0, 1.0, 0.0), i.world);
    return vec4<f32>(lit.rgb * mix(1.0, 0.92, snow), lit.a);
}

// ---------------------------------------------------------------------------------------------
// Models (instanced)

struct ModelIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec3<f32>,
    // 0 = fixed colour, 1 = leaves, 2 = lighter top leaves, 3 = flame (glows; hidden when out),
    // 4 = window (glows when lit), 5 = roof (takes snow)
    @location(3) flag: f32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    // rgb: leaf colour for this instance; a: 1 = use it, 2 = hide flames, 3 = windows lit,
    // 4 = windows lit and use the leaf colour
    @location(8) tint: vec4<f32>,
};

struct ModelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec3<f32>,
    @location(3) flag: f32,
};

@vertex
fn vs_model(v: ModelIn) -> ModelOut {
    let m = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var o: ModelOut;
    let w = m * vec4<f32>(v.pos, 1.0);
    o.world = w.xyz;
    o.normal = normalize((m * vec4<f32>(v.normal, 0.0)).xyz);
    var c = v.colour;
    let use_tint = (v.tint.a > 0.5 && v.tint.a < 1.5) || v.tint.a > 3.5;
    if (v.flag > 0.5 && v.flag < 3.5 && use_tint) {
        // Leaves take the season's tint; glowing things (lens rings) take the instance colour.
        c = v.tint.rgb * select(1.0, 1.22, v.flag > 1.5 && v.flag < 2.5);
    }
    o.colour = c;
    o.flag = v.flag;
    if (v.flag > 3.5 && v.flag < 4.5 && v.tint.a > 2.5) {
        // A lit window: lamp light (#FFB040), glowing like the fire (flag 6).
        o.colour = vec3<f32>(1.0, 0.434, 0.051);
        o.flag = 6.0;
    }
    o.clip = g.view_proj * w;
    if (v.flag > 2.5 && v.flag < 3.5 && v.tint.a > 1.5 && v.tint.a < 2.5) {
        o.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return o;
}

@fragment
fn fs_model(i: ModelOut) -> @location(0) vec4<f32> {
    if (i.flag > 2.5 && i.flag < 3.5) {
        // Flames glow and keep their colour at night.
        let flicker = 0.85 + 0.15 * sin(g.season.z * 9.0 + i.world.y * 20.0);
        return vec4<f32>(i.colour * 1.6 * flicker, 0.0);
    }
    if (i.flag > 5.5) {
        // Lit windows glow steadily.
        return vec4<f32>(i.colour * 1.4, 0.0);
    }
    let n = normalize(i.normal);
    var c = i.colour;
    if (i.flag > 4.5 && i.flag < 5.5) {
        // Snow settles on roofs, brighter than on the ground so roofs stand out (#F7FAFF). A
        // light fall lies in patches (per art pixel), so a roof is never tinted pink.
        let px = floor(i.world * 16.0) / 16.0;
        let spot = vnoise(px.xz * 2.4 + vec2<f32>(px.y * 1.7, 0.0)) * 0.85 + hash2(px.xz * 16.0 + px.y) * 0.15;
        let cover = g.season.y * smoothstep(0.25, 0.55, n.y);
        if (spot < cover * 1.1 - 0.05) {
            c = vec3<f32>(0.930, 0.955, 1.0);
        }
    }
    return shade(c, n, i.world);
}

// ---------------------------------------------------------------------------------------------
// People (skinned: one joint per vertex)

struct PersonIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec3<f32>,
    // 0 fixed, 1 skin, 2 hair, 3 clothes, 4 eyes
    @location(3) material: f32,
    // 0 body, 1 short hair, 2 long hair, 3 beard, 4 skirt
    @location(4) part: f32,
    @location(5) joint: u32,
    @location(6) m0: vec4<f32>,
    @location(7) m1: vec4<f32>,
    @location(8) m2: vec4<f32>,
    @location(9) m3: vec4<f32>,
    @location(10) skin: vec4<f32>,
    @location(11) hair: vec4<f32>,
    @location(12) clothes: vec4<f32>,
    @location(13) eyes: vec4<f32>,
    // x: visible parts (bits), y: first joint, z: selected
    @location(14) extra: vec4<f32>,
};

fn person_world(v: PersonIn) -> vec4<f32> {
    let m = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let j = joints[u32(v.extra.y + 0.5) + v.joint];
    return m * (j * vec4<f32>(v.pos, 1.0));
}

fn part_visible(v: PersonIn) -> bool {
    let part = u32(v.part + 0.5);
    return part == 0u || ((u32(v.extra.x + 0.5) >> part) & 1u) == 1u;
}

@vertex
fn vs_person(v: PersonIn) -> ModelOut {
    let m = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let j = joints[u32(v.extra.y + 0.5) + v.joint];
    let w = person_world(v);
    var o: ModelOut;
    o.world = w.xyz;
    o.normal = normalize((m * (j * vec4<f32>(v.normal, 0.0))).xyz);
    let k = u32(v.material + 0.5);
    var c = v.colour;
    if (k == 1u) { c = v.skin.rgb; }
    if (k == 2u) { c = v.hair.rgb; }
    if (k == 3u) { c = v.clothes.rgb; }
    if (k == 4u) { c = v.eyes.rgb; }
    o.colour = c;
    o.flag = 0.0;
    o.clip = g.view_proj * w;
    if (!part_visible(v)) {
        o.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return o;
}

@vertex
fn vs_shadow_person(v: PersonIn) -> @builtin(position) vec4<f32> {
    if (!part_visible(v)) {
        return vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return g.sun_view_proj * person_world(v);
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
