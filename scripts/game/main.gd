extends Node3D
## Game entry point: wires SimRunner + WorldView + IsoCamera + DayNight + Hud.
## Optional command-line args (after `--`):
##   --seed N            map/colony seed (default from data/config.json)
##   --prewarm D         fast-forward D days before showing (skip the early game / test visuals)
##   --speed I           0 pause, 1 = 1x, 2 = 3x, 3 = 10x
##   --shot path.png     save a screenshot after --shot-delay seconds, then quit (for automated checks)
##   --time H            jump to hour H of the current day before showing (e.g. 21 for night)
##   --zoom S            camera ortho size
##   --inspect what     open the inspector on: colony | building:<type> | node:<type> | colonist:<name> | tile
##   --day D            start on day D (skips ahead with the baseline brain; e.g. 13 = winter)
##   --talk [intent]    the first two colonists start a conversation (chat | news | ask) right away
##   --say "text"        with --select: say this to the selected colonist, then save the chat as a memory
##   --load             continue from the saved game (user://save.tres)
##   --brain B           laya (default, from data/config.json) or utility (hand-made baseline)

var runner := SimRunner.new()
var view := WorldView.new()
var cam := IsoCamera.new()
var day_night := DayNight.new()
var hud := Hud.new()
var chat: WatcherChat
var talks: ConversationLLM


func _ready() -> void:
	var args := _parse(OS.get_cmdline_user_args())
	var content := Content.get_instance()
	var seed_v := int(args.get("seed", str(content.cfg("default_seed", 1234))))
	var brain := String(args.get("brain", ""))
	# A new game from the menu: its seed, map options and brain.
	if not GameStart.new_game.is_empty() and SaveGame.pending.is_empty():
		seed_v = int(GameStart.new_game.get("seed", seed_v))
		runner.map_options = GameStart.new_game.get("map_options", {}).duplicate()
		if brain == "":
			brain = String(GameStart.new_game.get("brain", ""))
		GameStart.new_game = {}
		GameStart.slot_id = ""

	add_child(runner)
	if args.has("load") and SaveGame.pending.is_empty():
		SaveGame.pending = SaveGame.read()
	var saved := SaveGame.pending
	SaveGame.pending = {}
	runner.start(seed_v, brain, saved)
	if args.has("prewarm"):
		runner.prewarm(float(args["prewarm"]))
	if args.has("day"):
		runner.prewarm(maxf(0.0, float(args["day"]) - runner.sim.clock / 1440.0))
	if args.has("time"):
		var target := float(args["time"]) * 60.0
		while fmod(runner.sim.clock, 1440.0) < target - 0.5 or fmod(runner.sim.clock, 1440.0) > target + 0.5:
			runner.sim.tick(1.0)
	runner.set_speed_index(int(args.get("speed", "1")))

	add_child(day_night)
	day_night.sim = runner.sim
	add_child(cam)
	var n := runner.sim.map.size
	cam.bounds = Rect2(0, 0, n, n)
	cam.focus(Vector3(runner.sim.map.center.x, 0, runner.sim.map.center.y), true)
	if args.has("zoom"):
		cam._size_target = float(args["zoom"])
		cam.camera.size = cam._size_target
	add_child(view)
	view.bind(runner.sim, cam.camera)
	var seasons_view := SeasonView.new()
	add_child(seasons_view)
	seasons_view.setup(runner.sim, view, cam.camera, view.ground_material)
	talks = ConversationLLM.new(runner.sim, String(content.cfg("brain_server_url", "http://127.0.0.1:8765")))
	talks.name = "ConversationLLM"
	add_child(talks)
	talks.is_watched = func(conv: Dictionary) -> bool:
		for c in [conv["a"], conv["b"]]:
			if view.selected == c:
				return true
			var p: Variant = view.target_position(c)
			if p != null and Vector2((p as Vector3).x - cam.position.x, (p as Vector3).z - cam.position.z).length() < cam.camera.size * 0.9:
				return true
		return false
	chat = WatcherChat.new(runner.sim, String(content.cfg("brain_server_url", "http://127.0.0.1:8765")))
	chat.name = "WatcherChat"
	add_child(chat)
	add_child(hud)
	hud.bind(runner, view, chat)
	hud.inspector.focus_requested.connect(func(p: Vector3) -> void: cam.focus(p))
	hud.cam = cam
	view.follow_requested.connect(func(c: Colonist) -> void: cam.follow_node = view.colonist_root(c))
	if args.has("select"):
		var first: Colonist = runner.sim.colonists.values()[0]
		view.select(first)
		if args.has("say"):
			chat.request_heard.connect(func(c: Colonist, a: String) -> void: print("REQUEST_HEARD ", c.name, " ", a, " reqs=", runner.sim.active_requests(c).size()))
			chat.failed.connect(func(c: Colonist, m: String) -> void: print("CHAT_FAILED ", m))
			runner.sim.events.decision_made.connect(func(c: Colonist, ctx: Dictionary, d: Dictionary, _b: bool) -> void:
				if c == first: print("DECISION ", c.name, " -> ", d.get("action"), " | ", String(ctx["state_text"]).contains("Watcher asked")))
			chat.reply_received.connect(func(c: Colonist, _r: String) -> void: chat.save_memory(c), CONNECT_ONE_SHOT)
			chat.send(first, String(args["say"]))

	if args.has("hunt"):   # test hook: the first adult takes a spear and stalks the nearest deer, camera on them
		_hunt_for_test()

	if args.has("talk"):   # test hook: the first two colonists start a conversation now
		var people := runner.sim.colonists.values()
		if people.size() >= 2:
			view.select_target(people[0])
			cam.focus(view.target_position(people[0]), true)
			runner.sim.converse(people[0], people[1], String(args["talk"]) if args["talk"] != "1" else "chat")

	# Autosave every morning (not in automated screenshot runs).
	if not args.has("shot"):
		runner.sim.events.hour_passed.connect(func(_d: int, h: int) -> void:
			if h == 7:
				runner.autosave())

	if args.has("save"):   # test hook: save right away (e.g. after --prewarm)
		print("saved: ", runner.save_game() == OK)

	if args.has("settings"):
		hud._settings.visible = true

	if args.has("inspect"):
		_inspect_for_test(String(args["inspect"]))

	if args.has("shot"):
		_take_shot(String(args["shot"]), float(args.get("shot-delay", "2.0")))


## Test hook (--hunt): the first adult gets a spear, stands a few tiles from a deer, and starts
## hunting it; the land around is uncovered and the camera follows them.
func _hunt_for_test() -> void:
	var sim := runner.sim
	var deer: Array = sim.wildlife.animals.values().filter(func(a: Dictionary) -> bool: return a["kind"] == "deer")
	var hunter: Colonist = null
	for c: Colonist in sim.colonists.values():
		if c.is_adult():
			hunter = c
			break
	if deer.is_empty() or hunter == null:
		return
	var d: Dictionary = deer[0]
	var spot := sim.map.nearest_walkable(Vector2i((d["pos"] + Vector2(5, 2)).round()))
	hunter.pos = Vector2(spot)
	hunter.cell = spot
	sim.give_tool(hunter, "spear")
	if hunter.action != null:
		hunter.action.end(false)
	sim.look_around(hunter, 12.0)
	hunter.knowledge.see_game("deer", Vector2i(d["pos"].round()), sim.clock, 1)
	var a := sim.make_action(hunter, sim.content.actions_by_id["hunt"])
	if a.available() and a.begin():
		hunter.action = a
		hunter.state = "acting"
	view.select(hunter)
	cam.follow_node = view.colonist_root(hunter)


## Test hook: select something by kind so screenshots can show each inspector page.
func _inspect_for_test(what: String) -> void:
	var sim := runner.sim
	var t: Variant = null
	if what == "colony":
		t = "colony"
	elif what.begins_with("building:"):
		for b: Building in sim.buildings.values():
			if b.type == what.substr(9):
				t = b
	elif what.begins_with("node:"):
		for n: ResNode in sim.nodes.values():
			if n.type == what.substr(5):
				t = n
				break
	elif what.begins_with("colonist:"):
		for c: Colonist in sim.colonists.values():
			if c.name == what.substr(9):
				t = c
	elif what == "tile":
		t = sim.map.center + Vector2i(3, 3)
	view.select_target(t)
	var pos: Variant = view.target_position(t)
	if pos != null:
		cam.focus(pos, true)


func _take_shot(path: String, delay: float) -> void:
	await get_tree().create_timer(delay).timeout
	await RenderingServer.frame_post_draw
	print("shot debug: cam size %.1f pos %s | inspector visible %s rect %s | window %s" % [cam.camera.size, cam.position, hud.inspector.visible, hud.inspector.get_global_rect(), get_viewport().get_visible_rect().size])
	var img := get_viewport().get_texture().get_image()
	img.save_png(path)
	print("screenshot: ", path)
	get_tree().quit()


func _parse(argv: PackedStringArray) -> Dictionary:
	var out := {}
	var i := 0
	while i < argv.size():
		if argv[i].begins_with("--"):
			var key := argv[i].substr(2)
			if i + 1 < argv.size() and not argv[i + 1].begins_with("--"):
				out[key] = argv[i + 1]
				i += 1
			else:
				out[key] = "1"
		i += 1
	return out
