class_name SaveList
extends VBoxContainer
## The saved games, newest first: a thumbnail, the name, day / season / people / when, and Load
## and Delete. Used by the main menu and the pause menu.

signal load_requested(id: String)

var _confirm := ""   # slot waiting for a second click on Delete


func _ready() -> void:
	add_theme_constant_override("separation", 8)
	refresh()


func refresh() -> void:
	for ch in get_children():
		ch.queue_free()
	var saves := SaveSlots.list()
	if saves.is_empty():
		add_child(UiStyle.label("No saved games yet.", 15, UiStyle.MUTED))
		return
	for s in saves:
		add_child(_row(s))


func _row(s: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)
	var thumb := TextureRect.new()
	thumb.custom_minimum_size = Vector2(128, 72)
	thumb.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	thumb.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	if String(s.get("thumb", "")) != "":
		var img := Image.load_from_file(String(s["thumb"]))
		if img != null:
			thumb.texture = ImageTexture.create_from_image(img)
	row.add_child(thumb)
	var text := VBoxContainer.new()
	text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text.add_child(UiStyle.label(String(s.get("name", "Save")), 17))
	var bits: PackedStringArray = ["Day %d" % int(s.get("day", 0))]
	if String(s.get("season", "")) != "":
		bits.append(String(s["season"]).capitalize())
	bits.append("%d people" % int(s.get("pop", 0)))
	text.add_child(UiStyle.label(" · ".join(bits), 13, UiStyle.MUTED))
	var when := Time.get_datetime_string_from_unix_time(int(s.get("saved_at", 0)) + _tz_offset(), true)
	text.add_child(UiStyle.label("saved " + when.substr(0, 16), 12, Color(UiStyle.MUTED, 0.7)))
	row.add_child(text)
	var id := String(s["id"])
	row.add_child(UiStyle.make_button("Load", func() -> void: load_requested.emit(id)))
	if id != "legacy":
		var del := UiStyle.make_button("Delete", func() -> void: pass)
		del.pressed.connect(func() -> void:
			if _confirm == id:
				SaveSlots.remove(id)
				_confirm = ""
				refresh()
			else:
				_confirm = id
				del.text = "Really delete?")
		row.add_child(del)
	return row


static func _tz_offset() -> int:
	return int(Time.get_time_zone_from_system().get("bias", 0)) * 60
