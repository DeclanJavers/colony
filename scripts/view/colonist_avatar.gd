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
var _emote_until := 0.0            # real time (s) until which an emote overrides the normal clip
var _hand: BoneAttachment3D        # right hand: holds the tool for the current job
var _skel: Skeleton3D
var _kid_head := -1.0              # head scale currently applied (children get a bigger head)
const KID_HEAD := 1.3
var _tool := ""
const TOOL_FOR_ACTION := {"gather_wood": "axe", "gather_stone": "pickaxe", "build": "hammer",
	"farm": "hoe", "fetch_water": "bucket"}
const WOOD := Color(0.55, 0.38, 0.22)
const METAL := Color(0.62, 0.64, 0.68)


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
	var skel: Skeleton3D = null
	for n in model.find_children("*", "Skeleton3D", true, false):
		skel = n
	_skel = skel
	if skel != null and skel.find_bone("RightHand") >= 0:
		_hand = BoneAttachment3D.new()
		_hand.bone_name = "RightHand"
		skel.add_child(_hand)
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
## Play a one-off reaction (e.g. "celebrate") for a few seconds, then go back to normal.
func emote(clip: String, seconds: float = 2.5) -> void:
	if not has_clip(clip):
		return
	_emote_until = Time.get_ticks_msec() / 1000.0 + seconds
	current = ""
	play(clip)


func update_for(c: Colonist, speed_mps: float) -> void:
	_update_tool(c)
	_update_head(c)
	if Time.get_ticks_msec() / 1000.0 < _emote_until and speed_mps < 0.05:
		return
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


## Children have bigger heads (the animations don't key bone scale, so this sticks).
func _update_head(c: Colonist) -> void:
	var want := 1.0 if c.is_adult() else KID_HEAD
	if want == _kid_head or _skel == null:
		return
	_kid_head = want
	var i := _skel.find_bone("Head")
	if i >= 0:
		_skel.set_bone_pose_scale(i, Vector3.ONE * want)


## Show the tool for what they're doing (only while actually working, not walking there).
func _update_tool(c: Colonist) -> void:
	if _hand == null:
		return
	var want := ""
	var a := c.action
	if a != null and a.phase == "working":
		want = String(TOOL_FOR_ACTION.get(String(a.def.get("id", "")), ""))
		# Picking up sticks and pebbles is done by hand: the axe and pick are for trees and rocks.
		var src: Variant = a.get("source")
		if want in ["axe", "pickaxe"] and src is Dictionary and a.sim.tool_for(src) == "":
			want = ""
	if want == _tool:
		return
	_tool = want
	for ch in _hand.get_children():
		ch.queue_free()
	if want != "":
		_hand.add_child(_make_tool(want))


## Tools are a few simple shapes in the same low-poly style. In the hand bone's space, +Y runs
## along the hand toward the fingers and the handle runs across the fist (local Z).
static func _make_tool(kind: String) -> Node3D:
	var root := Node3D.new()
	root.position = Vector3(0, 0.03, 0)
	var handle_len: float = {"axe": 0.40, "pickaxe": 0.40, "hammer": 0.26, "hoe": 0.52, "bucket": 0.0}[kind]
	if handle_len > 0.0:
		var h := _part(CylinderMesh.new(), WOOD)
		(h.mesh as CylinderMesh).top_radius = 0.016
		(h.mesh as CylinderMesh).bottom_radius = 0.016
		(h.mesh as CylinderMesh).height = handle_len
		(h.mesh as CylinderMesh).radial_segments = 6
		h.rotation.x = PI / 2.0
		h.position.z = handle_len * 0.3
		root.add_child(h)
	var tip: float = handle_len * 0.8
	match kind:
		"axe":
			var head := _part(BoxMesh.new(), METAL)
			(head.mesh as BoxMesh).size = Vector3(0.025, 0.11, 0.07)
			head.position = Vector3(0, 0.05, tip)
			root.add_child(head)
		"pickaxe":
			var head := _part(BoxMesh.new(), METAL)
			(head.mesh as BoxMesh).size = Vector3(0.025, 0.24, 0.03)
			head.position = Vector3(0, 0, tip)
			root.add_child(head)
		"hammer":
			var head := _part(BoxMesh.new(), METAL)
			(head.mesh as BoxMesh).size = Vector3(0.045, 0.1, 0.045)
			head.position = Vector3(0, 0, tip)
			root.add_child(head)
		"hoe":
			var head := _part(BoxMesh.new(), METAL)
			(head.mesh as BoxMesh).size = Vector3(0.02, 0.09, 0.06)
			head.position = Vector3(0, 0.05, tip)
			root.add_child(head)
		"bucket":
			var b := _part(CylinderMesh.new(), WOOD)
			(b.mesh as CylinderMesh).top_radius = 0.06
			(b.mesh as CylinderMesh).bottom_radius = 0.048
			(b.mesh as CylinderMesh).height = 0.09
			(b.mesh as CylinderMesh).radial_segments = 8
			b.position = Vector3(0, 0.06, 0)
			root.add_child(b)
	return root


static func _part(mesh: Mesh, col: Color) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	var m := StandardMaterial3D.new()
	m.albedo_color = col
	m.roughness = 0.8
	mi.material_override = m
	return mi


## True when the avatar shows lying down itself (so the view doesn't need to tip the model over).
func lies_down() -> bool:
	return current == "lie_down"
