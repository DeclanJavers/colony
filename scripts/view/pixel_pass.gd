class_name PixelPass
extends MeshInstance3D
## Full-screen pass that makes the 3D world read as pixel art (see pixel_pass.gdshader and
## docs/STYLE.md). Lives under the camera. Setting "pixels": "off" | "fine" | "chunky".

## Art pixels per tile (metre) of ground. Pixels belong to the world, like classic pixel art: one
## art pixel is 1/16 of a tile at "fine" (the same as a ground texel, so models and ground match),
## so zooming in makes them bigger and zooming out smaller. 0 = no pixelation.
const PER_TILE := {"off": 0.0, "fine": 16.0, "chunky": 10.0}

## 0 by day .. 1 at night; DayNight sets it.
static var night := 0.0

var camera: Camera3D
var _mat := ShaderMaterial.new()
var _per_tile := 16.0
var _last_h := 0


func _init(p_camera: Camera3D) -> void:
	camera = p_camera
	var q := QuadMesh.new()
	q.size = Vector2(1, 1)
	mesh = q
	_mat.shader = preload("res://scripts/view/pixel_pass.gdshader")
	_mat.render_priority = -128     # first of the transparent pass: labels etc. draw on top, sharp
	material_override = _mat
	extra_cull_margin = 16384.0
	cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	position = Vector3(0, 0, -1)
	GameSettings.instance().changed.connect(func(k: String, _v: Variant) -> void:
		if k == "pixels": _apply())
	_apply()


func _apply() -> void:
	_per_tile = float(PER_TILE.get(String(GameSettings.get_value("pixels")), 16.0))
	_last_h = 0


## Big windows render the 3D world at a whole fraction of their size (1/2 at full screen on a Retina
## display): the pixel art throws that detail away anyway, and it's a quarter of the GPU work.
func _scale_3d() -> void:
	var real := float(get_window().size.y)
	get_viewport().scaling_3d_scale = 1.0 / ceilf(real / 1100.0) if _per_tile > 0.0 else 1.0


func _process(_delta: float) -> void:
	if camera == null or get_window() == null:
		return
	if get_window().size.y != _last_h:
		_last_h = get_window().size.y
		_scale_3d()
	# How many rendered pixels one tile spans at this zoom (orthographic: the view is camera.size
	# metres tall), and so how big one art pixel is.
	var rendered_h := float(get_window().size.y) * get_viewport().scaling_3d_scale
	var px_per_metre := rendered_h / maxf(camera.size, 0.01)
	var px := 1.0 if _per_tile <= 0.0 else maxf(1.0, px_per_metre / _per_tile)
	_mat.set_shader_parameter("pixel_size", px)
	# Outline threshold follows the zoom: a few art pixels' worth of depth.
	var world_per_block := px / px_per_metre
	_mat.set_shader_parameter("edge_depth", maxf(0.3, world_per_block * 4.0))
	_mat.set_shader_parameter("night", night)
