"""Build the rigged villager, retarget Kimodo motions onto it, and export assets/models/villager.glb.

  Blender -b -P tools/blender/villager.py -- --out assets/models [--bvh DIR] [--preview DIR]

* The rig uses Kimodo's SOMA joint names (Hips, Spine1, Chest, Neck1, Head, LeftArm, LeftForeArm,
  LeftHand, LeftLeg, LeftShin, LeftFoot, and the Right* mirror). Its rest pose is a T-pose, the
  same as Kimodo's standard T-pose, so motion transfers as world-space rotation deltas:
      target_pose_world = delta * target_rest_world,  delta = source_pose_world * source_rest_world^-1
  which works even though the villager's proportions differ from the mocap actor's.
* Every body part is bound 100% to one bone (rigid low-poly skinning).
* Variant meshes for per-colonist variety, shown or hidden in Godot: hair_short, hair_long, beard,
  skirt. The shirt uses a material named "shirt" so Godot can tint it per colonist.
* BVH clips must be exported by Kimodo with --bvh_standard_tpose (rest = standard T-pose).
  Each <name>.bvh becomes an animation <name>, made in-place (horizontal root motion and the
  initial heading are removed; the game moves the colonist itself).
* Placeholder clips "idle" and "walk" are hand-keyed so the game can use the rig before any
  Kimodo motions exist. A BVH clip with the same name replaces them.
"""
import glob
import math
import os
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


def arg(name, default=None):
    return ARGS[ARGS.index(name) + 1] if name in ARGS else default


OUT = os.path.abspath(arg("--out", "assets/models"))
BVH_DIR = arg("--bvh")
TAKES = arg("--takes")          # optional JSON {clip: "chop_s1.bvh"}; default: <clip>_s0.bvh or <clip>.bvh
ONE_SHOT = {"sit", "lie_down", "wave", "celebrate"}   # not looped (they play once and hold)
LOOP_BLEND = 10                 # frames blended from the end of a clip back into its start
PREVIEW = arg("--preview")
FPS = 30

SKIN = (0.93, 0.72, 0.56)
HAIR = (0.28, 0.16, 0.08)
SHIRT = (0.85, 0.85, 0.85)   # neutral: tinted per colonist in Godot
PANTS = (0.26, 0.22, 0.20)
BOOT = (0.20, 0.12, 0.07)
EYE = (0.04, 0.04, 0.05)

# Joint positions (metres, Blender Z-up, character faces -Y, its left is +X), T-pose.
J = {
    "Hips": (0, 0, 0.50), "Spine1": (0, 0, 0.56), "Chest": (0, 0, 0.68), "Neck1": (0, 0, 0.82),
    "Head": (0, 0, 0.89), "HeadEnd": (0, 0, 1.10),
    "LeftArm": (0.19, 0, 0.785), "LeftForeArm": (0.37, 0, 0.785), "LeftHand": (0.53, 0, 0.785),
    "LeftHandEnd": (0.61, 0, 0.785),
    "LeftLeg": (0.075, 0, 0.48), "LeftShin": (0.075, 0, 0.28), "LeftFoot": (0.075, 0, 0.075),
    "LeftFootEnd": (0.075, -0.13, 0.03),
}
for k in list(J):
    if k.startswith("Left"):
        x, y, z = J[k]
        J["Right" + k[4:]] = (-x, y, z)

# bone -> (parent, tail joint)
BONES = {
    "Hips": (None, "Spine1"), "Spine1": ("Hips", "Chest"), "Chest": ("Spine1", "Neck1"),
    "Neck1": ("Chest", "Head"), "Head": ("Neck1", "HeadEnd"),
}
for s in ("Left", "Right"):
    BONES.update({
        f"{s}Arm": ("Chest", f"{s}ForeArm"), f"{s}ForeArm": (f"{s}Arm", f"{s}Hand"),
        f"{s}Hand": (f"{s}ForeArm", f"{s}HandEnd"),
        f"{s}Leg": ("Hips", f"{s}Shin"), f"{s}Shin": (f"{s}Leg", f"{s}Foot"),
        f"{s}Foot": (f"{s}Shin", f"{s}FootEnd"),
    })
ORDER = ["Hips", "Spine1", "Chest", "Neck1", "Head"] + [
    f"{s}{b}" for s in ("Left", "Right") for b in ("Arm", "ForeArm", "Hand", "Leg", "Shin", "Foot")]

_mats = {}


def mat(name, rgb):
    if name in _mats:
        return _mats[name]
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes.get("Principled BSDF")
    b.inputs["Base Color"].default_value = (*rgb, 1)
    b.inputs["Roughness"].default_value = 0.85
    m.diffuse_color = (*rgb, 1)
    _mats[name] = m
    return m


# ------------------------------------------------------------------------------------ mesh parts

class Parts:
    """Collects primitive parts per output mesh; each part is bound to one bone."""

    def __init__(self):
        self.groups = {}   # mesh name -> [objects]

    def _add(self, o, m, bone, group):
        o.data.materials.append(m)
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        vg = o.vertex_groups.new(name=bone)
        vg.add(list(range(len(o.data.vertices))), 1.0, "REPLACE")
        self.groups.setdefault(group, []).append(o)
        return o

    def seg(self, p0, p1, r0, r1, m, bone, group="body", verts=8):
        p0, p1 = Vector(p0), Vector(p1)
        d = p1 - p0
        bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r0, radius2=r1, depth=d.length, location=(p0 + p1) / 2)
        o = bpy.context.active_object
        o.rotation_mode = "QUATERNION"
        o.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(d.normalized())
        return self._add(o, m, bone, group)

    def box(self, size, loc, m, bone, group="body", bevel=0.012):
        bpy.ops.mesh.primitive_cube_add(size=1, location=loc)
        o = bpy.context.active_object
        o.scale = size
        bpy.ops.object.transform_apply(scale=True)
        if bevel:
            bpy.ops.object.modifier_add(type="BEVEL")
            o.modifiers[-1].width = bevel
            o.modifiers[-1].segments = 1
            bpy.ops.object.modifier_apply(modifier=o.modifiers[-1].name)
        return self._add(o, m, bone, group)

    def ball(self, r, loc, m, bone, group="body", sub=2, scale=(1, 1, 1)):
        bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=sub, radius=r, location=loc)
        bpy.context.active_object.scale = scale
        return self._add(bpy.context.active_object, m, bone, group)

    def join(self, arm):
        out = {}
        for name, objs in self.groups.items():
            bpy.ops.object.select_all(action="DESELECT")
            for o in objs:
                o.select_set(True)
            bpy.context.view_layer.objects.active = objs[0]
            bpy.ops.object.join()
            o = bpy.context.active_object
            o.name = name
            o.data.name = name
            o.parent = arm
            mod = o.modifiers.new("armature", "ARMATURE")
            mod.object = arm
            out[name] = o
        return out


def build_mesh(arm):
    p = Parts()
    sh, sk, pa, bo, ey, ha = (mat("shirt", SHIRT), mat("skin", SKIN), mat("pants", PANTS),
                              mat("boot", BOOT), mat("eye", EYE), mat("hair", HAIR))
    for s, sx in (("Left", 1), ("Right", -1)):
        lx = 0.075 * sx
        p.box((0.10, 0.17, 0.08), (lx, -0.025, 0.04), bo, f"{s}Foot")
        p.seg((lx, 0, 0.08), (lx, 0, 0.28), 0.048, 0.055, pa, f"{s}Shin")
        p.seg((lx, 0, 0.28), (lx, 0, 0.49), 0.056, 0.068, pa, f"{s}Leg")
        # T-posed arm along X: sleeve, forearm, hand.
        p.seg((0.19 * sx, 0, 0.785), (0.37 * sx, 0, 0.785), 0.046, 0.04, sh, f"{s}Arm")
        p.seg((0.37 * sx, 0, 0.785), (0.53 * sx, 0, 0.785), 0.036, 0.032, sk, f"{s}ForeArm")
        p.ball(0.042, (0.565 * sx, 0, 0.785), sk, f"{s}Hand", sub=1, scale=(1.15, 1, 0.8))
    p.box((0.19, 0.12, 0.10), (0, 0, 0.49), pa, "Hips")
    p.seg((0, 0, 0.42), (0, 0, 0.56), 0.15, 0.13, sh, "Hips", verts=10)            # tunic hem
    p.seg((0, 0, 0.535), (0, 0, 0.575), 0.137, 0.137, bo, "Spine1", verts=10)       # belt
    p.seg((0, 0, 0.55), (0, 0, 0.80), 0.13, 0.165, sh, "Chest", verts=10)           # chest
    p.ball(0.165, (0, 0, 0.80), sh, "Chest", scale=(1, 0.65, 0.32))                 # shoulders
    p.seg((0, 0, 0.82), (0, 0, 0.88), 0.045, 0.042, sk, "Neck1")
    hz, hr = 0.96, 0.12
    p.ball(hr, (0, 0, hz), sk, "Head", scale=(0.95, 1.0, 1.08))
    for ex in (-0.4, 0.4):
        p.box((0.022, 0.012, 0.032), (ex * hr, -hr * 0.92, hz + hr * 0.05), ey, "Head", bevel=0)
    p.box((0.025, 0.03, 0.035), (0, -hr * 1.02, hz - hr * 0.18), sk, "Head", bevel=0.006)
    # Variants (Godot shows/hides them per colonist).
    p.ball(hr * 1.07, (0, hr * 0.12, hz + hr * 0.22), ha, "Head", group="hair_short", scale=(1, 1.03, 0.9))
    p.ball(hr * 1.07, (0, hr * 0.12, hz + hr * 0.22), ha, "Head", group="hair_long", scale=(1, 1.03, 0.9))
    p.box((hr * 1.7, 0.07, 0.24), (0, hr * 0.75, hz - 0.08), ha, "Head", group="hair_long", bevel=0.02)
    p.ball(hr * 0.75, (0, -hr * 0.55, hz - hr * 0.55), ha, "Head", group="beard", sub=1, scale=(1, 0.6, 0.7))
    p.seg((0, 0, 0.30), (0, 0, 0.56), 0.19, 0.13, sh, "Hips", group="skirt", verts=10)
    return p.join(arm)


# ------------------------------------------------------------------------------------ rig

def build_armature():
    arm_data = bpy.data.armatures.new("villager_rig")
    arm = bpy.data.objects.new("villager", arm_data)
    bpy.context.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm_data.edit_bones
    for name in ORDER:
        parent, tail = BONES[name]
        b = eb.new(name)
        b.head = J[name]
        b.tail = J[tail]
        if parent:
            b.parent = eb[parent]
            b.use_connect = False
    bpy.ops.object.mode_set(mode="OBJECT")
    for pb in arm.pose.bones:
        pb.rotation_mode = "QUATERNION"
    return arm


def rest_rot(arm, name):
    return (arm.matrix_world @ arm.data.bones[name].matrix_local).to_quaternion()


def key_world_pose(arm, frame, world_rots, hips_offset=Vector((0, 0, 0))):
    """Key the armature so each bone's world rotation is world_rots[bone] (missing = rest)."""
    rest = {n: rest_rot(arm, n) for n in ORDER}
    pose = {n: world_rots.get(n, rest[n]) for n in ORDER}
    for n in ORDER:
        parent = BONES[n][0]
        if parent is None:
            basis = rest[n].inverted() @ pose[n]
        else:
            basis = (rest[n].inverted() @ rest[parent]) @ pose[parent].inverted() @ pose[n]
        pb = arm.pose.bones[n]
        q = basis.normalized()
        prev = pb.get("_prevq")
        if prev is not None and Quaternion(prev).dot(q) < 0:
            q.negate()   # keep quaternions continuous so interpolation doesn't flip
        pb["_prevq"] = list(q)
        pb.rotation_quaternion = q
        pb.keyframe_insert("rotation_quaternion", frame=frame)
    hips = arm.pose.bones["Hips"]
    hips.location = rest[ "Hips"].inverted() @ hips_offset
    hips.keyframe_insert("location", frame=frame)


def new_action(arm, name):
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    arm.animation_data_create()
    arm.animation_data.action = act
    for pb in arm.pose.bones:
        if "_prevq" in pb:
            del pb["_prevq"]
    return act


def chain(arm, local):
    """World rotations from per-bone rotations given in world axes, composed down the hierarchy."""
    out = {}
    for n in ORDER:
        parent = BONES[n][0]
        own = local.get(n, Quaternion())
        delta = (out[parent] @ rest_rot(arm, parent).inverted() if parent else Quaternion()) @ own
        out[n] = delta @ rest_rot(arm, n)
    return out


def q_axis(axis, deg):
    return Quaternion(Vector(axis), math.radians(deg))


def arms_down(extra_l=0.0, extra_r=0.0, swing_l=0.0, swing_r=0.0):
    # Rotate T-pose arms down to the sides (about the forward axis), then swing forward/back.
    return {
        "LeftArm": q_axis((1, 0, 0), swing_l) @ q_axis((0, 1, 0), 78 + extra_l),
        "RightArm": q_axis((1, 0, 0), swing_r) @ q_axis((0, 1, 0), -78 - extra_r),
        "LeftForeArm": q_axis((1, 0, 0), -8), "RightForeArm": q_axis((1, 0, 0), -8),
    }


def make_placeholders(arm):
    """Hand-keyed idle + walk so the game can use the rig before Kimodo clips exist."""
    new_action(arm, "idle")
    for f in range(0, 61, 5):
        t = f / 60 * math.tau
        rots = chain(arm, {**arms_down(extra_l=2 * math.sin(t), extra_r=2 * math.sin(t)),
                           "Chest": q_axis((1, 0, 0), 1.5 * math.sin(t))})
        key_world_pose(arm, f + 1, rots, Vector((0, 0, -0.004 * (1 - math.cos(t)))))
    new_action(arm, "walk")
    n = 30  # one stride cycle per second
    for f in range(0, n + 1, 2):
        t = f / n * math.tau
        s = math.sin(t)
        loc = {**arms_down(swing_l=-25 * s, swing_r=25 * s),
               "LeftLeg": q_axis((1, 0, 0), 28 * s), "RightLeg": q_axis((1, 0, 0), -28 * s),
               "LeftShin": q_axis((1, 0, 0), -30 * max(0.0, -math.sin(t + 0.6))),
               "RightShin": q_axis((1, 0, 0), -30 * max(0.0, math.sin(t + 0.6))),
               "Chest": q_axis((0, 0, 1), 5 * s)}
        key_world_pose(arm, f + 1, chain(arm, loc), Vector((0, 0, 0.015 * abs(math.cos(t)) - 0.015)))


# ------------------------------------------------------------------------------------ retarget

def retarget_bvh(arm, path, name=None):
    name = name or os.path.splitext(os.path.basename(path))[0]
    before = set(bpy.data.objects)
    bpy.ops.import_anim.bvh(filepath=path, global_scale=0.01, frame_start=1, use_fps_scale=False,
                            update_scene_fps=False, update_scene_duration=False, rotate_mode="NATIVE",
                            axis_forward="-Z", axis_up="Y")
    src = next(o for o in bpy.data.objects if o not in before and o.type == "ARMATURE")
    act = src.animation_data.action
    act.name = "_bvh_" + name   # the importer names its action after the file; free the name
    f0, f1 = (int(x) for x in act.frame_range)
    sc = bpy.context.scene
    src_rest = {n: (src.matrix_world @ src.data.bones[n].matrix_local).to_quaternion()
                for n in ORDER if n in src.data.bones}
    rest_hips = src.matrix_world @ src.data.bones["Hips"].head_local
    height_ratio = J["Hips"][2] / max(rest_hips.z, 1e-6)

    sc.frame_set(f0)
    fwd = (src.matrix_world @ src.pose.bones["Hips"].matrix).to_quaternion() @ src_rest["Hips"].inverted() @ Vector((0, -1, 0))
    yaw0 = math.atan2(fwd.x, -fwd.y)
    unyaw = q_axis((0, 0, 1), math.degrees(yaw0))   # turn the clip so it starts facing -Y

    frames = []
    for f in range(f0, f1 + 1):
        sc.frame_set(f)
        world = {}
        for n in ORDER:
            if n not in src_rest:
                continue
            pose_q = (src.matrix_world @ src.pose.bones[n].matrix).to_quaternion()
            delta = unyaw @ pose_q @ src_rest[n].inverted()
            world[n] = delta @ rest_rot(arm, n)
        hips_now = src.matrix_world @ src.pose.bones["Hips"].head
        frames.append((world, Vector((0, 0, (hips_now.z - rest_hips.z) * height_ratio))))   # in place: keep only bob
    if name not in ONE_SHOT and len(frames) > 3 * LOOP_BLEND:
        # Seamless loop: ease the last frames into the first pose so repeating doesn't hitch.
        first_w, first_o = frames[0]
        n_fr = len(frames)
        for j in range(LOOP_BLEND):
            i = n_fr - LOOP_BLEND + j
            w = (j + 1) / LOOP_BLEND
            w = w * w * (3 - 2 * w)   # smoothstep
            rots, off = frames[i]
            frames[i] = ({b: rots[b].slerp(first_w[b], w) for b in rots}, off.lerp(first_o, w))
    new_action(arm, name)
    for k, (world, offset) in enumerate(frames):
        key_world_pose(arm, k + 1, world, offset)
    bpy.data.objects.remove(src)
    bpy.data.actions.remove(act)
    print(f"retargeted {name}: {f1 - f0 + 1} frames")


# ------------------------------------------------------------------------------------ preview

def preview(arm, out_dir, frames_per_clip=6):
    """Render a few frames of each action to PNG strips for checking the motion."""
    os.makedirs(out_dir, exist_ok=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_EEVEE"
    sc.render.resolution_x, sc.render.resolution_y = 300, 360
    sc.view_settings.view_transform = "Standard"
    world = sc.world or bpy.data.worlds.new("w")
    sc.world = world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.62, 0.74, 0.88, 1)
    world.node_tree.nodes["Background"].inputs[1].default_value = 0.5
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 4
    sun.rotation_euler = (math.radians(48), math.radians(12), math.radians(-30))
    sc.collection.objects.link(sun)
    bpy.ops.mesh.primitive_plane_add(size=20)
    bpy.context.active_object.data.materials.append(mat("grass", (0.30, 0.48, 0.22)))
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = 1.6
    yaw, pitch = math.radians(35), math.radians(12)
    d = Vector((math.sin(yaw) * math.cos(pitch), -math.cos(yaw) * math.cos(pitch), math.sin(pitch)))
    cam.location = Vector((0, 0, 0.55)) + d * 10
    cam.rotation_mode = "QUATERNION"
    cam.rotation_quaternion = (-d).to_track_quat("-Z", "Y")
    sc.collection.objects.link(cam)
    sc.camera = cam
    for act in bpy.data.actions:
        if act.name.endswith("Action") or not act.use_fake_user:
            continue
        arm.animation_data.action = act
        f0, f1 = (int(x) for x in act.frame_range)
        for i in range(frames_per_clip):
            f = f0 + round(i * (f1 - f0) / max(1, frames_per_clip - 1))
            sc.frame_set(f)
            sc.render.filepath = os.path.join(out_dir, f"{act.name}_{i}.png")
            bpy.ops.render.render(write_still=True)
        print("previewed", act.name)


# ------------------------------------------------------------------------------------ main

bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete()
bpy.context.scene.render.fps = FPS
arm = build_armature()
meshes = build_mesh(arm)
make_placeholders(arm)
if BVH_DIR:
    import json
    import re
    chosen = {}
    for path in sorted(glob.glob(os.path.join(BVH_DIR, "*.bvh"))):
        stem = os.path.splitext(os.path.basename(path))[0]
        m = re.match(r"(.+)_s(\d+)$", stem)
        clip, take = (m.group(1), int(m.group(2))) if m else (stem, 0)
        if take == 0 or clip not in chosen:
            chosen.setdefault(clip, path)
            if take == 0:
                chosen[clip] = path
    if TAKES and os.path.exists(TAKES):
        for clip, fname in json.load(open(TAKES)).items():
            if not clip.startswith("_") and os.path.exists(os.path.join(BVH_DIR, fname)):
                chosen[clip] = os.path.join(BVH_DIR, fname)
    for clip, path in sorted(chosen.items()):
        if clip in bpy.data.actions:
            bpy.data.actions.remove(bpy.data.actions[clip])   # Kimodo clip replaces placeholder
        retarget_bvh(arm, path, clip)
arm.animation_data.action = bpy.data.actions["idle"]
for pb in arm.pose.bones:
    if "_prevq" in pb:
        del pb["_prevq"]

if PREVIEW:
    preview(arm, os.path.abspath(PREVIEW))

os.makedirs(OUT, exist_ok=True)
bpy.ops.object.select_all(action="DESELECT")
arm.select_set(True)
for o in meshes.values():
    o.select_set(True)
path = os.path.join(OUT, "villager.glb")
bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True,
                          export_animation_mode="ACTIONS", export_force_sampling=True,
                          export_def_bones=False, export_apply=False)
print("wrote", path, "actions:", [a.name for a in bpy.data.actions if a.use_fake_user])
