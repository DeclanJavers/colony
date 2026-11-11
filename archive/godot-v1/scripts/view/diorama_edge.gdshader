// The sides of the map block: a grass lip over layered soil with a few stones (docs/STYLE.md).
shader_type spatial;
render_mode diffuse_lambert, specular_disabled;

uniform vec3 lip : source_color = vec3(0.37, 0.61, 0.23);
uniform vec3 soil : source_color = vec3(0.54, 0.35, 0.21);
uniform vec3 soil_dark : source_color = vec3(0.36, 0.23, 0.13);
uniform vec3 stone : source_color = vec3(0.60, 0.58, 0.56);
uniform float depth = 3.0;
uniform sampler2D tiles : filter_nearest;
uniform float map_size = 60.0;
uniform vec3 water : source_color = vec3(0.24, 0.50, 0.75);

varying vec3 wpos;
varying vec3 wnorm;

float hash(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	wnorm = (MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz;
}

void fragment() {
	float along = abs(wnorm.x) > 0.5 ? wpos.z : wpos.x;
	vec2 t = floor(vec2(along, wpos.y) * 16.0) / 16.0;
	float y = -wpos.y;                                  // 0 at the top, grows downward
	// Which map tile this bit of edge belongs to (water reaching the edge shows as water).
	vec2 cell = abs(wnorm.x) > 0.5 ? vec2(wnorm.x > 0.0 ? map_size - 1.0 : 0.0, floor(wpos.z + 0.5))
		: vec2(floor(wpos.x + 0.5), wnorm.z > 0.0 ? map_size - 1.0 : 0.0);
	bool wet = texture(tiles, (cell + 0.5) / map_size).r > 0.25;
	// Grass lip with a ragged lower edge, like turf hanging over the soil.
	float lip_h = 0.2 + 0.09 * hash(vec2(floor(along * 8.0), 1.0));
	vec3 col;
	if (wet && y < 0.45) {
		col = mix(water, water * 0.7, step(0.25, y));
	} else if (y < lip_h) {
		col = mix(lip, lip * 0.82, step(lip_h - 0.06, y));
	} else {
		float layer = floor((y + 0.08 * sin(along * 1.7)) * 2.2);
		col = mix(soil, soil_dark, mod(layer, 2.0) * 0.35 + clamp(y / depth, 0.0, 1.0) * 0.55);
		float h = hash(floor(t * 3.0));
		if (h > 0.96) col = stone;
		else if (h < 0.05) col = soil_dark * 0.8;
	}
	if (wnorm.y > 0.5) col = lip;       // top face hidden under the ground anyway
	ALBEDO = col;
	ROUGHNESS = 1.0;
}
