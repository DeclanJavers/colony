"""Generate the colony's 3D models as .glb files, in the style of docs/STYLE.md.

Run (headless):
  /Applications/Blender.app/Contents/MacOS/Blender -b -P tools/blender/make_models.py -- assets/models [only1 only2 ...]

Conventions: 1 tile = 1 m. Each model's origin is the centre of its footprint at ground level; the
front (door) faces -Y in Blender, which is +Z (towards the default camera) in Godot.
Parts the game changes keep their own object: `window` (lit at night), `leaves1`/`leaves2` (seasonal
colour), `flame`, `lamp`. Everything else is merged per material to keep draw calls low.
Variants: <name>_2.glb, <name>_3.glb... are picked per object by the game.
Colours are sRGB hex from the palette in docs/STYLE.md.
"""
import math
import os
import random
import sys

import bmesh
import bpy

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = os.path.abspath(ARGS[0] if ARGS else "assets/models")
ONLY = set(ARGS[1:])
os.makedirs(OUT, exist_ok=True)

KEEP = {"window", "leaves1", "leaves2", "flame", "lamp", "body", "head", "roof_tint"}
_mats = {}


def lin(h):
    """sRGB hex -> linear RGB (what Blender/glTF store)."""
    h = h.lstrip("#")
    out = []
    for i in (0, 2, 4):
        c = int(h[i:i + 2], 16) / 255.0
        out.append(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4)
    return tuple(out)


def mat(name, hexcol, rough=0.9, emit=0.0):
    if name in _mats:
        return _mats[name]
    rgb = lin(hexcol)
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Specular IOR Level"].default_value = 0.2
    if emit > 0:
        bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emit
    m.diffuse_color = (*rgb, 1.0)
    _mats[name] = m
    return m


# ---- Palette (docs/STYLE.md) -----------------------------------------------------------------
P = {
    "roof_red": "#B8432F", "roof_red_dark": "#8A2E22", "roof_red_mid": "#A33A29",
    "roof_blue": "#3F67A8", "roof_blue_dark": "#2D4C80", "roof_blue_mid": "#375B96",
    "plaster": "#EAD9B0", "timber": "#5A3620", "plank": "#9C6B3F", "plank_dark": "#7A4F2C",
    "stone": "#9A9590", "stone_dark": "#6E6A68", "stone_light": "#B8B3AA",
    "leaf": "#3F8A35", "leaf_dark": "#2C6A2B", "leaf_light": "#63AD45",
    "pine": "#2F6E3A", "pine_dark": "#24573A",
    "soil": "#8A5A35", "soil_dark": "#5C3A22", "crop": "#7DB040",
    "water": "#3C7FC0", "glass": "#2B3550", "shutter": "#4E7A6A", "shutter_blue": "#4B6A9A",
    "fire": "#FFB040", "fire_core": "#FFE08A", "berry": "#C8283C", "iron": "#3A3A40",
    "skin": "#E8BC98", "cloth": "#D8D8D8", "sack": "#D2B48C",
}


def M(key, **kw):
    return mat(key, P[key], **kw)


def clear():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete()
    for block in (bpy.data.meshes, bpy.data.curves):
        for b in list(block):
            if b.users == 0:
                block.remove(b)


def _finish(obj, m, name):
    obj.name = name
    obj.data.materials.clear()
    obj.data.materials.append(m)
    return obj


def box(name, size, loc, m, rot=(0, 0, 0)):
    bpy.ops.mesh.primitive_cube_add(size=1, location=loc, rotation=rot)
    o = bpy.context.active_object
    o.scale = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return _finish(o, m, name)


def cyl(name, r, h, loc, m, verts=10, r2=None, rot=(0, 0, 0)):
    if r2 is None:
        bpy.ops.mesh.primitive_cylinder_add(vertices=verts, radius=r, depth=h, location=loc, rotation=rot)
    else:
        bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r, radius2=r2, depth=h, location=loc, rotation=rot)
    return _finish(bpy.context.active_object, m, name)


def cone(name, r, h, loc, m, verts=8):
    bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r, radius2=0, depth=h, location=loc)
    return _finish(bpy.context.active_object, m, name)


def blob(name, r, loc, m, sub=2, scale=(1, 1, 1)):
    """Faceted round shape (icosphere): leaves, bushes, rocks."""
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=sub, radius=r, location=loc)
    o = bpy.context.active_object
    o.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return _finish(o, m, name)


def poly(name, verts, faces, m):
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    vs = [bm.verts.new(p) for p in verts]
    for f in faces:
        bm.faces.new([vs[i] for i in f])
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(o)
    return _finish(o, m, name)


def beam(name, a, b, t, m):
    """A square beam of thickness t from point a to point b."""
    ax, ay, az = a
    bx, by, bz = b
    dx, dy, dz = bx - ax, by - ay, bz - az
    length = math.sqrt(dx * dx + dy * dy + dz * dz)
    bpy.ops.mesh.primitive_cube_add(size=1, location=((ax + bx) / 2, (ay + by) / 2, (az + bz) / 2))
    o = bpy.context.active_object
    o.scale = (length, t, t)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    # Point local +X along the beam.
    yaw = math.atan2(dy, dx)
    pitch = math.atan2(dz, math.sqrt(dx * dx + dy * dy))
    o.rotation_euler = (0, -pitch, yaw)
    o.rotation_mode = "XYZ"
    # Euler XYZ applies X, then Y, then Z: pitch about Y first, then yaw about Z.
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    return _finish(o, m, name)


def rotate_all_z(angle):
    from mathutils import Matrix
    r = Matrix.Rotation(angle, 4, "Z")
    for o in bpy.context.scene.objects:
        o.matrix_world = r @ o.matrix_world


def merge_by_material():
    """Join every object sharing a material into one (fewer draw calls), except named parts."""
    groups = {}
    for o in list(bpy.context.scene.objects):
        if o.type != "MESH" or o.name.split(".")[0] in KEEP:
            continue
        groups.setdefault(o.data.materials[0].name, []).append(o)
    for mname, objs in groups.items():
        if len(objs) < 2:
            continue
        bpy.ops.object.select_all(action="DESELECT")
        for o in objs:
            o.select_set(True)
        bpy.context.view_layer.objects.active = objs[0]
        bpy.ops.object.join()
        bpy.context.active_object.name = mname


def join_named(name, objs):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    o = bpy.context.active_object
    o.name = name
    return o


def export(name):
    for o in bpy.context.scene.objects:
        if o.type == "MESH":
            for p in o.data.polygons:
                p.use_smooth = False
    bpy.ops.object.select_all(action="SELECT")
    path = os.path.join(OUT, f"{name}.glb")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True, export_apply=True)
    print("wrote", path)


# ---- Building kit -------------------------------------------------------------------------------

def footing(w, d, h, seed=1):
    """Grey stone footing with a few lighter and darker stones set into its faces."""
    box("footing", (w, d, h), (0, 0, h / 2), M("stone"))
    rnd = random.Random(seed)
    for face in range(4):
        n = int((w if face < 2 else d) / 0.22)
        for i in range(n):
            if rnd.random() < 0.45:
                continue
            u = -((w if face < 2 else d) / 2) + 0.11 + i * 0.22
            z = rnd.choice([h * 0.3, h * 0.72])
            m = M("stone_light") if rnd.random() < 0.5 else M("stone_dark")
            sz = (0.16, 0.02, h * 0.36)
            if face == 0:
                box("st", sz, (u, -d / 2 - 0.006, z), m)
            elif face == 1:
                box("st", sz, (u, d / 2 + 0.006, z), m)
            elif face == 2:
                box("st", (0.02, 0.16, h * 0.36), (-w / 2 - 0.006, u, z), m)
            else:
                box("st", (0.02, 0.16, h * 0.36), (w / 2 + 0.006, u, z), m)


def timber_face(axis, pos, a0, a1, z0, z1, braces=True, mid=True):
    """Dark timber framing on one wall face. axis 'y': the face is the plane y=pos spanning x a0..a1.
    axis 'x': plane x=pos spanning y a0..a1."""
    t = 0.07
    m = M("timber")

    def P3(u, z):
        return (u, pos, z) if axis == "y" else (pos, u, z)

    zm = (z0 + z1) / 2
    for u in (a0 + t / 2, a1 - t / 2):
        beam("tb", P3(u, z0), P3(u, z1), t, m)
    beam("tb", P3(a0, z1 - t / 2), P3(a1, z1 - t / 2), t, m)
    beam("tb", P3(a0, z0 + t / 2), P3(a1, z0 + t / 2), t, m)
    if mid:
        beam("tb", P3(a0, zm), P3(a1, zm), t * 0.9, m)
    if braces:
        span = a1 - a0
        q = span * 0.28
        beam("tb", P3(a0 + t, zm), P3(a0 + q, z1 - t), t * 0.8, m)
        beam("tb", P3(a1 - t, zm), P3(a1 - q, z1 - t), t * 0.8, m)


def window(axis, pos, u, z, shutter="shutter", w=0.26, h=0.3):
    """Glass (named `window`, lit at night) with a light frame, sill and two shutters."""
    out = 0.025
    s = -1 if pos < 0 else 1

    def P3(du, dz, dn=0.0):
        n = pos + s * dn
        return (u + du, n, z + dz) if axis == "y" else (n, u + du, z + dz)

    def S(su, sz, sn):
        return (su, sn, sz) if axis == "y" else (sn, su, sz)

    glass = box("window", S(w, h, 0.03), P3(0, 0, 0.0), M("glass", rough=0.3))
    box("frame", S(w + 0.06, 0.04, 0.05), P3(0, -h / 2 - 0.02, out), M("plank"))      # sill
    box("frame", S(0.03, h, 0.04), P3(0, 0, out * 0.6), M("plank"))                  # mullion
    for side in (-1, 1):
        box("shutter", S(w * 0.45, h + 0.02, 0.03), P3(side * (w / 2 + w * 0.25), 0, out), M(shutter))
    return glass


def door(axis, pos, u, z0, w=0.34, h=0.55):
    s = -1 if pos < 0 else 1

    def P3(du, dz, dn=0.0):
        n = pos + s * dn
        return (u + du, n, z0 + dz) if axis == "y" else (n, u + du, z0 + dz)

    def S(su, sz, sn):
        return (su, sn, sz) if axis == "y" else (sn, su, sz)

    box("door", S(w, h, 0.04), P3(0, h / 2, 0.01), M("plank_dark"))
    box("doorframe", S(w + 0.08, 0.06, 0.06), P3(0, h + 0.03, 0.02), M("timber"))
    for du in (-w / 2 - 0.03, w / 2 + 0.03):
        box("doorframe", S(0.06, h, 0.06), P3(du, h / 2, 0.02), M("timber"))
    box("ring", S(0.04, 0.04, 0.03), P3(w * 0.28, h * 0.5, 0.04), M("iron"))
    box("step", S(w + 0.16, 0.08, 0.16), P3(0, -z0 + 0.04 if z0 > 0.1 else 0.04, 0.1), M("stone_light"))


def tiled_roof(w, d, h, z, color="red", over=0.16, rows=6):
    """Gable roof along X with visible tile rows (alternating tones), barge boards and a ridge beam.
    Returns the roof objects. Footprint w x d (walls), ridge height h above z."""
    base, dark, mid = M(f"roof_{color}"), M(f"roof_{color}_dark"), M(f"roof_{color}_mid")
    hd = d / 2 + over
    hh = h * (hd / (d / 2))
    z_e = z + h - hh                       # eave height (lower than wall top because of overhang)
    L = math.sqrt(hd * hd + hh * hh)
    W = w + 2 * over
    t = 0.07
    parts = []
    for s in (-1, 1):
        ang = math.atan2(hh, -s * hd)      # tilt of the strip's local Y axis
        ux, uz = (-s * hd) / L, hh / L      # unit vector eave -> ridge (in y, z)
        nx, nz = s * hh / L, hd / L         # outward normal (y, z)
        rows = rows if W < 1.0 else max(rows, int(L / 0.17))
        for k in range(rows):
            f = (k + 0.5) / rows
            cy = s * hd + ux * L * f + nx * t / 2
            cz = z_e + uz * L * f + nz * t / 2
            lift = 0.012 * (rows - k)       # lower rows sit a hair proud: tile steps
            parts.append(box("roofrow", (W, L / rows * 1.04, t), (0, cy + nx * lift, cz + nz * lift),
                             base if k % 2 == 0 else mid, rot=(ang, 0, 0)))
        # barge boards along the gable edges
        for ex in (-W / 2, W / 2):
            parts.append(beam("barge", (ex, s * hd + nx * 0.08, z_e + nz * 0.08), (ex, nx * 0.08, z + h + 0.08), 0.06, M("timber")))
    parts.append(beam("ridge", (-W / 2 - 0.02, 0, z + h + 0.07), (W / 2 + 0.02, 0, z + h + 0.07), 0.1, dark))
    return parts


def gable_ends(w, d, h, z, timber=True):
    """Plaster triangles closing the roof at x = ±w/2, with a king post."""
    hd = d / 2
    for s in (-1, 1):
        x = s * w / 2
        poly("gable", [(x, -hd, z), (x, hd, z), (x, 0, z + h)], [(0, 1, 2) if s > 0 else (2, 1, 0)], M("plaster"))
        if timber:
            beam("tb", (x + s * 0.02, 0, z), (x + s * 0.02, 0, z + h - 0.05), 0.07, M("timber"))
            beam("tb", (x + s * 0.02, -hd + 0.05, z + 0.04), (x + s * 0.02, hd - 0.05, z + 0.04), 0.07, M("timber"))


def chimney(x, y, z_base, z_top, w=0.26):
    box("chimney", (w, w, z_top - z_base), (x, y, (z_base + z_top) / 2), M("stone"))
    box("chimneycap", (w + 0.06, w + 0.06, 0.06), (x, y, z_top), M("stone_dark"))
    print(f"CHIMNEY godot=({x:.2f}, {z_top + 0.1:.2f}, {-y:.2f})")


def crate(loc, s=0.28, rot=0.0):
    x, y, z = loc
    box("crate", (s, s, s), (x, y, z + s / 2), M("plank"), rot=(0, 0, rot))
    for dz in (0.03, s - 0.03):
        box("crateband", (s + 0.012, s + 0.012, 0.04), (x, y, z + dz), M("plank_dark"), rot=(0, 0, rot))


def barrel(loc, r=0.13, h=0.32):
    x, y, z = loc
    cyl("barrel", r, h, (x, y, z + h / 2), M("plank"), verts=10)
    for dz in (0.06, h - 0.06):
        cyl("hoop", r + 0.012, 0.035, (x, y, z + dz), M("iron"), verts=10)


def sack(loc):
    x, y, z = loc
    blob("sack", 0.12, (x, y, z + 0.1), M("sack"), sub=1, scale=(1, 0.9, 1.1))


def wall_box(w, d, z0, z1):
    box("walls", (w, d, z1 - z0), (0, 0, (z0 + z1) / 2), M("plaster"))


# ---- Buildings ----------------------------------------------------------------------------------

def house(roof_color="red", seed=1, shutter="shutter"):
    F, W, Z0, Z1 = 0.24, 1.5, 0.24, 1.2
    footing(W + 0.1, W + 0.1, F, seed)
    wall_box(W, W, Z0, Z1)
    for s in (-1, 1):
        timber_face("y", s * (W / 2 + 0.012), -W / 2, W / 2, Z0, Z1, braces=True)
        timber_face("x", s * (W / 2 + 0.012), -W / 2, W / 2, Z0, Z1, braces=False)
    door("y", -W / 2 - 0.02, -0.3, Z0 - 0.0, h=0.55)
    window("y", -W / 2 - 0.02, 0.35, 0.78, shutter)
    window("x", W / 2 + 0.02, 0.0, 0.78, shutter)
    window("x", -W / 2 - 0.02, 0.0, 0.78, shutter)
    window("y", W / 2 + 0.02, 0.0, 0.78, shutter)
    tiled_roof(W, W, 0.85, Z1, roof_color)
    gable_ends(W, W, 0.85, Z1)
    chimney(0.42, 0.3, Z1, Z1 + 1.05)
    rnd = random.Random(seed)
    if rnd.random() < 0.7:
        barrel((0.62, -0.95, 0))
    if rnd.random() < 0.6:
        crate((-0.75, -0.95, 0), rot=0.3)
    # glass panes: one object so the game lights them together
    join_named("window", [o for o in bpy.context.scene.objects if o.name.startswith("window")])


def storehouse():
    """Plank barn with big doors on the gable end (built along X, turned to face the front)."""
    F, W, Z0, Z1 = 0.2, 1.6, 0.2, 1.25
    footing(W + 0.1, W + 0.1, F, 7)
    box("walls", (W, W, Z1 - Z0), (0, 0, (Z0 + Z1) / 2), M("plank"))
    for i in range(9):                   # vertical boards
        u = -W / 2 + 0.09 + i * 0.178
        for s in (-1, 1):
            box("board", (0.03, 0.02, Z1 - Z0), (u, s * (W / 2 + 0.008), (Z0 + Z1) / 2), M("plank_dark"))
    for s in (-1, 1):
        timber_face("x", s * (W / 2 + 0.012), -W / 2, W / 2, Z0, Z1, braces=False, mid=False)
    # big double door on +X (becomes the front after the turn)
    box("door", (0.04, 0.7, 0.75), (W / 2 + 0.02, 0, Z0 + 0.375), M("plank_dark"))
    beam("tb", (W / 2 + 0.05, -0.35, Z0), (W / 2 + 0.05, 0.35, Z0 + 0.75), 0.05, M("timber"))
    beam("tb", (W / 2 + 0.05, 0.35, Z0), (W / 2 + 0.05, -0.35, Z0 + 0.75), 0.05, M("timber"))
    tiled_roof(W, W, 0.8, Z1, "red")
    gable_ends(W, W, 0.8, Z1, timber=False)
    box("loft", (0.04, 0.3, 0.26), (W / 2 + 0.02, 0, Z1 + 0.28), M("plank_dark"))
    crate((1.0, 0.55, 0), rot=0.2)
    crate((1.0, 0.55, 0.28), s=0.22, rot=0.6)
    sack((1.0, -0.6, 0))
    rotate_all_z(-math.pi / 2)


def workshop():
    F, W, Z0, Z1 = 0.0, 1.6, 0.0, 1.15
    box("walls", (W, W, Z1), (0, 0, Z1 / 2), M("stone"))
    rnd = random.Random(3)
    for i in range(40):                  # stone courses
        face = rnd.randrange(4)
        u = rnd.uniform(-0.7, 0.7)
        z = rnd.choice([0.15, 0.4, 0.65, 0.9])
        m = M("stone_light") if rnd.random() < 0.5 else M("stone_dark")
        if face < 2:
            box("st", (0.2, 0.02, 0.14), (u, (W / 2 + 0.006) * (1 if face else -1), z), m)
        else:
            box("st", (0.02, 0.2, 0.14), ((W / 2 + 0.006) * (1 if face == 3 else -1), u, z), m)
    door("y", -W / 2 - 0.02, -0.35, 0.0, w=0.4, h=0.6)
    window("y", -W / 2 - 0.02, 0.38, 0.65, "shutter_blue")
    tiled_roof(W, W, 0.75, Z1, "blue")
    gable_ends(W, W, 0.75, Z1)
    chimney(0.5, 0.42, Z1, Z1 + 1.0, w=0.3)
    box("anvilbase", (0.16, 0.16, 0.22), (0.55, -1.05, 0.11), M("plank_dark"))
    box("anvil", (0.32, 0.14, 0.1), (0.55, -1.05, 0.27), M("iron"))
    for i in range(3):
        cyl("log", 0.06, 0.5, (-0.62, -1.0 + i * 0.0, 0.06 + i * 0.1), M("plank"), verts=7, rot=(0, math.pi / 2, 0))


def kitchen():
    F, W, Z0, Z1 = 0.24, 1.5, 0.24, 1.15
    footing(W + 0.1, W + 0.1, F, 5)
    wall_box(W, W, Z0, Z1)
    for s in (-1, 1):
        timber_face("y", s * (W / 2 + 0.012), -W / 2, W / 2, Z0, Z1)
        timber_face("x", s * (W / 2 + 0.012), -W / 2, W / 2, Z0, Z1, braces=False)
    door("y", -W / 2 - 0.02, 0.3, Z0, h=0.55)
    window("y", -W / 2 - 0.02, -0.35, 0.75, "shutter")
    window("x", -W / 2 - 0.02, 0.0, 0.75, "shutter")
    window("x", W / 2 + 0.02, 0.0, 0.75, "shutter")
    tiled_roof(W, W, 0.8, Z1, "red")
    gable_ends(W, W, 0.8, Z1)
    chimney(-0.45, 0.35, Z1, Z1 + 1.1, w=0.32)
    cyl("pot", 0.17, 0.2, (0.62, -1.0, 0.16), M("iron"), verts=10)
    for a in range(3):
        ang = a * math.tau / 3
        beam("tripod", (0.62 + 0.18 * math.cos(ang), -1.0 + 0.18 * math.sin(ang), 0), (0.62, -1.0, 0.45), 0.03, M("timber"))
    barrel((-0.7, -0.95, 0))
    join_named("window", [o for o in bpy.context.scene.objects if o.name.startswith("window")])


def town_hall():
    W, Z0, Z1, Z2 = 2.5, 0.0, 1.0, 1.95
    box("walls_low", (W, W, Z1), (0, 0, Z1 / 2), M("stone"))
    rnd = random.Random(9)
    for i in range(60):
        face = rnd.randrange(4)
        u = rnd.uniform(-1.1, 1.1)
        z = rnd.choice([0.15, 0.4, 0.65, 0.88])
        m = M("stone_light") if rnd.random() < 0.5 else M("stone_dark")
        if face < 2:
            box("st", (0.22, 0.02, 0.14), (u, (W / 2 + 0.006) * (1 if face else -1), z), m)
        else:
            box("st", (0.02, 0.22, 0.14), ((W / 2 + 0.006) * (1 if face == 3 else -1), u, z), m)
    U = W + 0.16                          # upper floor juts out a little
    box("walls_up", (U, U, Z2 - Z1), (0, 0, (Z1 + Z2) / 2), M("plaster"))
    for s in (-1, 1):
        timber_face("y", s * (U / 2 + 0.012), -U / 2, U / 2, Z1, Z2)
        timber_face("x", s * (U / 2 + 0.012), -U / 2, U / 2, Z1, Z2)
        for k in (-1, 0, 1):
            window("y", s * (U / 2 + 0.02), k * 0.75, (Z1 + Z2) / 2 + 0.05, "shutter_blue")
    for k in (-1, 1):
        window("y", -W / 2 - 0.02, k * 0.8, 0.6, "shutter_blue")
    door("y", -W / 2 - 0.02, 0.0, 0.0, w=0.55, h=0.75)
    tiled_roof(U, U, 1.25, Z2, "blue", rows=8)
    gable_ends(U, U, 1.25, Z2)
    # bell tower on the ridge
    box("tower", (0.5, 0.5, 0.7), (0, 0, Z2 + 1.25 + 0.2), M("plaster"))
    timber_face("y", -0.262, -0.25, 0.25, Z2 + 1.0, Z2 + 1.8, braces=False, mid=False)
    box("bell", (0.2, 0.06, 0.22), (0, -0.25, Z2 + 1.55), M("roof_blue_dark"))
    cone("spire", 0.42, 0.75, (0, 0, Z2 + 1.8 + 0.37), M("roof_blue"), verts=4)
    box("banner", (0.3, 0.02, 0.45), (0.85, -W / 2 - 0.05, 0.62), M("roof_red"))
    barrel((-1.45, -1.2, 0))
    crate((1.35, -1.25, 0), rot=0.4)
    join_named("window", [o for o in bpy.context.scene.objects if o.name.startswith("window")])


def meeting_hall(size=3, level=1):
    """Where the colony meets each week. One model per size: the hall grows a level at a time
    (3x3, 4x4, 5x5 footprints) when the colony no longer fits inside."""
    W = size - 0.35
    Z0 = 0.24
    Z1 = Z0 + 1.0 + 0.12 * level
    footing(W + 0.1, W + 0.1, Z0, 20 + level)
    wall_box(W, W, Z0, Z1)
    bays = max(2, int(W / 0.9))
    for s in (-1, 1):
        # Long walls get extra posts per bay; gable walls a simple frame.
        timber_face("y", s * (W / 2 + 0.012), -W / 2, W / 2, Z0, Z1, braces=True)
        for k in range(1, bays):
            u = -W / 2 + k * W / bays
            beam("tb", (u, s * (W / 2 + 0.02), Z0), (u, s * (W / 2 + 0.02), Z1), 0.07, M("timber"))
        timber_face("x", s * (W / 2 + 0.012), -W / 2, W / 2, Z0, Z1, braces=False)
    # Double doors in the middle of the front, windows in the other bays.
    box("door", (0.62, 0.04, 0.72), (0, -W / 2 - 0.02, Z0 + 0.36), M("plank_dark"))
    beam("tb", (0, -W / 2 - 0.045, Z0), (0, -W / 2 - 0.045, Z0 + 0.72), 0.04, M("timber"))
    box("doorframe", (0.74, 0.06, 0.07), (0, -W / 2 - 0.03, Z0 + 0.75), M("timber"))
    box("step", (0.9, 0.18, 0.08), (0, -W / 2 - 0.12, 0.04), M("stone_light"))
    for k in range(bays):
        u = -W / 2 + (k + 0.5) * W / bays
        if abs(u) > 0.5:
            window("y", -W / 2 - 0.02, u, (Z0 + Z1) / 2 + 0.05, "shutter_blue", w=0.24, h=0.34)
        window("y", W / 2 + 0.02, u, (Z0 + Z1) / 2 + 0.05, "shutter_blue", w=0.24, h=0.34)
    for k in (-1, 1):
        window("x", k * (W / 2 + 0.02), 0.0, (Z0 + Z1) / 2 + 0.05, "shutter_blue", w=0.24, h=0.34)
    H = W * 0.55
    tiled_roof(W, W, H, Z1, "blue", rows=6 + 2 * level)
    gable_ends(W, W, H, Z1)
    # Bell-cote on the ridge: the bell that calls everyone to the meeting.
    zr = Z1 + H
    for x in (-0.14, 0.14):
        box("post", (0.06, 0.06, 0.42), (x + W * 0.25, 0, zr + 0.2), M("timber"))
    box("bell", (0.16, 0.16, 0.18), (W * 0.25, 0, zr + 0.24), M("iron"))
    tiled_roof(0.36, 0.3, 0.16, zr + 0.4, "red", over=0.05, rows=2)
    # Banners either side of the door, benches outside.
    for x in (-0.55, 0.55):
        box("banner", (0.22, 0.02, 0.42), (x, -W / 2 - 0.04, Z1 - 0.3), M("roof_red"))
    for x in (-W / 2 + 0.25, W / 2 - 0.25):
        box("bench", (0.5, 0.14, 0.04), (x, -W / 2 - 0.32, 0.2), M("plank"))
        for dx in (-0.2, 0.2):
            box("bench", (0.04, 0.12, 0.18), (x + dx, -W / 2 - 0.32, 0.09), M("plank_dark"))
    join_named("window", [o for o in bpy.context.scene.objects if o.name.startswith("window")])


def well():
    cyl("ring", 0.4, 0.42, (0, 0, 0.21), M("stone"), verts=12)
    rnd = random.Random(2)
    for i in range(12):
        a = i / 12 * math.tau + 0.13
        box("st", (0.14, 0.02, 0.1), (math.cos(a) * 0.405, math.sin(a) * 0.405, rnd.choice([0.1, 0.3])),
            M("stone_light") if i % 2 else M("stone_dark"), rot=(0, 0, a + math.pi / 2))
    cyl("lip", 0.43, 0.06, (0, 0, 0.44), M("stone_light"), verts=12)
    cyl("water", 0.33, 0.02, (0, 0, 0.36), M("water", rough=0.2), verts=12)
    for x in (-0.36, 0.36):
        box("post", (0.07, 0.07, 0.95), (x, 0, 0.47 + 0.0), M("timber"))
    cyl("crank", 0.03, 0.8, (0, 0, 0.82), M("timber"), verts=6, rot=(0, math.pi / 2, 0))
    box("bucket", (0.12, 0.12, 0.12), (0.05, 0, 0.62), M("plank"))
    tiled_roof(0.72, 0.5, 0.32, 0.95, "red", over=0.1, rows=3)
    for s in (-1, 1):
        poly("gable", [(s * 0.36, -0.25, 0.95), (s * 0.36, 0.25, 0.95), (s * 0.36, 0, 1.27)],
             [(0, 1, 2) if s > 0 else (2, 1, 0)], M("plank"))


def campfire():
    for i in range(9):
        a = i / 9 * math.tau
        blob("stone", 0.085, (math.cos(a) * 0.3, math.sin(a) * 0.3, 0.05), M("stone_light" if i % 3 else "stone_dark"), sub=1)
    for i in range(4):
        a = i * math.pi / 4
        beam("log", (math.cos(a) * 0.24, math.sin(a) * 0.24, 0.04), (-math.cos(a) * 0.08, -math.sin(a) * 0.08, 0.2), 0.07, M("plank_dark"))
    flame = cone("flame", 0.17, 0.42, (0, 0, 0.3), mat("fire", P["fire"], emit=4.0), verts=6)
    core = cone("flame_core", 0.09, 0.28, (0, 0, 0.26), mat("fire_core", P["fire_core"], emit=5.0), verts=6)
    join_named("flame", [flame, core])


def stockpile():
    """Plank deck with a low rim; the game stacks stored goods on it."""
    for i in range(7):
        y = -0.81 + i * 0.27
        box("plank", (1.8, 0.25, 0.08), (0, y, 0.05), M("plank" if i % 2 else "plank_dark"))
    for i, (x, y, w, d) in enumerate([(0, -0.9, 1.84, 0.07), (0, 0.9, 1.84, 0.07), (-0.9, 0, 0.07, 1.84), (0.9, 0, 0.07, 1.84)]):
        box("rim", (w, d, 0.1), (x, y, 0.13), M("timber"))
    for x in (-0.9, 0.9):
        for y in (-0.9, 0.9):
            box("post", (0.1, 0.1, 0.32), (x, y, 0.16), M("timber"))


def farm_plot():
    box("soil", (1.9, 1.9, 0.07), (0, 0, 0.035), M("soil_dark"))
    for i in range(5):
        y = -0.76 + i * 0.38
        box("ridge", (1.75, 0.2, 0.07), (0, y, 0.08), M("soil"))
    for i, (x, y, w, d) in enumerate([(0, -0.96, 1.96, 0.06), (0, 0.96, 1.96, 0.06), (-0.96, 0, 0.06, 1.96), (0.96, 0, 0.06, 1.96)]):
        box("edge", (w, d, 0.1), (x, y, 0.05), M("plank"))


def farm_crops():
    """Shown on a farm plot while it grows (the game scales it by growth and tints it when ripe)."""
    rnd = random.Random(4)
    for i in range(5):
        y = -0.76 + i * 0.38
        for j in range(6):
            x = -0.72 + j * 0.29 + rnd.uniform(-0.03, 0.03)
            for k in range(3):
                a = k * math.tau / 3 + rnd.uniform(0, 1)
                cone("crop", 0.05, 0.32, (x + math.cos(a) * 0.04, y + math.sin(a) * 0.04, 0.26), M("crop"), verts=4)


def site_frame():
    """Scaffolding shown on a 1x1 construction site; the game scales it to the footprint."""
    m = M("plank")
    for x in (-0.45, 0.45):
        for y in (-0.45, 0.45):
            box("post", (0.06, 0.06, 1.0), (x, y, 0.5), m)
    for z in (0.05, 0.95):
        for y in (-0.45, 0.45):
            box("beam", (0.96, 0.05, 0.05), (0, y, z), m)
        for x in (-0.45, 0.45):
            box("beam", (0.05, 0.96, 0.05), (x, 0, z), m)
    beam("brace", (-0.45, -0.47, 0.05), (0.45, -0.47, 0.95), 0.04, M("plank_dark"))


# ---- Nature -------------------------------------------------------------------------------------

def _canopy(blobs, tops):
    dark = [blob("leaves1", r, loc, M("leaf"), sub=2, scale=sc) for r, loc, sc in blobs]
    light = [blob("leaves2", r, loc, M("leaf_light"), sub=1, scale=sc) for r, loc, sc in tops]
    join_named("leaves1", dark)
    join_named("leaves2", light)


def _clumps(seed, n, centre_z, spread, r0, height):
    """A canopy of n overlapping leaf clumps (reference: lumpy pixel-art trees), lighter clumps on top."""
    rnd = random.Random(seed)
    blobs, tops = [], []
    for i in range(n):
        a = i / n * math.tau + rnd.uniform(-0.3, 0.3)
        d = spread * rnd.uniform(0.45, 1.0)
        z = centre_z + rnd.uniform(-0.15, 0.2) * height
        r = r0 * rnd.uniform(0.75, 1.05)
        blobs.append((r, (math.cos(a) * d, math.sin(a) * d, z), (1, 1, 0.85)))
    blobs.append((r0 * 1.2, (0, 0, centre_z + 0.12), (1, 1, 0.9)))
    blobs.append((r0, (0.05, 0.02, centre_z + 0.38 * height), (1, 1, 0.85)))
    for i in range(3):
        a = math.pi * 1.1 + i * 0.7 + rnd.uniform(-0.2, 0.2)    # highlights towards the light (back-left)
        tops.append((r0 * 0.55, (math.cos(a) * spread * 0.45, math.sin(a) * spread * 0.45, centre_z + 0.3 * height + i * 0.04), (1, 1, 0.8)))
    _canopy(blobs, tops)


def tree():
    cyl("trunk", 0.1, 0.8, (0, 0, 0.4), M("plank_dark"), verts=6, r2=0.07)
    _clumps(11, 6, 1.12, 0.36, 0.27, 1.0)


def tree_2():
    cyl("trunk", 0.09, 1.0, (0, 0, 0.5), M("plank_dark"), verts=6, r2=0.06)
    _clumps(23, 5, 1.32, 0.28, 0.24, 1.2)


def tree_3():
    """Pine."""
    cyl("trunk", 0.08, 0.5, (0, 0, 0.25), M("plank_dark"), verts=6)
    low = [cone("leaves1", 0.5, 0.75, (0, 0, 0.7), M("pine"), verts=7), cone("leaves1", 0.38, 0.65, (0, 0, 1.15), M("pine"), verts=7)]
    join_named("leaves1", low)
    top = cone("leaves2", 0.26, 0.55, (0, 0, 1.55), M("pine_dark"), verts=7)
    top.name = "leaves2"


def rock():
    blob("rock", 0.33, (0, 0, 0.16), M("stone"), sub=1, scale=(1.15, 0.95, 0.75))
    blob("rock", 0.2, (0.24, 0.16, 0.1), M("stone_dark"), sub=1)
    blob("rock", 0.12, (-0.25, -0.18, 0.06), M("stone_light"), sub=1)


def rock_2():
    blob("rock", 0.28, (0, 0, 0.2), M("stone_light"), sub=1, scale=(0.9, 1.1, 1.0))
    blob("rock", 0.22, (-0.2, 0.1, 0.1), M("stone"), sub=1)


def berry_bush():
    leaves = [blob("leaves1", r, (x, y, r * 0.85), M("leaf"), sub=2) for x, y, r in [(0, 0, 0.3), (0.2, 0.1, 0.22), (-0.17, 0.12, 0.21)]]
    join_named("leaves1", leaves)
    rnd = random.Random(5)
    for i in range(9):
        a = i / 9 * math.tau
        r = 0.27
        blob("berry", 0.045, (math.cos(a) * r, math.sin(a) * r, 0.2 + rnd.uniform(0, 0.2)), M("berry"), sub=1)


# ---- Props --------------------------------------------------------------------------------------

def prop_crate():
    crate((0, 0, 0), s=0.3)


def prop_barrel():
    barrel((0, 0, 0))


def lamp_post():
    box("lampbase", (0.12, 0.12, 0.08), (0, 0, 0.04), M("stone_dark"))
    cyl("lamppost", 0.035, 1.05, (0, 0, 0.55), M("iron"), verts=6)
    box("lamparm", (0.2, 0.03, 0.03), (0.08, 0, 1.05), M("iron"))
    lamp = box("lamp", (0.1, 0.1, 0.14), (0.16, 0, 0.95), mat("lamp", P["fire"], emit=2.5))
    box("lampcap", (0.14, 0.14, 0.04), (0.16, 0, 1.04), M("iron"))


def colonist():
    cyl("body", 0.22, 0.55, (0, 0, 0.42), M("cloth"), verts=10, r2=0.16)
    blob("head", 0.17, (0, 0, 0.86), M("skin"), sub=2)


MODELS = {
    "colonist": colonist, "campfire": campfire, "stockpile": stockpile, "well": well,
    "farm_plot": farm_plot, "farm_crops": farm_crops,
    "house": lambda: house("red", 1, "shutter"), "house_2": lambda: house("red", 2, "shutter_blue"),
    "house_3": lambda: house("blue", 3, "shutter"), "house_4": lambda: house("red", 4, "shutter"),
    "storehouse": storehouse, "workshop": workshop, "kitchen": kitchen, "town_hall": town_hall,
    "site_frame": site_frame, "tree": tree, "tree_2": tree_2, "tree_3": tree_3,
    "rock": rock, "rock_2": rock_2, "berry_bush": berry_bush,
    "meeting_hall": lambda: meeting_hall(3, 1), "meeting_hall_lv2": lambda: meeting_hall(4, 2),
    "meeting_hall_lv3": lambda: meeting_hall(5, 3),
    "meeting_hall_lv4": lambda: meeting_hall(6, 4),
    "prop_crate": prop_crate, "prop_barrel": prop_barrel, "lamp_post": lamp_post,
}

for name, fn in MODELS.items():
    if ONLY and name not in ONLY:
        continue
    clear()
    print("MODEL", name)
    fn()
    merge_by_material()
    export(name)
print("done")
