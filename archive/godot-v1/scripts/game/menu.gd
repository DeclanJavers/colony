extends Node3D
## Title screen: Continue, New game, Load, Quit, over a slowly turning diorama of a real map at
## golden hour. New game shows the map options; changing them regenerates the map behind the menu
## so you can see what you'll get. Command-line runs (tests, screenshots) skip straight to the game.

const GAME := "res://scenes/main.tscn"
const SIZES := [["Small", 48], ["Medium", 60], ["Large", 72]]
const AMOUNTS := [["Sparse", 0.6], ["Normal", 1.0], ["Thick", 1.5]]
const HILLS := [["Flat", 0.0], ["Gentle", 1.0], ["Hilly", 1.8]]
const PONDS := [["None", 0], ["A few", 2], ["Many", 5]]
const SETTLERS := [["3", 3], ["5", 5], ["8", 8]]
const GAME_AMOUNTS := [["Few", 0.5], ["Normal", 1.0], ["Many", 1.6]]
const BRAINS := [["Hand-made rules", "utility"], ["Laya (learned AI)", "laya"]]

var opts := {"seed": 0, "size": 60, "forests": 1.0, "rocks": 1.0, "hills": 1.0, "ponds": 2, "river": true, "game": 1.0, "settlers": 5, "brain": String(Content.get_instance().cfg("decision.brain", "laya"))}
var _preview: Node3D
var _cam: IsoCamera
var _rebuild_t := -1.0
var _main := VBoxContainer.new()
var _new := VBoxContainer.new()
var _load := VBoxContainer.new()
var _seed_edit := LineEdit.new()


func _ready() -> void:
	var args := OS.get_cmdline_user_args()
	if not args.is_empty() and not args.has("--menu"):
		get_tree().change_scene_to_file.call_deferred(GAME)
		return
	randomize()
	opts["seed"] = randi() % 100000
	_build_preview()
	_build_ui()
	# Test hooks: --menu [--menu-page new|load] [--shot path.png --shot-delay s]
	var i := args.find("--menu-page")
	if i >= 0 and i + 1 < args.size():
		match args[i + 1]:
			"new":
				_show(_new)
			"load":
				_open_load()
	i = args.find("--shot")
	if i >= 0 and i + 1 < args.size():
		var path := args[i + 1]
		var d := args.find("--shot-delay")
		var delay := float(args[d + 1]) if d >= 0 and d + 1 < args.size() else 4.0
		get_tree().create_timer(delay).timeout.connect(func() -> void:
			get_viewport().get_texture().get_image().save_png(path)
			print("screenshot: ", path)
			get_tree().quit())


## ---- The diorama behind the menu -----------------------------------------------------------

func _build_preview() -> void:
	if _preview != null:
		_preview.queue_free()
	_preview = Node3D.new()
	add_child(_preview)
	var sim := Sim.new()
	sim.map_options = _map_options()
	sim.setup(int(opts["seed"]))
	sim.clock = 16.8 * 60.0      # golden hour
	var dn := DayNight.new()
	dn.sim = sim
	_preview.add_child(dn)
	var prev_yaw := _cam.rotation.y if _cam != null else deg_to_rad(45.0)
	_cam = IsoCamera.new()
	_preview.add_child(_cam)
	_cam.set_process_unhandled_input(false)
	_cam.set_process_input(false)
	var n := sim.map.size
	_cam.bounds = Rect2(0, 0, n, n)
	_cam.focus(Vector3(n / 2.0, 0, n / 2.0), true)
	_cam._size_target = n * 0.82
	_cam.camera.size = _cam._size_target
	_cam.camera.h_offset = -n * 0.2      # the map sits to the right of the menu, however it turns
	_cam.rotation.y = prev_yaw
	_cam.set("_yaw_target", prev_yaw)
	var view := WorldView.new()
	_preview.add_child(view)
	view.bind(sim, _cam.camera)
	view.set_process_unhandled_input(false)
	var sv := SeasonView.new()
	_preview.add_child(sv)
	sv.setup(sim, view, _cam.camera, view.ground_material)
	# Everything is known on the title screen: no fog.
	sim.discovered.fill(1)
	sim.discovered_count = sim.discovered.size()


func _map_options() -> Dictionary:
	return {"size": opts["size"], "forests": opts["forests"], "rocks": opts["rocks"], "berries": 1.0,
		"hills": opts["hills"], "ponds": opts["ponds"], "river": opts["river"], "game": opts["game"], "settlers": opts["settlers"]}


func _process(delta: float) -> void:
	if _cam != null:
		_cam.rotation.y += delta * 0.06       # a slow turn
		_cam.set("_yaw_target", _cam.rotation.y)
	if _rebuild_t >= 0.0:
		_rebuild_t -= delta
		if _rebuild_t < 0.0:
			_build_preview()


func _changed() -> void:
	_rebuild_t = 0.35      # rebuild shortly after the last change


## ---- Menu --------------------------------------------------------------------------------

func _build_ui() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	var root := Control.new()
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layer.add_child(root)
	var shade := TextureRect.new()       # a soft dark band on the left so the menu reads
	var g := Gradient.new()
	g.offsets = PackedFloat32Array([0.0, 0.55, 1.0])
	g.colors = PackedColorArray([Color(0.03, 0.03, 0.05, 0.85), Color(0.03, 0.03, 0.05, 0.55), Color(0.03, 0.03, 0.05, 0.0)])
	var gt := GradientTexture2D.new()
	gt.gradient = g
	gt.fill_to = Vector2(1, 0)
	shade.texture = gt
	shade.anchor_bottom = 1.0
	shade.offset_right = 760
	shade.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(shade)
	var col := VBoxContainer.new()
	col.position = Vector2(70, 110)
	col.custom_minimum_size = Vector2(440, 0)
	col.add_theme_constant_override("separation", 10)
	root.add_child(col)
	var title := UiStyle.label("Colony", 72, Color("#F2DFA8"))
	title.add_theme_color_override("font_outline_color", Color("#2A1E1A"))
	title.add_theme_constant_override("outline_size", 10)
	col.add_child(title)
	col.add_child(UiStyle.label("a little village that thinks for itself", 17, UiStyle.MUTED))
	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0, 26)
	col.add_child(gap)
	col.add_child(_main)
	col.add_child(_new)
	col.add_child(_load)
	_build_main()
	_build_new()
	_show(_main)


func _show(which: Control) -> void:
	for p in [_main, _new, _load]:
		p.visible = p == which


func _build_main() -> void:
	_main.add_theme_constant_override("separation", 4)
	var latest := SaveSlots.latest()
	if not latest.is_empty():
		var cont := UiStyle.make_button("Continue", func() -> void: _load_slot(String(latest["id"])), true)
		_main.add_child(cont)
		_main.add_child(UiStyle.label("    %s · day %d · %d people" % [latest.get("name", ""), int(latest.get("day", 0)), int(latest.get("pop", 0))], 13, UiStyle.MUTED))
	_main.add_child(UiStyle.make_button("New game", func() -> void: _show(_new), true))
	_main.add_child(UiStyle.make_button("Load game", func() -> void: _open_load(), true))
	_main.add_child(UiStyle.make_button("Quit", func() -> void: get_tree().quit(), true))


func _build_new() -> void:
	_new.add_theme_constant_override("separation", 8)
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", UiStyle.panel(0.7))
	_new.add_child(panel)
	var grid := GridContainer.new()
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", 18)
	grid.add_theme_constant_override("v_separation", 8)
	panel.add_child(grid)
	grid.add_child(UiStyle.label("Seed"))
	var seed_row := HBoxContainer.new()
	_seed_edit.text = str(opts["seed"])
	_seed_edit.custom_minimum_size = Vector2(120, 0)
	_seed_edit.text_submitted.connect(func(t: String) -> void:
		opts["seed"] = int(t) if t.is_valid_int() else abs(t.hash()) % 100000
		_changed())
	seed_row.add_child(_seed_edit)
	seed_row.add_child(UiStyle.make_button("Random", func() -> void:
		opts["seed"] = randi() % 100000
		_seed_edit.text = str(opts["seed"])
		_changed()))
	grid.add_child(seed_row)
	_choice(grid, "Map size", "size", SIZES)
	_choice(grid, "Forests", "forests", AMOUNTS)
	_choice(grid, "Rocks", "rocks", AMOUNTS)
	_choice(grid, "Hills", "hills", HILLS)
	_choice(grid, "Ponds", "ponds", PONDS)
	_choice(grid, "River", "river", [["Yes", true], ["No", false]])
	_choice(grid, "Deer and rabbits", "game", GAME_AMOUNTS)
	_choice(grid, "Settlers", "settlers", SETTLERS)
	_choice(grid, "Their minds", "brain", BRAINS, false)
	var row := HBoxContainer.new()
	row.add_child(UiStyle.make_button("Start", _start, true))
	row.add_child(UiStyle.make_button("Back", func() -> void: _show(_main), true))
	_new.add_child(row)


func _choice(grid: GridContainer, title: String, key: String, choices: Array, regen: bool = true) -> void:
	grid.add_child(UiStyle.label(title))
	var ob := OptionButton.new()
	ob.focus_mode = Control.FOCUS_NONE
	for i in choices.size():
		ob.add_item(String(choices[i][0]), i)
		if choices[i][1] == opts[key]:
			ob.select(i)
	ob.item_selected.connect(func(i: int) -> void:
		opts[key] = choices[i][1]
		if regen:
			_changed())
	grid.add_child(ob)


func _open_load() -> void:
	for ch in _load.get_children():
		ch.queue_free()
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", UiStyle.panel(0.7))
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size = Vector2(560, 420)
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	var list := SaveList.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list.load_requested.connect(_load_slot)
	scroll.add_child(list)
	panel.add_child(scroll)
	_load.add_child(panel)
	_load.add_child(UiStyle.make_button("Back", func() -> void: _show(_main), true))
	_show(_load)


func _start() -> void:
	if _seed_edit.text != str(opts["seed"]):
		opts["seed"] = int(_seed_edit.text) if _seed_edit.text.is_valid_int() else abs(_seed_edit.text.hash()) % 100000
	GameStart.new_game = {"seed": int(opts["seed"]), "map_options": _map_options(), "brain": opts["brain"]}
	GameStart.slot_id = ""
	get_tree().change_scene_to_file(GAME)


func _load_slot(id: String) -> void:
	var data := SaveSlots.read(id)
	if data.is_empty():
		return
	SaveGame.pending = data
	GameStart.slot_id = id
	get_tree().change_scene_to_file(GAME)
