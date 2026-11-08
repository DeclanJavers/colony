class_name ColonistAvatar
extends Node3D
## The animated villager for one colonist (assets/models/villager.glb, from tools/blender/villager.py).
##
## Looks: the shirt (material "shirt") takes the colonist's colour; hair style, beard, dress, hair
## colour and skin tone are picked from the colonist's id, so everyone looks a little different.
## Animation: plays the clip that matches what the colonist is doing (see CLIP_FOR_ACTION). Clips
## that don't exist yet fall back to walk/idle, so new Kimodo clips show up just by being in the GLB.

const MODEL := "res://assets/models/villager.glb"
const BLEND := 0.25
const WALK_SPEED_REF := 0.6          # m/s the walk clip looks right at
const CLIP_FOR_ACTION := {           # action id -> clip while working at the spot
	"gather_wood": "chop", "gather_stone": "mine", "gather_food": "gather", "fetch_water": "draw_water",
	"build": "build", "farm": "farm", "eat": "eat", "drink": "drink", "rest": "sit", "sleep": "lie_down",
	"socialize": "talk", "have_child": "talk",
}
const ONE_SHOT := ["sit", "lie_down", "wave", "celebrate"]   # play once and hold the last frame
const SKIN_TONES := [Color(0.93, 0.72, 0.56), Color(0.80, 0.58, 0.42), Color(0.62, 0.42, 0.28), Color(0.45, 0.30, 0.20)]
const HAIR_COLORS := [Color(0.28, 0.16, 0.08), Color(0.08, 0.06, 0.05), Color(0.75, 0.55, 0.25), Color(0.55, 0.22, 0.10), Color(0.6, 0.6, 0.6)]

static var _scene: PackedScene
static var _loops_set := false

var model: Node3D
var anim: AnimationPlayer
var current := ""


static func available() -> bool:
	return ResourceLoader.exists(MODEL)


func setup(c: Colonist, shirt: Color) -> void:
	if _scene == null:
		_scene = load(MODEL)
	model = _scene.instantiate()
	add_child(model)
	anim = model.find_child("AnimationPlayer", true, false) as AnimationPlayer
	if anim != null and not _loops_set:
		_loops_set = true
		for name in anim.get_animation_list():
			anim.get_animation(name).loop_mode = Animation.LOOP_NONE if name in ONE_SHOT else Animation.LOOP_LINEAR
	_dress(c, shirt)
	play("idle")


func _dress(c: Colonist, shirt: Color) -> void:
	var h := absi(hash(c.id * 7919 + 13))
	var style := h % 4                     # 0 short+beard, 1 long+dress, 2 short, 3 long
	_show("hair_short", style == 0 or style == 2)
	_show("hair_long", style == 1 or style == 3)
	_show("beard", style == 0 and c.is_adult())
	_show("skirt", style == 1)
	var skin: Color = SKIN_TONES[(h / 4) % SKIN_TONES.size()]
	var hair: Color = HAIR_COLORS[(h / 16) % HAIR_COLORS.size()]
	for mi in model.find_children("*", "MeshInstance3D", true, false):
		_recolor(mi as MeshInstance3D, {"shirt": shirt, "skin": skin, "hair": hair})


func _show(mesh_name: String, on: bool) -> void:
	var n := model.find_child(mesh_name, true, false) as Node3D
	if n != null:
		n.visible = on


func _recolor(mi: MeshInstance3D, colors: Dictionary) -> void:
	if mi.mesh == null:
		return
	for i in mi.mesh.get_surface_count():
		var m := mi.mesh.surface_get_material(i)
		if m == null or not colors.has(m.resource_name):
			continue
		var copy := (m as BaseMaterial3D).duplicate() as BaseMaterial3D
		copy.albedo_color = colors[m.resource_name]
		mi.set_surface_override_material(i, copy)


func has_clip(name: String) -> bool:
	return anim != null and anim.has_animation(name)


func play(name: String, speed := 1.0) -> void:
	if anim == null:
		return
	if not has_clip(name):
		name = "idle"
	if name != current:
		current = name
		anim.play(name, BLEND)
	anim.speed_scale = speed


## Pick and play the clip for this frame. speed_mps = how fast the colonist is moving on screen.
func update_for(c: Colonist, speed_mps: float) -> void:
	var a := c.action
	if speed_mps > 0.05:
		var carrying := a != null and a.phase == "carrying"
		var clip := "carry" if carrying and has_clip("carry") else "walk"
		play(clip, clampf(speed_mps / WALK_SPEED_REF, 0.5, 4.0))
		return
	if a == null:
		play("idle")
		return
	if a.phase == "walking":
		play("idle")   # arrived or waiting for a path
		return
	play(String(CLIP_FOR_ACTION.get(String(a.def.get("id", "")), "idle")))


## True when the avatar shows lying down itself (so the view doesn't need to tip the model over).
func lies_down() -> bool:
	return current == "lie_down"
