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
var _settings := PanelContainer.new()
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
	hb.add_theme_constant_override("separation", 16)
	bar.add_child(hb)
	for l in [_time, _pop, _stock, _tier]:
		l.add_theme_font_size_override("font_size", 17)
		hb.add_child(l)
	# Status text takes whatever room is left and is cut short rather than pushing the buttons off.
	var status := HBoxContainer.new()
	status.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	status.alignment = BoxContainer.ALIGNMENT_END
	status.add_theme_constant_override("separation", 12)
	hb.add_child(status)
	_lag.add_theme_color_override("font_color", Color(1, 0.8, 0.3))
	_lag.add_theme_font_size_override("font_size", 13)
	_lag.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	status.add_child(_lag)
	_brain.add_theme_font_size_override("font_size", 13)
	_brain.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_brain.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_brain.clip_text = true
	_brain.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_brain.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	status.add_child(_brain)
	var settings_btn := Button.new()
	settings_btn.text = "Settings"
	settings_btn.focus_mode = Control.FOCUS_NONE
	settings_btn.pressed.connect(func() -> void:
		_settings.visible = not _settings.visible)
	hb.add_child(settings_btn)
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
	help.add_theme_color_override("font_color", Color(1, 1, 1, 0.75))
	help.mouse_filter = Control.MOUSE_FILTER_IGNORE
	# On a soft dark strip so it reads on snow as well as grass.
	var help_panel := PanelContainer.new()
	var hs := _panel_style()
	hs.bg_color.a *= 0.7
	hs.content_margin_top = 2
	hs.content_margin_bottom = 2
	help_panel.add_theme_stylebox_override("panel", hs)
	help_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	help_panel.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	help_panel.grow_horizontal = Control.GROW_DIRECTION_BOTH
	help_panel.grow_vertical = Control.GROW_DIRECTION_BEGIN
	help_panel.offset_bottom = -6
	help_panel.add_child(help)
	root.add_child(help_panel)

	# --- hover tooltip (follows the mouse)
	_tooltip.add_theme_font_size_override("font_size", 13)
	_tooltip.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.9))
	_tooltip.add_theme_constant_override("outline_size", 5)
	_tooltip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_tooltip.visible = false
	root.add_child(_tooltip)

	# --- settings (small panel under the top bar)
	_build_settings(root)

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
	ev.season_changed.connect(func(season: String) -> void:
		var msg := {"spring": "Spring has come.", "summer": "Summer is here.",
			"autumn": "Autumn: winter is coming, time to stock firewood.", "winter": "Winter has come."}
		_log("[color=#c5d8e8]%s[/color]" % msg.get(season, season.capitalize())))
	ev.weather_changed.connect(func(w: String) -> void:
		if w == "storm" or w == "snow":
			_log("[color=#c5d8e8]%s[/color]" % ("A storm rolls in." if w == "storm" else "Snow is falling.")))
	ev.tier_unlocked.connect(func(t: int) -> void: _log("[color=#80d8ff]Tier %d unlocked![/color]" % t))
	runner.speed_changed.connect(func(_s: int) -> void: _refresh_speed())
	view.target_selected.connect(func(t: Variant) -> void: inspector.show_target(t))
	view.hovered.connect(_on_hover)
	chat.memory_saved.connect(func(c: Colonist, text: String) -> void:
		_log("[color=#80d8ff]%s will remember: %s[/color]" % [InspectText.link(c), text]))
	_refresh_speed()
	_log("Welcome back: day %d, %d people." % [runner.sim.day(), runner.sim.alive_count()] if runner.loaded else "The colony begins with %d settlers." % runner.sim.alive_count())


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
		runner.chronicle.append({"t": runner.sim.clock, "text": text})
		if runner.chronicle.size() > 300:
			runner.chronicle.pop_front()
	while _feed_lines.size() > FEED_LINES:
		_feed_lines.remove_at(0)
	# Older lines fade so the newest stands out.
	var shown: PackedStringArray = []
	for i in _feed_lines.size():
		var age := _feed_lines.size() - 1 - i
		var alpha := clampf(1.0 - 0.09 * age, 0.4, 1.0)
		shown.append("[color=#ffffff%02x]%s[/color]" % [int(alpha * 255), _feed_lines[i]])
	_feed.text = "\n".join(shown)


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


func _build_settings(root: Control) -> void:
	_settings.add_theme_stylebox_override("panel", _panel_style())
	_settings.set_anchors_preset(Control.PRESET_TOP_RIGHT)
	_settings.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	_settings.position = Vector2(-470, 52)
	_settings.visible = false
	root.add_child(_settings)
	var grid := GridContainer.new()
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", 14)
	grid.add_theme_constant_override("v_separation", 8)
	_settings.add_child(grid)
	_add_choice(grid, "Conversations", "talk_mode", [["all", "Chat model writes all"], ["near", "Only ones on screen"], ["quick", "Quick (no chat model)"]])
	_add_choice(grid, "Names", "names", [["auto", "When zoomed in"], ["always", "Always"], ["never", "Never"]])
	_add_choice(grid, "Pixels", "pixels", [["fine", "Fine pixel art"], ["chunky", "Chunky pixel art"], ["off", "Smooth (no pixels)"]])
	var lbl := Label.new()
	lbl.text = "Speech bubbles"
	grid.add_child(lbl)
	var cb := CheckButton.new()
	cb.button_pressed = bool(GameSettings.get_value("bubbles"))
	cb.focus_mode = Control.FOCUS_NONE
	cb.toggled.connect(func(on: bool) -> void: GameSettings.set_value("bubbles", on))
	grid.add_child(cb)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	var save_btn := Button.new()
	save_btn.text = "Save (F5)"
	save_btn.focus_mode = Control.FOCUS_NONE
	save_btn.pressed.connect(_save)
	var load_btn := Button.new()
	load_btn.text = "Load (F9)"
	load_btn.focus_mode = Control.FOCUS_NONE
	load_btn.pressed.connect(_load)
	row.add_child(save_btn)
	row.add_child(load_btn)
	var spacer := Label.new()
	spacer.text = "Game"
	grid.add_child(spacer)
	grid.add_child(row)


func _save() -> void:
	var err := runner.save_game()
	_log("[color=#9be29b]Game saved.[/color]" if err == OK else "[color=#ff8a80]Couldn't save (error %d).[/color]" % err)


func _load() -> void:
	if not runner.load_game():
		_log("[color=#ff8a80]No saved game yet.[/color]")


func _add_choice(grid: GridContainer, title: String, key: String, opts: Array) -> void:
	var lbl := Label.new()
	lbl.text = title
	grid.add_child(lbl)
	var ob := OptionButton.new()
	ob.focus_mode = Control.FOCUS_NONE
	for i in opts.size():
		ob.add_item(opts[i][1], i)
		if opts[i][0] == GameSettings.get_value(key):
			ob.select(i)
	ob.item_selected.connect(func(i: int) -> void: GameSettings.set_value(key, opts[i][0]))
	grid.add_child(ob)


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
	_time.text = "Day %d   %02d:%02d %s  ·  %s%s" % [sim.day(), int(t), int(fmod(t, 1.0) * 60.0), "(night)" if sim.is_night() else "",
		sim.season().capitalize(), (", " + String(sim.weather_def().get("label", sim.weather))) if sim.weather != "clear" else ""]
	var adults := 0
	for c: Colonist in sim.colonists.values():
		if c.is_adult():
			adults += 1
	_pop.text = "Pop %d (%d kids)" % [sim.alive_count(), sim.alive_count() - adults]
	var parts: PackedStringArray = []
	for r in Knowledge.STOCK_KEYS:   # fixed order, whatever order the stock dictionary is in
		parts.append("%s %d" % [String(r).capitalize(), int(sim.stock.get(r, 0))])
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
			KEY_F5:
				_save()
			KEY_F9:
				_load()
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
