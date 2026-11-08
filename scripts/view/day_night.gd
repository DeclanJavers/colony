class_name DayNight
extends Node3D
## Sun, sky colour and ambient light that follow the sim clock.

var sim: Sim
var sun := DirectionalLight3D.new()
var env := Environment.new()

const DAY_SKY := Color(0.55, 0.72, 0.88)
const DUSK_SKY := Color(0.85, 0.55, 0.38)
const NIGHT_SKY := Color(0.05, 0.07, 0.14)


func _ready() -> void:
	add_child(sun)
	sun.shadow_enabled = true
	sun.directional_shadow_max_distance = 120.0
	var we := WorldEnvironment.new()
	env.background_mode = Environment.BG_COLOR
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
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
		sun.light_energy = 0.2 + 0.8 * height
		sun.light_color = Color(1.0, 0.97, 0.9).lerp(Color(1.0, 0.65, 0.4), dusk)
	else:
		sun.light_energy = 0.24
		sun.light_color = Color(0.55, 0.65, 1.0)
		sun.rotation = Vector3(-deg_to_rad(55.0), deg_to_rad(30.0), 0)

	var sky := NIGHT_SKY
	if is_day:
		sky = DAY_SKY.lerp(DUSK_SKY, dusk)
	elif t > 18.0 and t < 19.5:
		sky = DUSK_SKY.lerp(NIGHT_SKY, (t - 18.0) / 1.5)
	elif t > 4.5 and t < 6.0:
		sky = NIGHT_SKY.lerp(DUSK_SKY, (t - 4.5) / 1.5)
	env.background_color = sky
	env.ambient_light_color = Color(0.85, 0.9, 1.0) if is_day else Color(0.35, 0.42, 0.7)
	env.ambient_light_energy = (0.3 + 0.2 * height) if is_day else 0.4
