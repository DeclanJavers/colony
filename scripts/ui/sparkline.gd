class_name Sparkline
extends Control
## A tiny, quiet line chart: one series, a label, and the latest value. Used on the Colony page.

var label := ""
var values: PackedFloat32Array = []
var color := Color(0.6, 0.75, 0.95)


func _init(p_label: String = "", p_color: Color = Color(0.6, 0.75, 0.95)) -> void:
	label = p_label
	color = p_color
	custom_minimum_size = Vector2(0, 34)
	mouse_filter = Control.MOUSE_FILTER_IGNORE


func set_values(v: PackedFloat32Array) -> void:
	values = v
	queue_redraw()


func _draw() -> void:
	var font := get_theme_default_font()
	var fs := 12
	var label_w := 64.0
	var value_w := 52.0
	draw_string(font, Vector2(0, size.y * 0.5 + fs * 0.35), label, HORIZONTAL_ALIGNMENT_LEFT, label_w, fs, Color(1, 1, 1, 0.75))
	var area := Rect2(label_w, 4, size.x - label_w - value_w, size.y - 8)
	draw_line(Vector2(area.position.x, area.end.y), Vector2(area.end.x, area.end.y), Color(1, 1, 1, 0.12), 1.0)
	if values.size() < 2:
		draw_string(font, Vector2(area.position.x, size.y * 0.5 + fs * 0.35), "collecting...", HORIZONTAL_ALIGNMENT_LEFT, -1, fs, Color(1, 1, 1, 0.35))
		return
	var lo := values[0]
	var hi := values[0]
	for v in values:
		lo = minf(lo, v)
		hi = maxf(hi, v)
	var span := maxf(hi - lo, 1.0)
	var pts := PackedVector2Array()
	for i in values.size():
		var x := area.position.x + area.size.x * float(i) / float(values.size() - 1)
		var y := area.end.y - area.size.y * (values[i] - lo) / span
		pts.append(Vector2(x, y))
	draw_polyline(pts, color, 2.0, true)
	draw_circle(pts[pts.size() - 1], 2.5, color)
	draw_string(font, Vector2(size.x - value_w + 6, size.y * 0.5 + fs * 0.35), str(int(values[values.size() - 1])),
		HORIZONTAL_ALIGNMENT_LEFT, value_w, fs, Color(1, 1, 1, 0.85))
