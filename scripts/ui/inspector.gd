class_name Inspector
extends PanelContainer
## Right-hand side panel for whatever is selected.
## Colonists: who they are, needs, family and friends, what their brain weighed for the last
## choice, memories (highlighting the ones Laya reads), and a chat box to talk to them.
## Everything else (buildings, sites, farms, resources, graves, ground, the colony overview) gets
## a page from InspectText. Names on every page are links: clicking one inspects it and moves
## the camera there.

signal focus_requested(world_pos: Vector3)

const WIDTH := 430.0
const REFRESH_S := 0.25
const NEED_COLORS := {"good": Color(0.45, 0.8, 0.45), "mid": Color(0.95, 0.8, 0.35), "bad": Color(0.95, 0.4, 0.35)}

var runner: SimRunner
var view: WorldView
var chat: WatcherChat
var colonist: Colonist            # set when a living colonist is inspected
var target: Variant = null        # whatever is inspected (Colonist, Building, ResNode, Vector2i, "colony")
var _colonist_only: Array[Control] = []

var _title := RichTextLabel.new()
var _info := RichTextLabel.new()
var _needs := GridContainer.new()
var _need_rows: Dictionary = {}   # need id -> [ProgressBar, Label]
var _why_title := RichTextLabel.new()
var _why := VBoxContainer.new()
var _why_key := ""
var _memories := RichTextLabel.new()
var _recent := RichTextLabel.new()
var _beliefs := RichTextLabel.new()
var _convos := RichTextLabel.new()
var _charts := VBoxContainer.new()   # colony page: trends over the last 3 days
var _sparks: Dictionary = {}
var _state_btn := Button.new()
var _state := RichTextLabel.new()
var _chat_title := Label.new()
var _chat_log := RichTextLabel.new()
var _chat_status := Label.new()
var _entry := LineEdit.new()
var _send := Button.new()
var _t := 0.0


func setup(p_runner: SimRunner, p_view: WorldView, p_chat: WatcherChat, style: StyleBox) -> void:
	runner = p_runner
	view = p_view
	chat = p_chat
	var st := style.duplicate() as StyleBoxFlat
	if st != null:
		st.bg_color.a = 0.93
		style = st
	add_theme_stylebox_override("panel", style)
	chat.reply_received.connect(func(c: Colonist, _t2: String) -> void: if c == colonist: _render_chat())
	chat.busy_changed.connect(func(c: Colonist, _b: bool) -> void: if c == colonist: _render_chat())
	chat.memory_saved.connect(func(c: Colonist, text: String) -> void:
		if c == colonist:
			_chat_status.text = "%s will remember: \"%s\"" % [c.name, text]
			_refresh())
	chat.request_heard.connect(func(c: Colonist, action_id: String) -> void:
		if c == colonist:
			_chat_status.text = "%s is thinking about your request: %s." % [c.name, runner.sim.content.actions_by_id[action_id].get("label", action_id)])
	chat.failed.connect(func(c: Colonist, msg: String) -> void:
		if c == colonist:
			_chat_status.text = "Couldn't reach %s: %s (is brain_server/run.sh running?)" % [c.name, msg]
			_render_chat())


func _ready() -> void:
	set_anchors_preset(Control.PRESET_RIGHT_WIDE)
	offset_left = -WIDTH - 10
	offset_right = -10
	offset_top = 48
	offset_bottom = -10
	visible = false

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 6)
	add_child(col)

	var head := HBoxContainer.new()
	col.add_child(head)
	_rich(_title, 18)
	_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(_title)
	var close := Button.new()
	close.text = "Close"
	close.focus_mode = Control.FOCUS_NONE
	close.pressed.connect(func() -> void: view.select(null))
	head.add_child(close)

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	col.add_child(scroll)
	var body := VBoxContainer.new()
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_theme_constant_override("separation", 8)
	scroll.add_child(body)

	_charts.add_theme_constant_override("separation", 2)
	_charts.visible = false
	body.add_child(_charts)
	for spec in [["people", Color(0.95, 0.85, 0.55)], ["food", Color(0.92, 0.45, 0.45)], ["water", Color(0.45, 0.70, 0.98)],
			["wood", Color(0.80, 0.62, 0.40)], ["stone", Color(0.78, 0.80, 0.84)]]:
		var sp := Sparkline.new(spec[0], spec[1])
		_charts.add_child(sp)
		_sparks[spec[0]] = sp
	_rich(_info, 14)
	_info.meta_underlined = true
	_info.meta_clicked.connect(_on_link)
	body.add_child(_info)

	_needs.columns = 3
	_needs.add_theme_constant_override("h_separation", 8)
	body.add_child(_needs)

	_rich(_why_title, 14)
	body.add_child(_why_title)
	_why.add_theme_constant_override("separation", 1)
	body.add_child(_why)

	_rich(_beliefs, 13)
	body.add_child(_beliefs)
	_rich(_recent, 13)
	body.add_child(_recent)
	_rich(_convos, 13)
	body.add_child(_convos)
	_rich(_memories, 13)
	body.add_child(_memories)

	_state_btn.text = "Show what Laya reads"
	_state_btn.toggle_mode = true
	_state_btn.focus_mode = Control.FOCUS_NONE
	_state_btn.toggled.connect(func(on: bool) -> void:
		_state.visible = on
		_state_btn.text = "Hide what Laya reads" if on else "Show what Laya reads")
	body.add_child(_state_btn)
	_rich(_state, 12)
	_state.add_theme_color_override("default_color", Color(0.8, 0.85, 0.9))
	_state.visible = false
	body.add_child(_state)

	var sep := HSeparator.new()
	col.add_child(sep)
	_chat_title.add_theme_font_size_override("font_size", 15)
	col.add_child(_chat_title)
	_rich(_chat_log, 14)
	_chat_log.fit_content = false
	_chat_log.scroll_active = true
	_chat_log.scroll_following = true
	_chat_log.custom_minimum_size = Vector2(0, 190)
	col.add_child(_chat_log)
	_chat_status.add_theme_font_size_override("font_size", 12)
	_chat_status.add_theme_color_override("font_color", Color(1, 1, 1, 0.6))
	_chat_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	col.add_child(_chat_status)
	var row := HBoxContainer.new()
	col.add_child(row)
	_entry.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_entry.placeholder_text = "Say something as the Watcher..."
	_entry.text_submitted.connect(func(_s: String) -> void: _on_send())
	row.add_child(_entry)
	_send.text = "Send"
	_send.focus_mode = Control.FOCUS_NONE
	_send.pressed.connect(_on_send)
	row.add_child(_send)
	_colonist_only.assign([_needs, _why_title, _why, _beliefs, _recent, _convos, _memories, _state_btn, sep, _chat_title, _chat_log, _chat_status, row])


## Inspect anything (see WorldView.target_selected for the kinds). null closes the panel.
func show_target(t: Variant) -> void:
	if t is Colonist and (t as Colonist).alive():
		target = t
		_set_colonist_mode(true)
		show_colonist(t)
		return
	if colonist != null:
		chat.save_memory(colonist)
	colonist = null
	target = t
	visible = t != null
	_entry.release_focus()
	if t == null:
		return
	_set_colonist_mode(false)
	_refresh_generic()


func _update_charts() -> void:
	var series := {"people": PackedFloat32Array(), "food": PackedFloat32Array(), "water": PackedFloat32Array(),
		"wood": PackedFloat32Array(), "stone": PackedFloat32Array()}
	for snap in runner.history:
		series["people"].append(float(snap["pop"]))
		for k in ["food", "water", "wood", "stone"]:
			series[k].append(float(snap["stock"].get(k, 0)))
	for k in series:
		(_sparks[k] as Sparkline).set_values(series[k])


func _set_colonist_mode(on: bool) -> void:
	for ctl in _colonist_only:
		ctl.visible = on
	_charts.visible = target is String and target == "colony"
	_state.visible = on and _state_btn.button_pressed


## Open a link like "c:12" (see InspectText): inspect it and move the camera there.
func open_link(meta: String) -> void:
	_on_link(meta)


func _on_link(meta: Variant) -> void:
	var t: Variant = resolve_link(runner.sim, String(meta))
	if t == null:
		return
	view.select_target(t)
	var pos: Variant = view.target_position(t)
	if pos != null:
		focus_requested.emit(pos)


static func resolve_link(sim: Sim, meta: String) -> Variant:
	if meta == "colony":
		return "colony"
	var parts := meta.split(":", true, 1)
	if parts.size() != 2:
		return null
	match parts[0]:
		"c":
			return sim.find_colonist(int(parts[1]))
		"b":
			return sim.buildings.get(int(parts[1]))
		"n":
			return sim.nodes.get(int(parts[1]))
		"t":
			var xy := parts[1].split(",")
			return Vector2i(int(xy[0]), int(xy[1]))
	return null


func _refresh_generic() -> void:
	var sim := runner.sim
	var t: Variant = target
	if t is Colonist:   # a dead colonist = their grave
		_title.text = "[b]Grave of %s[/b]" % (t as Colonist).name
		_info.text = InspectText.grave(sim, t)
	elif t is Building:
		var b := t as Building
		if not sim.buildings.has(b.id):
			show_target(null)
			return
		_title.text = "[b]%s[/b]" % InspectText.label_of(b)
		_info.text = InspectText.building(sim, b)
	elif t is ResNode:
		var n := t as ResNode
		if not sim.nodes.has(n.id):
			_title.text = "[b]%s[/b]  [color=#aaaaaa]used up[/color]" % String(n.type).replace("_", " ").capitalize()
			_info.text = "Nothing left here."
			return
		_title.text = "[b]%s[/b]" % String(n.type).replace("_", " ").capitalize()
		_info.text = InspectText.node(sim, n)
	elif t is Vector2i:
		_title.text = "[b]Ground[/b]  [color=#aaaaaa](%d, %d)[/color]" % [t.x, t.y]
		_info.text = InspectText.tile(sim, t)
	elif t is String and t == "colony":
		_title.text = "[b]The colony[/b]  [color=#aaaaaa]last 3 days[/color]"
		_info.text = InspectText.colony(sim, runner)
		_update_charts()


func show_colonist(c: Colonist) -> void:
	if colonist != null and colonist != c:
		chat.save_memory(colonist)   # leaving a conversation turns it into a memory
	colonist = c
	visible = c != null
	_why_key = ""
	_chat_status.text = ""
	if c == null:
		_entry.release_focus()
		return
	_build_need_rows()
	_refresh()
	_render_chat()


func _on_send() -> void:
	if colonist == null or _entry.text.strip_edges() == "":
		return
	chat.send(colonist, _entry.text)
	_entry.text = ""
	_render_chat()


func _process(delta: float) -> void:
	if not visible or target == null:
		return
	_t += delta
	if _t >= REFRESH_S:
		_t = 0.0
		if colonist != null:
			_refresh()
		else:
			_refresh_generic()


func _input(event: InputEvent) -> void:
	# Clicking anywhere outside the panel gives the keyboard back to the camera and hotkeys.
	if visible and event is InputEventMouseButton and event.pressed and _entry.has_focus():
		if not get_global_rect().has_point(event.position):
			_entry.release_focus()


## ---- Rendering ------------------------------------------------------------------------------

func _refresh() -> void:
	var c := colonist
	var sim := runner.sim
	var traits: PackedStringArray = []
	for t in c.traits:
		traits.append(t)
	_title.text = "[b]%s[/b]  [color=#aaaaaa]%s, %.1f days · %s[/color]" % [c.name, c.life_stage(), c.age_days(), ", ".join(traits)]
	if c.role != "":
		_title.text += "\n[color=#d8c79a]the colony's %s[/color]" % runner.sim.role_name(c.role)

	var lines: PackedStringArray = []
	if not c.alive():
		lines.append("[color=#ff8a80]Died of %s.[/color]" % c.cause_of_death)
	else:
		lines.append("Doing: [b]%s[/b]" % view.short_action(c))
		var tgt: Variant = InspectText.action_target(c.action)
		if tgt != null:
			lines.append("Working on: %s" % InspectText.link(tgt))
	var brain := sim.brain_for(c)
	var model := ""
	if not c.last_decision.is_empty():
		model = LayaClient.short_model_name(String(c.last_decision["decision"].get("model", ""))) if brain != null and brain.id == "laya" else ""
	lines.append("[color=#aaaaaa]Brain: %s%s[/color]" % [brain.id if brain != null else "?", (" (%s)" % model) if model != "" else ""])
	lines.append_array(_family_lines(c))
	_info.text = "\n".join(lines)

	for need_id in _need_rows:
		var v := float(c.needs.get(need_id, 0.0))
		var bar: ProgressBar = _need_rows[need_id][0]
		var word: Label = _need_rows[need_id][1]
		bar.value = v
		var key := "good" if v > 55 else "mid" if v > 25 else "bad"
		bar.modulate = NEED_COLORS[key]
		word.text = sim.need_word(c, need_id)

	_render_why(c)
	_render_beliefs(c)
	_render_recent(c)
	_render_convos(c)
	_render_memories(c)
	if _state.visible:
		_state.text = String(c.last_decision["ctx"]["state_text"]) if not c.last_decision.is_empty() else "(no decision yet)"
	_chat_title.text = "Talk to %s" % c.name
	var can_talk := c.alive()
	_entry.editable = can_talk
	_send.disabled = not can_talk or chat.is_busy(c)


func _build_need_rows() -> void:
	for ch in _needs.get_children():
		ch.queue_free()
	_need_rows.clear()
	for need_id in runner.sim.content.needs:
		var lbl := Label.new()
		lbl.text = String(runner.sim.content.needs[need_id].get("label", need_id)).capitalize()
		lbl.custom_minimum_size = Vector2(80, 0)
		var bar := ProgressBar.new()
		bar.max_value = 100
		bar.show_percentage = false
		bar.custom_minimum_size = Vector2(150, 14)
		bar.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		var word := Label.new()
		word.add_theme_font_size_override("font_size", 13)
		_needs.add_child(lbl)
		_needs.add_child(bar)
		_needs.add_child(word)
		_need_rows[need_id] = [bar, word]


func _family_lines(c: Colonist) -> PackedStringArray:
	var sim := runner.sim
	var out: PackedStringArray = []
	var fam: PackedStringArray = []
	if c.partner_id >= 0:
		var p := sim.find_colonist(c.partner_id)
		if p != null:
			fam.append("partner %s" % InspectText.link(p))
	for pid in c.parent_ids:
		var p := sim.find_colonist(int(pid))
		if p != null:
			fam.append("parent %s" % InspectText.link(p))
	for o: Colonist in sim.colonists.values():
		if o.parent_ids.has(c.id):
			fam.append("child %s" % InspectText.link(o))
	if not fam.is_empty():
		out.append("Family: %s" % ", ".join(fam))
	var home := sim.buildings.get(c.home_id) as Building
	out.append("Home: %s" % (InspectText.link(home) if home != null else "no bed yet"))
	if c.pregnant_until > 0.0:
		out.append("[color=#f8bbd0]Expecting a child[/color]")
	# Closest relationships (affection -100..100).
	var ids := c.affection.keys()
	ids.sort_custom(func(a, b) -> bool: return float(c.affection[a]) > float(c.affection[b]))
	var rel: PackedStringArray = []
	for oid in ids.slice(0, 4):
		var o := sim.find_colonist(int(oid))
		if o != null and o.alive():
			rel.append("%s %+d" % [InspectText.link(o), int(c.affection[oid])])
	if not rel.is_empty():
		out.append("Feelings: %s" % ", ".join(rel))
	return out


func _render_why(c: Colonist) -> void:
	if c.last_decision.is_empty():
		_why_title.text = "[b]Last choice[/b]  [color=#aaaaaa]none yet[/color]"
		for ch in _why.get_children():
			ch.queue_free()
		return
	var last: Dictionary = c.last_decision
	var d: Dictionary = last["decision"]
	var key := "%s/%s" % [last["ctx"]["request_id"], last["began"]]
	if key == _why_key:
		return
	_why_key = key
	var mins := int(runner.sim.clock - float(last["t"]))
	var status := "" if last["began"] else "  [color=#ff8a80](couldn't do it)[/color]"
	_why_title.text = "[b]Last choice[/b] [color=#aaaaaa]%d min ago, what %s weighed:[/color]%s" % [mins, String(d.get("brain", "?")).capitalize(), status]
	for ch in _why.get_children():
		ch.queue_free()
	_add_prob_rows(last["ctx"]["options"], d.get("probs", {}), String(d.get("action", "")), 8)
	if String(d.get("action", "")) == "build" and not (d.get("build_probs", {}) as Dictionary).is_empty():
		_add_heading("Build what:")
		_add_prob_rows(last["ctx"]["build_options"], d["build_probs"], String(d.get("build_what", "")), 4)
	if String(d.get("action", "")) == "socialize" and not (d.get("social_probs", {}) as Dictionary).is_empty():
		_add_heading("With whom:")
		_add_prob_rows(last["ctx"]["social_options"], d["social_probs"], str(d.get("with_whom", "")), 4)


func _add_heading(text: String) -> void:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", 12)
	l.add_theme_color_override("font_color", Color(1, 1, 1, 0.6))
	_why.add_child(l)


func _add_prob_rows(opts: Array, probs: Dictionary, chosen: String, limit: int) -> void:
	var labels := {}
	for o in opts:
		labels[str(o["id"])] = o["label"]
	var keys := probs.keys()
	keys.sort_custom(func(a, b) -> bool: return float(probs[a]) > float(probs[b]))
	for k in keys.slice(0, limit):
		var row := HBoxContainer.new()
		var lbl := Label.new()
		lbl.text = String(labels.get(str(k), str(k)))
		lbl.custom_minimum_size = Vector2(150, 0)
		lbl.clip_text = true
		lbl.add_theme_font_size_override("font_size", 13)
		if str(k) == chosen:
			lbl.add_theme_color_override("font_color", Color(1.0, 0.9, 0.5))
		var bar := ProgressBar.new()
		bar.max_value = 1.0
		bar.value = float(probs[k])
		bar.show_percentage = false
		bar.custom_minimum_size = Vector2(0, 10)
		bar.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		bar.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		bar.modulate = Color(1.0, 0.85, 0.45) if str(k) == chosen else Color(0.6, 0.75, 0.95)
		var pct := Label.new()
		pct.text = "%d%%" % roundi(float(probs[k]) * 100.0)
		pct.custom_minimum_size = Vector2(42, 0)
		pct.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		pct.add_theme_font_size_override("font_size", 13)
		row.add_child(lbl)
		row.add_child(bar)
		row.add_child(pct)
		_why.add_child(row)


## What they believe about the stores vs what's really there, and what they've been asked to do.
func _render_beliefs(c: Colonist) -> void:
	var sim := runner.sim
	var lines: PackedStringArray = ["[b]What %s believes[/b]  [color=#aaaaaa](actual in grey)[/color]" % c.name]
	for k in Knowledge.STOCK_KEYS:
		lines.append("%s  [color=#aaaaaa]· %d[/color]" % [c.knowledge.phrase(k, sim.clock), int(sim.stock.get(k, 0))])
	lines.append("[color=#b8d4a8]%s[/color]" % sim.describer.places_phrase(sim, c))
	for r in sim.active_requests(c):
		lines.append("[color=#c9b6e4]%s asked them to %s (%dh ago)[/color]" % [r["from"], r["label"], int((sim.clock - float(r["t"])) / 60.0)])
	_beliefs.text = "\n".join(lines)


func _render_convos(c: Colonist) -> void:
	if c.conversations.is_empty():
		_convos.text = ""
		return
	var lines: PackedStringArray = ["[b]Recent conversations[/b]"]
	var items := c.conversations.duplicate()
	items.reverse()
	for conv in items.slice(0, 2):
		var other := _find_by_name(String(conv["with"]))
		var who := InspectText.link(other) if other != null else String(conv["with"])
		lines.append("[color=#aaaaaa]with[/color] %s [color=#aaaaaa]%s ago[/color]" % [who, _ago(float(conv["t"]))])
		for l in conv["lines"]:
			lines.append("  [color=#ffe082]%s:[/color] %s" % [l["who"], l["text"]])
		for x in conv.get("learned", []):
			lines.append("  [color=#9be29b]learned: %s ~%d (from %s)[/color]" % [x["key"], int(x["value"]), x["from"]])
	_convos.text = "\n".join(lines)


func _find_by_name(n: String) -> Colonist:
	for o in runner.sim.colonists.values() + runner.sim.graveyard:
		if (o as Colonist).name == n:
			return o
	return null


func _ago(t: float) -> String:
	var m := runner.sim.clock - t
	return "%d min" % int(m) if m < 60.0 else "%d h" % int(m / 60.0)


func _render_recent(c: Colonist) -> void:
	if c.recent.size() < 2:
		_recent.text = ""
		return
	var lines: PackedStringArray = ["[b]Recent choices[/b]"]
	var items := c.recent.duplicate()
	items.reverse()
	for r in items.slice(1, 7):   # the newest one is "Last choice" above
		var t := float(r["t"])
		var ok := "" if r["began"] else "  [color=#ff8a80]couldn't[/color]"
		lines.append("[color=#aaaaaa]%02d:%02d[/color]  %s [color=#aaaaaa]%d%%[/color]%s" % [
			int(fmod(t, 1440.0) / 60.0), int(fmod(t, 60.0)), r["label"], roundi(float(r["p"]) * 100.0), ok])
	_recent.text = "\n".join(lines)


func _render_memories(c: Colonist) -> void:
	var read_by_laya := c.top_memories(runner.sim.clock, StateDescriber.MAX_MEMORIES)
	var lines: PackedStringArray = ["[b]Memories[/b]  [color=#ffe082]yellow[/color][color=#aaaaaa] = what Laya reads now, [/color][color=#80d8ff]blue[/color][color=#aaaaaa] = from you[/color]"]
	var recent := c.memories.duplicate()
	recent.reverse()
	for m in recent.slice(0, 10):
		var col := "#dddddd"
		if read_by_laya.has(m):
			col = "#ffe082"
		elif m.get("source", "") == "watcher":
			col = "#80d8ff"
		var text := String(m["text"])
		if m.get("source", "") == "watcher" and col == "#ffe082":
			text = "[u]%s[/u]" % text
		lines.append("[color=%s]· %s[/color]" % [col, text])
	if c.memories.is_empty():
		lines.append("[color=#aaaaaa]nothing yet[/color]")
	_memories.text = "\n".join(lines)


func _render_chat() -> void:
	if colonist == null:
		return
	var lines: PackedStringArray = []
	for m in chat.history(colonist):
		if m["role"] == "user":
			lines.append("[color=#80d8ff][b]You:[/b][/color] %s" % m["content"])
		else:
			lines.append("[color=#ffe082][b]%s:[/b][/color] %s" % [colonist.name, m["content"]])
	if chat.is_busy(colonist):
		lines.append("[color=#aaaaaa][i]%s is thinking...[/i][/color]" % colonist.name)
	if lines.is_empty():
		lines.append("[color=#aaaaaa][i]%s can hear a voice nobody else can. What you talk about becomes a memory, and memories are part of what Laya reads when %s decides what to do.[/i][/color]" % [colonist.name, colonist.name])
	_chat_log.text = "\n".join(lines)
	_send.disabled = chat.is_busy(colonist) or not colonist.alive()


static func _rich(l: RichTextLabel, size: int) -> void:
	l.bbcode_enabled = true
	l.fit_content = true
	l.scroll_active = false
	l.selection_enabled = false
	l.add_theme_font_size_override("normal_font_size", size)
	l.add_theme_font_size_override("bold_font_size", size)
