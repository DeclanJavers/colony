class_name Overhead
extends Node3D
## What floats above one colonist: a round icon badge for what they're doing (assets/icons, made by
## tools/icons/make_icons.py) and their name. Danger (very thirsty, starving, exhausted) replaces
## the activity with a red badge that gently pulses; "…" shows while their brain is thinking.
## Names only show when zoomed in, hovered or selected, so a busy colony stays calm to look at.

const ICON_DIR := "res://assets/icons/"
const DANGER := [["thirst", 15.0, "danger_thirst"], ["hunger", 15.0, "danger_hunger"], ["energy", 8.0, "danger_energy"]]

static var _tex: Dictionary = {}

var badge := Sprite3D.new()
var name_label := Label3D.new()
var bubble := Label3D.new()      # what they're saying right now (conversations)
var _bubble_tw: Tween
var _key := ""
var _t := 0.0


func _ready() -> void:
	badge.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	badge.fixed_size = true
	badge.no_depth_test = true
	badge.render_priority = 2
	badge.pixel_size = 0.0006
	badge.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
	add_child(badge)
	name_label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	name_label.fixed_size = true
	name_label.no_depth_test = true
	name_label.render_priority = 1
	name_label.pixel_size = 0.0009
	name_label.font_size = 32
	name_label.outline_size = 10
	name_label.outline_modulate = Color(0, 0, 0, 0.75)
	name_label.position.y = -0.0
	add_child(name_label)
	bubble.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	bubble.fixed_size = true
	bubble.no_depth_test = true
	bubble.render_priority = 3
	bubble.pixel_size = 0.001
	bubble.font_size = 28
	bubble.outline_size = 9
	bubble.outline_modulate = Color(0.08, 0.09, 0.12, 0.9)
	bubble.modulate = Color(1, 0.97, 0.88, 0.0)
	bubble.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	bubble.width = 520
	bubble.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	bubble.visible = false
	add_child(bubble)


## Show a line of speech for `seconds`, fading in and out.
func say(text: String, seconds: float) -> void:
	bubble.text = "\u201c%s\u201d" % text
	bubble.visible = true
	if _bubble_tw != null:
		_bubble_tw.kill()
	_bubble_tw = create_tween()
	_bubble_tw.tween_property(bubble, "modulate:a", 1.0, 0.25)
	_bubble_tw.tween_interval(maxf(0.5, seconds - 0.5))
	_bubble_tw.tween_property(bubble, "modulate:a", 0.0, 0.25)
	_bubble_tw.tween_callback(func() -> void: bubble.visible = false)


static func icon(key: String) -> Texture2D:
	if not _tex.has(key):
		var path := ICON_DIR + key + ".svg"
		_tex[key] = load(path) if ResourceLoader.exists(path) else null
	return _tex[key]


static func badge_key(c: Colonist) -> String:
	for d in DANGER:
		if float(c.needs.get(d[0], 100.0)) < float(d[1]):
			return d[2]
	if c.state == "thinking":
		return "thinking"
	if c.action != null:
		return String(c.action.def.get("id", ""))
	return ""


func update_for(c: Colonist, show_name: bool, delta: float) -> void:
	var key := badge_key(c)
	if key != _key:
		_key = key
		badge.texture = icon(key) if key != "" else null
	badge.visible = badge.texture != null
	_t += delta
	var pulse := 1.0 + (0.12 * sin(_t * 6.0) if key.begins_with("danger") else 0.0)
	badge.scale = Vector3.ONE * pulse
	name_label.text = c.name
	name_label.visible = show_name
	name_label.modulate = Color(1, 0.75, 0.7) if key.begins_with("danger") else Color(1, 1, 1, 0.95)
	# Badge sits above the name when both show.
	name_label.position.y = 0.0
	badge.position.y = 0.42 if show_name else 0.0
	bubble.position.y = badge.position.y + 0.42
