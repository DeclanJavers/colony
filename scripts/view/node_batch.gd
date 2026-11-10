class_name NodeBatch
extends Node3D
## Draws all the resource nodes (trees, rocks, bushes, sticks, pebbles) in a few MultiMeshes: one
## per mesh part of each model variant, instead of a scene per node. Hundreds of trees become a
## dozen draw calls. Each node keeps its own spot, turn and size; it can be hidden (fog), shrunk
## (used up) or removed, and its leaves tinted (autumn, snow) through a per-instance colour.

## For each model variant: the parts to draw ({mesh, material, local: Transform3D, leaves: bool,
## colour: Color}) and one MultiMeshInstance3D per part.
var _kinds: Dictionary = {}       # "type#variant" -> {parts: Array, mms: Array[MultiMesh], free: Array[int], count: int}
var _where: Dictionary = {}       # node id -> {kind: String, index: int, base: Transform3D, scale: float, tint: Variant}
var _leaf_mats: Dictionary = {}   # original leaves material -> the per-instance-colour copy


## Add a node drawn as `scene` (one variant of its model) at `base` (position, turn, base size).
## `capacity`: how many of this kind to make room for up front.
func add(id: int, kind: String, scene: Node3D, base: Transform3D, capacity: int = 64) -> void:
	if not _kinds.has(kind):
		_kinds[kind] = _make_kind(scene, capacity)
	var k: Dictionary = _kinds[kind]
	var index: int
	if not (k["free"] as Array).is_empty():
		index = (k["free"] as Array).pop_back()
	else:
		index = int(k["count"])
		if index >= (k["mms"][0] as MultiMesh).instance_count:
			_grow(k)
		k["count"] = index + 1
	_where[id] = {"kind": kind, "index": index, "base": base, "scale": 1.0, "visible": true, "tint": null}
	_write(id)


func remove(id: int) -> void:
	var w: Dictionary = _where.get(id, {})
	if w.is_empty():
		return
	var k: Dictionary = _kinds[w["kind"]]
	for mm: MultiMesh in k["mms"]:
		mm.set_instance_transform(int(w["index"]), Transform3D(Basis().scaled(Vector3.ZERO), Vector3.ZERO))
	(k["free"] as Array).append(int(w["index"]))
	_where.erase(id)


func has(id: int) -> bool:
	return _where.has(id)


func ids() -> Array:
	return _where.keys()


## Shown or hidden (fog), and how big relative to its base size (shrinks as it's used up).
func set_state(id: int, visible_now: bool, scale_now: float) -> void:
	var w: Dictionary = _where.get(id, {})
	if w.is_empty() or (bool(w["visible"]) == visible_now and is_equal_approx(float(w["scale"]), scale_now)):
		return
	w["visible"] = visible_now
	w["scale"] = scale_now
	_write(id)


## Leaf colour for this node (null = its natural colour). leaves2 parts get it a little lighter.
func set_tint(id: int, tint: Variant) -> void:
	var w: Dictionary = _where.get(id, {})
	if w.is_empty() or w["tint"] == tint:
		return
	w["tint"] = tint
	_write(id)


func variant_of(id: int) -> int:
	var w: Dictionary = _where.get(id, {})
	return int(String(w.get("kind", "#1")).get_slice("#", 1)) if not w.is_empty() else 1


func _write(id: int) -> void:
	var w: Dictionary = _where[id]
	var k: Dictionary = _kinds[w["kind"]]
	var i := int(w["index"])
	var t: Transform3D = w["base"]
	if not bool(w["visible"]):
		t = Transform3D(Basis().scaled(Vector3.ZERO), t.origin)
	else:
		t = t.scaled_local(Vector3.ONE * float(w["scale"]))
	var parts: Array = k["parts"]
	for p in parts.size():
		var part: Dictionary = parts[p]
		var mm: MultiMesh = k["mms"][p]
		mm.set_instance_transform(i, t * (part["local"] as Transform3D))
		if bool(part["leaves"]):
			var c: Color = part["colour"]
			if w["tint"] != null:
				c = (w["tint"] as Color).lightened(0.18) if bool(part["light"]) else (w["tint"] as Color)
			mm.set_instance_color(i, c)


func _make_kind(scene: Node3D, capacity: int) -> Dictionary:
	var parts: Array = []
	var mms: Array[MultiMesh] = []
	for mi: MeshInstance3D in scene.find_children("*", "MeshInstance3D", true, false):
		if mi.mesh == null:
			continue
		var local := _local_to(scene, mi)
		var leaves := String(mi.name).begins_with("leaves")
		var mat := mi.get_active_material(0)
		var colour := Color.WHITE
		if leaves and mat is StandardMaterial3D:
			colour = (mat as StandardMaterial3D).albedo_color
			mat = _leaf_material(mat as StandardMaterial3D)
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.use_colors = leaves
		mm.mesh = mi.mesh
		mm.instance_count = capacity
		for i in capacity:
			mm.set_instance_transform(i, Transform3D(Basis().scaled(Vector3.ZERO), Vector3.ZERO))
		var mmi := MultiMeshInstance3D.new()
		mmi.multimesh = mm
		if mat != null:
			mmi.material_override = mat
		add_child(mmi)
		parts.append({"local": local, "leaves": leaves, "light": String(mi.name) == "leaves2", "colour": colour})
		mms.append(mm)
	scene.free()
	return {"parts": parts, "mms": mms, "free": [], "count": 0}


## The per-instance-colour version of a leaves material (same look, colour from the instance).
func _leaf_material(m: StandardMaterial3D) -> StandardMaterial3D:
	if not _leaf_mats.has(m):
		var c := m.duplicate() as StandardMaterial3D
		c.albedo_color = Color.WHITE
		c.vertex_color_use_as_albedo = true
		c.vertex_color_is_srgb = true
		_leaf_mats[m] = c
	return _leaf_mats[m]


## Room for more of a kind: copy the instances into bigger buffers.
func _grow(k: Dictionary) -> void:
	for p in (k["mms"] as Array).size():
		var mm: MultiMesh = k["mms"][p]
		var old := mm.instance_count
		var keep: Array[Transform3D] = []
		var cols: Array[Color] = []
		for i in old:
			keep.append(mm.get_instance_transform(i))
			if mm.use_colors:
				cols.append(mm.get_instance_color(i))
		mm.instance_count = old * 2
		for i in old * 2:
			mm.set_instance_transform(i, keep[i] if i < old else Transform3D(Basis().scaled(Vector3.ZERO), Vector3.ZERO))
			if mm.use_colors and i < old:
				mm.set_instance_color(i, cols[i])


func _local_to(root: Node3D, n: Node3D) -> Transform3D:
	var t := Transform3D.IDENTITY
	var cur: Node = n
	while cur != null and cur != root:
		if cur is Node3D:
			t = (cur as Node3D).transform * t
		cur = cur.get_parent()
	return t
