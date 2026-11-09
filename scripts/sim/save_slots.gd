class_name SaveSlots
extends RefCounted
## Named save slots in user://saves/: <id>.tres (the game, see SaveGame) and <id>.png (a
## thumbnail), plus a small <id>.json with what the Load screen shows (so it doesn't have to read
## whole saves). "autosave" is written each morning. The old single save (user://save.tres) is
## listed too, as "Earlier save".

const DIR := "user://saves/"


static func path_of(id: String) -> String:
	return SaveGame.PATH if id == "legacy" else DIR + id + ".tres"


static func thumb_of(id: String) -> String:
	return DIR + id + ".png"


## A fresh slot id for a new save: slot_1, slot_2, ...
static func new_id() -> String:
	DirAccess.make_dir_recursive_absolute(DIR)
	var n := 1
	while FileAccess.file_exists(DIR + "slot_%d.tres" % n):
		n += 1
	return "slot_%d" % n


## Save the colony to a slot, with a thumbnail of what's on screen (if a viewport is given).
static func save(id: String, sim: Sim, history: Array, chronicle: Array, viewport: Viewport = null, name: String = "") -> Error:
	DirAccess.make_dir_recursive_absolute(DIR)
	var err := SaveGame.save(sim, history, path_of(id), chronicle)
	if err != OK:
		return err
	var meta := {"id": id, "name": name if name != "" else _default_name(id, sim), "day": sim.day(),
		"season": sim.season(), "pop": sim.alive_count(), "tier": sim.unlocked_tier, "seed": sim.seed_value,
		"saved_at": int(Time.get_unix_time_from_system())}
	var f := FileAccess.open(DIR + id + ".json", FileAccess.WRITE)
	if f != null:
		f.store_string(JSON.stringify(meta))
	if viewport != null:
		var img := viewport.get_texture().get_image()
		if img != null:
			img.resize(320, 180, Image.INTERPOLATE_LANCZOS)
			img.save_png(thumb_of(id))
	return OK


static func _default_name(id: String, sim: Sim) -> String:
	if id == "autosave":
		return "Autosave"
	return "Colony on seed %d" % sim.seed_value


## Every save there is, newest first: [{id, name, day, season, pop, tier, saved_at, thumb}].
static func list() -> Array:
	var out := []
	var d := DirAccess.open(DIR)
	if d != null:
		for f in d.get_files():
			if not f.ends_with(".json"):
				continue
			var meta = JSON.parse_string(FileAccess.get_file_as_string(DIR + f))
			if typeof(meta) == TYPE_DICTIONARY and FileAccess.file_exists(path_of(String(meta.get("id", "")))):
				meta["thumb"] = thumb_of(String(meta["id"])) if FileAccess.file_exists(thumb_of(String(meta["id"]))) else ""
				out.append(meta)
	if FileAccess.file_exists(SaveGame.PATH):
		var data := SaveGame.read()
		if not data.is_empty():
			out.append({"id": "legacy", "name": "Earlier save", "day": int(float(data["clock"]) / 1440.0),
				"season": "", "pop": (data.get("colonists", []) as Array).filter(func(c) -> bool: return not bool(c.get("dead", false))).size(),
				"tier": int(data.get("tier", 0)), "saved_at": int(FileAccess.get_modified_time(SaveGame.PATH)), "thumb": ""})
	out.sort_custom(func(a, b) -> bool: return int(a["saved_at"]) > int(b["saved_at"]))
	return out


static func latest() -> Dictionary:
	var l := list()
	return l[0] if not l.is_empty() else {}


static func read(id: String) -> Dictionary:
	return SaveGame.read(path_of(id))


## Delete a slot (its save, thumbnail and info). The old single save can't be deleted from here.
static func remove(id: String) -> void:
	if id == "legacy":
		return
	for ext in [".tres", ".png", ".json"]:
		if FileAccess.file_exists(DIR + id + ext):
			DirAccess.remove_absolute(DIR + id + ext)
