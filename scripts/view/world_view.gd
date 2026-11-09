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
signal follow_requested(c: Colonist)   # double-click a colonist

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
var _variants: Dictionary = {}     # type -> [PackedScene] (type_2, type_3, ...)
var _nodes: Dictionary = {}        # ResNode id -> Node3D
var _buildings: Dictionary = {}    # Building id -> {root, model, frame, crops, light}
var _colonists: Dictionary = {}    # Colonist id -> {root, model, label, ring}
var _fire_lights: Array[OmniLight3D] = []
var _hover: Variant = null              # thing under the mouse
var ground_material: StandardMaterial3D = null   # (old flat ground; Ground replaces it)
var ground: Ground                       # SeasonView recolours it
var _intent := MeshInstance3D.new()     # dotted line from the selected colonist to their target
var _node_t := 0.0
var _nudge: Dictionary = {}             # colonist id -> small visual offset so nobody overlaps
const PERSONAL_SPACE := 0.45            # tiles
const NAME_ZOOM := 20.0                 # names show when zoomed in closer than this (camera size)
const RES_COLORS := {"wood": Color(0.80, 0.62, 0.40), "stone": Color(0.78, 0.80, 0.84),
	"food": Color(0.92, 0.45, 0.45), "water": Color(0.45, 0.70, 0.98)}
var _select_ring: MeshInstance3D


func bind(p_sim: Sim, p_camera: Camera3D) -> void:
	sim = p_sim
	camera = p_camera
	camera.add_child(PixelPass.new(camera))
	_build_terrain()
	var fog := FogView.new()
	add_child(fog)
	var wildlife := WildlifeView.new()
	add_child(wildlife)
	for n: ResNode in sim.nodes.values():
		_add_node(n)
	for b: Building in sim.buildings.values():
		_add_building(b)
	for c: Colonist in sim.colonists.values():
		_add_colonist(c)
	for g in sim.graveyard:
		_add_grave(g)
	fog.setup(sim, self)
	wildlife.setup(sim, self, camera)
	sim.events.node_depleted.connect(_on_node_depleted)
	sim.events.building_placed.connect(_add_building)
	sim.events.building_completed.connect(_on_building_completed)
	sim.events.colonist_born.connect(_add_colonist)
	sim.events.colonist_died.connect(_on_colonist_died)
	sim.events.delivered.connect(_on_delivered)
	sim.events.conversation_finished.connect(_on_conversation)
	sim.events.building_expansion_started.connect(_rebuild_building)
	sim.events.building_expanded.connect(_rebuild_building)
	sim.events.meeting_held.connect(_on_meeting)
	# Little moments: parents celebrate a birth, whoever finishes a building cheers.
	sim.events.colonist_born.connect(func(c: Colonist) -> void:
		for pid in c.parent_ids:
			_emote(sim.colonists.get(int(pid)), "celebrate"))
	sim.events.building_completed.connect(func(b: Building) -> void:
		for c: Colonist in sim.colonists.values():
			if c.action != null and InspectText.same(InspectText.action_target(c.action), b):
				_emote(c, "celebrate"))
	_intent.mesh = ImmediateMesh.new()
	var im := _flat_mat(Color(1, 1, 1, 0.75), true)
	im.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	im.no_depth_test = true
	_intent.material_override = im
	add_child(_intent)
	_select_ring = MeshInstance3D.new()
	var torus := TorusMesh.new()
	torus.inner_radius = 0.38
	torus.outer_radius = 0.48
	_select_ring.mesh = torus
	_select_ring.material_override = _flat_mat(Color("#E8C468"), true)   # STYLE.md: warm, not neon
	_select_ring.visible = false
	add_child(_select_ring)


static func to_world(p: Vector2) -> Vector3:
	return Vector3(p.x, 0.0, p.y)


## ---- Terrain ---------------------------------------------------------------

func _build_terrain() -> void:
	ground = Ground.new()
	add_child(ground)
	ground.setup(sim)


## ---- Models ----------------------------------------------------------------

## A model by type. Variants (<type>_2.glb, <type>_3.glb...) are picked by `key` (an object id),
## so the same tree or house always looks the same.
func _model(type: String, key: int = 0) -> Node3D:
	if not _models.has(type):
		var path := MODEL_DIR + type + ".glb"
		_models[type] = load(path) if ResourceLoader.exists(path) else null
		var variants: Array = []
		var i := 2
		while ResourceLoader.exists(MODEL_DIR + "%s_%d.glb" % [type, i]):
			variants.append(load(MODEL_DIR + "%s_%d.glb" % [type, i]))
			i += 1
		_variants[type] = variants
	var ps: PackedScene = _models[type]
	var vs: Array = _variants.get(type, [])
	if ps != null and not vs.is_empty():
		var k := int(_hash01(key * 97 + 13) * (vs.size() + 1)) % (vs.size() + 1)
		if k > 0:
			ps = vs[k - 1]
		var inst := ps.instantiate() as Node3D
		inst.set_meta("variant", k + 1)
		return inst
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


## A model by type (with its variants), for other views (wildlife).
func make_model(type: String, key: int = 0) -> Node3D:
	return _model(type, key)


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
	return float(hash(i) & 0xFFFF) / 65535.0


## ---- Resource nodes ---------------------------------------------------------

func _add_node(n: ResNode) -> void:
	var m := _model(n.type, n.id)
	m.position = to_world(Vector2(n.cell))
	m.rotation.y = _hash01(n.id) * TAU
	var s := 0.85 + 0.3 * _hash01(n.id * 7)
	m.scale = Vector3(s, s, s)
	m.set_meta("base_scale", s)
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
	# Buildings that grow (the meeting hall) have a model per level: <type>_lv2, <type>_lv3...
	var mtype := b.type if b.level <= 1 else "%s_lv%d" % [b.type, b.level]
	var model := _model(mtype, b.id)
	root.add_child(model)
	if b.expanding and b.prev_size != Vector2i.ZERO:
		# While it's being enlarged the old hall stands where it was, inside the new scaffolding.
		var prev_center := Vector2(b.prev_cell) + Vector2(b.prev_size - Vector2i.ONE) * 0.5
		model.position = to_world(prev_center) - to_world(b.center())
	# Square buildings face different ways (STYLE.md: variety), picked per building.
	if b.size.x == b.size.y and b.size.x > 1 and b.provides("farm") == null and b.type != "stockpile":
		model.rotation.y = float(int(_hash01(b.id * 31 + 7) * 4.0) % 4) * PI / 2.0
	var entry := {"root": root, "model": model, "frame": null, "crops": null, "light": null, "pct": null, "goods": null,
		"window": model.find_child("window", true, false), "lit": false, "smoke": null}
	if b.provides("farm") != null:
		var crops := _model("farm_crops")
		crops.visible = false
		root.add_child(crops)
		entry["crops"] = crops
	if b.provides("rest_spot") != null:
		var light := OmniLight3D.new()
		light.light_color = Color(1.0, 0.6, 0.25)
		light.omni_range = 5.0
		light.position.y = 0.6
		light.light_energy = 0.0
		root.add_child(light)
		entry["light"] = light
		_fire_lights.append(light)
	if b.is_site():
		var frame := _model("site_frame")
		frame.scale = Vector3(b.size.x, 1.0 + 0.3 * (b.size.x - 1), b.size.y)
		root.add_child(frame)
		entry["frame"] = frame
		var pct := _make_label("", 26)
		pct.modulate = Color(1.0, 0.88, 0.6)
		pct.font_size = 30
		pct.position.y = 1.2 + 0.25 * b.size.x
		root.add_child(pct)
		entry["pct"] = pct
	_buildings[b.id] = entry
	_snow_roof(model, _roofs_snowy)
	if b.complete:
		_add_smoke(b, entry)
		_add_lamp(b, entry)
	_update_building(b)


func _on_building_completed(b: Building) -> void:
	if not _buildings.has(b.id):
		_add_building(b)
	var e: Dictionary = _buildings[b.id]
	if e["frame"] != null:
		(e["frame"] as Node3D).queue_free()
		e["frame"] = null
	if e["pct"] != null:
		(e["pct"] as Node3D).queue_free()
		e["pct"] = null
	_add_smoke(b, e)
	_add_lamp(b, e)
	_update_building(b)


func _update_building(b: Building) -> void:
	var e: Dictionary = _buildings.get(b.id, {})
	if e.is_empty():
		return
	var model: Node3D = e["model"]
	# Sites rise out of the ground as work progresses (an enlargement keeps the old hall standing).
	model.scale = Vector3.ONE if b.complete else Vector3(1.0, maxf(0.04, b.progress()), 1.0)
	model.visible = b.complete or b.progress() > 0.03     # a fresh site is just its frame
	var crops: Node3D = e["crops"]
	if crops != null:
		crops.visible = b.complete and b.farm_stage != "empty"
		var g := 1.0 if b.farm_stage == "ripe" else 0.15 + 0.85 * b.farm_growth
		crops.scale = Vector3(1.0, g, 1.0)
		_tint(crops, Color(0.95, 0.78, 0.25) if b.farm_stage == "ripe" else null)
	var pct: Label3D = e.get("pct")
	if pct != null:
		pct.text = "%d%%" % int(b.progress() * 100)
		pct.visible = camera.size < NAME_ZOOM * 1.5


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
	var over := Overhead.new()
	root.add_child(over)
	var carry := MeshInstance3D.new()   # what they're carrying, held in front of the chest
	carry.visible = false
	carry.position = Vector3(0, 0.62, 0.2)
	model.add_child(carry)
	_colonists[c.id] = {"root": root, "model": model, "over": over, "carry": carry}


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
	_update_nudges()
	for cid in _colonists:
		var c := sim.colonists.get(cid) as Colonist
		if c == null:
			continue
		var e: Dictionary = _colonists[cid]
		var root: Node3D = e["root"]
		var model: Node3D = e["model"]
		var target: Vector3 = to_world(c.pos) + (_nudge.get(cid, Vector3.ZERO) as Vector3)
		var move := target - root.position
		var before := root.position
		root.position = root.position.lerp(target, follow)
		var sleeping := c.action != null and c.action.phase == "sleeping"
		var avatar := model as ColonistAvatar
		if avatar != null:
			avatar.update_for(c, (root.position - before).length() / maxf(delta, 0.0001))
		# Sleeping at home = inside the house, and sitting inside the hall at a meeting: hide the
		# body, keep the label.
		var in_hall := c.action != null and c.action.phase == "inside" and String(c.action.def.get("id", "")) == "attend_meeting"
		model.visible = not (sleeping and c.action.get("where") == "at home") and not in_hall
		var face: Variant = null
		if move.length() > 0.01 and not sleeping:
			face = move
		elif not sleeping and c.action != null and c.action.phase != "walking":
			# Working, eating, talking...: turn to face what they're working on.
			var tp: Variant = target_position(InspectText.action_target(c.action))
			if tp != null:
				var to: Vector3 = (tp as Vector3) - root.position
				to.y = 0.0
				if to.length() > 0.15:
					face = to
		if face != null:
			model.rotation.y = lerp_angle(model.rotation.y, atan2((face as Vector3).x, (face as Vector3).z), clampf(delta * 8.0, 0.0, 1.0))
		# Tip the model over to sleep unless it has a real lie-down animation.
		var tip := sleeping and not (avatar != null and avatar.lies_down())
		model.rotation.x = lerpf(model.rotation.x, -PI / 2.0 if tip else 0.0, follow)
		var lift := 0.0
		if sleeping:
			# Lying on a stockpile deck or farm plot: on top of it, not inside it.
			var under := sim.building_at_cell(Vector2i(roundi(c.pos.x), roundi(c.pos.y)))
			if under != null and under.type != "house":
				lift = 0.35 if under.type == "stockpile" else 0.12
		model.position.y = (0.12 if tip else 0.0) + lift
		var s := 1.0 if c.is_adult() else 0.65
		model.scale = Vector3(s, s, s)
		var over: Overhead = e["over"]
		var names_mode := String(GameSettings.get_value("names"))
		var show_name: bool = names_mode == "always" or (names_mode == "auto" and (camera.size < NAME_ZOOM or c == selected or InspectText.same(c, _hover)))
		over.visible = not in_hall or c == selected     # inside the hall: the hall's own label says who's there
		over.position.y = 0.75 if sleeping else (1.3 * s + 0.12)
		over.update_for(c, show_name, delta)
		_update_carry(c, e["carry"])
	for bid in _buildings:
		var b := sim.buildings.get(bid) as Building
		if b != null and (b.is_site() or b.provides("farm") != null):
			_update_building(b)
	var night := 1.0 if sim.is_night() else 0.0
	for l in _fire_lights:
		l.light_energy = lerpf(l.light_energy, 0.3 + 1.4 * night, follow)
	if selected_target != null:
		_place_ring()
	_update_intent_line()
	_update_meeting_label()
	_node_t += delta
	if _node_t > 0.5:
		_node_t = 0.0
		_update_nodes()
		_update_windows()
		_update_goods()


func _critical(c: Colonist) -> bool:
	return float(c.needs.get("thirst", 100)) < 15.0 or float(c.needs.get("hunger", 100)) < 15.0


## ---- Readability helpers ---------------------------------------------------

## Tint every mesh under n (null = back to the model's own colours).
func _tint(n: Node3D, col: Variant) -> void:
	for mi in n.find_children("*", "MeshInstance3D", true, false):
		(mi as MeshInstance3D).material_override = _flat_mat(col) if col != null else null


func _update_carry(c: Colonist, carry: MeshInstance3D) -> void:
	var a := c.action
	var res := ""
	if a != null and a.phase == "carrying":
		res = String(a.params.get("resource", "food"))
	carry.visible = res != ""
	if res == "" or carry.get_meta("res", "") == res:
		return
	carry.set_meta("res", res)
	match res:
		"wood":
			var m := CylinderMesh.new()
			m.top_radius = 0.045
			m.bottom_radius = 0.045
			m.height = 0.42
			m.radial_segments = 6
			carry.mesh = m
			carry.rotation = Vector3(0, 0, PI / 2)
		"stone":
			var m := BoxMesh.new()
			m.size = Vector3(0.16, 0.12, 0.14)
			carry.mesh = m
			carry.rotation = Vector3(0.3, 0.4, 0)
		"water":
			var m := CylinderMesh.new()
			m.top_radius = 0.08
			m.bottom_radius = 0.06
			m.height = 0.13
			m.radial_segments = 8
			carry.mesh = m
			carry.rotation = Vector3.ZERO
		_:
			var m := SphereMesh.new()   # a little bundle of berries / harvest
			m.radius = 0.08
			m.height = 0.14
			m.radial_segments = 8
			m.rings = 4
			carry.mesh = m
			carry.rotation = Vector3.ZERO
	carry.material_override = _flat_mat(RES_COLORS.get(res, Color.WHITE))
	carry.scale = Vector3.ONE * 1.5     # big enough to read at normal zoom


func _emote(c: Colonist, clip: String) -> void:
	if c == null:
		return
	var e: Dictionary = _colonists.get(c.id, {})
	var avatar := e.get("model") as ColonistAvatar if not e.is_empty() else null
	if avatar != null:
		avatar.emote(clip)


## Play a conversation as speech bubbles, one line at a time, when it's worth showing: zoomed in,
## or one of the speakers is selected.
func _on_conversation(conv: Dictionary) -> void:
	var lines: Array = conv.get("lines", [])
	var a: Colonist = conv["a"]
	var b: Colonist = conv["b"]
	if lines.is_empty() or not bool(GameSettings.get_value("bubbles")) or not (camera.size < NAME_ZOOM * 1.3 or selected == a or selected == b):
		return
	var by_name := {a.name: a, b.name: b}
	var delay := 0.0
	for line in lines:
		var who: Colonist = by_name.get(line["who"])
		if who == null:
			continue
		var secs := clampf(1.2 + String(line["text"]).length() * 0.045, 2.0, 5.0)
		get_tree().create_timer(delay).timeout.connect(func() -> void:
			var e: Dictionary = _colonists.get(who.id, {})
			if not e.is_empty():
				(e["over"] as Overhead).say(line["text"], secs))
		delay += secs


var _meeting_label: Label3D


## Over the hall during a meeting: how many are inside and outside.
func _update_meeting_label() -> void:
	var m := sim.meetings.current
	if m.is_empty():
		if _meeting_label != null:
			_meeting_label.visible = false
		return
	if _meeting_label == null:
		_meeting_label = _make_label("", 30)
		_meeting_label.modulate = Color("#F2DFA8")
		add_child(_meeting_label)
	var place: Building = m["place"]
	var inside := (m["inside"] as Array).size()
	var outside := (m["present"] as Array).size() - inside
	_meeting_label.text = "Meeting · %d inside%s" % [inside, (" · %d outside" % outside) if outside > 0 else ""]
	_meeting_label.position = to_world(place.center()) + Vector3(0, 3.2 + 0.4 * place.level, 0)
	_meeting_label.visible = true


## Rebuild a building's visuals (it changed shape: started or finished being enlarged).
func _rebuild_building(b: Building) -> void:
	var e: Dictionary = _buildings.get(b.id, {})
	if not e.is_empty():
		var light: Variant = e.get("light")
		if light != null:
			_fire_lights.erase(light)
		(e["root"] as Node3D).queue_free()
		_buildings.erase(b.id)
	_add_building(b)
	if _roofs_snowy:
		_snow_roof(_buildings[b.id]["model"], true)


## The meeting's discussion, as speech bubbles over the speakers (when it's worth showing).
func _on_meeting(plan: Dictionary) -> void:
	var lines: Array = plan.get("lines", [])
	if lines.is_empty() or not bool(GameSettings.get_value("bubbles")) or camera.size > NAME_ZOOM * 1.6:
		return
	var by_name := {}
	for c: Colonist in sim.colonists.values():
		by_name[c.name] = c
	var delay := 0.0
	for line in lines:
		var who: Colonist = by_name.get(line["who"])
		if who == null:
			continue
		var secs := clampf(1.2 + String(line["text"]).length() * 0.045, 2.0, 5.0)
		get_tree().create_timer(delay).timeout.connect(func() -> void:
			var e: Dictionary = _colonists.get(who.id, {})
			if not e.is_empty():
				(e["over"] as Overhead).say(line["text"], secs))
		delay += secs


## A soft "+6 wood" that rises and fades where goods reach storage.
func _on_delivered(c: Colonist, res: String, amount: float) -> void:
	var e: Dictionary = _colonists.get(c.id, {})
	if e.is_empty() or camera.size > NAME_ZOOM * 2.0:
		return
	var l := _make_label("+%d %s" % [roundi(amount), res], 28)
	l.modulate = RES_COLORS.get(res, Color.WHITE)
	l.position = (e["root"] as Node3D).position + Vector3(0, 1.7, 0)
	add_child(l)
	var tw := create_tween().set_parallel(true)
	tw.tween_property(l, "position:y", l.position.y + 0.8, 1.6).set_ease(Tween.EASE_OUT).set_trans(Tween.TRANS_CUBIC)
	tw.tween_property(l, "modulate:a", 0.0, 1.6).set_ease(Tween.EASE_IN)
	tw.chain().tween_callback(l.queue_free)


## Dotted line from the selected colonist to whatever they're heading for or working on.
func _update_intent_line() -> void:
	var im := _intent.mesh as ImmediateMesh
	im.clear_surfaces()
	if selected == null or not _colonists.has(selected.id):
		return
	var tgt: Variant = InspectText.action_target(selected.action)
	if tgt == null:
		return
	var to: Variant = target_position(tgt)
	if to == null:
		return
	var from: Vector3 = (_colonists[selected.id]["root"] as Node3D).position + Vector3(0, 0.06, 0)
	var end: Vector3 = (to as Vector3) + Vector3(0, 0.06, 0)
	var d := end - from
	var n := int(d.length() / 0.25)
	if n < 1:
		return
	im.surface_begin(Mesh.PRIMITIVE_LINES)
	for i in range(0, n, 2):
		im.surface_add_vertex(from + d * (float(i) / n))
		im.surface_add_vertex(from + d * (float(i + 1) / n))
	im.surface_end()


## Visual personal space: colonists standing closer than PERSONAL_SPACE get gently pushed apart
## (rendering only; the sim already gives everyone their own tile where it can).
func _update_nudges() -> void:
	var ids := _colonists.keys()
	var pos := {}
	for cid in ids:
		var c := sim.colonists.get(cid) as Colonist
		if c != null:
			pos[cid] = c.pos
	var out := {}
	for i in ids.size():
		var a = ids[i]
		if not pos.has(a):
			continue
		for j in range(i + 1, ids.size()):
			var b = ids[j]
			if not pos.has(b):
				continue
			var d: Vector2 = pos[a] - pos[b]
			var dist := d.length()
			if dist >= PERSONAL_SPACE:
				continue
			# Same spot: push apart along a stable direction from their ids.
			var dir := d / dist if dist > 0.001 else Vector2.from_angle(_hash01(int(a) * 31 + int(b)) * TAU)
			var push := (PERSONAL_SPACE - dist) * 0.5
			out[a] = out.get(a, Vector3.ZERO) + Vector3(dir.x, 0, dir.y) * push
			out[b] = out.get(b, Vector3.ZERO) - Vector3(dir.x, 0, dir.y) * push
	_nudge = out


## Chimney tops (from tools/blender/make_models.py). Houses only smoke while heated.
const CHIMNEYS := {"kitchen": Vector3(-0.45, 2.35, -0.35), "workshop": Vector3(0.50, 2.25, -0.42),
	"house": Vector3(0.42, 2.35, -0.30)}
## Lamp posts that come with some buildings: offset from the building's centre.
## (in the model's own frame, so they turn with it; placed clear of the props by the door)
const LAMPS := {"town_hall": [Vector3(-0.62, 0, 1.58), Vector3(0.62, 0, 1.58)], "kitchen": [Vector3(0.98, 0, 0.25)]}


## A thin wisp of smoke from chimneys: quiet signs of life.
func _add_smoke(b: Building, e: Dictionary) -> void:
	if not CHIMNEYS.has(b.type) or e.get("smoke") != null:
		return
	var p := CPUParticles3D.new()
	p.position = CHIMNEYS[b.type]
	p.amount = 7
	p.lifetime = 4.5
	p.preprocess = 4.0
	p.direction = Vector3(0.15, 1, 0)
	p.spread = 12.0
	p.gravity = Vector3(0.05, 0.12, 0)
	p.initial_velocity_min = 0.18
	p.initial_velocity_max = 0.3
	var m := SphereMesh.new()
	m.radius = 0.09
	m.height = 0.18
	m.radial_segments = 6
	m.rings = 3
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(0.85, 0.85, 0.88, 0.45)
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.vertex_color_use_as_albedo = true
	m.material = mat
	p.mesh = m
	var curve := Curve.new()
	curve.add_point(Vector2(0, 0.4))
	curve.add_point(Vector2(1, 1.8))
	p.scale_amount_curve = curve
	var grad := Gradient.new()
	grad.set_color(0, Color(1, 1, 1, 0.55))
	grad.set_color(1, Color(1, 1, 1, 0.0))
	p.color_ramp = grad
	(e["model"] as Node3D).add_child(p)
	e["smoke"] = p
	if b.type == "house":
		p.emitting = b.heated


func _add_lamp(b: Building, e: Dictionary) -> void:
	if not LAMPS.has(b.type) or e.get("lamp") != null:
		return
	var lit: Array = []
	for pos in LAMPS[b.type]:
		var lamp := _model("lamp_post")
		lamp.position = pos
		(e["model"] as Node3D).add_child(lamp)
		lit.append(lamp.find_child("lamp", true, false))
	e["lamp"] = lit
	e["lamp_lit"] = true


## ---- Stored goods ------------------------------------------------------------

const GOODS_UNIT := 4.0        # goods per item shown
const GOODS_PER_LAYER := 9     # 3 x 3
const GOODS_LAYERS := 3
const GOODS_CORNER := {"wood": Vector2(-0.42, -0.42), "stone": Vector2(0.42, -0.42),
	"food": Vector2(-0.42, 0.42), "water": Vector2(0.42, 0.42)}
static var _goods_mesh: Dictionary = {}


## Open-air stockpiles show what the colony has stored: logs, stone blocks, food sacks, water
## barrels. Shared stock fills the open stockpiles first (by capacity); the rest is out of sight
## inside storehouses.
func _update_goods() -> void:
	var piles: Array = []
	var open_cap := 0.0
	for bid in _buildings:
		var b := sim.buildings.get(bid) as Building
		if b != null and b.complete and b.type == "stockpile":
			piles.append(b)
			open_cap += float(b.provides("storage", 0))
	if piles.is_empty():
		return
	for b: Building in piles:
		var e: Dictionary = _buildings[b.id]
		if e["goods"] == null:
			e["goods"] = _make_goods(e["root"])
		var share := float(b.provides("storage", 0)) / maxf(open_cap, 1.0)
		var shown := minf(1.0, open_cap / maxf(sim.stock_total(), 1.0))
		for res in GOODS_CORNER:
			var amount := float(sim.stock.get(res, 0.0)) * share * shown
			var n := clampi(int(ceil(amount / GOODS_UNIT)), 0, GOODS_PER_LAYER * GOODS_LAYERS)
			(e["goods"][res] as MultiMeshInstance3D).multimesh.visible_instance_count = n


func _make_goods(root: Node3D) -> Dictionary:
	var out := {}
	for res in GOODS_CORNER:
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.mesh = _goods_item_mesh(res)
		mm.instance_count = GOODS_PER_LAYER * GOODS_LAYERS
		var h := 0.12 if res != "water" else 0.2
		var corner: Vector2 = GOODS_CORNER[res]
		for i in mm.instance_count:
			var layer := i / GOODS_PER_LAYER
			var k := i % GOODS_PER_LAYER
			var x := corner.x + (float(k % 3) - 1.0) * 0.24
			var z := corner.y + (float(k / 3) - 1.0) * 0.24
			var jitter := (_hash01(i * 13 + res.length()) - 0.5) * 0.35
			var basis := Basis(Vector3.UP, jitter)
			if res == "wood":
				basis = basis * Basis(Vector3(0, 0, 1), PI / 2.0)   # logs lie on their side
			mm.set_instance_transform(i, Transform3D(basis, Vector3(x, 0.1 + h * 0.5 + layer * h, z)))
		mm.visible_instance_count = 0
		var mi := MultiMeshInstance3D.new()
		mi.multimesh = mm
		root.add_child(mi)
		out[res] = mi
	return out


static func _goods_item_mesh(res: String) -> Mesh:
	if _goods_mesh.has(res):
		return _goods_mesh[res]
	var m: PrimitiveMesh
	match res:
		"wood":
			var c := CylinderMesh.new()
			c.top_radius = 0.055
			c.bottom_radius = 0.055
			c.height = 0.22
			c.radial_segments = 6
			m = c
		"stone":
			var b := BoxMesh.new()
			b.size = Vector3(0.2, 0.11, 0.18)
			m = b
		"food":
			var sph := SphereMesh.new()
			sph.radius = 0.1
			sph.height = 0.14
			sph.radial_segments = 7
			sph.rings = 4
			m = sph
		_:
			var w := CylinderMesh.new()
			w.top_radius = 0.085
			w.bottom_radius = 0.08
			w.height = 0.2
			w.radial_segments = 8
			m = w
	var mat := StandardMaterial3D.new()
	# STYLE.md palette: plank logs, light stone, sacking, and barrels with a blue tint so water reads.
	mat.albedo_color = {"wood": Color("#9C6B3F"), "stone": Color("#B8B3AA"),
		"food": Color("#D2B48C"), "water": Color("#557FAE")}[res]
	mat.roughness = 0.85
	m.material = mat
	_goods_mesh[res] = m
	return m


## Houses glow warm in the evening and at night while someone is home.
func _update_windows() -> void:
	var h := sim.time_of_day()
	var dark := h >= 18.5 or h < 6.0
	# Lamps burn all night; windows glow in the evening and before dawn wherever someone lives
	# (and in the kitchen and town hall), then go dark while everyone sleeps.
	var evening := (h >= 18.5 and h < 23.5) or (h >= 5.0 and h < 6.5)
	var home := {}
	for c: Colonist in sim.colonists.values():
		if c.home_id >= 0:
			home[c.home_id] = true
	for bid in _buildings:
		var e: Dictionary = _buildings[bid]
		var bb := sim.buildings.get(bid) as Building
		var smoke := e.get("smoke") as CPUParticles3D
		if smoke != null and bb != null and bb.type == "house":
			smoke.emitting = bb.heated
		var lamps: Variant = e.get("lamp")
		if lamps is Array and bool(e.get("lamp_lit", false)) != dark:
			e["lamp_lit"] = dark
			for lamp: MeshInstance3D in lamps:
				if lamp != null:
					lamp.material_override = _glow_mat() if dark else _dim_lamp_mat()
		var w := e.get("window") as MeshInstance3D
		if w == null:
			continue
		var meeting_here: bool = not sim.meetings.current.is_empty() and sim.meetings.current["place"] == bb
		var lit: bool = meeting_here or (evening and (home.has(bid) or (bb != null and bb.type in ["kitchen", "town_hall"])))
		if lit == bool(e["lit"]):
			continue
		e["lit"] = lit
		w.material_override = _glow_mat() if lit else null


var _glow: StandardMaterial3D
var _dim_lamp: StandardMaterial3D


## Warm window / lamp light (STYLE.md "Fire, window glow").
func _glow_mat() -> StandardMaterial3D:
	if _glow == null:
		_glow = StandardMaterial3D.new()
		_glow.albedo_color = Color("#FFB040")
		_glow.emission_enabled = true
		_glow.emission = Color("#FFA030")
		_glow.emission_energy_multiplier = 2.4
	return _glow


func _dim_lamp_mat() -> StandardMaterial3D:
	if _dim_lamp == null:
		_dim_lamp = StandardMaterial3D.new()
		_dim_lamp.albedo_color = Color("#C8A060")
	return _dim_lamp


var _roofs_snowy := false
var _snow_mat: StandardMaterial3D


## Snow on every roof in winter (roof parts are named after their roof material).
func snow_roofs(on: bool) -> void:
	_roofs_snowy = on
	for bid in _buildings:
		_snow_roof(_buildings[bid]["model"], on)


func _snow_roof(model: Node3D, on: bool) -> void:
	if _snow_mat == null:
		_snow_mat = _flat_mat(Color(0.97, 0.98, 1.0))   # brighter than the ground snow so roofs stand out
	for mi in model.find_children("roof_*", "MeshInstance3D", true, false):
		var n := String(mi.name)
		if n.ends_with("_dark"):      # ridge and eaves stay dark so the roof shape still reads
			continue
		(mi as MeshInstance3D).material_override = _snow_mat if on else null


## Seasonal tree tops: autumn colours (picked per tree), snow-dusted in winter, natural otherwise.
func tint_trees(season: String, autumn: Array, winter: Color) -> void:
	for nid in _nodes:
		var n := sim.nodes.get(nid) as ResNode
		var m: Node3D = _nodes[nid]
		if n == null or m == null or not (n.type == "tree" or n.type == "berry_bush"):
			continue
		var col: Variant = null
		if season == "autumn" and n.type == "tree" and int(m.get_meta("variant", 1)) != 3:   # pines stay green
			col = autumn[int(_hash01(nid * 3) * autumn.size()) % autumn.size()]
		elif season == "winter":
			col = winter
		for leaf in ["leaves1", "leaves2"]:
			var mi := m.find_child(leaf, true, false) as MeshInstance3D
			if mi != null:
				var c2: Variant = col
				if col != null and leaf == "leaves2":
					c2 = (col as Color).lightened(0.18)
				mi.material_override = _flat_mat(c2) if c2 != null else null


var _fog: Callable    # cell -> bool (discovered); unseen resources stay hidden


func set_fog(is_seen: Callable) -> void:
	_fog = is_seen
	_update_nodes()


## Trees and rocks shrink as they're used up; ones nobody has seen yet stay hidden in the fog.
func _update_nodes() -> void:
	for nid in _nodes:
		var n := sim.nodes.get(nid) as ResNode
		var m: Node3D = _nodes[nid]
		if n == null or m == null:
			continue
		m.visible = not _fog.is_valid() or bool(_fog.call(n.cell))
		var base := float(m.get_meta("base_scale", 1.0))
		var f := 0.55 + 0.45 * clampf(n.amount / maxf(n.max_amount, 1.0), 0.0, 1.0)
		m.scale = Vector3.ONE * base * f


## ---- Picking ----------------------------------------------------------------

func select(c: Colonist) -> void:
	select_target(c)


func select_target(t: Variant) -> void:
	selected_target = t
	selected = t as Colonist if (t is Colonist and (t as Colonist).alive()) else null
	_place_ring()
	colonist_clicked.emit(selected)
	target_selected.emit(t)


## The scene node that carries a living colonist (for the follow camera), or null.
func colonist_root(c: Colonist) -> Node3D:
	var e: Dictionary = _colonists.get(c.id, {}) if c != null else {}
	return e.get("root")


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
	if n != null and sim.is_discovered(cell):
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
		var t: Variant = pick(event.position)
		select_target(t)
		if event.double_click and t is Colonist:
			follow_requested.emit(t)
	elif event is InputEventMouseMotion:
		_hover = pick(event.position)
		hovered.emit(_hover, event.position)
