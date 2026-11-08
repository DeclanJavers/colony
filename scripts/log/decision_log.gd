class_name DecisionLog
extends RefCounted
## Writes every decision + its outcome to logs/decisions/<run_id>.jsonl for fine-tuning later.
## A line is written once the outcome is known: N game-hours later, or when the colonist dies.

var sim: Sim
var run_id: String = ""
var path: String = ""
var _file: FileAccess
var _pending: Array = []
var _after_min: float = 360.0
var written: int = 0


func attach(p_sim: Sim, label: String = "") -> void:
	sim = p_sim
	_after_min = float(sim.content.cfg("log.outcome_after_hours", 6)) * 60.0
	var stamp := Time.get_datetime_string_from_system(false, true).replace(":", "-").replace(" ", "_")
	run_id = "%s_seed%d%s" % [stamp, sim.seed_value, ("_" + label) if label != "" else ""]
	var dir := ProjectSettings.globalize_path(str(sim.content.cfg("log.dir", "res://logs/decisions")))
	DirAccess.make_dir_recursive_absolute(dir)
	path = dir.path_join(run_id + ".jsonl")
	_file = FileAccess.open(path, FileAccess.WRITE)
	sim.events.decision_made.connect(_on_decision)
	sim.events.hour_passed.connect(_on_hour)
	sim.events.colonist_died.connect(_on_died)


func _on_decision(c: Colonist, ctx: Dictionary, decision: Dictionary, began: bool) -> void:
	_pending.append({
		"run_id": run_id, "seed": sim.seed_value, "t": sim.clock, "day": sim.day(), "hour": sim.hour(),
		"colonist_id": c.id, "name": c.name, "stage": c.life_stage(),
		"brain": decision.get("brain", "?"), "describer_version": ctx.get("describer_version", ""),
		"state_text": ctx["state_text"],
		"options": ctx["options"], "build_options": ctx["build_options"], "social_options": ctx["social_options"],
		"probs": decision.get("probs", {}), "build_probs": decision.get("build_probs", {}),
		"social_probs": decision.get("social_probs", {}),
		"chosen": decision.get("action", ""), "build_what": decision.get("build_what", ""),
		"with_whom": decision.get("with_whom", -1), "began": began,
		"latency_ms": decision.get("latency_ms", 0.0),
		"needs_before": c.needs.duplicate(),
		"outcome": null, "label": null,
	})


func _on_hour(_d: int, _h: int) -> void:
	var keep := []
	for e in _pending:
		if sim.clock - float(e["t"]) >= _after_min:
			var c := sim.colonists.get(int(e["colonist_id"])) as Colonist
			e["outcome"] = {"hours": (sim.clock - float(e["t"])) / 60.0, "alive": c != null,
				"needs_after": c.needs.duplicate() if c != null else {}}
			_write(e)
		else:
			keep.append(e)
	_pending = keep
	if _file != null:
		_file.flush()


func _on_died(c: Colonist, cause: String) -> void:
	var keep := []
	for e in _pending:
		if int(e["colonist_id"]) == c.id:
			e["outcome"] = {"hours": (sim.clock - float(e["t"])) / 60.0, "alive": false, "died_of": cause}
			_write(e)
		else:
			keep.append(e)
	_pending = keep


func close() -> void:
	for e in _pending:
		var c := sim.colonists.get(int(e["colonist_id"])) as Colonist
		e["outcome"] = {"hours": (sim.clock - float(e["t"])) / 60.0, "alive": c != null, "partial": true,
			"needs_after": c.needs.duplicate() if c != null else {}}
		_write(e)
	_pending.clear()
	if _file != null:
		_file.close()
		_file = null


func _write(e: Dictionary) -> void:
	if _file != null:
		_file.store_line(JSON.stringify(e))
		written += 1
