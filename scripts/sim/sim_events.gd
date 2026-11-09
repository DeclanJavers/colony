class_name SimEvents
extends RefCounted
## Event bus owned by a Sim. Views, UI, loggers and future upgrades subscribe here
## instead of being wired into the core.

signal ticked(clock_minutes: float)
signal hour_passed(day: int, hour: int)
signal day_passed(day: int)

signal colonist_born(c: Colonist)
signal colonist_grew_up(c: Colonist)
signal colonist_died(c: Colonist, cause: String)
signal couple_formed(a: Colonist, b: Colonist)
signal memory_added(c: Colonist, memory: Dictionary)

signal decision_requested(c: Colonist, ctx: Dictionary)
signal decision_made(c: Colonist, ctx: Dictionary, decision: Dictionary, began: bool)
signal action_started(c: Colonist, action: ActionBase)
signal action_finished(c: Colonist, action: ActionBase, ok: bool)

signal building_placed(b: Building)
signal building_completed(b: Building)
signal node_depleted(n: ResNode)
signal tier_unlocked(tier: int)
signal stock_changed(stock: Dictionary)
signal season_changed(season: String)
signal weather_changed(weather: String)
signal delivered(c: Colonist, resource: String, amount: float)   # a colonist dropped goods at storage
signal conversation_started(conv: Dictionary)    # {id, t, a, b, intent, topic}
signal conversation_finished(conv: Dictionary)   # + lines, facts, requests, learned
## Someone has become known for a kind of work (data/roles.json). role = skill id, "" = none.
signal role_changed(c: Colonist, role: String)
