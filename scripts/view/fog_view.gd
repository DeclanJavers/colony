class_name FogView
extends Node3D
## Land nobody in the colony has seen yet stays under a soft dark veil, and resources there are
## hidden. It lifts as colonists explore (Sim.discovered, updated as they look around).

const VEIL := Color(0.118, 0.102, 0.122, 0.6)   # STYLE.md background, so unseen land fades into the void
const REFRESH_S := 0.5

var sim: Sim
var view: WorldView
var _img: Image
var _tex: ImageTexture
var _mesh := MeshInstance3D.new()
var _t := 0.0
var _last_count := -1


func setup(p_sim: Sim, p_view: WorldView) -> void:
	sim = p_sim
	view = p_view
	var n := sim.map.size
	_img = Image.create_empty(n, n, false, Image.FORMAT_RGBA8)
	_tex = ImageTexture.create_from_image(_img)
	var plane := PlaneMesh.new()
	plane.size = Vector2(n, n)
	_mesh.mesh = plane
	var m := StandardMaterial3D.new()
	m.albedo_texture = _tex
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
	m.render_priority = -1
	m.no_depth_test = true          # over hills too (nothing of the colony is under unseen land)
	_mesh.material_override = m
	_mesh.position = Vector3(n / 2.0 - 0.5, 1.2, n / 2.0 - 0.5)
	_mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(_mesh)
	_refresh()


func _process(delta: float) -> void:
	if sim == null:
		return
	_t += delta
	if _t >= REFRESH_S:
		_t = 0.0
		if sim.discovered_count != _last_count:
			_refresh()


func _refresh() -> void:
	_last_count = sim.discovered_count
	var n := sim.map.size
	for y in n:
		for x in n:
			var seen := sim.discovered[y * n + x] > 0
			_img.set_pixel(x, y, Color(0, 0, 0, 0) if seen else VEIL)
	_tex.update(_img)
	view.set_fog(func(cell: Vector2i) -> bool: return sim.is_discovered(cell))
