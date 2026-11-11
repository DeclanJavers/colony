class_name PauseMenu
extends Control
## Esc in the game: Resume, Save, Load, Main menu, Quit. The colony pauses while it's open.

var runner: SimRunner
var _panel := PanelContainer.new()
var _main := VBoxContainer.new()
var _load := VBoxContainer.new()
var _note := Label.new()
var _was_speed := 1


func _init(p_runner: SimRunner) -> void:
	runner = p_runner


func _ready() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	visible = false
	var dim := ColorRect.new()
	dim.color = Color(0.03, 0.03, 0.05, 0.45)
	dim.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(dim)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(center)
	_panel.add_theme_stylebox_override("panel", UiStyle.panel())
	_panel.custom_minimum_size = Vector2(380, 0)
	center.add_child(_panel)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	_panel.add_child(box)
	box.add_child(UiStyle.label("Paused", 28, UiStyle.ACCENT))
	box.add_child(_main)
	box.add_child(_load)
	_note.add_theme_color_override("font_color", UiStyle.MUTED)
	box.add_child(_note)
	for spec in [["Resume", close], ["Save game", _save], ["Load game", _show_load],
			["Main menu", func() -> void: get_tree().change_scene_to_file("res://scenes/menu.tscn")],
			["Quit", func() -> void: get_tree().quit()]]:
		_main.add_child(UiStyle.make_button(spec[0], spec[1], true))
	_load.visible = false


func open() -> void:
	_was_speed = runner.speed_index
	runner.set_speed_index(0)
	_main.visible = true
	_load.visible = false
	_note.text = ""
	visible = true


func close() -> void:
	visible = false
	runner.set_speed_index(_was_speed if _was_speed > 0 else 1)


func toggle() -> void:
	if visible:
		close()
	else:
		open()


func _save() -> void:
	visible = false           # keep the menu out of the thumbnail
	await get_tree().process_frame
	await get_tree().process_frame
	var err := runner.save_game()
	visible = true
	_note.text = "Saved." if err == OK else "Couldn't save (error %d)." % err


func _show_load() -> void:
	for ch in _load.get_children():
		ch.queue_free()
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size = Vector2(560, 320)
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	var list := SaveList.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list.load_requested.connect(func(id: String) -> void:
		runner.set_speed_index(_was_speed if _was_speed > 0 else 1)
		runner.load_game(id))
	scroll.add_child(list)
	_load.add_child(scroll)
	_load.add_child(UiStyle.make_button("Back", func() -> void:
		_load.visible = false
		_main.visible = true))
	_main.visible = false
	_load.visible = true


func _unhandled_key_input(event: InputEvent) -> void:
	if visible and event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
		close()
		get_viewport().set_input_as_handled()
