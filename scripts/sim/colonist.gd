class_name Colonist
extends RefCounted
## One colonist agent. Pure data; behaviour comes from its Brain + ActionBase scripts.

const MAX_MEMORIES := 30

var id: int
var name: String
var age_minutes: float = 0.0
var lifespan_days: float = 30.0
var traits: Array[String] = []
var needs: Dictionary = {}          # need_id -> 0..100 (100 = satisfied)
var zero_minutes: Dictionary = {}   # need_id -> minutes spent at 0
var skills: Dictionary = {}         # skill_id -> level (0 = novice); each level = 8 hours of practice
var role: String = ""               # skill id they're known for (data/roles.json), "" = none yet
var practice: Dictionary = {}       # skill_id -> recent minutes of practice (fades; see data/roles.json)
var plan_day: int = -1              # day of the meeting whose plan they know (were there or were told)
var memories: Array = []            # [{t, text, importance}]
var recent: Array = []              # last few decisions, newest last: {t, action, label, p, began, brain}
var knowledge: Knowledge            # what they believe about the colony (see knowledge.gd)
var requests: Array = []            # things others asked them to do: {from, action, label, t, trust}
var conversations: Array = []       # recent talks: {t, with, lines:[{who, text}], intent}
var affection: Dictionary = {}      # other colonist id -> -100..100
var partner_id: int = -1
var parent_ids: Array = []
var home_id: int = -1               # house building id

var pos: Vector2 = Vector2.ZERO     # tile coords, float, for smooth movement
var cell: Vector2i = Vector2i.ZERO

var state: String = "idle"          # idle | thinking | acting | dead
var action: ActionBase = null
var thinking_since: float = 0.0
var hesitate_until: float = 0.0
var fail_streak: int = 0
var pending_request_id: int = -1
var last_decision: Dictionary = {}  # ctx + decision, for the inspector
var brain_id: String = ""           # "" = sim default brain

var pregnant_until: float = -1.0
var last_child_at: float = -1e9
var cause_of_death: String = ""
var died_at: float = -1.0


func age_days() -> float:
	return age_minutes / 1440.0


func is_adult() -> bool:
	return age_days() >= float(Content.get_instance().cfg("adult_age_days", 3.0))


func life_stage() -> String:
	return "adult" if is_adult() else "child"


func alive() -> bool:
	return state != "dead"


func trait_mod(key: String) -> float:
	## Multiplies every trait's numeric modifier for key (default 1.0).
	var m := 1.0
	var t_defs := Content.get_instance().traits
	for t in traits:
		m *= float(t_defs.get(t, {}).get("mods", {}).get(key, 1.0))
	return m


func skill_mult(skill: String) -> float:
	return 1.0 + 0.04 * minf(float(skills.get(skill, 0.0)), 15.0)


func add_skill_xp(skill: String, minutes: float) -> void:
	if skill == "":
		return
	# One level per ~8 hours of practice. Recent practice (which fades) is what roles go by.
	skills[skill] = float(skills.get(skill, 0.0)) + minutes / 480.0
	practice[skill] = float(practice.get(skill, 0.0)) + minutes


func remember(t: float, text: String, importance: float) -> Dictionary:
	var m := {"t": t, "text": text, "importance": importance}
	memories.append(m)
	if memories.size() > MAX_MEMORIES:
		# Drop the least important of the oldest half.
		var worst := 0
		for i in range(memories.size() / 2):
			if float(memories[i]["importance"]) < float(memories[worst]["importance"]):
				worst = i
		memories.remove_at(worst)
	return m


func top_memories(now: float, k: int) -> Array:
	## Rank by importance with recency decay (half-life one day).
	var scored := []
	for m in memories:
		var age_h := (now - float(m["t"])) / 60.0
		scored.append([float(m["importance"]) * pow(0.5, age_h / 24.0), m])
	scored.sort_custom(func(a, b): return a[0] > b[0])
	var out := []
	for i in mini(k, scored.size()):
		out.append(scored[i][1])
	return out


func to_dict() -> Dictionary:
	return {
		"id": id, "name": name, "age_days": snappedf(age_days(), 0.01), "stage": life_stage(),
		"traits": traits, "needs": needs.duplicate(), "skills": skills.duplicate(),
		"partner_id": partner_id, "home_id": home_id, "state": state,
		"action": action.describe() if action != null else "",
		"cell": [cell.x, cell.y], "cause_of_death": cause_of_death,
	}
