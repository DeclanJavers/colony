// Hill sides: a turf lip over layered soil with a few stones, like the diorama edge (docs/STYLE.md).
// UV.x runs along the face, UV.y is the depth below the top of this face (metres).
shader_type spatial;
render_mode diffuse_lambert, specular_disabled, cull_disabled;

uniform vec3 lip : source_color = vec3(0.37, 0.61, 0.23);
uniform vec3 soil : source_color = vec3(0.54, 0.35, 0.21);
uniform vec3 soil_dark : source_color = vec3(0.36, 0.23, 0.13);
uniform vec3 stone : source_color = vec3(0.60, 0.58, 0.56);

float hash(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

void fragment() {
	vec2 t = floor(UV * 16.0) / 16.0;
	float y = UV.y;
	float lip_h = 0.1 + 0.07 * hash(vec2(floor(UV.x * 8.0), 3.0));
	vec3 col;
	if (y < lip_h) {
		col = mix(lip, lip * 0.82, step(lip_h - 0.04, y));
	} else {
		float layer = floor((y + 0.05 * sin(UV.x * 2.3)) * 5.0);
		col = mix(soil, soil_dark, mod(layer, 2.0) * 0.3 + clamp(y, 0.0, 1.0) * 0.35);
		float h = hash(floor(t * 3.0) + 7.0);
		if (h > 0.95) col = stone;
		else if (h < 0.05) col = soil_dark * 0.85;
	}
	ALBEDO = col;
	ROUGHNESS = 1.0;
}
