class_name LayaBrain
extends Brain
## Laya decides. Each colonist's context goes to the brain server through a LayaClient
## (batched HTTP), and Laya's answer comes back asynchronously. No rules sit on top of it:
## whatever Laya picks is what the colonist tries to do, and the decision log records it.
##
## pick = "argmax" takes Laya's top choice; "sample" draws from its probabilities (more variety).

var client: LayaClient
var pick: String = "argmax"
var _rng := RandomNumberGenerator.new()


func _init(p_client: LayaClient, p_pick: String = "argmax") -> void:
	id = "laya"
	client = p_client
	pick = p_pick
	_rng.randomize()


func decide(ctx: Dictionary, _sim: Sim, done: Callable) -> void:
	var t0 := Time.get_ticks_msec()
	client.decide(ctx, func(r: Dictionary) -> void: done.call(_to_decision(r, t0)))


func _to_decision(r: Dictionary, t0: int) -> Dictionary:
	var probs: Dictionary = r.get("probs", {})
	var build_probs: Dictionary = r.get("build_probs", {})
	var social_probs: Dictionary = r.get("social_probs", {})
	var action := String(r.get("action", ""))
	var build_what := String(r.get("build_what", ""))
	var with_whom := int(r.get("with_whom", -1))
	if pick == "sample":
		if not probs.is_empty():
			action = Brain.sample(probs, _rng)
		if not build_probs.is_empty():
			build_what = Brain.sample(build_probs, _rng)
		if not social_probs.is_empty():
			with_whom = int(Brain.sample(social_probs, _rng))
	return {
		"action": action, "build_what": build_what, "with_whom": with_whom,
		"probs": probs, "build_probs": build_probs, "social_probs": social_probs,
		"brain": id, "model": r.get("model", ""), "error": r.get("error", ""),
		"latency_ms": float(r.get("latency_ms", 0.0)),          # Laya's own time
		"wait_ms": float(Time.get_ticks_msec() - t0),            # queue + HTTP + Laya
	}
