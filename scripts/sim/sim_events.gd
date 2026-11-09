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
## A building (the meeting hall) started being enlarged / finished: it has its new footprint.
signal building_expansion_started(b: Building)
signal building_expanded(b: Building)
## The meeting bell rang (meeting = ColonyMeetings.current) / the meeting agreed a plan.
signal meeting_called(meeting: Dictionary)
signal meeting_held(plan: Dictionary)
## A week of the colony's story closed: {week, from_day, to_day, lines}.
signal week_summarized(week: Dictionary)
## Someone made a tool (data/tools.json).
signal tool_made(c: Colonist, tool: String)
## A fish was caught at this bit of shore.
signal fish_caught(c: Colonist, shore: Vector2i)
## A hunter killed a rabbit or a deer here.
signal animal_killed(c: Colonist, kind: String, cell: Vector2i)
## A hunter's spear missed and the animal got away.
signal hunt_missed(c: Colonist, kind: String, cell: Vector2i)
## A few animals of this kind moved in from the edge of the map (game was scarce).
signal animals_arrived(kind: String, home: Vector2i)
## A building was taken down (an old hall replaced by a bigger one).
signal building_removed(b: Building)
