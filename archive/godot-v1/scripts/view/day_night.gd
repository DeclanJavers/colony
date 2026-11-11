class_name DayNight
extends Node3D
## Sun, sky colour and ambient light that follow the sim clock.

var sim: Sim
var sun := DirectionalLight3D.new()
var env := Environment.new()

const DAY_SKY := Color(0.55, 0.72, 0.88)
const DUSK_SKY := Color(0.85, 0.55, 0.38)
const NIGHT_SKY := Color(0.05, 0.07, 0.14)
## The void around the diorama (STYLE.md): dark, faintly tinted by the sky.
const BACKDROP := Color("#1E1A1F")


func _ready() -> void:
	add_child(sun)
	sun.shadow_enabled = true
	sun.shadow_opacity = 0.7
	sun.shadow_blur = 1.5
	sun.directional_shadow_max_distance = 120.0
	var we := WorldEnvironment.new()
	env.background_mode = Environment.BG_COLOR
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	# A soft glow around fire, lamps and lit windows (only bright, emissive things bloom).
	env.glow_enabled = true
	env.glow_intensity = 0.55
	env.glow_strength = 0.9
	env.glow_bloom = 0.0
	env.glow_hdr_threshold = 1.15
	env.glow_blend_mode = Environment.GLOW_BLEND_MODE_SOFTLIGHT
	we.environment = env
	add_child(we)


func _process(_delta: float) -> void:
	if sim == null:
		return
	var t := sim.time_of_day()                      # 0..24
	var day_phase := (t - 6.0) / 12.0               # 0 at 6:00, 1 at 18:00
	var height := sin(clampf(day_phase, 0.0, 1.0) * PI)  # 0 at sunrise/sunset, 1 at noon
	var is_day := day_phase > 0.0 and day_phase < 1.0
	var dusk := 1.0 - clampf(height * 3.0, 0.0, 1.0) if is_day else 0.0

	# Sun sweeps east -> west; at night a dim moon light keeps the colony readable.
	sun.rotation = Vector3(-deg_to_rad(15.0 + 60.0 * height), deg_to_rad(-60.0 + 120.0 * clampf(day_phase, 0, 1)), 0)
	if is_day:
		sun.light_energy = (0.2 + 0.8 * height) * (0.6 if sim.weather == "storm" else 0.8 if sim.weather in ["rain", "snow"] else 1.0)
		sun.light_color = Color(1.0, 0.95, 0.84).lerp(Color(1.0, 0.65, 0.4), dusk)
	else:
		sun.light_energy = 0.3
		sun.light_color = Color(0.62, 0.68, 0.92)
		sun.rotation = Vector3(-deg_to_rad(55.0), deg_to_rad(30.0), 0)

	var sky := NIGHT_SKY
	if is_day:
		sky = DAY_SKY.lerp(DUSK_SKY, dusk)
	elif t > 18.0 and t < 19.5:
		sky = DUSK_SKY.lerp(NIGHT_SKY, (t - 18.0) / 1.5)
	elif t > 4.5 and t < 6.0:
		sky = NIGHT_SKY.lerp(DUSK_SKY, (t - 4.5) / 1.5)
	# Weather and season: greyer skies in rain and storms, paler in winter.
	var w := sim.weather
	if w == "rain" or w == "snow":
		sky = sky.lerp(Color(0.55, 0.58, 0.62) if is_day else sky, 0.45)
	elif w == "storm":
		sky = sky.lerp(Color(0.35, 0.37, 0.42) if is_day else sky.darkened(0.2), 0.6)
	if sim.season() == "winter" and is_day:
		sky = sky.lerp(Color(0.78, 0.84, 0.90), 0.3)
	env.background_color = BACKDROP.lerp(sky, 0.18)
	# Night grade (applied in the pixel pass so lit windows stay warm): fades in through dusk.
	var nightness := 1.0
	if is_day:
		nightness = 0.0
	elif t > 18.0 and t < 19.5:
		nightness = (t - 18.0) / 1.5
	elif t > 4.5 and t < 6.0:
		nightness = 1.0 - (t - 4.5) / 1.5
	PixelPass.night = nightness
	# Cool, slightly purple fill so shadows lean blue rather than black (STYLE.md).
	env.ambient_light_color = (Color(0.80, 0.82, 1.0) if is_day else Color(0.35, 0.42, 0.7)).lerp(sky, 0.25)
	env.ambient_light_energy = (0.45 + 0.2 * height) if is_day else 0.45
