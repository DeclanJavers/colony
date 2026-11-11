class_name PixelPass
extends MeshInstance3D
## Full-screen pass that makes the 3D world read as pixel art (see pixel_pass.gdshader and
## docs/STYLE.md). Lives under the camera. Setting "pixels": "off" | "fine" | "chunky".

## Art pixels per tile (metre) of ground. Pixels belong to the world, like classic pixel art, so
## zooming in makes them bigger and zooming out smaller. 0 = no pixelation. An art pixel is always a
## whole number of rendered pixels (even columns, no "breathing" while zooming), and the camera is
## snapped to the art-pixel grid so panning moves the picture a whole art pixel at a time instead of
## the world crawling under a fixed grid.
const PER_TILE := {"off": 0.0, "fine": 24.0, "chunky": 14.0}

## 0 by day .. 1 at night; DayNight sets it.
static var night := 0.0

var camera: Camera3D
var _mat := ShaderMaterial.new()
var _per_tile := 16.0
var _last_h := 0
static var snap := true
static var step_m := 0.0       # one art pixel in metres right now        # (test hook: --pan-test compares with it off)


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
	var px := 1.0 if _per_tile <= 0.0 else maxf(1.0, roundf(px_per_metre / _per_tile))
	_mat.set_shader_parameter("pixel_size", px)
	_snap_camera(px / px_per_metre if px > 1.0 and snap else 0.0)
	step_m = px / px_per_metre
	# Outline threshold follows the zoom: a few art pixels' worth of depth.
	var world_per_block := px / px_per_metre
	_mat.set_shader_parameter("edge_depth", maxf(0.3, world_per_block * 4.0))
	_mat.set_shader_parameter("night", night)


## Shift the camera (within its own view plane) so the world lands on the same art-pixel grid every
## frame: `step` is one art pixel in metres (0 = don't snap). The camera rig itself keeps its smooth
## position; only the lens moves by less than one art pixel.
func _snap_camera(step: float) -> void:
	var rig := camera.get_parent() as Node3D       # the pitch node: the camera's axes are its axes
	if rig == null:
		return
	var base := Vector3(0, 0, camera.position.z)
	if step <= 0.0:
		camera.position = base
		return
	var b := rig.global_transform.basis
	var o := rig.global_position
	var r := o.dot(b.x.normalized())
	var u := o.dot(b.y.normalized())
	camera.position = base + Vector3(snappedf(r, step) - r, snappedf(u, step) - u, 0.0)

