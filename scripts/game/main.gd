extends Node3D
## Game entry point: wires SimRunner + WorldView + IsoCamera + DayNight + Hud.
## Optional command-line args (after `--`):
##   --seed N            map/colony seed (default from data/config.json)
##   --prewarm D         fast-forward D days before showing (skip the early game / test visuals)
##   --speed I           0 pause, 1 = 1x, 2 = 3x, 3 = 10x
##   --shot path.png     save a screenshot after --shot-delay seconds, then quit (for automated checks)
##   --time H            jump to hour H of the current day before showing (e.g. 21 for night)
##   --zoom S            camera ortho size
##   --brain B           laya (default, from data/config.json) or utility (hand-made baseline)

var runner := SimRunner.new()
var view := WorldView.new()
var cam := IsoCamera.new()
var day_night := DayNight.new()
var hud := Hud.new()


func _ready() -> void:
	var args := _parse(OS.get_cmdline_user_args())
	var content := Content.get_instance()
	var seed_v := int(args.get("seed", str(content.cfg("default_seed", 1234))))

	add_child(runner)
	runner.start(seed_v, String(args.get("brain", "")))
	if args.has("prewarm"):
		runner.prewarm(float(args["prewarm"]))
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
	cam.focus(Vector3(runner.sim.map.center.x, 0, runner.sim.map.center.y))
	if args.has("zoom"):
		cam._size_target = float(args["zoom"])
		cam.camera.size = cam._size_target
	add_child(view)
	view.bind(runner.sim, cam.camera)
	add_child(hud)
	hud.bind(runner, view)
	if args.has("select"):
		var first: Colonist = runner.sim.colonists.values()[0]
		view.select(first)

	if args.has("shot"):
		_take_shot(String(args["shot"]), float(args.get("shot-delay", "2.0")))


func _take_shot(path: String, delay: float) -> void:
	await get_tree().create_timer(delay).timeout
	await RenderingServer.frame_post_draw
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
