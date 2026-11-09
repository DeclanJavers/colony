// The pixel pass: draws the 3D world as chunky pixel art.
//   * the screen is sampled once per big pixel (pixel_size screen pixels square),
//   * dark outlines where a shape ends against something further away,
//   * softer inner lines where surfaces meet at an angle (roof against wall),
//   * light colour banding and a little extra warmth.
// It runs before transparent things draw, so labels, speech bubbles, smoke and rain stay sharp.
shader_type spatial;
render_mode unshaded, depth_draw_never, depth_test_disabled, cull_disabled, fog_disabled, shadows_disabled;

uniform sampler2D screen_tex : hint_screen_texture, filter_nearest;
uniform sampler2D depth_tex : hint_depth_texture, filter_nearest;
uniform sampler2D normal_tex : hint_normal_roughness_texture, filter_nearest;

uniform float pixel_size = 3.0;
uniform float edge_depth = 0.35;        // metres of depth jump that count as an outline
uniform vec4 outline_color : source_color = vec4(0.165, 0.118, 0.102, 1.0);
uniform float outline_strength = 0.85;
uniform float inner_strength = 0.28;
uniform float bands = 0.0;              // colour levels per channel (0 = off)
uniform float saturation = 1.04;
uniform float night = 0.0;              // 0 day .. 1 night: cool blue grade (warm lights keep their colour)
uniform vec3 night_tint : source_color = vec3(0.50, 0.60, 1.0);

void vertex() {
	POSITION = vec4(VERTEX.xy * 2.0, 1.0, 1.0);
}

float view_depth(vec2 uv, mat4 inv_proj) {
	float d = texture(depth_tex, uv).r;
	vec4 v = inv_proj * vec4(uv * 2.0 - 1.0, d, 1.0);
	return -v.z / v.w;
}

vec3 normal_at(vec2 uv) {
	return texture(normal_tex, uv).xyz * 2.0 - 1.0;
}

void fragment() {
	vec2 vp = VIEWPORT_SIZE;
	vec2 cell = floor(FRAGCOORD.xy / pixel_size);
	vec2 uv = (cell + 0.5) * pixel_size / vp;
	vec2 step_uv = vec2(pixel_size) / vp;

	vec3 col = texture(screen_tex, uv).rgb;
	float d = view_depth(uv, INV_PROJECTION_MATRIX);
	vec3 n = normal_at(uv);

	// Outer outline: a neighbour is clearly nearer, so this pixel is the background just past an edge.
	float outer = 0.0;
	float inner = 0.0;
	vec2 offs[4] = vec2[](vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0));
	for (int i = 0; i < 4; i++) {
		vec2 o = uv + offs[i] * step_uv;
		float dn = view_depth(o, INV_PROJECTION_MATRIX);
		if (d - dn > edge_depth) {
			outer = 1.0;
		}
		// Inner line: surfaces meeting at a sharp angle (roof against wall, wall against ground).
		// Drawn on the nearer of the two pixels so the line is one pixel thick.
		if (abs(d - dn) < edge_depth && d <= dn + 0.001) {
			vec3 nn = normal_at(o);
			if (dot(n, nn) < 0.7) {
				inner = 1.0;
			}
		}
	}

	// Gentle grade: a touch more saturation, then band the colour.
	float lum = dot(col, vec3(0.299, 0.587, 0.114));
	col = mix(vec3(lum), col, saturation);
	float keep_warm = smoothstep(0.75, 1.2, max(col.r, max(col.g, col.b)));   // windows, fire, lamps
	vec3 moon = vec3(lum) * night_tint * 1.15;
	col = mix(col, moon, night * 0.68 * (1.0 - keep_warm));
	if (bands > 0.0) {
		col = floor(col * bands + 0.5) / bands;
	}
	col = mix(col, col * 0.55, inner * inner_strength / 0.45);
	col = mix(col, outline_color.rgb, outer * outline_strength);
	ALBEDO = clamp(col, 0.0, 1.0);
}
