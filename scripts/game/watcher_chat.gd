class_name WatcherChat
extends Node
## Talking to colonists. The player is "the Watcher", a voice only colonists can hear.
##
## send() posts the colonist's current snapshot + chat history to the brain server's /chat
## (LFM3 via llama.cpp). When a conversation ends (another colonist is selected, the panel is
## closed, or every MEMORY_EVERY messages), the unsaved part of the transcript goes to /remember,
## which compresses it into one first-person sentence. That sentence becomes a high-importance
## memory, and memories are part of the state text Laya reads when it decides what to do.

signal reply_received(c: Colonist, text: String)
signal memory_saved(c: Colonist, text: String)
signal failed(c: Colonist, message: String)
signal busy_changed(c: Colonist, busy: bool)
signal request_heard(c: Colonist, action_id: String)

const MEMORY_EVERY := 6          # messages (both sides) before a memory is saved mid-conversation
const MEMORY_IMPORTANCE := 7.0   # same league as "became a couple" (8) and "was born" (5)
const MAX_HISTORY := 40

var sim: Sim
var base_url: String = "http://127.0.0.1:8765"
var histories: Dictionary = {}   # colonist id -> [{role, content}]
var _saved_upto: Dictionary = {} # colonist id -> index into history already turned into memory
var _busy: Dictionary = {}       # colonist id -> true while waiting for a reply
var _asked_now: Dictionary = {}  # colonist id -> {action: true} requested via /intent since the last memory


func _init(p_sim: Sim, url: String = "") -> void:
	sim = p_sim
	if url != "":
		base_url = url.trim_suffix("/")


func history(c: Colonist) -> Array:
	return histories.get(c.id, [])


func is_busy(c: Colonist) -> bool:
	return _busy.get(c.id, false)


func send(c: Colonist, message: String) -> void:
	message = message.strip_edges()
	if message == "" or c == null or not c.alive() or is_busy(c):
		return
	var h: Array = histories.get(c.id, [])
	var body := {"colonist": snapshot(c), "history": h.slice(maxi(0, h.size() - 10)), "message": message}
	h.append({"role": "user", "content": message})
	histories[c.id] = h
	# Asked for a job? It becomes a request right away (not only when the conversation ends).
	_post("/intent", {"colonist": c.name, "message": message}, func(ok: bool, data: Dictionary) -> void:
		var req := String(data.get("request", "none")) if ok else "none"
		if req != "none" and sim.content.actions_by_id.has(req) and c.alive():
			var asked: Dictionary = _asked_now.get(c.id, {})
			asked[req] = true
			_asked_now[c.id] = asked
			sim.add_request(c, "the Watcher", req, 0.8, true)
			request_heard.emit(c, req))
	_set_busy(c, true)
	_post("/chat", body, func(ok: bool, data: Dictionary) -> void:
		_set_busy(c, false)
		var reply := String(data.get("reply", "")) if ok else ""
		if reply == "":
			# Drop the unanswered message so the history stays a clean back-and-forth.
			h.pop_back()
			failed.emit(c, String(data.get("error", "no reply")) if ok else String(data.get("error", "brain server unreachable")))
			return
		h.append({"role": "assistant", "content": reply})
		while h.size() > MAX_HISTORY:
			h.pop_front()
			_saved_upto[c.id] = maxi(0, int(_saved_upto.get(c.id, 0)) - 1)
		reply_received.emit(c, reply)
		if h.size() - int(_saved_upto.get(c.id, 0)) >= MEMORY_EVERY:
			save_memory(c))


## Turn whatever hasn't been remembered yet into one memory. Safe to call any time.
func save_memory(c: Colonist) -> void:
	if c == null:
		return
	var h: Array = histories.get(c.id, [])
	var from := int(_saved_upto.get(c.id, 0))
	var part := h.slice(from)
	var has_reply := part.any(func(m: Dictionary) -> bool: return m["role"] == "assistant")
	if not has_reply:
		return
	_saved_upto[c.id] = h.size()
	_post("/remember", {"colonist": snapshot(c), "transcript": part}, func(ok: bool, data: Dictionary) -> void:
		var text := String(data.get("memory", "")).strip_edges() if ok else ""
		if text == "":
			_saved_upto[c.id] = from  # try again next time
			failed.emit(c, "couldn't save a memory: %s" % data.get("error", "no reply"))
			return
		if not c.alive():
			return
		sim.add_memory(c, text, MEMORY_IMPORTANCE)
		c.memories[-1]["source"] = "watcher"
		var req := String(data.get("request", "none"))
		var asked: Dictionary = _asked_now.get(c.id, {})
		_asked_now.erase(c.id)
		# Usually already registered message-by-message (/intent); only add one that was missed.
		if req != "none" and sim.content.actions_by_id.has(req) and not asked.has(req):
			sim.add_request(c, "the Watcher", req, 0.8)   # the voice they hear is fairly persuasive
		memory_saved.emit(c, text))


## What the LLM is told about the colonist (see persona() in brain_server/server.py).
func snapshot(c: Colonist) -> Dictionary:
	var need_words: Array = []
	for need_id in sim.content.needs:
		need_words.append(sim.need_word(c, need_id))
	var traits: Array = []
	for t in c.traits:
		traits.append(String(sim.content.traits.get(t, {}).get("text", t)))
	if c.role != "":
		traits.append("the colony's %s" % sim.role_name(c.role))
	var mems: Array = []
	for m in c.top_memories(sim.clock, 8):
		mems.append(String(m["text"]))
	var why := ""
	if not c.last_decision.is_empty():
		var d: Dictionary = c.last_decision["decision"]
		for o in c.last_decision["ctx"]["options"]:
			if o["id"] == d.get("action", ""):
				why = "You recently decided to %s%s." % [o["label"], "" if c.last_decision["began"] else ", but couldn't"]
	var family: PackedStringArray = []
	if c.partner_id >= 0 and sim.colonists.has(c.partner_id):
		family.append("your partner is %s" % (sim.colonists[c.partner_id] as Colonist).name)
	for pid in c.parent_ids:
		var p := sim.find_colonist(int(pid))
		if p != null:
			family.append("%s is your parent%s" % [p.name, "" if p.alive() else " (dead)"])
	for o: Colonist in sim.colonists.values():
		if o.parent_ids.has(c.id):
			family.append("%s is your child" % o.name)
	var st: PackedStringArray = []
	for r in sim.stock:
		st.append("%d %s" % [int(sim.stock[r]), r])
	var colony := "Day %d, %s. %d people. Shared stock: %s. %s" % [
		sim.day(), "night" if sim.is_night() else "day", sim.alive_count(), ", ".join(st),
		("Family: " + "; ".join(family) + ".") if not family.is_empty() else ""]
	return {
		"name": c.name, "stage": c.life_stage(), "traits": traits, "need_words": need_words,
		"action": c.action.describe() if c.action != null else ("deciding what to do" if c.state == "thinking" else "idle"),
		"why": why, "colony": colony, "memories": mems,
	}


func _set_busy(c: Colonist, v: bool) -> void:
	_busy[c.id] = v
	busy_changed.emit(c, v)


func _post(path: String, body: Dictionary, cb: Callable) -> void:
	var req := HTTPRequest.new()
	req.timeout = 90.0
	add_child(req)
	req.request_completed.connect(func(result: int, code: int, _h: PackedStringArray, raw: PackedByteArray) -> void:
		req.queue_free()
		if result != HTTPRequest.RESULT_SUCCESS or code != 200:
			cb.call(false, {"error": "HTTP %d" % code if result == HTTPRequest.RESULT_SUCCESS else LayaClient._result_text(result)})
			return
		var data = JSON.parse_string(raw.get_string_from_utf8())
		cb.call(typeof(data) == TYPE_DICTIONARY, data if typeof(data) == TYPE_DICTIONARY else {"error": "bad response"}))
	var err := req.request(base_url + path, ["Content-Type: application/json"], HTTPClient.METHOD_POST, JSON.stringify(body))
	if err != OK:
		req.queue_free()
		cb.call(false, {"error": "request error %d" % err})
