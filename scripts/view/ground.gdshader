// Ground of the colony, drawn as pixel-art terrain at 16 texels per tile (docs/STYLE.md).
// A small per-tile data texture says what each tile is:
//   r: water (1) / ford (0.5) / land (0)     g: footstep wear 0..1     b: near a building 0..1
// Grass wears to dirt where people walk; busy ground near buildings becomes cobblestone.
shader_type spatial;
render_mode diffuse_lambert, specular_disabled;

uniform sampler2D tiles : filter_nearest;
uniform sampler2D tiles_smooth : filter_linear;
uniform float map_size = 60.0;
uniform vec3 grass : source_color = vec3(0.37, 0.61, 0.23);
uniform vec3 grass_dark : source_color = vec3(0.29, 0.50, 0.19);
uniform vec3 grass_light : source_color = vec3(0.47, 0.71, 0.29);
uniform vec3 dirt : source_color = vec3(0.54, 0.35, 0.21);
uniform vec3 dirt_dark : source_color = vec3(0.36, 0.23, 0.13);
uniform vec3 cobble : source_color = vec3(0.60, 0.58, 0.56);
uniform vec3 cobble_dark : source_color = vec3(0.43, 0.42, 0.41);
uniform vec3 water : source_color = vec3(0.24, 0.50, 0.75);
uniform vec3 water_light : source_color = vec3(0.50, 0.72, 0.90);
uniform vec3 shore : source_color = vec3(0.80, 0.72, 0.50);
uniform float snow = 0.0;              // 0..1: winter covers grass, paths stay darker

varying vec3 wpos;

float hash(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	float a = hash(i), b = hash(i + vec2(1.0, 0.0)), c = hash(i + vec2(0.0, 1.0)), d = hash(i + vec2(1.0, 1.0));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	// Snap to the 16-per-tile texel grid so it reads as pixel art up close.
	vec2 w = wpos.xz + 0.5;                     // tile (x, y) covers [x-0.5, x+0.5)
	vec2 t = floor(w * 16.0) / 16.0 + 1.0 / 32.0;
	vec2 tuv = t / map_size;
	vec4 here = texture(tiles, tuv);
	vec4 soft = texture(tiles_smooth, tuv);

	float clump = vnoise(t * 0.9) * 0.6 + vnoise(t * 2.7) * 0.4;
	float speck = hash(floor(t * 16.0));

	// Grass: soft clumps of darker and lighter green, a few bright specks.
	vec3 col = mix(grass_dark, grass, smoothstep(0.30, 0.55, clump));
	col = mix(col, grass_light, smoothstep(0.62, 0.80, clump) * 0.8);
	if (speck > 0.975) col = mix(col, grass_light * 1.06, 0.6);
	else if (speck < 0.025) col = mix(col, grass_dark * 0.92, 0.5);

	// Little grass tufts (a "v" of two blades), scattered on a 6-texel grid. With the default
	// camera, screen-up-left is world -x and screen-up-right is world -z.
	vec2 T = floor(w * 16.0);
	vec2 G = floor(T / 6.0);
	vec2 B = G * 6.0 + floor(vec2(hash(G + 7.1), hash(G + 3.3)) * 4.0) + 1.0;
	vec2 d = T - B;
	bool blade = hash(G) > 0.45 && ((d.y == 0.0 && d.x <= 0.0 && d.x >= -2.0) || (d.x == 0.0 && d.y <= 0.0 && d.y >= -2.0));
	if (blade) col = mix(col, grass_dark * 0.82, 0.85);

	// Snow over the grass in winter (thin where clumps are dark, so it isn't flat white).
	vec3 snow_col = mix(vec3(0.74, 0.79, 0.86), vec3(0.84, 0.88, 0.93), smoothstep(0.3, 0.7, clump));
	col = mix(col, snow_col, snow);

	// Footpaths: wear (smoothly blended between tiles, ragged at the edge).
	float wear = soft.g + (vnoise(t * 3.1) - 0.5) * 0.16;
	float path = smoothstep(0.36, 0.42, wear);
	vec3 dcol = mix(dirt_dark, dirt, smoothstep(0.25, 0.75, vnoise(t * 4.0)));
	if (speck > 0.94) dcol = dirt_dark;
	dcol = mix(dcol, mix(dcol, vec3(0.78, 0.78, 0.80), 0.55), snow);   // trodden slush
	col = mix(col, dcol, path);

	// Cobbles: offset rows of rounded stones with dark gaps, on busy ground near buildings.
	float cob = smoothstep(0.55, 0.62, wear) * smoothstep(0.35, 0.6, soft.b + (vnoise(t * 2.0) - 0.5) * 0.3);
	if (cob > 0.0) {
		vec2 cs = t * 4.0;                      // 4 stones per tile
		cs.x += step(1.0, mod(floor(cs.y), 2.0)) * 0.5;
		vec2 cell = floor(cs);
		vec2 f = fract(cs);
		float edge = min(min(f.x, 1.0 - f.x), min(f.y, 1.0 - f.y));
		vec3 stone = mix(cobble_dark, cobble, 0.45 + 0.55 * hash(cell));
		stone = mix(stone, mix(stone, vec3(0.92, 0.94, 0.97), 0.6), snow * 0.7);
		vec3 gap = mix(cobble_dark * 0.75, snow_col * 0.92, snow);   // snow settles in the gaps
		vec3 ccol = edge < 0.13 ? gap : stone;
		col = mix(col, ccol, step(0.5, cob));
	}

	// Water with drifting ripple dashes, a sandy rim along the shore; fords show stepping stones.
	float wet = soft.r;
	if (here.r > 0.75) {
		vec3 wc = water;
		float rip = hash(vec2(floor(t.x * 4.0 + floor(TIME * 1.5)), floor(t.y * 16.0)));
		if (rip > 0.93) wc = water_light;
		wc = mix(wc, vec3(0.80, 0.88, 0.95), snow * 0.5);
		col = wc;
	} else if (here.r > 0.25) {
		vec3 wc = mix(water, water_light, 0.35);
		vec2 sp = fract(t * 2.0) - 0.5;
		if (length(sp) < 0.28 && hash(floor(t * 2.0)) > 0.35) wc = mix(cobble, cobble_dark, hash(floor(t * 2.0) + 3.0));
		col = wc;
	} else if (wet > 0.12) {
		col = mix(col, shore, smoothstep(0.12, 0.2, wet) * (1.0 - snow * 0.6));
	}

	ALBEDO = col;
	ROUGHNESS = 1.0;
}
