class_name WorldView
extends Node3D
## Draws a Sim in 3D. Read-only: it never changes the sim, it only mirrors it.
## Tile (x, y) sits at world (x, 0, y). Models come from assets/models/<type>.glb
## (made by tools/blender/make_models.py); a missing model falls back to a labelled box.

signal colonist_clicked(c: Colonist)
## Anything clicked: Colonist (alive, or dead = their grave), Building, ResNode, Vector2i tile,
## "colony", or null (deselect).
signal target_selected(target: Variant)
signal hovered(target: Variant, screen_pos: Vector2)

const MODEL_DIR := "res://assets/models/"
const GRASS := Color(0.36, 0.55, 0.26)
const WATER := Color(0.16, 0.38, 0.62)
const FORD := Color(0.36, 0.56, 0.70)

var sim: Sim
var camera: Camera3D
var selected: Colonist = null          # selected living colonist (for the ring + follow)
var selected_target: Variant = null    # whatever is inspected
var _hover_t := 0.0

var _models: Dictionary = {}       # type -> PackedScene (or null)
var _nodes: Dictionary = {}        # ResNode id -> Node3D
var _buildings: Dictionary = {}    # Building id -> {root, model, frame, crops, light}
var _colonists: Dictionary = {}    # Colonist id -> {root, model, label, ring}
var _fire_lights: Array[OmniLight3D] = []
var _select_ring: MeshInstance3D


func bind(p_sim: Sim, p_camera: Camera3D) -> void:
	sim = p_sim
	camera = p_camera
	_build_terrain()
	for n: ResNode in sim.nodes.values():
		_add_node(n)
	for b: Building in sim.buildings.values():
		_add_building(b)
	for c: Colonist in sim.colonists.values():
		_add_colonist(c)
	for g in sim.graveyard:
		_add_grave(g)
	sim.events.node_depleted.connect(_on_node_depleted)
	sim.events.building_placed.connect(_add_building)
	sim.events.building_completed.connect(_on_building_completed)
	sim.events.colonist_born.connect(_add_colonist)
	sim.events.colonist_died.connect(_on_colonist_died)
	_select_ring = MeshInstance3D.new()
	var torus := TorusMesh.new()
	torus.inner_radius = 0.38
	torus.outer_radius = 0.48
	_select_ring.mesh = torus
	_select_ring.material_override = _flat_mat(Color(1.0, 0.9, 0.2), true)
	_select_ring.visible = false
	add_child(_select_ring)


static func to_world(p: Vector2) -> Vector3:
	return Vector3(p.x, 0.0, p.y)


## ---- Terrain ---------------------------------------------------------------

func _build_terrain() -> void:
	var n := sim.map.size
	var img := Image.create_empty(n, n, false, Image.FORMAT_RGB8)
	for y in n:
		for x in n:
			var t := sim.map.tile(Vector2i(x, y))
			var col := GRASS
			if t == WorldMap.Tile.WATER:
				col = WATER
			elif t == WorldMap.Tile.FORD:
				col = FORD
			else:
				# Gentle checker + hash variation so tiles read as a grid without noise.
				var h := float(((x * 73856093) ^ (y * 19349663)) & 255) / 255.0
				col = col.darkened(0.04 * h + (0.03 if (x + y) % 2 == 0 else 0.0))
			img.set_pixel(x, y, col)
	var tex := ImageTexture.create_from_image(img)
	var ground := MeshInstance3D.new()
	var plane := PlaneMesh.new()
	plane.size = Vector2(n, n)
	ground.mesh = plane
	var m := StandardMaterial3D.new()
	m.albedo_texture = tex
	m.texture_filter = BaseMaterial3D.TEXTURE_FILTER_NEAREST
	m.roughness = 1.0
	ground.material_override = m
	ground.position = Vector3(n / 2.0 - 0.5, 0.0, n / 2.0 - 0.5)
	add_child(ground)
	# A dark slab under the map so the edge reads as an edge.
	var slab := MeshInstance3D.new()
	var bm := BoxMesh.new()
	bm.size = Vector3(n + 0.6, 1.0, n + 0.6)
	slab.mesh = bm
	slab.material_override = _flat_mat(Color(0.22, 0.16, 0.10))
	slab.position = ground.position + Vector3(0, -0.52, 0)
	add_child(slab)


## ---- Models ----------------------------------------------------------------

func _model(type: String) -> Node3D:
	if not _models.has(type):
		var path := MODEL_DIR + type + ".glb"
		_models[type] = load(path) if ResourceLoader.exists(path) else null
	var ps: PackedScene = _models[type]
	if ps != null:
		return ps.instantiate() as Node3D
	# Fallback: a box with the type name, so new data-only content still shows up.
	var root := Node3D.new()
	var mi := MeshInstance3D.new()
	var bm := BoxMesh.new()
	bm.size = Vector3(0.8, 0.8, 0.8)
	mi.mesh = bm
	mi.position.y = 0.4
	mi.material_override = _flat_mat(Color.from_hsv(float(type.hash() % 360) / 360.0, 0.5, 0.8))
	root.add_child(mi)
	var lbl := _make_label(type, 32)
	lbl.position.y = 1.1
	root.add_child(lbl)
	return root


func _flat_mat(c: Color, unshaded: bool = false) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.roughness = 0.9
	if unshaded:
		m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	return m


func _make_label(text: String, size: int = 40) -> Label3D:
	var l := Label3D.new()
	l.text = text
	l.font_size = size
	# Fixed screen size so labels stay readable at any zoom.
	l.pixel_size = 0.0011
	l.outline_size = 12
	l.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	l.no_depth_test = true
	l.fixed_size = true
	l.modulate = Color.WHITE
	l.outline_modulate = Color(0, 0, 0, 0.85)
	return l


func _hash01(i: int) -> float:
	return float((i * 2654435761) & 0xFFFF) / 65535.0


## ---- Resource nodes ---------------------------------------------------------

func _add_node(n: ResNode) -> void:
	var m := _model(n.type)
	m.position = to_world(Vector2(n.cell))
	m.rotation.y = _hash01(n.id) * TAU
	var s := 0.85 + 0.3 * _hash01(n.id * 7)
	m.scale = Vector3(s, s, s)
	add_child(m)
	_nodes[n.id] = m


func _on_node_depleted(n: ResNode) -> void:
	var m: Node3D = _nodes.get(n.id)
	if m != null:
		m.queue_free()
		_nodes.erase(n.id)


## ---- Buildings --------------------------------------------------------------

func _add_building(b: Building) -> void:
	if _buildings.has(b.id):
		return
	var root := Node3D.new()
	root.position = to_world(b.center())
	add_child(root)
	var model := _model(b.type)
	root.add_child(model)
	var entry := {"root": root, "model": model, "frame": null, "crops": null, "light": null}
	if b.provides("farm") != null:
		var crops := _model("farm_crops")
		crops.visible = false
		root.add_child(crops)
		entry["crops"] = crops
	if b.provides("rest_spot") != null:
		var light := OmniLight3D.new()
		light.light_color = Color(1.0, 0.6, 0.25)
		light.omni_range = 6.0
		light.position.y = 0.6
		light.light_energy = 0.0
		root.add_child(light)
		entry["light"] = light
		_fire_lights.append(light)
	if not b.complete:
		var frame := _model("site_frame")
		frame.scale = Vector3(b.size.x, 1.0 + 0.3 * (b.size.x - 1), b.size.y)
		root.add_child(frame)
		entry["frame"] = frame
	_buildings[b.id] = entry
	_update_building(b)


func _on_building_completed(b: Building) -> void:
	if not _buildings.has(b.id):
		_add_building(b)
	var e: Dictionary = _buildings[b.id]
	if e["frame"] != null:
		(e["frame"] as Node3D).queue_free()
		e["frame"] = null
	_update_building(b)


func _update_building(b: Building) -> void:
	var e: Dictionary = _buildings.get(b.id, {})
	if e.is_empty():
		return
	var model: Node3D = e["model"]
	# Sites rise out of the ground as work progresses.
	model.scale = Vector3.ONE if b.complete else Vector3(1.0, maxf(0.04, b.progress()), 1.0)
	var crops: Node3D = e["crops"]
	if crops != null:
		crops.visible = b.complete and b.farm_stage != "empty"
		var g := 1.0 if b.farm_stage == "ripe" else 0.15 + 0.85 * b.farm_growth
		crops.scale = Vector3(1.0, g, 1.0)


## ---- Colonists --------------------------------------------------------------

func colonist_color(c: Colonist) -> Color:
	return Color.from_hsv(fmod(c.id * 0.618034, 1.0), 0.65, 0.95)


func _add_colonist(c: Colonist) -> void:
	if _colonists.has(c.id):
		return
	var root := Node3D.new()
	root.position = to_world(c.pos)
	add_child(root)
	var model: Node3D
	if ColonistAvatar.available():
		var avatar := ColonistAvatar.new()
		root.add_child(avatar)
		avatar.setup(c, colonist_color(c))
		model = avatar
	else:   # old capsule placeholder
		model = _model("colonist")
		root.add_child(model)
		var body := model.find_child("body", true, false) as MeshInstance3D
		if body != null:
			body.set_surface_override_material(0, _flat_mat(colonist_color(c)))
	var label := _make_label(c.name, 36)
	label.position.y = 1.45
	root.add_child(label)
	_colonists[c.id] = {"root": root, "model": model, "label": label}


func _on_colonist_died(c: Colonist, _cause: String) -> void:
	var e: Dictionary = _colonists.get(c.id, {})
	if not e.is_empty():
		(e["root"] as Node3D).queue_free()
		_colonists.erase(c.id)
	if selected == c:
		select_target(c)   # keep inspecting: the page becomes their grave
	_add_grave(c)


func _add_grave(c: Colonist) -> void:
	var g := MeshInstance3D.new()
	var bm := BoxMesh.new()
	bm.size = Vector3(0.3, 0.45, 0.1)
	g.mesh = bm
	g.material_override = _flat_mat(Color(0.55, 0.55, 0.58))
	g.position = to_world(c.pos) + Vector3(0, 0.22, 0)
	add_child(g)


func short_action(c: Colonist) -> String:
	match c.state:
		"thinking":
			return "…thinking"
		"idle":
			return "idle"
		"acting":
			return c.action.describe() if c.action != null else ""
	return ""


func _process(delta: float) -> void:
	if sim == null:
		return
	var follow := clampf(delta * 12.0, 0.0, 1.0)
	for cid in _colonists:
		var c := sim.colonists.get(cid) as Colonist
		if c == null:
			continue
		var e: Dictionary = _colonists[cid]
		var root: Node3D = e["root"]
		var model: Node3D = e["model"]
		var target := to_world(c.pos)
		var move := target - root.position
		var before := root.position
		root.position = root.position.lerp(target, follow)
		var sleeping := c.action != null and c.action.phase == "sleeping"
		var avatar := model as ColonistAvatar
		if avatar != null:
			avatar.update_for(c, (root.position - before).length() / maxf(delta, 0.0001))
		# Sleeping at home = inside the house: hide the body, keep the label.
		model.visible = not (sleeping and c.action.get("where") == "at home")
		if move.length() > 0.01 and not sleeping:
			model.rotation.y = lerp_angle(model.rotation.y, atan2(move.x, move.z), follow)
		# Tip the model over to sleep unless it has a real lie-down animation.
		var tip := sleeping and not (avatar != null and avatar.lies_down())
		model.rotation.x = lerpf(model.rotation.x, -PI / 2.0 if tip else 0.0, follow)
		model.position.y = 0.12 if tip else 0.0
		var s := 1.0 if c.is_adult() else 0.65
		model.scale = Vector3(s, s, s)
		var lbl: Label3D = e["label"]
		# Zoomed far out, show names only to avoid clutter.
		lbl.text = c.name if camera.size > 34.0 else "%s\n%s" % [c.name, short_action(c)]
		lbl.position.y = 0.7 if sleeping else (1.45 * s + 0.1)
		lbl.modulate = Color(1, 0.55, 0.5) if _critical(c) else Color.WHITE
	for bid in _buildings:
		var b := sim.buildings.get(bid) as Building
		if b != null and (not b.complete or b.provides("farm") != null):
			_update_building(b)
	var night := 1.0 if sim.is_night() else 0.0
	for l in _fire_lights:
		l.light_energy = lerpf(l.light_energy, 0.4 + 2.2 * night, follow)
	if selected_target != null:
		_place_ring()


func _critical(c: Colonist) -> bool:
	return float(c.needs.get("thirst", 100)) < 15.0 or float(c.needs.get("hunger", 100)) < 15.0


## ---- Picking ----------------------------------------------------------------

func select(c: Colonist) -> void:
	select_target(c)


func select_target(t: Variant) -> void:
	selected_target = t
	selected = t as Colonist if (t is Colonist and (t as Colonist).alive()) else null
	_place_ring()
	colonist_clicked.emit(selected)
	target_selected.emit(t)


## World position of an inspectable target (for the camera), or null.
func target_position(t: Variant) -> Variant:
	if t is Colonist:
		var c := t as Colonist
		var e: Dictionary = _colonists.get(c.id, {})
		return (e["root"] as Node3D).position if not e.is_empty() else to_world(c.pos)
	if t is Building:
		return to_world((t as Building).center())
	if t is ResNode:
		return to_world(Vector2((t as ResNode).cell))
	if t is Vector2i:
		return to_world(Vector2(t))
	return null


func _place_ring() -> void:
	var t: Variant = selected_target
	var pos: Variant = target_position(t)
	_select_ring.visible = pos != null and not (t is String)
	if pos == null:
		return
	var s := 1.0
	if t is Building:
		s = maxf((t as Building).size.x, (t as Building).size.y) * 1.15
	elif t is Vector2i:
		s = 0.8
	_select_ring.scale = Vector3(s, 1, s)
	_select_ring.position = (pos as Vector3) + Vector3(0, 0.03, 0)


func pick_colonist(screen_pos: Vector2, radius_px: float = 28.0) -> Colonist:
	var best: Colonist = null
	var best_d := radius_px
	for cid in _colonists:
		var root: Node3D = _colonists[cid]["root"]
		if camera.is_position_behind(root.global_position):
			continue
		var p := camera.unproject_position(root.global_position + Vector3(0, 0.6, 0))
		var d := p.distance_to(screen_pos)
		if d < best_d:
			best_d = d
			best = sim.colonists.get(cid)
	return best


## Whatever is under the mouse: a colonist first, then grave, building, resource, else the ground tile.
func pick(screen_pos: Vector2) -> Variant:
	var c := pick_colonist(screen_pos)
	if c != null:
		return c
	var from := camera.project_ray_origin(screen_pos)
	var dir := camera.project_ray_normal(screen_pos)
	if absf(dir.y) < 0.0001:
		return null
	var hit := from + dir * (-from.y / dir.y)
	var cell := Vector2i(roundi(hit.x), roundi(hit.z))
	if not sim.map.in_bounds(cell):
		return null
	for g in sim.graveyard:
		if (g as Colonist).pos.distance_to(Vector2(hit.x, hit.z)) < 0.45:
			return g
	var b := sim.building_at_cell(cell)
	if b != null:
		return b
	var n := sim.node_at_cell(cell)
	if n != null:
		return n
	return cell


static func describe_short(t: Variant) -> String:
	if t is Colonist:
		var c := t as Colonist
		return c.name if c.alive() else "Grave of %s" % c.name
	if t is Building:
		return InspectText.label_of(t)
	if t is ResNode:
		var n := t as ResNode
		return "%s (%d %s left)" % [String(n.type).replace("_", " ").capitalize(), int(n.amount), n.def().get("resource", "")]
	return ""


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		select_target(pick(event.position))
	elif event is InputEventMouseMotion:
		hovered.emit(pick(event.position), event.position)
