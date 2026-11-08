class_name LayaClient
extends Node
## HTTP client for the brain server's /decide endpoint (brain_server/server.py).
## Brains queue decision contexts here; the client sends them in small batches, keeps a few
## requests in flight, and hands each result back to its callback.
##
## If the server is down, requests stay queued and are retried every few seconds. Colonists
## just keep "thinking" (the runner slows the sim while they wait), and `status()` says why.

signal status_changed(online: bool, message: String)

const BATCH_MAX := 4
const IN_FLIGHT := 2
const TIMEOUT_S := 20.0
const RETRY_S := 3.0

var base_url: String = "http://127.0.0.1:8765"
var online: bool = false
var last_error: String = ""
var sent: int = 0
var answered: int = 0
var errors: int = 0
var avg_latency_ms: float = 0.0   # server-side Laya time per decision (moving average)

var _queue: Array[Dictionary] = []          # [{ctx, done}]
var _free: Array[HTTPRequest] = []
var _busy: Dictionary = {}                  # HTTPRequest -> Array of queued items
var _retry_at: float = 0.0
var _health := HTTPRequest.new()


func _init(url: String = "") -> void:
	if url != "":
		base_url = url.trim_suffix("/")


func _ready() -> void:
	for i in IN_FLIGHT:
		var h := HTTPRequest.new()
		h.timeout = TIMEOUT_S
		h.use_threads = true
		add_child(h)
		h.request_completed.connect(_on_completed.bind(h))
		_free.append(h)
	_health.timeout = 3.0
	add_child(_health)
	_health.request_completed.connect(_on_health)
	check_health()


func check_health() -> void:
	if _health.get_http_client_status() == HTTPClient.STATUS_DISCONNECTED:
		_health.request(base_url + "/health")


func decide(ctx: Dictionary, done: Callable) -> void:
	_queue.append({"ctx": ctx, "done": done})


func pending() -> int:
	var n := _queue.size()
	for items in _busy.values():
		n += (items as Array).size()
	return n


func status() -> String:
	if online:
		return "Laya online · %d decisions · %.0f ms each" % [answered, avg_latency_ms]
	if last_error != "":
		return "Laya offline: %s (run brain_server/run.sh)" % last_error
	return "connecting to Laya..."


func _process(_delta: float) -> void:
	if _queue.is_empty() or _free.is_empty():
		return
	if Time.get_ticks_msec() / 1000.0 < _retry_at:
		return
	var h: HTTPRequest = _free.pop_back()
	var items: Array = []
	while not _queue.is_empty() and items.size() < BATCH_MAX:
		items.append(_queue.pop_front())
	var reqs := []
	for it in items:
		reqs.append(it["ctx"])
	var err := h.request(base_url + "/decide", ["Content-Type: application/json"],
		HTTPClient.METHOD_POST, JSON.stringify({"requests": reqs}))
	if err != OK:
		_fail(items, h, "request error %d" % err)
		return
	_busy[h] = items
	sent += items.size()


func _on_completed(result: int, code: int, _headers: PackedStringArray, body: PackedByteArray, h: HTTPRequest) -> void:
	var items: Array = _busy.get(h, [])
	_busy.erase(h)
	if result != HTTPRequest.RESULT_SUCCESS or code != 200:
		_fail(items, h, "HTTP %d / result %d" % [code, result] if result == HTTPRequest.RESULT_SUCCESS else _result_text(result))
		return
	var data = JSON.parse_string(body.get_string_from_utf8())
	if typeof(data) != TYPE_DICTIONARY or not data.has("results"):
		_fail(items, h, "bad response")
		return
	_free.append(h)
	_set_online(true, "")
	var by_id := {}
	for r in data["results"]:
		by_id[int(r.get("request_id", -1))] = r
	for it in items:
		var rid := int(it["ctx"]["request_id"])
		var r: Dictionary = by_id.get(rid, {"error": "missing result", "action": ""})
		if r.has("error"):
			errors += 1
			push_warning("Laya error for request %d: %s" % [rid, r["error"]])
		else:
			answered += 1
			avg_latency_ms = lerpf(avg_latency_ms, float(r.get("latency_ms", 0.0)), 0.1 if answered > 1 else 1.0)
		(it["done"] as Callable).call(r)


func _fail(items: Array, h: HTTPRequest, why: String) -> void:
	# Put the work back at the front of the queue and wait before trying again.
	errors += 1
	for i in range(items.size() - 1, -1, -1):
		_queue.push_front(items[i])
	_free.append(h)
	_retry_at = Time.get_ticks_msec() / 1000.0 + RETRY_S
	_set_online(false, why)


func _on_health(result: int, code: int, _headers: PackedStringArray, body: PackedByteArray) -> void:
	if result == HTTPRequest.RESULT_SUCCESS and code == 200:
		var data = JSON.parse_string(body.get_string_from_utf8())
		var loaded: bool = typeof(data) == TYPE_DICTIONARY and bool(data.get("laya_loaded", false))
		_set_online(loaded, "" if loaded else "model still loading")
	else:
		_set_online(false, _result_text(result))


func _set_online(v: bool, why: String) -> void:
	var changed := v != online or why != last_error
	online = v
	last_error = why
	if changed:
		status_changed.emit(online, status())


static func _result_text(result: int) -> String:
	match result:
		HTTPRequest.RESULT_CANT_CONNECT, HTTPRequest.RESULT_CONNECTION_ERROR:
			return "can't connect"
		HTTPRequest.RESULT_TIMEOUT:
			return "timed out"
		_:
			return "network error %d" % result
