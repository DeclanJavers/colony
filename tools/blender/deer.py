"""Build the rigged, animated deer (docs/STYLE.md) and export assets/models/deer.glb, deer_2.glb, deer_3.glb.

  Blender -b -P tools/blender/deer.py -- --out assets/models

* Low-poly and chunky like everything else: a deep chest, slim legs with dark hooves, the neck
  carried forward (not upright), a pale belly, rump patch and tail, big ears. deer = a stag with
  antlers, deer_2 = a doe, deer_3 = a paler doe.
* Rig: Hips (root) > Spine > Neck > Head (> EarL, EarR), Hips > Tail, and two bones per leg
  (upper, lower; the hoof rides on the lower). Every part is bound 100% to one bone, like the villager.
* Clips, looped, in place (the game moves the deer): idle (breathing, ear and tail flicks, a look
  round), graze (head down, nibbling, steps now and then), walk (four-beat walk), run (bounding).
* The deer faces -Y in Blender, which is +Z in Godot (same as every model).
"""
import math
import os
import sys

import bpy
from mathutils import Quaternion, Vector

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = os.path.abspath(ARGS[ARGS.index("--out") + 1] if "--out" in ARGS else "assets/models")
FPS = 24


def lin(h):
    h = h.lstrip("#")
    out = []
    for i in (0, 2, 4):
        c = int(h[i:i + 2], 16) / 255.0
        out.append(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4)
    return tuple(out)


_mats = {}


def mat(name, hexcol):
    if name in _mats:
        return _mats[name]
    rgb = lin(hexcol)
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes.get("Principled BSDF")
    b.inputs["Base Color"].default_value = (*rgb, 1)
    b.inputs["Roughness"].default_value = 0.9
    b.inputs["Specular IOR Level"].default_value = 0.2
    m.diffuse_color = (*rgb, 1)
    _mats[name] = m
    return m


# Joints (metres; Z up; the deer faces -Y; its left is +X).
J = {
    "Hips": (0, 0.24, 0.66), "Spine": (0, 0.0, 0.67), "Chest": (0, -0.22, 0.69),
    "Neck": (0, -0.25, 0.76), "Head": (0, -0.42, 0.99), "Nose": (0, -0.60, 0.92),
    "Tail": (0, 0.34, 0.71), "TailEnd": (0, 0.40, 0.62),
}
for s, sx in (("L", 1), ("R", -1)):
    x = 0.085 * sx
    J[f"Front{s}"] = (x, -0.20, 0.58)
    J[f"FrontKnee{s}"] = (x, -0.19, 0.31)
    J[f"FrontHoof{s}"] = (x, -0.20, 0.03)
    J[f"Hind{s}"] = (x, 0.25, 0.60)
    J[f"Hock{s}"] = (x, 0.31, 0.30)
    J[f"HindHoof{s}"] = (x, 0.27, 0.03)
    J[f"Ear{s}"] = (0.04 * sx, -0.41, 1.04)
    J[f"EarTip{s}"] = (0.11 * sx, -0.38, 1.10)

# bone -> (parent, head joint, tail joint)
BONES = {
    "Hips": (None, "Hips", "Spine"), "Spine": ("Hips", "Spine", "Chest"),
    "Neck": ("Spine", "Neck", "Head"), "Head": ("Neck", "Head", "Nose"),
    "Tail": ("Hips", "Tail", "TailEnd"),
}
for s in ("L", "R"):
    BONES.update({
        f"FrontUpper{s}": ("Spine", f"Front{s}", f"FrontKnee{s}"),
        f"FrontLower{s}": (f"FrontUpper{s}", f"FrontKnee{s}", f"FrontHoof{s}"),
        f"HindUpper{s}": ("Hips", f"Hind{s}", f"Hock{s}"),
        f"HindLower{s}": (f"HindUpper{s}", f"Hock{s}", f"HindHoof{s}"),
        f"Ear{s}": ("Head", f"Ear{s}", f"EarTip{s}"),
    })
ORDER = ["Hips", "Spine", "Neck", "Head", "Tail"] + [
    f"{b}{s}" for s in ("L", "R") for b in ("FrontUpper", "FrontLower", "HindUpper", "HindLower", "Ear")]


class Parts:
    """Primitive parts, each bound 100% to one bone, joined into one mesh."""

    def __init__(self):
        self.objs = []

    def _add(self, o, m, bone):
        o.data.materials.append(m)
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        vg = o.vertex_groups.new(name=bone)
        vg.add(list(range(len(o.data.vertices))), 1.0, "REPLACE")
        self.objs.append(o)
        return o

    def seg(self, p0, p1, r0, r1, m, bone, verts=6):
        p0, p1 = Vector(p0), Vector(p1)
        d = p1 - p0
        bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r0, radius2=r1, depth=d.length, location=(p0 + p1) / 2)
        o = bpy.context.active_object
        o.rotation_mode = "QUATERNION"
        o.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(d.normalized())
        return self._add(o, m, bone)

    def ball(self, r, loc, m, bone, sub=2, scale=(1, 1, 1)):
        bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=sub, radius=r, location=loc)
        bpy.context.active_object.scale = scale
        return self._add(bpy.context.active_object, m, bone)

    def box(self, size, loc, m, bone, rot=(0, 0, 0)):
        bpy.ops.mesh.primitive_cube_add(size=1, location=loc, rotation=rot)
        bpy.context.active_object.scale = size
        return self._add(bpy.context.active_object, m, bone)

    def join(self, arm, name):
        bpy.ops.object.select_all(action="DESELECT")
        for o in self.objs:
            o.select_set(True)
        bpy.context.view_layer.objects.active = self.objs[0]
        bpy.ops.object.join()
        o = bpy.context.active_object
        o.name = name
        o.data.name = name
        for p in o.data.polygons:
            p.use_smooth = False
        o.parent = arm
        mod = o.modifiers.new("armature", "ARMATURE")
        mod.object = arm
        return o


def build_armature():
    data = bpy.data.armatures.new("deer_rig")
    arm = bpy.data.objects.new("deer", data)
    bpy.context.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = data.edit_bones
    for name in ORDER:
        parent, h, t = BONES[name]
        b = eb.new(name)
        b.head = J[h]
        b.tail = J[t]
        b.use_connect = False
        if parent:
            b.parent = eb[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return arm


def build_mesh(arm, coat, antlers):
    coat_m = mat("fur_" + coat, coat)
    light = mat("fur_light", "#D9B98C")          # belly, inner legs
    rump = mat("fur_rump", "#EFE2C4")            # rump patch and tail underside
    dark = mat("hoof", "#3A2A20")                # hooves, nose
    eye = mat("eye", "#1E1A1A")
    antler = mat("antler", "#D2B48C")
    p = Parts()
    # Body: one long, narrow barrel (chest on Spine, rump on Hips, overlapping so it reads as one),
    # deeper at the chest; a pale belly, a pale patch on the rump.
    p.ball(0.18, (0, -0.11, 0.675), coat_m, "Spine", scale=(0.66, 1.55, 0.88))
    p.ball(0.17, (0, 0.14, 0.67), coat_m, "Hips", scale=(0.64, 1.35, 0.8))
    p.ball(0.13, (0, 0.0, 0.575), light, "Spine", sub=1, scale=(0.62, 1.9, 0.42))
    p.ball(0.09, (0, 0.335, 0.675), rump, "Hips", sub=1, scale=(0.72, 0.3, 0.85))
    # Neck carried forward, thick where it meets the chest; head with a darker nose.
    p.ball(0.09, (0, -0.25, 0.75), coat_m, "Neck", sub=1, scale=(0.8, 0.9, 1.0))
    p.seg((0, -0.255, 0.75), (0, -0.42, 0.975), 0.075, 0.05, coat_m, "Neck")
    p.ball(0.06, (0, -0.25, 0.71), light, "Neck", sub=1, scale=(0.7, 0.8, 0.9))    # throat
    p.ball(0.075, (0, -0.47, 0.98), coat_m, "Head", scale=(0.85, 1.35, 0.85))
    p.ball(0.042, (0, -0.57, 0.935), coat_m, "Head", sub=1, scale=(0.85, 1.25, 0.8))
    p.ball(0.022, (0, -0.615, 0.93), dark, "Head", sub=1)
    for sx in (-1, 1):
        p.ball(0.012, (0.05 * sx, -0.50, 1.005), eye, "Head", sub=1)
        s = "L" if sx > 0 else "R"
        # Ears: flattened leaves, angled out.
        p.ball(0.055, (0.075 * sx, -0.405, 1.045), coat_m, f"Ear{s}", sub=1, scale=(1.0, 0.32, 0.6))
    # Legs: thigh/upper leg tapering into a thin lower leg, dark hoof.
    for s, sx in (("L", 1), ("R", -1)):
        p.seg(J[f"Front{s}"], J[f"FrontKnee{s}"], 0.05, 0.03, coat_m, f"FrontUpper{s}")
        p.seg(J[f"FrontKnee{s}"], J[f"FrontHoof{s}"], 0.026, 0.02, coat_m, f"FrontLower{s}")
        p.box((0.04, 0.05, 0.04), (J[f"FrontHoof{s}"][0], J[f"FrontHoof{s}"][1] - 0.005, 0.02), dark, f"FrontLower{s}")
        p.ball(0.08, (0.08 * sx, 0.22, 0.58), coat_m, f"HindUpper{s}", sub=1, scale=(0.5, 1.0, 1.2))   # haunch
        p.seg(J[f"Hind{s}"], J[f"Hock{s}"], 0.05, 0.03, coat_m, f"HindUpper{s}")
        p.seg(J[f"Hock{s}"], J[f"HindHoof{s}"], 0.026, 0.02, coat_m, f"HindLower{s}")
        p.box((0.04, 0.05, 0.04), (J[f"HindHoof{s}"][0], J[f"HindHoof{s}"][1] - 0.005, 0.02), dark, f"HindLower{s}")
    # Tail: short, dark above, pale under.
    p.seg(J["Tail"], J["TailEnd"], 0.035, 0.022, coat_m, "Tail")
    p.ball(0.03, (0, 0.375, 0.645), rump, "Tail", sub=1, scale=(0.9, 0.7, 1.2))
    if antlers:
        for sx in (-1, 1):
            base = (0.03 * sx, -0.42, 1.04)
            mid = (0.10 * sx, -0.38, 1.17)
            top = (0.13 * sx, -0.43, 1.29)
            p.seg(base, mid, 0.016, 0.013, antler, "Head")
            p.seg(mid, top, 0.013, 0.008, antler, "Head")
            p.seg(mid, (0.06 * sx, -0.49, 1.23), 0.011, 0.007, antler, "Head")
            p.seg((0.115 * sx, -0.405, 1.23), (0.18 * sx, -0.38, 1.27), 0.01, 0.006, antler, "Head")
    return p.join(arm, "deer_body")


# ------------------------------------------------------------------------------------ animation

def rest_q(arm, bone):
    return arm.data.bones[bone].matrix_local.to_quaternion()


def turn(arm, bone, axis, deg):
    """Rotation of `bone` about a world axis (in its rest frame), as the pose bone's local rotation."""
    b = rest_q(arm, bone)
    w = Quaternion(Vector(axis), math.radians(deg))
    return b.inverted() @ w @ b


def pose(arm, frame, rots, hips_offset=(0, 0, 0)):
    """rots: {bone: [(axis, degrees), ...]} applied in order. Unlisted bones go back to rest."""
    for pb in arm.pose.bones:
        q = Quaternion()
        for axis, deg in rots.get(pb.name, []):
            q = turn(arm, pb.name, axis, deg) @ q
        pb.rotation_mode = "QUATERNION"
        pb.rotation_quaternion = q
        pb.keyframe_insert("rotation_quaternion", frame=frame)
    hips = arm.pose.bones["Hips"]
    hips.location = rest_q(arm, "Hips").inverted() @ Vector(hips_offset)
    hips.keyframe_insert("location", frame=frame)


X = (1, 0, 0)        # +X turn: a leg's foot swings back (toward +Y); the neck and head tip down
Y = (0, 1, 0)
Z = (0, 0, 1)


def legs(phase_fl, phase_fr, phase_hl, phase_hr, t, swing, lift):
    """Leg rotations for a gait at cycle time t (0..1): each leg swings forward and back
    (forward = negative X turn), and folds its lower leg while it's off the ground."""
    out = {}
    for name, ph, front in (("L", phase_fl, True), ("R", phase_fr, True)):
        a = 2 * math.pi * (t - ph)
        out[f"FrontUpper{name}"] = [(X, -swing * math.sin(a))]
        fold = lift * max(0.0, math.sin(a + math.pi / 2))     # folds as it comes forward
        out[f"FrontLower{name}"] = [(X, fold * 1.6)]          # the knee folds back
    for name, ph in (("L", phase_hl), ("R", phase_hr)):
        a = 2 * math.pi * (t - ph)
        out[f"HindUpper{name}"] = [(X, -swing * math.sin(a))]
        fold = lift * max(0.0, math.sin(a + math.pi / 2))
        out[f"HindLower{name}"] = [(X, -fold * 1.3)]          # the hock folds forward
    return out


def merge(*ds):
    out = {}
    for d in ds:
        for k, v in d.items():
            out.setdefault(k, []).extend(v)
    return out


def new_action(arm, name):
    if arm.animation_data is None:
        arm.animation_data_create()
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    arm.animation_data.action = act
    return act


def make_walk(arm):
    new_action(arm, "walk")
    n = 24
    for f in range(n + 1):
        t = f / n
        # Four-beat walk: left hind, left fore, right hind, right fore.
        r = legs(0.25, 0.75, 0.0, 0.5, t, 18, 22)
        bob = math.sin(4 * math.pi * t)
        r = merge(r, {"Neck": [(X, 3 * bob)], "Head": [(X, -2 * bob)],
                      "Spine": [(Z, 2.5 * math.sin(2 * math.pi * t))], "Tail": [(X, 6)]})
        pose(arm, f + 1, r, (0, 0, 0.012 * bob))


def make_run(arm):
    new_action(arm, "run")
    n = 14
    for f in range(n + 1):
        t = f / n
        # Bounding: forelegs together, hind legs together, half a cycle apart; the back flexes.
        r = legs(0.0, 0.06, 0.5, 0.56, t, 38, 40)
        a = 2 * math.pi * t
        r = merge(r, {"Spine": [(X, 7 * math.sin(a))], "Neck": [(X, -8 + 6 * math.sin(a + 1.0))],
                      "Head": [(X, -6)], "Tail": [(X, 35)],                       # flag up when fleeing
                      "EarL": [(X, -25)], "EarR": [(X, -25)]})
        pose(arm, f + 1, r, (0, 0, 0.06 * max(0.0, math.sin(a)) + 0.02))


def make_graze(arm):
    new_action(arm, "graze")
    n = 96
    for f in range(n + 1):
        t = f / n
        nib = math.sin(2 * math.pi * t * 6)                    # nibbling
        step = max(0.0, math.sin(2 * math.pi * (t - 0.6))) if 0.6 < t < 1.1 else 0.0
        r = {"Neck": [(X, 98)], "Head": [(X, 30 + 4 * nib)],
             "Spine": [(X, 7)], "HindUpperL": [(X, 6)], "HindUpperR": [(X, 6)], "FrontUpperL": [(X, -10 * step)], "FrontLowerL": [(X, 25 * step)],
             "EarL": [(Y, 8 * math.sin(2 * math.pi * t))], "EarR": [(Y, -8 * math.sin(2 * math.pi * t + 0.7))],
             "Tail": [(Z, 25 * math.sin(2 * math.pi * t * 3)) if t < 0.2 else (Z, 0)]}
        pose(arm, f + 1, r, (0, 0, -0.02))


def make_idle(arm):
    new_action(arm, "idle")
    n = 120
    for f in range(n + 1):
        t = f / n
        breathe = math.sin(2 * math.pi * t * 2)
        look = math.sin(2 * math.pi * t) * (1.0 if 0.25 < t < 0.75 else 0.3)    # a look round
        flick = math.sin(2 * math.pi * t * 8) if 0.4 < t < 0.48 else 0.0
        r = {"Spine": [(X, 1.0 * breathe)], "Neck": [(Z, 22 * look), (X, -4)], "Head": [(Z, 10 * look)],
             "EarL": [(Y, 18 * flick)], "EarR": [(Y, -10 * flick)],
             "Tail": [(Z, 30 * math.sin(2 * math.pi * t * 6)) if 0.8 < t < 0.92 else (Z, 0)]}
        pose(arm, f + 1, r, (0, 0, 0.004 * breathe))


def build(name, coat, antlers):
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete()
    for a in list(bpy.data.actions):
        bpy.data.actions.remove(a)
    _mats.clear()
    for m in list(bpy.data.materials):
        bpy.data.materials.remove(m)
    bpy.context.scene.render.fps = FPS
    arm = build_armature()
    body = build_mesh(arm, coat, antlers)
    for fn in (make_idle, make_graze, make_walk, make_run):
        fn(arm)
    arm.animation_data.action = bpy.data.actions["idle"]
    bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    body.select_set(True)
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, f"{name}.glb")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True,
                              export_animation_mode="ACTIONS", export_force_sampling=True,
                              export_def_bones=False, export_apply=False)
    print("wrote", path, [a.name for a in bpy.data.actions])


build("deer", "#A0683A", True)
build("deer_2", "#A0683A", False)
build("deer_3", "#B88A5C", False)
