class_name SeasonView
extends Node3D
## Makes seasons and weather visible, quietly: the ground and tree tops change colour with the
## season, and rain or snow falls around wherever the camera is looking.

const GROUND := {
	"spring": Color(0.36, 0.55, 0.26), "summer": Color(0.42, 0.56, 0.24),
	"autumn": Color(0.52, 0.50, 0.25), "winter": Color(0.86, 0.89, 0.92),
}
const WATER := {"spring": Color(0.16, 0.38, 0.62), "summer": Color(0.16, 0.40, 0.62),
	"autumn": Color(0.18, 0.36, 0.55), "winter": Color(0.55, 0.68, 0.80)}
const AUTUMN_LEAVES := [Color("#D9822B"), Color("#B9472A"), Color("#E3B23C")]
const WINTER_LEAVES := Color(0.86, 0.90, 0.92)

var sim: Sim
var view: WorldView
var camera: Camera3D
var _ground_mat: StandardMaterial3D
var _textures := {}
var _season := ""
var _rain := CPUParticles3D.new()
var _snow := CPUParticles3D.new()


func setup(p_sim: Sim, p_view: WorldView, p_camera: Camera3D, ground_mat: StandardMaterial3D) -> void:
	sim = p_sim
	view = p_view
	camera = p_camera
	_ground_mat = ground_mat
	_rain = _make_precip(Color(0.80, 0.87, 1.0, 0.42), Vector3(0.018, 0.26, 0.018), 650, 14.0, 0.9)
	_snow = _make_precip(Color(1, 1, 1, 0.9), Vector3(0.06, 0.06, 0.06), 380, 1.6, 5.0)
	add_child(_rain)
	add_child(_snow)
	sim.events.season_changed.connect(func(_s: String) -> void: _apply_season())
	sim.events.node_depleted.connect(func(_n: ResNode) -> void: pass)
	_apply_season()


func _make_precip(col: Color, size: Vector3, amount: int, speed: float, life: float) -> CPUParticles3D:
	var p := CPUParticles3D.new()
	var m := BoxMesh.new()
	m.size = size
	var mat := StandardMaterial3D.new()
	mat.albedo_color = col
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.material = mat
	p.mesh = m
	p.amount = amount
	p.lifetime = life
	p.preprocess = life
	p.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	p.emission_box_extents = Vector3(18, 0.5, 18)
	p.direction = Vector3(0.08, -1, 0.04)
	p.spread = 4.0 if speed > 5.0 else 25.0
	p.gravity = Vector3.ZERO
	p.initial_velocity_min = speed * 0.9
	p.initial_velocity_max = speed * 1.1
	p.local_coords = false
	p.emitting = false
	return p


## Texture for the ground in a season (same layout as WorldView's terrain, recoloured).
func _ground_texture(season: String) -> ImageTexture:
	if _textures.has(season):
		return _textures[season]
	var n := sim.map.size
	var img := Image.create_empty(n, n, false, Image.FORMAT_RGB8)
	var grass: Color = GROUND.get(season, GROUND["spring"])
	var water: Color = WATER.get(season, WATER["spring"])
	for y in n:
		for x in n:
			var t := sim.map.tile(Vector2i(x, y))
			var col := grass
			if t == WorldMap.Tile.WATER:
				col = water
			elif t == WorldMap.Tile.FORD:
				col = water.lightened(0.2)
			else:
				var h := float(((x * 73856093) ^ (y * 19349663)) & 255) / 255.0
				col = col.darkened(0.04 * h + (0.03 if (x + y) % 2 == 0 else 0.0))
			img.set_pixel(x, y, col)
	_textures[season] = ImageTexture.create_from_image(img)
	return _textures[season]


func _apply_season() -> void:
	var s := sim.season()
	if s == _season:
		return
	_season = s
	if view.ground != null:
		view.ground.set_season(s)
	elif _ground_mat != null:
		_ground_mat.albedo_texture = _ground_texture(s)
	view.tint_trees(s, AUTUMN_LEAVES, WINTER_LEAVES)
	view.snow_roofs(s == "winter")


func _process(_delta: float) -> void:
	if sim == null:
		return
	# Precipitation follows the point the camera looks at.
	var vp := get_viewport().get_visible_rect().size
	var from := camera.project_ray_origin(vp * 0.5)
	var dir := camera.project_ray_normal(vp * 0.5)
	if absf(dir.y) > 0.001:
		var focus := from + dir * (-from.y / dir.y)
		var ext := camera.size * 0.75
		for p in [_rain, _snow]:
			p.emission_box_extents = Vector3(ext, 0.5, ext)
			p.global_position = focus + Vector3(0, 7.5, 0)
	var w := sim.weather
	# No falling rain drawn (it made the view busy and felt like it never stopped). Rainy weather
	# still matters in the sim (water, work, colonists talk about it) and shows in the top bar.
	_rain.emitting = false
	_snow.emitting = w == "snow"
