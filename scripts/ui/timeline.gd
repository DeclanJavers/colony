class_name StoryTimeline
extends Control
## The colony's story as a quiet strip on the Colony page: one dot per moment that mattered, in
## three lanes (people, buildings, colony life), from day 0 to now. Hover a dot to read it.

const LANES := {
	"birth": 0, "death": 0, "couple": 0,
	"built": 1, "expanded": 1, "tier": 1,
	"meeting": 2, "role": 2, "season": 2,
}
const LANE_NAMES := ["people", "building", "colony"]
const COLORS := {
	"birth": Color("#9be29b"), "death": Color("#ff8a80"), "couple": Color("#f2b8d8"),
	"built": Color("#d9b27c"), "expanded": Color("#f2c640"), "tier": Color("#f2c640"),
	"meeting": Color("#e8c468"), "role": Color("#c9b6e4"), "season": Color("#8fc3ea"),
}

var sim: Sim
var _dots: Array = []   # {pos: Vector2, text: String}


func _init() -> void:
	custom_minimum_size = Vector2(0, 70)
	mouse_filter = Control.MOUSE_FILTER_PASS
	tooltip_text = " "    # needed for _get_tooltip to be asked


func refresh() -> void:
	queue_redraw()


func _draw() -> void:
	_dots.clear()
	if sim == null:
		return
	var font := get_theme_default_font()
	var fs := 11
	var label_w := 64.0
	var area := Rect2(label_w, 4, size.x - label_w - 6, size.y - 18)
	var lane_h := area.size.y / 3.0
	var span := maxf(sim.clock, 1440.0)
	for i in 3:
		var y := area.position.y + lane_h * (i + 0.5)
		draw_string(font, Vector2(0, y + fs * 0.35), LANE_NAMES[i], HORIZONTAL_ALIGNMENT_LEFT, label_w, fs, Color(1, 1, 1, 0.55))
		draw_line(Vector2(area.position.x, y), Vector2(area.end.x, y), Color(1, 1, 1, 0.08), 1.0)
	# Week marks along the bottom.
	var week := 7.0 * 1440.0
	var w := week
	while w < span:
		var x := area.position.x + area.size.x * (w / span)
		draw_line(Vector2(x, area.position.y), Vector2(x, area.end.y), Color(1, 1, 1, 0.07), 1.0)
		w += week
	draw_string(font, Vector2(area.position.x, size.y - 2), "day 0", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.4))
	draw_string(font, Vector2(area.end.x - 60, size.y - 2), "day %d" % sim.day(), HORIZONTAL_ALIGNMENT_RIGHT, 60, 10, Color(1, 1, 1, 0.4))
	for e in sim.story.timeline:
		var kind := String(e["kind"])
		if not LANES.has(kind):
			continue
		var p := Vector2(area.position.x + area.size.x * clampf(float(e["t"]) / span, 0.0, 1.0),
			area.position.y + lane_h * (int(LANES[kind]) + 0.5))
		draw_circle(p, 3.0 if kind != "meeting" else 4.0, COLORS.get(kind, Color.WHITE))
		_dots.append({"pos": p, "text": "Day %d: %s" % [int(float(e["t"]) / 1440.0), e["text"]]})


func _get_tooltip(at: Vector2) -> String:
	var best := ""
	var best_d := 8.0
	for d in _dots:
		var dist := at.distance_to(d["pos"])
		if dist < best_d:
			best_d = dist
			best = d["text"]
	return best
