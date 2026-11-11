class_name UiStyle
extends RefCounted
## Shared look for menus and panels (docs/STYLE.md: dark, rounded, translucent, light text).

const BG := Color(0.08, 0.09, 0.12, 0.82)
const ACCENT := Color("#E8C468")
const TEXT := Color(0.94, 0.92, 0.86)
const MUTED := Color(0.72, 0.70, 0.66)


static func panel(alpha: float = 0.82, radius: int = 10) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = Color(BG, alpha)
	for side in ["left", "right", "top", "bottom"]:
		s.set("content_margin_" + side, 18)
	s.set_corner_radius_all(radius)
	s.border_color = Color(1, 1, 1, 0.06)
	s.set_border_width_all(1)
	return s


static func button(b: Button, big: bool = false) -> Button:
	b.focus_mode = Control.FOCUS_NONE
	b.alignment = HORIZONTAL_ALIGNMENT_LEFT
	b.add_theme_font_size_override("font_size", 22 if big else 16)
	b.add_theme_color_override("font_color", TEXT)
	b.add_theme_color_override("font_hover_color", ACCENT)
	b.add_theme_color_override("font_pressed_color", ACCENT)
	b.add_theme_color_override("font_disabled_color", Color(MUTED, 0.45))
	var normal := StyleBoxFlat.new()
	normal.bg_color = Color(1, 1, 1, 0.0)
	normal.set_corner_radius_all(6)
	normal.content_margin_left = 12
	normal.content_margin_right = 12
	normal.content_margin_top = 6
	normal.content_margin_bottom = 6
	var hover := normal.duplicate() as StyleBoxFlat
	hover.bg_color = Color(1, 1, 1, 0.07)
	b.add_theme_stylebox_override("normal", normal)
	b.add_theme_stylebox_override("hover", hover)
	b.add_theme_stylebox_override("pressed", hover)
	b.add_theme_stylebox_override("disabled", normal)
	return b


static func make_button(text: String, on_press: Callable, big: bool = false) -> Button:
	var b := button(Button.new(), big)
	b.text = text
	b.pressed.connect(on_press)
	return b


static func label(text: String, size: int = 15, col: Color = TEXT) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_color", col)
	return l
