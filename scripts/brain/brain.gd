class_name Brain
extends RefCounted
## Plug-in point for decision making. Implementations: UtilityBrain (hand-made baseline),
## LayaBrain (Laya over HTTP), and anything later (fine-tuned Laya, scripted test brains...).
##
## decide() receives a context built by Sim.build_context():
##   {request_id, colonist_id, state_text, options:[{id,label,description}],
##    build_options:[...], social_options:[...], ...}
## and must eventually call done.call(decision) exactly once, where decision is:
##   {action: <option id>, build_what: <id or "">, with_whom: <colonist id or -1>,
##    probs: {option id: p}, build_probs: {...}, social_probs: {...},
##    brain: <id>, latency_ms: <float>}
## It may call done synchronously (UtilityBrain) or later (LayaBrain).

var id: String = "base"


func decide(_ctx: Dictionary, _sim: Sim, _done: Callable) -> void:
	push_error("Brain.decide() not implemented in %s" % id)


## Shared helpers --------------------------------------------------------------

static func argmax(probs: Dictionary) -> String:
	var best := ""
	var best_p := -INF
	for k in probs:
		if float(probs[k]) > best_p:
			best_p = float(probs[k])
			best = k
	return best


static func sample(probs: Dictionary, rng: RandomNumberGenerator) -> String:
	var r := rng.randf()
	var acc := 0.0
	var last := ""
	for k in probs:
		acc += float(probs[k])
		last = k
		if r <= acc:
			return k
	return last


static func softmax(scores: Dictionary, temperature: float) -> Dictionary:
	var mx := -INF
	for k in scores:
		mx = maxf(mx, float(scores[k]))
	var total := 0.0
	var out := {}
	for k in scores:
		var e := exp((float(scores[k]) - mx) / maxf(temperature, 0.0001))
		out[k] = e
		total += e
	for k in out:
		out[k] = float(out[k]) / total
	return out
