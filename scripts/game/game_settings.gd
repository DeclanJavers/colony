class_name GameSettings
extends RefCounted
## Player preferences, saved to user://settings.cfg. Read anywhere with GameSettings.get_value().
##   talk_mode: "all"   every conversation is written by the chat model (falls back when it can't keep up)
##              "near"  only conversations you can see (near the camera or with someone selected)
##              "quick" never use the chat model: colonists just exchange facts
##   names:     "auto" (when zoomed in / hovered / selected) | "always" | "never"
##   bubbles:   show speech bubbles
##   pixels:    "fine" | "chunky" | "off" (how big the pixel-art pixels are; see docs/STYLE.md)

signal changed(key: String, value: Variant)

const PATH := "user://settings.cfg"
const DEFAULTS := {"talk_mode": "all", "names": "auto", "bubbles": true, "pixels": "fine"}

static var _inst: GameSettings
var values: Dictionary = DEFAULTS.duplicate()


static func instance() -> GameSettings:
	if _inst == null:
		_inst = GameSettings.new()
		_inst._load()
	return _inst


static func get_value(key: String) -> Variant:
	return instance().values.get(key, DEFAULTS.get(key))


static func set_value(key: String, value: Variant) -> void:
	var s := instance()
	s.values[key] = value
	s._save()
	s.changed.emit(key, value)


func _load() -> void:
	var cf := ConfigFile.new()
	if cf.load(PATH) != OK:
		return
	for k in DEFAULTS:
		values[k] = cf.get_value("game", k, DEFAULTS[k])


func _save() -> void:
	var cf := ConfigFile.new()
	for k in values:
		cf.set_value("game", k, values[k])
	cf.save(PATH)
