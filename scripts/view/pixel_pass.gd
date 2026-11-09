class_name PixelPass
extends MeshInstance3D
## Full-screen pass that makes the 3D world read as pixel art (see pixel_pass.gdshader and
## docs/STYLE.md). Lives under the camera. Setting "pixels": "off" | "fine" | "chunky".

const SIZES := {"off": 1.0, "fine": 2.0, "chunky": 3.0}

## 0 by day .. 1 at night; DayNight sets it.
static var night := 0.0

var camera: Camera3D
var _mat := ShaderMaterial.new()


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
	var px: float = SIZES.get(String(GameSettings.get_value("pixels")), 2.0)
	_mat.set_shader_parameter("pixel_size", px)


func _process(_delta: float) -> void:
	if camera == null:
		return
	# Outline threshold follows the zoom: a few big pixels' worth of depth.
	var vp := get_viewport().get_visible_rect().size
	var px := float(_mat.get_shader_parameter("pixel_size"))
	var world_per_block := camera.size / maxf(vp.y, 1.0) * px
	_mat.set_shader_parameter("edge_depth", maxf(0.3, world_per_block * 4.0))
	_mat.set_shader_parameter("night", night)
