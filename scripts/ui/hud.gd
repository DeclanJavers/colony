class_name Hud
extends CanvasLayer
## Top bar (time, population, stock, tier, speed), event feed, and a small selection card.
## Keys: Space pause, 1/2/3 = 1x/3x/10x.

const FEED_LINES := 8

var runner: SimRunner
var view: WorldView
var _time := Label.new()
var _pop := Label.new()
var _stock := Label.new()
var _tier := Label.new()
var _lag := Label.new()
var _brain := Label.new()
var _speed_buttons: Array[Button] = []
var _feed := RichTextLabel.new()
var _feed_lines: PackedStringArray = []
var inspector := Inspector.new()
var cam: IsoCamera                 # set by main, for follow / Tab
var _tooltip := Label.new()
var _root: Control


func _ready() -> void:
	var root := Control.new()
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)
	_root = root

	# --- top bar
	var bar := PanelContainer.new()
	bar.add_theme_stylebox_override("panel", _panel_style())
	bar.set_anchors_preset(Control.PRESET_TOP_WIDE)
	root.add_child(bar)
	var hb := HBoxContainer.new()
	hb.add_theme_constant_override("separation", 22)
	bar.add_child(hb)
	for l in [_time, _pop, _stock, _tier]:
		l.add_theme_font_size_override("font_size", 17)
		hb.add_child(l)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hb.add_child(spacer)
	_lag.add_theme_color_override("font_color", Color(1, 0.8, 0.3))
	hb.add_child(_lag)
	_brain.add_theme_font_size_override("font_size", 13)
	_brain.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	hb.add_child(_brain)
	var colony_btn := Button.new()
	colony_btn.text = "Colony"
	colony_btn.focus_mode = Control.FOCUS_NONE
	colony_btn.tooltip_text = "Colony overview: people, stock trends, next tier, buildings (C)"
	colony_btn.pressed.connect(func() -> void: view.select_target("colony"))
	hb.add_child(colony_btn)
	var labels := ["Pause", "1x", "3x", "10x"]
	for i in labels.size():
		var b := Button.new()
		b.text = labels[i]
		b.toggle_mode = true
		b.focus_mode = Control.FOCUS_NONE
		b.pressed.connect(func() -> void: runner.set_speed_index(i))
		hb.add_child(b)
		_speed_buttons.append(b)

	# --- event feed (bottom left)
	var feed_panel := PanelContainer.new()
	feed_panel.add_theme_stylebox_override("panel", _panel_style())
	feed_panel.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	feed_panel.position = Vector2(10, -10)
	feed_panel.custom_minimum_size = Vector2(430, 0)
	feed_panel.grow_vertical = Control.GROW_DIRECTION_BEGIN
	feed_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(feed_panel)
	_feed.fit_content = true
	_feed.bbcode_enabled = true
	_feed.scroll_active = false
	_feed.custom_minimum_size = Vector2(430, 0)
	_feed.add_theme_font_size_override("normal_font_size", 14)
	_feed.meta_underlined = false
	_feed.meta_clicked.connect(func(meta: Variant) -> void: inspector.open_link(String(meta)))
	feed_panel.add_child(_feed)

	# --- controls hint (bottom centre, out of the way of the top bar)
	var help := Label.new()
	help.text = "click anything to inspect · Tab next colonist · F follow · C colony · Space pause · 1/2/3 speed"
	help.add_theme_font_size_override("font_size", 12)
	help.add_theme_color_override("font_color", Color(1, 1, 1, 0.6))
	help.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.8))
	help.add_theme_constant_override("outline_size", 4)
	help.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	help.grow_horizontal = Control.GROW_DIRECTION_BOTH
	help.grow_vertical = Control.GROW_DIRECTION_BEGIN
	help.offset_bottom = -8
	help.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(help)

	# --- hover tooltip (follows the mouse)
	_tooltip.add_theme_font_size_override("font_size", 13)
	_tooltip.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.9))
	_tooltip.add_theme_constant_override("outline_size", 5)
	_tooltip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_tooltip.visible = false
	root.add_child(_tooltip)

	# --- inspector + chat (right side), set up in bind() once the chat service exists
	root.add_child(inspector)


func bind(p_runner: SimRunner, p_view: WorldView, chat: WatcherChat) -> void:
	runner = p_runner
	view = p_view
	inspector.setup(runner, view, chat, _panel_style())
	var ev := runner.sim.events
	ev.colonist_born.connect(func(c: Colonist) -> void:
		if not c.parent_ids.is_empty():
			_log("[color=#9be29b]%s was born.[/color]" % InspectText.link(c)))
	ev.colonist_died.connect(func(c: Colonist, cause: String) -> void:
		_log("[color=#ff8a80]%s died of %s.[/color]" % [InspectText.link(c), cause]))
	ev.colonist_grew_up.connect(func(c: Colonist) -> void: _log("%s grew up." % InspectText.link(c)))
	ev.couple_formed.connect(func(a: Colonist, b: Colonist) -> void:
		_log("[color=#f8bbd0]%s and %s became a couple.[/color]" % [InspectText.link(a), InspectText.link(b)]))
	ev.building_completed.connect(func(b: Building) -> void:
		_log("[color=#ffe082]Built a %s.[/color]" % InspectText.link(b)))
	ev.building_placed.connect(func(b: Building) -> void:
		if not b.complete:
			_log("Started a %s." % InspectText.link(b)))
	ev.conversation_finished.connect(func(conv: Dictionary) -> void:
		var a: Colonist = conv["a"]
		var b: Colonist = conv["b"]
		if conv["intent"] == "ask" and conv["topic"] != "":
			var lbl: String = runner.sim.content.actions_by_id.get(conv["topic"], {}).get("label", conv["topic"])
			_log("[color=#c9b6e4]%s asked %s to %s.[/color]" % [InspectText.link(a), InspectText.link(b), lbl])
		elif conv["intent"] == "news":
			_log("[color=#c9b6e4]%s told %s the news.[/color]" % [InspectText.link(a), InspectText.link(b)]))
	ev.tier_unlocked.connect(func(t: int) -> void: _log("[color=#80d8ff]Tier %d unlocked![/color]" % t))
	runner.speed_changed.connect(func(_s: int) -> void: _refresh_speed())
	view.target_selected.connect(func(t: Variant) -> void: inspector.show_target(t))
	view.hovered.connect(_on_hover)
	chat.memory_saved.connect(func(c: Colonist, text: String) -> void:
		_log("[color=#80d8ff]%s will remember: %s[/color]" % [InspectText.link(c), text]))
	_refresh_speed()
	_log("The colony begins with %d settlers." % runner.sim.alive_count())


var _last_log := ""
var _last_log_n := 1


func _log(text: String) -> void:
	var stamp := "[color=#aaaaaa]D%d %02d:00[/color] " % [runner.sim.day(), runner.sim.hour()]
	if text == _last_log and not _feed_lines.is_empty():
		_last_log_n += 1   # same event again: one line with a count instead of a wall of repeats
		_feed_lines[-1] = "%s%s [color=#aaaaaa]x%d[/color]" % [stamp, text, _last_log_n]
	else:
		_last_log = text
		_last_log_n = 1
		_feed_lines.append(stamp + text)
	while _feed_lines.size() > FEED_LINES:
		_feed_lines.remove_at(0)
	_feed.text = "\n".join(_feed_lines)


## Tab / Shift+Tab: inspect the next colonist and move the camera to them.
func _cycle_colonist(dir: int) -> void:
	var people := runner.sim.colonists.values()
	if people.is_empty():
		return
	people.sort_custom(func(a, b) -> bool: return a.id < b.id)
	var i := people.find(view.selected)
	var c: Colonist = people[(i + dir + people.size()) % people.size()] if i >= 0 else people[0]
	view.select_target(c)
	if cam != null:
		if cam.follow_node != null:
			cam.follow_node = view.colonist_root(c)
		else:
			cam.focus(view.target_position(c))


func _on_hover(t: Variant, pos: Vector2) -> void:
	var text := WorldView.describe_short(t)
	_tooltip.visible = text != ""
	_tooltip.text = text
	_tooltip.position = pos + Vector2(16, 14)


func _refresh_speed() -> void:
	for i in _speed_buttons.size():
		_speed_buttons[i].button_pressed = i == runner.speed_index


func _process(_delta: float) -> void:
	if runner == null or runner.sim == null:
		return
	var sim := runner.sim
	var t := sim.time_of_day()
	_time.text = "Day %d   %02d:%02d %s" % [sim.day(), int(t), int(fmod(t, 1.0) * 60.0), "(night)" if sim.is_night() else ""]
	var adults := 0
	for c: Colonist in sim.colonists.values():
		if c.is_adult():
			adults += 1
	_pop.text = "Pop %d (%d kids)" % [sim.alive_count(), sim.alive_count() - adults]
	var parts: PackedStringArray = []
	for r in sim.stock:
		parts.append("%s %d" % [String(r).capitalize(), int(sim.stock[r])])
	_stock.text = "%s   [%d/%d]" % ["  ".join(parts), int(sim.stock_total()), int(sim.storage_capacity())]
	_tier.text = "Tier %d" % sim.unlocked_tier
	_lag.text = "waiting on Laya..." if runner.lag_paused else ""
	if runner.laya != null:
		_brain.text = runner.laya.status()
		_brain.add_theme_color_override("font_color", Color(0.6, 0.9, 0.6) if runner.laya.online else Color(1, 0.55, 0.45))
	else:
		_brain.text = "brain: %s" % runner.brain_id


func _unhandled_input(event: InputEvent) -> void:
	if runner == null:
		return
	if event is InputEventKey and event.pressed and not event.echo:
		match event.keycode:
			KEY_SPACE:
				runner.toggle_pause()
			KEY_1:
				runner.set_speed_index(1)
			KEY_2:
				runner.set_speed_index(2)
			KEY_3:
				runner.set_speed_index(3)
			KEY_C:
				view.select_target("colony")
			KEY_F:
				if cam != null and view.selected != null:
					cam.follow_node = null if cam.follow_node != null else view.colonist_root(view.selected)
			KEY_TAB:
				_cycle_colonist(-1 if event.shift_pressed else 1)
			KEY_ESCAPE:
				view.select_target(null)


func _panel_style() -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = Color(0.08, 0.09, 0.12, 0.78)
	s.content_margin_left = 12
	s.content_margin_right = 12
	s.content_margin_top = 6
	s.content_margin_bottom = 6
	s.corner_radius_bottom_left = 6
	s.corner_radius_bottom_right = 6
	s.corner_radius_top_left = 6
	s.corner_radius_top_right = 6
	return s
