class_name SimRunner
extends Node
## Runs a Sim inside the game: advances it at the chosen speed, owns the brains and the
## decision log. Views and UI read `sim` and listen to `sim.events`.

signal speed_changed(speed: int)
signal started(sim: Sim)

const SPEEDS: Array[int] = [0, 1, 3, 10]
const MAX_TICKS_PER_FRAME := 400

var sim: Sim
var log: DecisionLog
var brain_id: String = "laya"        # which brain runs the colony
var laya: LayaClient                 # set when brain_id == "laya"
var loaded := false                  # true when this run continued from a save
var history: Array = []
var chronicle: Array = []            # everything the event feed showed: {t, text (BBCode with links)}              # hourly snapshots {clock, stock, pop}, newest last (3 days kept)
var speed_index: int = 1
var minutes_per_second: float = 12.0
var lag_paused: bool = false     # true while waiting on a slow brain (sim slows rather than skips)
var _acc: float = 0.0
var _last_running_index: int = 1
var _max_lag: float = 15.0


## brain_override: "laya" or "utility"; empty = data/config.json decision.brain.
## saved: a SaveGame dictionary to continue from (its seed rebuilds the same map).
func start(p_seed: int, brain_override: String = "", saved: Dictionary = {}) -> void:
	if not saved.is_empty():
		p_seed = int(saved["seed"])
	var content := Content.get_instance()
	sim = Sim.new()
	brain_id = brain_override if brain_override != "" else String(content.cfg("decision.brain", "laya"))
	var pick := String(content.cfg("decision.pick", "argmax"))
	sim.register_brain(UtilityBrain.new(pick), brain_id == "utility")
	if brain_id == "laya":
		laya = LayaClient.new(String(content.cfg("brain_server_url", "http://127.0.0.1:8765")))
		laya.name = "LayaClient"
		add_child(laya)
		sim.register_brain(LayaBrain.new(laya, pick), true)
	sim.setup(p_seed)
	if not saved.is_empty():
		SaveGame.apply(sim, saved)
		loaded = true
		history = saved.get("history", []).duplicate(true)
		chronicle = saved.get("chronicle", []).duplicate(true)
	minutes_per_second = 1440.0 / float(content.cfg("real_seconds_per_day", 120.0))
	_max_lag = float(content.cfg("decision.max_lag_minutes", 15))
	log = DecisionLog.new()
	log.attach(sim, "game")
	sim.events.hour_passed.connect(func(_d: int, _h: int) -> void:
		history.append({"clock": sim.clock, "stock": sim.stock.duplicate(), "pop": sim.alive_count()})
		if history.size() > 72:
			history.pop_front())
	started.emit(sim)


## Fast-forward without rendering (for testing or to skip the early game).
## Laya answers asynchronously, so a synchronous skip-ahead can't wait for it: prewarm always
## runs on the baseline brain (its decisions are logged as brain "utility").
func prewarm(days: float) -> void:
	var keep := sim.default_brain_id
	sim.default_brain_id = "utility"
	for i in int(days * 1440.0):
		sim.tick(1.0)
	sim.default_brain_id = keep


## The snapshot from about `hours` ago (or the oldest one), {} if none yet.
func snapshot_hours_ago(hours: float) -> Dictionary:
	var want := sim.clock - hours * 60.0
	for snap in history:
		if float(snap["clock"]) >= want:
			return snap
	return {} if history.is_empty() else history[0]


func save_game() -> Error:
	return SaveGame.save(sim, history, SaveGame.PATH, chronicle)


## Reload the scene and continue from the save (a clean restart of view, HUD and brains).
func load_game() -> bool:
	var data := SaveGame.read()
	if data.is_empty():
		return false
	SaveGame.pending = data
	get_tree().reload_current_scene()
	return true


func speed() -> int:
	return SPEEDS[speed_index]


func set_speed_index(i: int) -> void:
	speed_index = clampi(i, 0, SPEEDS.size() - 1)
	if speed_index > 0:
		_last_running_index = speed_index
	speed_changed.emit(speed())


func toggle_pause() -> void:
	set_speed_index(_last_running_index if speed_index == 0 else 0)


func _process(delta: float) -> void:
	if sim == null or speed() == 0:
		return
	_acc += delta * minutes_per_second * speed()
	var n := 0
	lag_paused = false
	while _acc >= 1.0 and n < MAX_TICKS_PER_FRAME:
		if sim.oldest_thinking_minutes() > _max_lag:
			lag_paused = true
			_acc = minf(_acc, 1.0)
			break
		sim.tick(1.0)
		_acc -= 1.0
		n += 1
	if n >= MAX_TICKS_PER_FRAME:
		_acc = 0.0  # can't keep up: drop the backlog instead of spiralling


func _exit_tree() -> void:
	if log != null:
		log.close()
