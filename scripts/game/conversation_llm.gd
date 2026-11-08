class_name ConversationLLM
extends Node
## Lets the chat model write colonists' conversations. Takes over Sim.conversation_handler: each
## conversation is sent to the brain server's /converse, which writes the dialogue from what each
## speaker believes and feels, and extracts what they told each other (and any requests). The sim
## then applies exactly that, so news spreads in the colonists' own words, mistakes included.
##
## One conversation at a time. If the server is down, or a conversation has waited longer than
## MAX_WAIT_MIN of sim time (e.g. at 10x speed), it falls back to the sim's quick symbolic exchange.

const MAX_WAIT_MIN := 120.0
const MAX_QUEUE := 4          # beyond this (high speed / busy), the oldest waiting talk goes quick
var print_talks := OS.get_cmdline_user_args().has("--print-talks")

var sim: Sim
var is_watched: Callable          # (conv) -> bool, set by the game: is this talk on screen?
var base_url := "http://127.0.0.1:8765"
var queue: Array = []
var busy := false
var done := 0
var fallbacks := 0
var _req := HTTPRequest.new()
var _current: Dictionary = {}


func _init(p_sim: Sim, url: String = "") -> void:
	sim = p_sim
	if url != "":
		base_url = url.trim_suffix("/")


func _ready() -> void:
	_req.timeout = 60.0
	add_child(_req)
	_req.request_completed.connect(_on_done)
	sim.conversation_handler = func(conv: Dictionary) -> bool:
		var mode := String(GameSettings.get_value("talk_mode"))
		if mode == "quick" or (mode == "near" and is_watched.is_valid() and not bool(is_watched.call(conv))):
			fallbacks += 1
			return false   # the sim does its quick fact exchange
		queue.append(conv)
		while queue.size() > MAX_QUEUE:
			_fallback(queue.pop_front())
		return true


func _process(_delta: float) -> void:
	if busy or queue.is_empty():
		return
	var conv: Dictionary = queue.pop_front()
	if sim.clock - float(conv["t"]) > MAX_WAIT_MIN:
		_fallback(conv)
		return
	_current = conv
	var a: Colonist = conv["a"]
	var b: Colonist = conv["b"]
	var topic := ""
	if conv["topic"] != "":
		topic = String(sim.content.actions_by_id.get(conv["topic"], {}).get("label", conv["topic"]))
	var body := {"a": _snapshot(a), "b": _snapshot(b), "intent": conv["intent"], "topic": topic, "topic_id": conv["topic"],
		"relation": sim.describer.relation_text(sim, a, b).split(",")[0]}
	busy = true
	if _req.request(base_url + "/converse", ["Content-Type: application/json"], HTTPClient.METHOD_POST, JSON.stringify(body)) != OK:
		busy = false
		_fallback(conv)


func _snapshot(c: Colonist) -> Dictionary:
	var traits: Array = []
	for t in c.traits:
		traits.append(String(sim.content.traits.get(t, {}).get("text", t)))
	var needs: Array = []
	for n in sim.content.needs:
		needs.append(sim.need_word(c, n))
	var beliefs: Array = []
	for k in Knowledge.STOCK_KEYS:
		beliefs.append(c.knowledge.phrase(k, sim.clock))
	var mems: Array = []
	for m in c.top_memories(sim.clock, 3):
		mems.append(String(m["text"]))
	return {"name": c.name, "traits": traits, "need_words": needs, "beliefs": beliefs, "memories": mems,
		"action": c.action.describe() if c.action != null else "idle"}


func _on_done(result: int, code: int, _h: PackedStringArray, raw: PackedByteArray) -> void:
	busy = false
	var conv := _current
	_current = {}
	if conv.is_empty():
		return
	var data = JSON.parse_string(raw.get_string_from_utf8()) if result == HTTPRequest.RESULT_SUCCESS and code == 200 else null
	if typeof(data) != TYPE_DICTIONARY or (data.get("lines", []) as Array).is_empty():
		_fallback(conv)
		return
	# Hearsay keeps the time the speaker originally learned it; made-up numbers count as "now".
	var by_name := {(conv["a"] as Colonist).name: conv["a"], (conv["b"] as Colonist).name: conv["b"]}
	for f in data.get("facts", []):
		var sp: Colonist = by_name.get(f.get("from", ""))
		f["t_source"] = float(sp.knowledge.facts[f["key"]]["t"]) if sp != null and sp.knowledge.facts.has(f["key"]) else sim.clock
	data["llm"] = true
	done += 1
	if print_talks:
		print("TALK %s & %s (%s): %s" % [(conv["a"] as Colonist).name, (conv["b"] as Colonist).name, conv["intent"],
			" | ".join((data["lines"] as Array).map(func(l) -> String: return "%s: %s" % [l["who"], l["text"]]))])
		print("     facts %s requests %s" % [JSON.stringify(data.get("facts", [])), JSON.stringify(data.get("requests", []))])
	sim.apply_conversation(conv, data)


func _fallback(conv: Dictionary) -> void:
	fallbacks += 1
	sim.apply_conversation(conv, sim.symbolic_conversation(conv))


func status() -> String:
	return "talks: %d written, %d quick, %d waiting" % [done, fallbacks, queue.size() + (1 if busy else 0)]
