class_name Content
extends RefCounted
## All game content, loaded from res://data/*.json.
## To add a building, need, trait, node, or action: edit the JSON. Code reads it generically.

const DATA_DIR := "res://data/"

static var _instance: Content

var config: Dictionary = {}
var resources: Dictionary = {}
var needs: Dictionary = {}
var nodes: Dictionary = {}
var buildings: Dictionary = {}
var tiers: Array = []
var actions: Array = []            # ordered list of action defs
var actions_by_id: Dictionary = {}
var traits: Dictionary = {}
var names: Array = []
var seasons: Dictionary = {}


static func get_instance() -> Content:
	if _instance == null:
		_instance = Content.new()
		_instance.load_all()
	return _instance


## Forces a reload (e.g. after editing data while the editor is open).
static func reload() -> Content:
	_instance = null
	return get_instance()


func load_all() -> void:
	config = _load("config.json")
	resources = _strip_doc(_load("resources.json"))
	needs = _strip_doc(_load("needs.json"))
	nodes = _strip_doc(_load("nodes.json"))
	buildings = _strip_doc(_load("buildings.json"))
	tiers = _load("tiers.json")
	actions = _load("actions.json")
	traits = _strip_doc(_load("traits.json"))
	names = _load("names.json")
	seasons = _load("seasons.json")
	actions_by_id.clear()
	for a in actions:
		actions_by_id[a["id"]] = a


func cfg(path: String, default: Variant = null) -> Variant:
	## Dotted lookup into config.json, e.g. cfg("decision.brain").
	var cur: Variant = config
	for part in path.split("."):
		if cur is Dictionary and (cur as Dictionary).has(part):
			cur = cur[part]
		else:
			return default
	return cur


func _load(file: String) -> Variant:
	var path := DATA_DIR + file
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		push_error("Content: cannot open %s" % path)
		return {}
	var data: Variant = JSON.parse_string(f.get_as_text())
	if data == null:
		push_error("Content: bad JSON in %s" % path)
		return {}
	return data


func _strip_doc(d: Variant) -> Dictionary:
	var out: Dictionary = {}
	if d is Dictionary:
		for k in d:
			if not String(k).begins_with("_"):
				out[k] = d[k]
	return out
