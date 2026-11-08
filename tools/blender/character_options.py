"""Render candidate colonist designs so one can be picked before rigging.

  /Applications/Blender.app/Contents/MacOS/Blender -b -P tools/blender/character_options.py -- <out_dir>

For each design writes <out_dir>/char_<design>.png: a man, a woman and a child, spaced apart
and seen from a 3/4 angle. Also writes <out_dir>/char_all_game.png at roughly in-game zoom.
Every limb is its own part (upper arm, forearm, hand, thigh, shin, foot) so the chosen design
can be rigged with one bone per part.
Conventions match make_models.py: 1 tile = 1 m, character faces -Y, feet at z = 0.
"""
import math
import os
import sys

import bpy
from mathutils import Vector

OUT = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "."
OUT = os.path.abspath(OUT)
os.makedirs(OUT, exist_ok=True)

SKIN = [(0.93, 0.72, 0.56), (0.62, 0.42, 0.28), (0.80, 0.58, 0.42)]
HAIR = [(0.28, 0.16, 0.08), (0.08, 0.06, 0.05), (0.75, 0.55, 0.25)]
SHIRT = [(0.78, 0.26, 0.20), (0.22, 0.46, 0.78), (0.95, 0.72, 0.18)]
PANTS = (0.26, 0.22, 0.20)
BOOT = (0.20, 0.12, 0.07)
EYE = (0.04, 0.04, 0.05)
WOOD = (0.78, 0.60, 0.40)
STRAW = (0.85, 0.72, 0.38)

_mats = {}


def mat(rgb):
    key = tuple(round(c, 3) for c in rgb)
    if key in _mats:
        return _mats[key]
    m = bpy.data.materials.new("m%d" % len(_mats))
    m.use_nodes = True
    b = m.node_tree.nodes.get("Principled BSDF")
    b.inputs["Base Color"].default_value = (*rgb, 1)
    b.inputs["Roughness"].default_value = 0.85
    m.diffuse_color = (*rgb, 1)
    _mats[key] = m
    return m


class Figure:
    """Collects parts under one empty, so a whole character can be moved or scaled."""

    def __init__(self, name, x, y=0.0, s=1.0):
        self.root = bpy.data.objects.new(name, None)
        bpy.context.collection.objects.link(self.root)
        self.root.location = (x, y, 0)
        self.root.scale = (s, s, s)

    def _own(self, o, rgb, smooth=False):
        o.data.materials.append(mat(rgb))
        o.parent = self.root
        if smooth:
            bpy.ops.object.shade_smooth()
        return o

    def seg(self, p0, p1, r0, r1, rgb, verts=8):
        p0, p1 = Vector(p0), Vector(p1)
        d = p1 - p0
        bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r0, radius2=r1, depth=d.length, location=(p0 + p1) / 2)
        o = bpy.context.active_object
        o.rotation_mode = "QUATERNION"
        o.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(d.normalized())
        return self._own(o, rgb)

    def box(self, size, loc, rgb, bevel=0.015, rot=(0, 0, 0)):
        bpy.ops.mesh.primitive_cube_add(size=1, location=loc, rotation=rot)
        o = bpy.context.active_object
        o.scale = size
        bpy.ops.object.transform_apply(scale=True)
        if bevel:
            m = o.modifiers.new("bevel", "BEVEL")
            m.width = bevel
            m.segments = 1
        return self._own(o, rgb)

    def ball(self, r, loc, rgb, sub=2, scale=(1, 1, 1)):
        bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=sub, radius=r, location=loc)
        o = bpy.context.active_object
        o.scale = scale
        return self._own(o, rgb)


# ---------------------------------------------------------------------------------- designs
# Each design takes a variant: 0 = man, 1 = woman, 2 = child. Children are built at adult size
# with a bigger head, then scaled down.

def villager(x, v):
    """1. Villager: believable little person. Tunic, belt, trousers, boots, hair variety."""
    f = Figure("villager", x, s=0.66 if v == 2 else 1.0)
    skin, hair, shirt = SKIN[v], HAIR[v], SHIRT[v]
    head_s = 1.25 if v == 2 else 1.0
    for side in (-1, 1):
        lx = 0.075 * side
        f.box((0.10, 0.17, 0.08), (lx, -0.025, 0.04), BOOT)                 # boot
        f.seg((lx, 0, 0.08), (lx, 0, 0.28), 0.048, 0.055, PANTS)            # shin
        f.seg((lx, 0, 0.28), (lx, 0, 0.49), 0.056, 0.068, PANTS)            # thigh
    f.box((0.19, 0.12, 0.10), (0, 0, 0.49), PANTS)                          # hips (inside the tunic)
    if v == 1:
        f.seg((0, 0, 0.30), (0, 0, 0.56), 0.19, 0.13, shirt, verts=10)      # dress skirt
    else:
        f.seg((0, 0, 0.42), (0, 0, 0.56), 0.15, 0.13, shirt, verts=10)      # tunic hem
    f.seg((0, 0, 0.55), (0, 0, 0.80), 0.13, 0.165, shirt, verts=10)         # chest
    f.ball(0.165, (0, 0, 0.80), shirt, sub=2, scale=(1, 0.65, 0.32))        # shoulders
    f.seg((0, 0, 0.535), (0, 0, 0.575), 0.137, 0.137, BOOT, verts=10)       # belt
    f.seg((0, 0, 0.82), (0, 0, 0.88), 0.045, 0.042, skin)                   # neck
    hz = 0.96 + 0.12 * (head_s - 1)
    hr = 0.12 * head_s
    f.ball(hr, (0, 0, hz), skin, scale=(0.95, 1.0, 1.08))                   # head
    f.ball(hr * 1.07, (0, hr * 0.12, hz + hr * 0.22), hair, scale=(1, 1.03, 0.9))  # hair cap
    if v == 1:                                                              # long hair down the back
        f.box((hr * 1.7, 0.07, 0.24), (0, hr * 0.75, hz - 0.08), hair, bevel=0.02)
    if v == 0:                                                              # beard
        f.ball(hr * 0.75, (0, -hr * 0.55, hz - hr * 0.55), hair, sub=1, scale=(1, 0.6, 0.7))
    for ex in (-0.4, 0.4):
        f.box((0.022, 0.012, 0.032), (ex * hr, -hr * 0.92, hz + hr * 0.05), EYE, bevel=0)
    f.box((0.025, 0.03, 0.035), (0, -hr * 1.02, hz - hr * 0.18), skin, bevel=0.006)  # nose
    for side in (-1, 1):
        sx = 0.19 * side
        f.seg((sx, 0, 0.79), (sx * 1.12, 0, 0.61), 0.046, 0.04, shirt)       # upper arm
        f.seg((sx * 1.12, 0, 0.61), (sx * 1.17, -0.02, 0.45), 0.036, 0.032, skin)  # forearm
        f.ball(0.042, (sx * 1.18, -0.025, 0.415), skin, sub=1, scale=(0.8, 1, 1.15))  # hand
    return f


def mannequin(x, v):
    """2. Wooden mannequin: artist's doll with ball joints. Neutral, made to show motion."""
    f = Figure("mannequin", x, s=0.66 if v == 2 else 1.0)
    shirt = SHIRT[v]
    head_s = 1.25 if v == 2 else 1.0
    for side in (-1, 1):
        lx = 0.075 * side
        f.ball(0.05, (lx, -0.03, 0.035), WOOD, scale=(0.9, 1.6, 0.7))       # foot
        f.ball(0.035, (lx, 0, 0.075), WOOD)                                 # ankle
        f.seg((lx, 0, 0.09), (lx, 0, 0.27), 0.04, 0.052, WOOD, verts=10)    # shin
        f.ball(0.045, (lx, 0, 0.285), WOOD)                                 # knee
        f.seg((lx, 0, 0.30), (lx, 0, 0.47), 0.052, 0.064, WOOD, verts=10)   # thigh
    f.ball(0.13, (0, 0, 0.51), WOOD, scale=(1, 0.75, 0.6))                 # pelvis
    f.ball(0.06, (0, 0, 0.60), WOOD)                                        # waist joint
    f.ball(0.16, (0, 0, 0.72), shirt, scale=(1, 0.7, 0.9))                  # chest (colony colour)
    f.seg((0, 0, 0.82), (0, 0, 0.88), 0.035, 0.032, WOOD)                   # neck
    hz = 0.96 + 0.1 * (head_s - 1)
    f.ball(0.105 * head_s, (0, 0, hz), WOOD, scale=(0.85, 0.95, 1.15))      # egg head
    if v == 1:
        f.ball(0.08 * head_s, (0, 0.06, hz + 0.08), shirt, sub=1, scale=(1, 1, 0.8))  # bun
    for side in (-1, 1):
        sx = 0.17 * side
        f.ball(0.045, (sx, 0, 0.79), WOOD)                                  # shoulder
        f.seg((sx * 1.04, 0, 0.77), (sx * 1.12, 0, 0.62), 0.036, 0.032, WOOD, verts=10)
        f.ball(0.033, (sx * 1.13, 0, 0.605), WOOD)                          # elbow
        f.seg((sx * 1.13, 0, 0.59), (sx * 1.17, -0.02, 0.45), 0.031, 0.027, WOOD, verts=10)
        f.ball(0.04, (sx * 1.18, -0.025, 0.42), WOOD, sub=1, scale=(0.75, 1, 1.2))  # hand
    return f


def settler(x, v):
    """3. Toy settler: rounded toy proportions, big hands and boots, hats. Cute but long-limbed."""
    f = Figure("settler", x, s=0.68 if v == 2 else 1.0)
    skin, hair, shirt = SKIN[v], HAIR[v], SHIRT[v]
    for side in (-1, 1):
        lx = 0.085 * side
        f.ball(0.075, (lx, -0.03, 0.05), BOOT, scale=(0.95, 1.35, 0.75))    # big boot
        f.seg((lx, 0, 0.08), (lx, 0, 0.24), 0.05, 0.055, PANTS, verts=8)    # shin
        f.seg((lx, 0, 0.24), (lx, 0, 0.40), 0.058, 0.066, PANTS, verts=8)   # thigh
    f.ball(0.19, (0, 0, 0.56), shirt, scale=(1, 0.85, 1.05))                # round body
    f.seg((0, 0, 0.40), (0, 0, 0.44), 0.15, 0.17, PANTS, verts=12)          # waist band
    f.ball(0.17, (0, 0, 0.88), skin)                                        # head
    f.ball(0.035, (0, -0.17, 0.86), skin, sub=1)                            # round nose
    for ex in (-0.06, 0.06):
        f.ball(0.022, (ex, -0.155, 0.9), EYE, sub=1, scale=(1, 0.6, 1.3))
    if v == 0:                                                              # straw hat
        f.seg((0, 0, 0.98), (0, 0, 1.0), 0.28, 0.28, STRAW, verts=16)
        f.seg((0, 0, 1.0), (0, 0, 1.10), 0.15, 0.12, STRAW, verts=16)
    elif v == 1:                                                            # headscarf
        f.ball(0.178, (0, 0.02, 0.93), shirt, scale=(1, 1, 0.82))
        f.ball(0.06, (0, 0.17, 0.86), shirt, sub=1)
    else:                                                                   # beanie
        f.ball(0.175, (0, 0.01, 0.94), (0.30, 0.55, 0.30), scale=(1, 1, 0.75))
        f.ball(0.04, (0, 0.0, 1.08), (0.95, 0.95, 0.9), sub=1)
    for side in (-1, 1):
        sx = 0.19 * side
        f.seg((sx, 0, 0.66), (sx * 1.2, 0, 0.52), 0.045, 0.04, shirt)        # upper arm
        f.seg((sx * 1.2, 0, 0.52), (sx * 1.27, -0.02, 0.40), 0.038, 0.035, skin)  # forearm
        f.ball(0.06, (sx * 1.29, -0.025, 0.36), skin, sub=1)                 # big round hand
    return f


DESIGNS = [("1 villager", "villager", villager), ("2 mannequin", "mannequin", mannequin),
           ("3 settler", "settler", settler)]


# ---------------------------------------------------------------------------------- scene

def reset_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete()
    sc = bpy.context.scene
    for eng in ("BLENDER_EEVEE", "BLENDER_EEVEE_NEXT", "BLENDER_WORKBENCH"):
        try:
            sc.render.engine = eng
            break
        except TypeError:
            continue
    world = sc.world or bpy.data.worlds.new("w")
    sc.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    bg.inputs[0].default_value = (0.62, 0.74, 0.88, 1)
    bg.inputs[1].default_value = 0.45
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 4.0
    sun.angle = math.radians(8)
    so = bpy.data.objects.new("sun", sun)
    so.rotation_euler = (math.radians(48), math.radians(12), math.radians(-30))
    sc.collection.objects.link(so)
    bpy.ops.mesh.primitive_plane_add(size=80, location=(0, 0, 0))
    bpy.context.active_object.data.materials.append(mat((0.30, 0.48, 0.22)))
    try:
        sc.view_settings.view_transform = "Standard"
    except Exception:
        pass


def camera(target, ortho, yaw_deg, pitch_deg):
    cam = bpy.data.cameras.new("cam")
    cam.type = "ORTHO"
    cam.ortho_scale = ortho
    co = bpy.data.objects.new("cam", cam)
    bpy.context.scene.collection.objects.link(co)
    yaw, pitch = math.radians(yaw_deg), math.radians(pitch_deg)
    d = Vector((math.sin(yaw) * math.cos(pitch), -math.cos(yaw) * math.cos(pitch), math.sin(pitch)))
    co.location = Vector(target) + d * 30
    co.rotation_mode = "QUATERNION"
    co.rotation_quaternion = (-d).to_track_quat("-Z", "Y")
    bpy.context.scene.camera = co


def render(path, w, h):
    sc = bpy.context.scene
    sc.render.resolution_x, sc.render.resolution_y = w, h
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print("wrote", path)


SPACING = 0.9   # metres between figures, so nobody overlaps from the camera

for title, key, fn in DESIGNS:
    reset_scene()
    for v in range(3):
        fn((v - 1) * SPACING, v)
    camera((0, 0, 0.5), 3.4, yaw_deg=25, pitch_deg=14)
    render(os.path.join(OUT, f"char_{key}.png"), 1200, 760)

# All three at roughly in-game zoom (ortho 18 on a 900 px tall window, scaled to this image).
reset_scene()
for i, (title, key, fn) in enumerate(DESIGNS):
    for v in range(3):
        fn((i - 1) * 3.2 + (v - 1) * SPACING, v)
camera((0, 0, 0.5), 11.0, yaw_deg=0, pitch_deg=35.264)
render(os.path.join(OUT, "char_all_game.png"), 900, 300)
