class_name Ground
extends Node3D
## The colony's ground and the diorama block it sits on (docs/STYLE.md).
## Paths are not painted: grass wears to dirt where colonists actually walk (Sim.footfall), and the
## busiest ground near buildings turns to cobblestone.

const SEASONS := {
	"spring": {"grass": Color("#5C9439"), "grass_dark": Color("#4E8233"), "grass_light": Color("#6CA443"), "snow": 0.0},
	"summer": {"grass": Color("#578E35"), "grass_dark": Color("#497B2E"), "grass_light": Color("#669D3F"), "snow": 0.0},
	"autumn": {"grass": Color("#86913D"), "grass_dark": Color("#727D35"), "grass_light": Color("#9E9A45"), "snow": 0.0},
	"winter": {"grass": Color("#6F8A5C"), "grass_dark": Color("#5A7449"), "grass_light": Color("#86A06E"), "snow": 0.85},
}
const DEPTH := 3.0
const H := 0.5                   # metres per hill level (WorldMap.height)
const WEAR_HALF := 60.0          # minutes walked on a tile for it to be half worn

var sim: Sim
var _n := 0
var _img: Image
var _tex: ImageTexture
var _mat := ShaderMaterial.new()
var _edge_mat := ShaderMaterial.new()
var _cliff_mat := ShaderMaterial.new()
var _near := PackedFloat32Array()
var _push_t := 0.0


func setup(p_sim: Sim) -> void:
	sim = p_sim
	_n = sim.map.size
	_near.resize(_n * _n)
	_img = Image.create_empty(_n, _n, false, Image.FORMAT_RGBA8)
	for y in _n:
		for x in _n:
			_write(x, y)
	_tex = ImageTexture.create_from_image(_img)

	var g := MeshInstance3D.new()
	g.mesh = _top_mesh()
	_mat.shader = preload("res://scripts/view/ground.gdshader")
	_mat.set_shader_parameter("tiles", _tex)
	_mat.set_shader_parameter("tiles_smooth", _tex)
	_mat.set_shader_parameter("map_size", float(_n))
	g.material_override = _mat
	add_child(g)
	var cliffs := MeshInstance3D.new()
	cliffs.mesh = _cliff_mesh()
	_cliff_mat.shader = preload("res://scripts/view/cliff.gdshader")
	cliffs.material_override = _cliff_mat
	add_child(cliffs)

	var slab := MeshInstance3D.new()
	var bm := BoxMesh.new()
	bm.size = Vector3(_n, DEPTH, _n)
	slab.mesh = bm
	_edge_mat.shader = preload("res://scripts/view/diorama_edge.gdshader")
	_edge_mat.set_shader_parameter("depth", DEPTH)
	_edge_mat.set_shader_parameter("tiles", _tex)
	_edge_mat.set_shader_parameter("map_size", float(_n))
	slab.material_override = _edge_mat
	slab.position = Vector3(_n / 2.0 - 0.5, -DEPTH / 2.0 - 0.001, _n / 2.0 - 0.5)
	add_child(slab)

	sim.events.building_completed.connect(func(_b: Building) -> void: _refresh_near())
	_refresh_near()
	set_season(sim.season() if sim.has_method("season") else "spring")


## Ground height at a point (tiles are flat steps).
func y_at(p: Vector2) -> float:
	return float(sim.map.height_at(Vector2i(roundi(p.x), roundi(p.y)))) * H


## The fog veil's mesh: the terrain's top faces with UVs across the whole map (0..1), so the veil
## texture (one pixel per tile) lines up with the tiles, plus the cliff faces.
func veil_mesh() -> ArrayMesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	st.set_normal(Vector3.UP)
	for y in _n:
		for x in _n:
			var h := float(sim.map.height_at(Vector2i(x, y))) * H
			var corners := [Vector3(x - 0.5, h, y - 0.5), Vector3(x + 0.5, h, y - 0.5), Vector3(x + 0.5, h, y + 0.5), Vector3(x - 0.5, h, y + 0.5)]
			for k in [0, 1, 2, 0, 2, 3]:
				var v: Vector3 = corners[k]
				st.set_uv(Vector2((v.x + 0.5) / _n, (v.z + 0.5) / _n))
				st.add_vertex(v)
	return st.commit()


## One flat quad per tile at its height.
func _top_mesh() -> ArrayMesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	st.set_normal(Vector3.UP)
	for y in _n:
		for x in _n:
			var h := float(sim.map.height_at(Vector2i(x, y))) * H
			var a := Vector3(x - 0.5, h, y - 0.5)
			var b := Vector3(x + 0.5, h, y - 0.5)
			var c := Vector3(x + 0.5, h, y + 0.5)
			var d := Vector3(x - 0.5, h, y + 0.5)
			for v in [a, b, c, a, c, d]:
				st.set_uv(Vector2(v.x, v.z))
				st.add_vertex(v)
	return st.commit()


## The earth sides where a tile stands higher than its neighbour (or the map's edge).
func _cliff_mesh() -> ArrayMesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var dirs := [Vector2i(1, 0), Vector2i(-1, 0), Vector2i(0, 1), Vector2i(0, -1)]
	var any := false
	for y in _n:
		for x in _n:
			var c := Vector2i(x, y)
			var h := sim.map.height_at(c)
			if h == 0:
				continue
			for dvec: Vector2i in dirs:
				var nb := c + dvec
				var hn := sim.map.height_at(nb) if sim.map.in_bounds(nb) else 0
				if hn >= h:
					continue
				any = true
				var top := float(h) * H
				var bot := float(hn) * H
				var n := Vector3(dvec.x, 0, dvec.y)
				var side := Vector3(-dvec.y, 0, dvec.x) * 0.5
				var mid := Vector3(x, 0, y) + n * 0.5
				var p0 := mid - side
				var p1 := mid + side
				var along0 := float(x + y) * 1.0
				var quad := [[p0 + Vector3(0, top, 0), Vector2(along0, 0)], [p1 + Vector3(0, top, 0), Vector2(along0 + 1, 0)],
					[p1 + Vector3(0, bot, 0), Vector2(along0 + 1, top - bot)], [p0 + Vector3(0, bot, 0), Vector2(along0, top - bot)]]
				st.set_normal(n)
				for k in [0, 2, 1, 0, 3, 2]:
					st.set_uv(quad[k][1])
					st.add_vertex(quad[k][0])
	if not any:
		return ArrayMesh.new()
	return st.commit()


func set_season(s: String) -> void:
	var p: Dictionary = SEASONS.get(s, SEASONS["spring"])
	for k in ["grass", "grass_dark", "grass_light"]:
		_mat.set_shader_parameter(k, p[k])
	_mat.set_shader_parameter("snow", p["snow"])
	_mat.set_shader_parameter("flowers", {"spring": 1.0, "summer": 0.85, "autumn": 0.3, "winter": 0.0}.get(s, 1.0))
	_edge_mat.set_shader_parameter("lip", Color(0.90, 0.93, 0.97) if s == "winter" else p["grass"])
	_cliff_mat.set_shader_parameter("lip", Color(0.90, 0.93, 0.97) if s == "winter" else p["grass"])


func _write(x: int, y: int) -> void:
	var i := y * _n + x
	var t := sim.map.tile(Vector2i(x, y))
	var r := 1.0 if t == WorldMap.Tile.WATER else (0.5 if t == WorldMap.Tile.FORD else 0.0)
	var steps := sim.footfall[i] if i < sim.footfall.size() else 0.0
	var wear := steps / (steps + WEAR_HALF)
	_img.set_pixel(x, y, Color(r, wear, _near[i], 1.0))


## How close each tile is to a finished building (cobbles only form near buildings).
func _refresh_near() -> void:
	_near.fill(0.0)
	for b: Building in sim.buildings.values():
		if not b.complete or b.provides("farm") != null:
			continue
		var reach := 3.5
		for y in range(maxi(0, b.cell.y - 4), mini(_n, b.cell.y + b.size.y + 4)):
			for x in range(maxi(0, b.cell.x - 4), mini(_n, b.cell.x + b.size.x + 4)):
				var dx := maxf(0.0, maxf(b.cell.x - x, x - (b.cell.x + b.size.x - 1)))
				var dy := maxf(0.0, maxf(b.cell.y - y, y - (b.cell.y + b.size.y - 1)))
				var v := clampf(1.0 - Vector2(dx, dy).length() / reach, 0.0, 1.0)
				var i := y * _n + x
				_near[i] = maxf(_near[i], v)
	_rewrite_all()


func _rewrite_all() -> void:
	for y in _n:
		for x in _n:
			_write(x, y)
	if _tex != null:
		_tex.update(_img)


func _process(delta: float) -> void:
	if sim == null:
		return
	_push_t += delta
	if _push_t >= 2.0:     # paths change slowly; refreshing every couple of seconds is plenty
		_push_t = 0.0
		_rewrite_all()
