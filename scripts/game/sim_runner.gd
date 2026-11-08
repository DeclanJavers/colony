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
var speed_index: int = 1
var minutes_per_second: float = 12.0
var lag_paused: bool = false     # true while waiting on a slow brain (sim slows rather than skips)
var _acc: float = 0.0
var _last_running_index: int = 1
var _max_lag: float = 15.0


func start(p_seed: int) -> void:
	var content := Content.get_instance()
	sim = Sim.new()
	sim.register_brain(UtilityBrain.new(), true)
	sim.setup(p_seed)
	minutes_per_second = 1440.0 / float(content.cfg("real_seconds_per_day", 120.0))
	_max_lag = float(content.cfg("decision.max_lag_minutes", 15))
	log = DecisionLog.new()
	log.attach(sim, "game")
	started.emit(sim)


## Fast-forward without rendering (for testing or to skip the early game).
func prewarm(days: float) -> void:
	for i in int(days * 1440.0):
		sim.tick(1.0)


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
