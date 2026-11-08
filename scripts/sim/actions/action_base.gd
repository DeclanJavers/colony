class_name ActionBase
extends RefCounted
## Base class for action TYPES. Each file in scripts/sim/actions/types/ is one type
## (consume, gather, build, ...). data/actions.json creates many actions from few types
## by passing params. To add a new kind of behaviour, drop a new file in types/.

enum { RUNNING, DONE, FAILED }

var sim: Sim
var c: Colonist
var def: Dictionary
var params: Dictionary
var extra: Dictionary = {}       # decision extras: build_what, with_whom
var path: Array[Vector2i] = []
var goal_cell := Vector2i(-1, -1)
var phase: String = ""
var timer: float = 0.0
var fail_reason: String = ""
var started_at: float = 0.0


func setup(p_sim: Sim, p_c: Colonist, p_def: Dictionary) -> ActionBase:
	sim = p_sim
	c = p_c
	def = p_def
	params = p_def.get("params", {})
	return self


## ---- Override these -------------------------------------------------------

## Can this colonist start this action right now? (physical possibility only)
func available() -> bool:
	return true


## Pick targets, reserve things. Return false if it can't start after all.
func begin() -> bool:
	return true


## Advance by dt game-minutes. Return RUNNING, DONE or FAILED.
func tick(_dt: float) -> int:
	return DONE


## Cleanup (release reservations). Always called once after begin() succeeded.
func end(_ok: bool) -> void:
	pass


## Multiplier on a need's decay while doing this action (sleep slows hunger, etc).
func need_decay_mult(_need: String) -> float:
	return 1.0


## Short text shown over the colonist's head and in the inspector.
func describe() -> String:
	var base: String = def.get("label", def.get("id", "?"))
	return base if phase == "" else "%s (%s)" % [base, phase]


## Broad category, used by UtilityBrain trait biases: work | gather | build | rest | socialize | need
func kind() -> String:
	return "work"


## ---- Helpers --------------------------------------------------------------

func fail(reason: String) -> int:
	fail_reason = reason
	return FAILED


func go_to(cell: Vector2i, own_spot: bool = true) -> bool:
	## Plan a path to cell. False if unreachable.
	## own_spot: if someone else is already standing there or heading there, take the nearest
	## free walkable cell next to it instead, so colonists don't stand inside each other.
	if cell.x < 0:
		goal_cell = cell
		return false
	sim.ensure_walkable(c)
	if own_spot:
		cell = sim.free_spot_near(cell, c)
	goal_cell = cell
	if c.cell == cell:
		path.clear()
		return true
	path = sim.map.find_path(c.cell, cell)
	return not path.is_empty()


func walk(dt: float) -> bool:
	## Move along the path. Returns true once arrived.
	var remaining := sim.move_speed(c) * dt
	while remaining > 0.0 and not path.is_empty():
		var nxt: Vector2i = path[0]
		var to := Vector2(nxt) - c.pos
		var d := to.length()
		if d <= remaining:
			c.pos = Vector2(nxt)
			c.cell = nxt
			path.remove_at(0)
			remaining -= d
		else:
			c.pos += to / d * remaining
			remaining = 0.0
	return path.is_empty()


func work_rate(skill: String) -> float:
	return c.skill_mult(skill) * c.trait_mod("work_speed") * sim.night_mult() * sim.exhaustion_mult(c, "work_speed")
