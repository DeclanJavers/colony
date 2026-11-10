class_name WildlifeView
extends Node3D
## Makes the world feel alive (docs/STYLE.md):
##   * rabbits and deer are the sim's game animals (Sim.wildlife): drawn here where the sim has them,
##     hopping or trotting as they go, and falling over when a hunter gets one,
##   * birds peck about in little flocks, take off when disturbed and fly to another spot (scenery),
##   * fish jump in the river now and then (and when someone lands one),
##   * butterflies by day in spring and summer, fireflies on warm nights, falling leaves in
##     autumn, ripples on the water when it rains.
## Animals stay hidden in unexplored fog.

const BIRD_FLOCKS := 4

var sim: Sim
var view: WorldView
var camera: Camera3D
var rng := RandomNumberGenerator.new()
var _animals: Array = []      # birds: {kind, node, pos: Vector2, target: Vector2, state, timer, speed, flee, phase, y}
var _game: Dictionary = {}    # sim animal id -> {node, pos: Vector2 (as drawn), phase}
var _fish_t := 3.0
var _ripples: Array[MeshInstance3D] = []
var _butterflies: GPUParticles3D
var _fireflies: GPUParticles3D
var _leaves: GPUParticles3D


func setup(p_sim: Sim, p_view: WorldView, p_camera: Camera3D) -> void:
	sim = p_sim
	view = p_view
	camera = p_camera
	rng.seed = sim.seed_value * 7 + 3
	for f in BIRD_FLOCKS:
		var c := _random_cell(6.0, true)
		for k in rng.randi_range(3, 5):
			var cc := _near_walkable(c, 1)
			if cc.x >= 0:
				_spawn("bird", cc, f * 10 + k)
	_butterflies = _make_particles(Color("#F2DFA8"), Color("#E8A0C8"), 0.11, 40, 6.0, false)
	_butterfly_look(_butterflies)
	_fireflies = _make_particles(Color("#E8F27A"), Color("#B8F28A"), 0.035, 60, 5.0, true)
	_leaves = _make_particles(Color("#D9822B"), Color("#B9472A"), 0.07, 50, 6.0, false)
	(_leaves.process_material as ParticleProcessMaterial).gravity = Vector3(0, -0.35, 0)
	# Leaves are lit like everything else (unlit, they'd glow like embers at night).
	((_leaves.draw_pass_1 as QuadMesh).material as StandardMaterial3D).shading_mode = BaseMaterial3D.SHADING_MODE_PER_PIXEL
	for k in 24:
		var r := MeshInstance3D.new()
		var t := TorusMesh.new()
		t.inner_radius = 0.16
		t.outer_radius = 0.2
		t.rings = 12
		t.ring_segments = 3
		r.mesh = t
		var m := StandardMaterial3D.new()
		m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		m.albedo_color = Color(0.85, 0.92, 1.0, 0.0)
		r.material_override = m
		r.visible = false
		add_child(r)
		_ripples.append(r)
	sim.events.fish_caught.connect(func(_c: Colonist, shore: Vector2i) -> void:
		for nb in sim.map.neighbors4(shore):
			if sim.map.in_bounds(nb) and sim.map.tile(nb) == WorldMap.Tile.WATER:
				_fish_jump(nb, true)
				return)


func _spawn(kind: String, cell: Vector2i, key: int) -> void:
	var node := view.make_model(kind, key)
	node.position = Vector3(cell.x, 0, cell.y)
	node.rotation.y = rng.randf() * TAU
	var s: float = {"rabbit": 1.0, "deer": 1.0, "bird": 1.2}[kind] * rng.randf_range(0.9, 1.1)
	node.scale = Vector3.ONE * s
	add_child(node)
	_animals.append({"kind": kind, "node": node, "pos": Vector2(cell), "target": Vector2(cell), "state": "idle",
		"timer": rng.randf_range(1.0, 6.0), "speed": 0.0, "phase": rng.randf() * TAU, "y": 0.0,
		"wing_l": node.find_child("wing_l", true, false), "wing_r": node.find_child("wing_r", true, false)})


func _random_cell(min_from_camp: float, near_trees: bool = false) -> Vector2i:
	var n := sim.map.size
	for tries in 200:
		var c := Vector2i(rng.randi_range(1, n - 2), rng.randi_range(1, n - 2))
		if Vector2(c - sim.map.center).length() < min_from_camp or not sim.map.is_walkable(c):
			continue
		if near_trees:
			var trees := 0
			for dy in range(-3, 4):
				for dx in range(-3, 4):
					var nd := sim.node_at_cell(c + Vector2i(dx, dy))
					if nd != null and nd.type == "tree":
						trees += 1
			if trees < 3:
				continue
		return c
	return Vector2i(-1, -1)


func _near_walkable(c: Vector2i, r: int) -> Vector2i:
	for tries in 20:
		var cc := c + Vector2i(rng.randi_range(-r, r), rng.randi_range(-r, r))
		if sim.map.is_walkable(cc):
			return cc
	return Vector2i(-1, -1)


func _make_particles(c1: Color, c2: Color, size: float, amount: int, life: float, glow: bool) -> GPUParticles3D:
	var p := GPUParticles3D.new()
	p.amount = amount
	p.lifetime = life
	p.preprocess = life
	p.local_coords = false
	p.emitting = false
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_BOX
	pm.emission_box_extents = Vector3(10, 0.4, 10)
	pm.direction = Vector3(0, 1, 0)
	pm.spread = 180.0
	pm.initial_velocity_min = 0.05
	pm.initial_velocity_max = 0.25
	pm.gravity = Vector3.ZERO
	pm.turbulence_enabled = true
	pm.turbulence_noise_strength = 1.4
	pm.turbulence_noise_scale = 3.0
	pm.turbulence_influence_min = 0.05
	pm.turbulence_influence_max = 0.15
	var grad := Gradient.new()
	grad.set_color(0, Color(c1, 0.0))
	grad.add_point(0.15, c1)
	grad.add_point(0.6, c2)
	grad.set_color(grad.get_point_count() - 1, Color(c2, 0.0))
	var gt := GradientTexture1D.new()
	gt.gradient = grad
	pm.color_ramp = gt
	p.process_material = pm
	var q := QuadMesh.new()
	q.size = Vector2(size, size)
	var m := StandardMaterial3D.new()
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.vertex_color_use_as_albedo = true
	m.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	if glow:
		m.emission_enabled = true
		m.emission = c1
		m.emission_energy_multiplier = 3.0
	q.material = m
	p.draw_pass_1 = q
	add_child(p)
	return p


## Butterflies get a wing shape (two lobes each side) and a colour each: yellow, orange, blue, white.
func _butterfly_look(p: GPUParticles3D) -> void:
	var img := Image.create_empty(8, 8, false, Image.FORMAT_RGBA8)
	var shape := ["........", ".##..##.", "###..###", "###..###", ".##..##.", ".##..##.", "..#..#..", "........"]
	for y in 8:
		for x in 8:
			img.set_pixel(x, y, Color(1, 1, 1, 1) if shape[y][x] == "#" else Color(0, 0, 0, 0))
	var m := (p.draw_pass_1 as QuadMesh).material as StandardMaterial3D
	m.albedo_texture = ImageTexture.create_from_image(img)
	m.texture_filter = BaseMaterial3D.TEXTURE_FILTER_NEAREST
	var pm := p.process_material as ParticleProcessMaterial
	var cols := Gradient.new()
	cols.offsets = PackedFloat32Array([0.0, 0.3, 0.6, 0.85, 1.0])
	cols.colors = PackedColorArray([Color("#F2D45C"), Color("#E8873A"), Color("#7FA8E8"), Color("#F4F1E8"), Color("#F2D45C")])
	cols.interpolation_mode = Gradient.GRADIENT_INTERPOLATE_CONSTANT
	var ct := GradientTexture1D.new()
	ct.gradient = cols
	pm.color_initial_ramp = ct
	var fade := Gradient.new()
	fade.offsets = PackedFloat32Array([0.0, 0.1, 0.9, 1.0])
	fade.colors = PackedColorArray([Color(1, 1, 1, 0), Color(1, 1, 1, 1), Color(1, 1, 1, 1), Color(1, 1, 1, 0)])
	var ft := GradientTexture1D.new()
	ft.gradient = fade
	pm.color_ramp = ft


func _process(delta: float) -> void:
	if sim == null:
		return
	var people: Array[Vector2] = []
	for c: Colonist in sim.colonists.values():
		people.append(c.pos)
	for a in _animals:
		_update_animal(a, delta, people)
	_update_game(delta)
	_update_ambient(delta)


## Rabbits and deer follow the sim: glide to where it has them, face the way they go, hop.
func _update_game(delta: float) -> void:
	var live: Dictionary = sim.wildlife.animals
	for id in _game.keys():
		if not live.has(id):
			_fall(_game[id]["node"])      # hunted (or, rarely, gone some other way)
			_game.erase(id)
	for id in live:
		var a: Dictionary = live[id]
		var g: Dictionary = _game.get(id, {})
		if g.is_empty():
			var node := view.make_model(String(a["kind"]), int(id))
			node.scale = Vector3.ONE * rng.randf_range(0.9, 1.1)
			node.rotation.y = rng.randf() * TAU
			add_child(node)
			g = {"node": node, "pos": a["pos"], "phase": rng.randf() * TAU,
				"anim": _anim_of(node), "clip": "", "rest": "graze", "rest_t": rng.randf_range(2.0, 12.0)}
			_game[id] = g
		var node: Node3D = g["node"]
		var was: Vector2 = g["pos"]
		var to: Vector2 = a["pos"] - was
		var now: Vector2 = a["pos"] if to.length() > 4.0 else was + to * clampf(delta * 10.0, 0.0, 1.0)
		g["pos"] = now
		var step := now - was
		var moving := step.length() > 0.0005
		if moving:
			# Models face +Z (make_models.py), so turn +Z toward where it's going.
			node.rotation.y = lerp_angle(node.rotation.y, atan2(step.x, step.y), clampf(delta * 10.0, 0.0, 1.0))
		g["phase"] = float(g["phase"]) + delta * (2.0 if a["state"] == "flee" else 1.0)
		var y := 0.0
		var anim := g["anim"] as AnimationPlayer
		if anim != null:
			_animate(g, anim, moving, a["state"] == "flee", step.length() / maxf(delta, 0.0001), delta)
		elif moving:
			y = absf(sin(float(g["phase"]) * 9.0)) * 0.12 if a["kind"] == "rabbit" else absf(sin(float(g["phase"]) * 5.0)) * 0.03
		node.visible = sim.is_discovered(Vector2i(now.round()))
		node.position = Vector3(now.x, y + (view.ground.y_at(now) if view.ground != null else 0.0), now.y)


## Rigged animals (the deer, tools/blender/deer.py): run when fleeing and walk when wandering, each
## sped up to match how fast it's actually going; when resting, mostly grazing, now and then
## lifting its head to look round.
const WALK_REF := 0.7     # m/s the walk clip covers at speed 1
const RUN_REF := 3.4      # m/s the run clip covers at speed 1


func _anim_of(node: Node3D) -> AnimationPlayer:
	var ap := node.find_child("AnimationPlayer", true, false) as AnimationPlayer
	if ap != null:
		for n in ap.get_animation_list():
			ap.get_animation(n).loop_mode = Animation.LOOP_LINEAR
	return ap


func _animate(g: Dictionary, ap: AnimationPlayer, moving: bool, fleeing: bool, speed: float, delta: float) -> void:
	var clip: String
	var rate := 1.0
	if moving and fleeing:
		clip = "run"
		rate = clampf(speed / RUN_REF, 0.8, 2.5)
	elif moving:
		clip = "walk"
		rate = clampf(speed / WALK_REF, 0.6, 2.5)
	else:
		g["rest_t"] = float(g["rest_t"]) - delta
		if float(g["rest_t"]) <= 0.0:
			var grazing: bool = g["rest"] == "graze"
			g["rest"] = "idle" if grazing else "graze"
			g["rest_t"] = rng.randf_range(3.0, 6.0) if grazing else rng.randf_range(6.0, 16.0)
		clip = String(g["rest"])
	if not ap.has_animation(clip):
		return
	if clip != g["clip"]:
		g["clip"] = clip
		ap.play(clip, 0.25)
	ap.speed_scale = rate


func _fall(node: Node3D) -> void:
	var tw := create_tween()
	tw.tween_property(node, "rotation:z", PI * 0.5, 0.35).set_ease(Tween.EASE_IN)
	tw.tween_interval(2.0)
	tw.tween_property(node, "scale", Vector3.ONE * 0.01, 0.4)
	tw.tween_callback(node.queue_free)


func _update_animal(a: Dictionary, delta: float, people: Array[Vector2]) -> void:
	var node: Node3D = a["node"]
	var kind: String = a["kind"]
	var cell := Vector2i(roundi(a["pos"].x), roundi(a["pos"].y))
	node.visible = sim.is_discovered(cell)
	# Is anyone too close? (deer are warier than rabbits and birds)
	var wary: float = {"rabbit": 2.5, "deer": 5.0, "bird": 2.2}[kind]
	var threat := Vector2.INF
	for p in people:
		if p.distance_to(a["pos"]) < wary and (threat == Vector2.INF or p.distance_to(a["pos"]) < threat.distance_to(a["pos"])):
			threat = p
	if threat != Vector2.INF and a["state"] != "flee":
		var away: Vector2 = (a["pos"] - threat).normalized()
		if away == Vector2.ZERO:
			away = Vector2.from_angle(rng.randf() * TAU)
		var dist: float = {"rabbit": 5.0, "deer": 8.0, "bird": 10.0}[kind]
		a["target"] = _walkable_toward(a["pos"] + away * dist, kind == "bird")
		a["state"] = "flee"
		a["speed"] = {"rabbit": 3.2, "deer": 2.6, "bird": 4.0}[kind]
	match a["state"]:
		"idle":
			a["timer"] -= delta
			if a["timer"] <= 0.0:
				var reach: float = {"rabbit": 2.5, "deer": 3.0, "bird": 6.0}[kind]
				a["target"] = _walkable_toward(a["pos"] + Vector2.from_angle(rng.randf() * TAU) * rng.randf_range(1.0, reach), kind == "bird")
				a["state"] = "move"
				a["speed"] = {"rabbit": 1.2, "deer": 0.5, "bird": 2.2}[kind]
		"move", "flee":
			# Ground animals go round trees, buildings and water (path), birds fly straight.
			if kind != "bird":
				var here := Vector2i(roundi(a["pos"].x), roundi(a["pos"].y))
				if not sim.map.is_walkable(here):   # (a building went up where it stood)
					var w := sim.map.nearest_walkable(here)
					if w.x >= 0:
						a["pos"] = Vector2(w)
				var path: Array = a.get("path", [])
				if path.is_empty() or a.get("path_to") != a["target"]:
					path = Array(sim.map.find_path(Vector2i(roundi(a["pos"].x), roundi(a["pos"].y)), Vector2i(a["target"])))
					a["path_to"] = a["target"]
					if path.is_empty():
						a["state"] = "idle"
						a["timer"] = rng.randf_range(1.0, 3.0)
						return
				while path.size() > 1 and Vector2(path[0]).distance_to(a["pos"]) < 0.3:
					path.pop_front()
				a["path"] = path
			var aim: Vector2 = a["target"] if kind == "bird" or (a["path"] as Array).is_empty() else Vector2((a["path"] as Array)[0])
			var to: Vector2 = aim - a["pos"]
			var step: float = a["speed"] * delta
			if to.length() <= step and aim == a["target"]:
				a["pos"] = a["target"]
				a["state"] = "idle"
				a["path"] = []
				a["timer"] = rng.randf_range(2.0, 7.0) if kind != "bird" else rng.randf_range(1.0, 4.0)
			else:
				a["pos"] += to.normalized() * step
				# Models face +Z (docs/STYLE.md / make_models.py), so turn +Z toward where it's going.
				node.rotation.y = lerp_angle(node.rotation.y, atan2(to.x, to.y), clampf(delta * 8.0, 0.0, 1.0))
	a["phase"] = float(a["phase"]) + delta
	var moving: bool = a["state"] != "idle"
	var y := 0.0
	match kind:
		"rabbit":
			y = absf(sin(float(a["phase"]) * 9.0)) * 0.12 if moving else 0.0
		"deer":
			y = absf(sin(float(a["phase"]) * 5.0)) * 0.03 if moving else 0.0
		"bird":
			# Flying between spots (higher the further the hop), pecking on the ground otherwise.
			if moving:
				var total: float = maxf(0.5, (a["target"] - a["pos"]).length() + 0.5)
				y = 1.2 + minf(2.5, total * 0.3)
				var flap := sin(float(a["phase"]) * 28.0) * 0.9
				for w in ["wing_l", "wing_r"]:
					var wn := a[w] as Node3D
					if wn != null:
						wn.rotation.y = flap * (1.0 if w == "wing_l" else -1.0)
			else:
				y = 0.08 + absf(sin(float(a["phase"]) * 6.0)) * 0.02
				for w in ["wing_l", "wing_r"]:
					var wn := a[w] as Node3D
					if wn != null:
						wn.rotation.y = 0.0
	a["y"] = lerpf(float(a["y"]), y, clampf(delta * 6.0, 0.0, 1.0)) if kind == "bird" else y
	node.position = Vector3(a["pos"].x, a["y"] + (view.ground.y_at(a["pos"]) if view.ground != null else 0.0), a["pos"].y)


## Where an animal heads: open ground near `p` (birds too: they land on open ground, not on
## water, trees or roofs).
func _walkable_toward(p: Vector2, _flying: bool) -> Vector2:
	var n := sim.map.size
	var c := Vector2i(clampi(roundi(p.x), 1, n - 2), clampi(roundi(p.y), 1, n - 2))
	if sim.map.is_walkable(c):
		return Vector2(c)
	var w := sim.map.nearest_walkable(c)
	return Vector2(w) if w.x >= 0 else p


## ---- Ambient: fish, butterflies, fireflies, leaves, rain ripples ---------------------------

func _focus() -> Vector3:
	var vp := get_viewport().get_visible_rect().size
	var from := camera.project_ray_origin(vp * 0.5)
	var dir := camera.project_ray_normal(vp * 0.5)
	if absf(dir.y) < 0.001:
		return Vector3.ZERO
	return from + dir * (-from.y / dir.y)


func _update_ambient(delta: float) -> void:
	var focus := _focus()
	var ext := camera.size * 0.7
	var season := sim.season()
	var night := sim.is_night()
	var wet := sim.weather in ["rain", "storm"]
	for p in [_butterflies, _fireflies, _leaves]:
		p.global_position = focus + Vector3(0, 0.6, 0)
		(p.process_material as ParticleProcessMaterial).emission_box_extents = Vector3(ext, 0.4, ext)
	_butterflies.emitting = not night and not wet and season in ["spring", "summer"] and camera.size < 40.0
	_fireflies.emitting = night and season in ["spring", "summer", "autumn"] and camera.size < 40.0
	_leaves.global_position = focus + Vector3(0, 2.5, 0)
	_leaves.emitting = season == "autumn" and camera.size < 45.0
	# Fish jump now and then somewhere near where you're looking.
	_fish_t -= delta
	if _fish_t <= 0.0:
		_fish_t = rng.randf_range(2.5, 7.0)
		var c := _water_near(focus, ext)
		if c.x >= 0 and sim.is_discovered(c) and season != "winter":
			_fish_jump(c, false)
	# (No rain rings on the water: rain isn't drawn, see SeasonView.)


func _water_near(focus: Vector3, ext: float) -> Vector2i:
	for tries in 12:
		var c := Vector2i(roundi(focus.x + rng.randf_range(-ext, ext)), roundi(focus.z + rng.randf_range(-ext, ext)))
		if sim.map.in_bounds(c) and sim.map.tile(c) == WorldMap.Tile.WATER:
			return c
	return Vector2i(-1, -1)


func _fish_jump(cell: Vector2i, caught: bool) -> void:
	var f := view.make_model("fish", rng.randi())
	var start := Vector3(cell.x + rng.randf_range(-0.3, 0.3), 0.0, cell.y + rng.randf_range(-0.3, 0.3))
	var dir := Vector3.FORWARD.rotated(Vector3.UP, rng.randf() * TAU) * 0.5
	f.position = start
	f.look_at_from_position(start, start + dir, Vector3.UP, true)   # head (+Z) first
	add_child(f)
	_ripple(start + Vector3(0, 0.03, 0), 0.6)
	var tw := create_tween()
	var dur := 0.7
	tw.tween_method(func(t: float) -> void:
		f.position = start + dir * t + Vector3(0, sin(t * PI) * (0.55 if not caught else 0.35), 0)
		f.rotation.x = lerpf(-0.9, 0.9, t), 0.0, 1.0, dur)
	tw.tween_callback(func() -> void:
		_ripple(start + dir + Vector3(0, 0.03, 0), 0.7)
		f.queue_free())


func _ripple(at: Vector3, size: float) -> void:
	for r in _ripples:
		if r.visible:
			continue
		r.visible = true
		r.position = at
		r.scale = Vector3.ONE * 0.3
		var m := r.material_override as StandardMaterial3D
		m.albedo_color.a = 0.7
		var tw := create_tween().set_parallel(true)
		tw.tween_property(r, "scale", Vector3.ONE * (1.0 + size), 0.9)
		tw.tween_property(m, "albedo_color:a", 0.0, 0.9)
		tw.chain().tween_callback(func() -> void: r.visible = false)
		return
