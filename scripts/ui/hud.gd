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
var _speed_buttons: Array[Button] = []
var _feed := RichTextLabel.new()
var _feed_lines: PackedStringArray = []
var _card := PanelContainer.new()
var _card_text := RichTextLabel.new()


func _ready() -> void:
	var root := Control.new()
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)

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
	var help := Label.new()
	help.text = "WASD/drag pan · wheel zoom · Q/E rotate · click select · Space pause · 1/2/3 speed"
	help.add_theme_font_size_override("font_size", 12)
	help.add_theme_color_override("font_color", Color(1, 1, 1, 0.55))
	help.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	hb.add_child(help)
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
	_feed.mouse_filter = Control.MOUSE_FILTER_IGNORE
	feed_panel.add_child(_feed)

	# --- selection card (bottom right). The full inspector comes in step 4.
	_card.add_theme_stylebox_override("panel", _panel_style())
	_card.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	_card.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	_card.grow_vertical = Control.GROW_DIRECTION_BEGIN
	_card.position = Vector2(-10, -10)
	_card.custom_minimum_size = Vector2(320, 0)
	_card.visible = false
	root.add_child(_card)
	_card_text.fit_content = true
	_card_text.bbcode_enabled = true
	_card_text.scroll_active = false
	_card_text.custom_minimum_size = Vector2(320, 0)
	_card_text.add_theme_font_size_override("normal_font_size", 14)
	_card.add_child(_card_text)



func bind(p_runner: SimRunner, p_view: WorldView) -> void:
	runner = p_runner
	view = p_view
	var ev := runner.sim.events
	ev.colonist_born.connect(func(c: Colonist) -> void:
		if not c.parent_ids.is_empty():
			_log("[color=#9be29b]%s was born.[/color]" % c.name))
	ev.colonist_died.connect(func(c: Colonist, cause: String) -> void:
		_log("[color=#ff8a80]%s died of %s.[/color]" % [c.name, cause]))
	ev.colonist_grew_up.connect(func(c: Colonist) -> void: _log("%s grew up." % c.name))
	ev.couple_formed.connect(func(a: Colonist, b: Colonist) -> void:
		_log("[color=#f8bbd0]%s and %s became a couple.[/color]" % [a.name, b.name]))
	ev.building_completed.connect(func(b: Building) -> void:
		_log("[color=#ffe082]Built a %s.[/color]" % b.def().get("label", b.type)))
	ev.building_placed.connect(func(b: Building) -> void:
		if not b.complete:
			_log("Started a %s." % b.def().get("label", b.type)))
	ev.tier_unlocked.connect(func(t: int) -> void: _log("[color=#80d8ff]Tier %d unlocked![/color]" % t))
	runner.speed_changed.connect(func(_s: int) -> void: _refresh_speed())
	view.colonist_clicked.connect(func(c: Colonist) -> void: _card.visible = c != null)
	_refresh_speed()
	_log("The colony begins with %d settlers." % runner.sim.alive_count())


func _log(text: String) -> void:
	_feed_lines.append("[color=#aaaaaa]D%d %02d:00[/color] %s" % [runner.sim.day(), runner.sim.hour(), text])
	while _feed_lines.size() > FEED_LINES:
		_feed_lines.remove_at(0)
	_feed.text = "\n".join(_feed_lines)


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
	_lag.text = "waiting on AI..." if runner.lag_paused else ""
	if _card.visible and view.selected != null:
		_card_text.text = _card_bbcode(view.selected)


func _card_bbcode(c: Colonist) -> String:
	var sim := runner.sim
	var lines: PackedStringArray = []
	lines.append("[b]%s[/b]  (%s, %.1f days)" % [c.name, c.life_stage(), c.age_days()])
	lines.append("[color=#bbbbbb]%s[/color]" % ", ".join(c.traits))
	lines.append("Doing: [b]%s[/b]" % view.short_action(c))
	for need_id in sim.content.needs:
		var v := float(c.needs.get(need_id, 0))
		var col := "#9be29b" if v > 55 else "#ffe082" if v > 25 else "#ff8a80"
		lines.append("%s: [color=%s]%s[/color] (%d)" % [String(sim.content.needs[need_id].get("label", need_id)).capitalize(), col, sim.need_word(c, need_id), int(v)])
	if c.partner_id >= 0 and sim.colonists.has(c.partner_id):
		lines.append("Partner: %s" % (sim.colonists[c.partner_id] as Colonist).name)
	return "\n".join(lines)


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
