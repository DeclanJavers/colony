"""Generate the colony's placeholder 3D models as .glb files.

Run (headless):
  /Applications/Blender.app/Contents/MacOS/Blender -b -P tools/blender/make_models.py -- assets/models

Conventions: 1 tile = 1 m. Each model's origin is the centre of its footprint at ground level.
Models are deliberately simple and colour-coded so the colony is readable from far away.
To restyle a model, edit its function below and rerun; Godot re-imports automatically.
"""
import math
import os
import sys

import bpy

OUT = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "assets/models"
OUT = os.path.abspath(OUT)
os.makedirs(OUT, exist_ok=True)

_mats = {}


def mat(name, rgb, rough=0.85, emit=0.0):
    if name in _mats:
        return _mats[name]
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    if emit > 0:
        bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emit
    m.diffuse_color = (*rgb, 1.0)
    _mats[name] = m
    return m


# Palette (linear-ish values; Godot shows them a bit brighter)
WOOD = (0.42, 0.25, 0.12)
WOOD_LIGHT = (0.62, 0.42, 0.22)
STONE = (0.45, 0.45, 0.47)
STONE_DARK = (0.30, 0.30, 0.33)
ROOF_RED = (0.62, 0.18, 0.12)
ROOF_BLUE = (0.18, 0.30, 0.55)
ROOF_GREEN = (0.20, 0.42, 0.22)
ROOF_GOLD = (0.75, 0.55, 0.12)
WALL = (0.85, 0.78, 0.62)
SOIL = (0.30, 0.19, 0.10)
CROP = (0.45, 0.65, 0.18)
LEAF = (0.16, 0.45, 0.16)
LEAF_DARK = (0.10, 0.32, 0.12)
BERRY = (0.75, 0.08, 0.20)
FIRE = (1.0, 0.45, 0.05)
WATER = (0.12, 0.35, 0.75)
SKIN = (0.90, 0.72, 0.58)
CLOTH = (0.85, 0.85, 0.85)  # tinted per colonist in Godot


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
    bpy.ops.object.transform_apply(scale=True)
    return _finish(o, m, name)


def cyl(name, r, h, loc, m, verts=16, r2=None):
    if r2 is None:
        bpy.ops.mesh.primitive_cylinder_add(vertices=verts, radius=r, depth=h, location=loc)
    else:
        bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r, radius2=r2, depth=h, location=loc)
    return _finish(bpy.context.active_object, m, name)


def cone(name, r, h, loc, m, verts=16):
    bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r, radius2=0, depth=h, location=loc)
    return _finish(bpy.context.active_object, m, name)


def sphere(name, r, loc, m, seg=16, scale=(1, 1, 1)):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=seg, ring_count=seg // 2, radius=r, location=loc)
    o = bpy.context.active_object
    o.scale = scale
    bpy.ops.object.transform_apply(scale=True)
    bpy.ops.object.shade_smooth()
    return _finish(o, m, name)


def ico(name, r, loc, m, sub=1, scale=(1, 1, 1)):
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=sub, radius=r, location=loc)
    o = bpy.context.active_object
    o.scale = scale
    bpy.ops.object.transform_apply(scale=True)
    return _finish(o, m, name)


def roof(name, w, d, h, z, m):
    """Gable roof (triangular prism) along X, footprint w x d, ridge height h, base at z."""
    import bmesh
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    hw, hd = w / 2, d / 2
    v = [bm.verts.new(p) for p in [(-hw, -hd, z), (hw, -hd, z), (hw, hd, z), (-hw, hd, z), (-hw, 0, z + h), (hw, 0, z + h)]]
    for f in [(0, 1, 5, 4), (2, 3, 4, 5), (0, 4, 3), (1, 2, 5), (0, 3, 2, 1)]:
        bm.faces.new([v[i] for i in f])
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(o)
    return _finish(o, m, name)


def pyramid(name, w, h, z, m):
    bpy.ops.mesh.primitive_cone_add(vertices=4, radius1=w * 0.7071, radius2=0, depth=h, location=(0, 0, z + h / 2),
                                    rotation=(0, 0, math.pi / 4))
    return _finish(bpy.context.active_object, m, name)


def export(name):
    bpy.ops.object.select_all(action="SELECT")
    path = os.path.join(OUT, f"{name}.glb")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True, export_apply=True)
    print("wrote", path)


# ----------------------------------------------------------------------------- models

def colonist():
    cyl("body", 0.22, 0.55, (0, 0, 0.42), mat("cloth", CLOTH), verts=12, r2=0.16)
    sphere("head", 0.17, (0, 0, 0.86), mat("skin", SKIN))
    box("nose", (0.06, 0.08, 0.06), (0, -0.17, 0.86), mat("skin", SKIN))  # faces -Y = forward
    for x in (-0.09, 0.09):
        cyl(f"leg{x}", 0.06, 0.18, (x, 0, 0.09), mat("wood_dark", STONE_DARK), verts=8)


def campfire():
    for i in range(8):
        a = i / 8 * math.tau
        ico(f"stone{i}", 0.09, (math.cos(a) * 0.3, math.sin(a) * 0.3, 0.06), mat("stone", STONE), sub=1)
    for i in range(3):
        box(f"log{i}", (0.5, 0.08, 0.08), (0, 0, 0.08), mat("wood", WOOD), rot=(0, 0.25, i * math.pi / 3))
    cone("flame", 0.18, 0.45, (0, 0, 0.32), mat("fire", FIRE, emit=4.0), verts=8)


def stockpile():
    # A bare pallet with a low rim: the game stacks whatever is stored on it (logs, stone, sacks, barrels).
    box("pallet", (1.8, 1.8, 0.1), (0, 0, 0.05), mat("wood_light", WOOD_LIGHT))
    for i, (x, y, w, d) in enumerate([(0, -0.88, 1.8, 0.06), (0, 0.88, 1.8, 0.06), (-0.88, 0, 0.06, 1.8), (0.88, 0, 0.06, 1.8)]):
        box(f"rim{i}", (w, d, 0.08), (x, y, 0.14), mat("wood", WOOD))


def well():
    cyl("ring", 0.42, 0.5, (0, 0, 0.25), mat("stone", STONE), verts=16)
    cyl("water", 0.34, 0.05, (0, 0, 0.47), mat("water", WATER, rough=0.2), verts=16)
    for x in (-0.38, 0.38):
        box(f"post{x}", (0.07, 0.07, 0.9), (x, 0, 0.75), mat("wood", WOOD))
    roof("roof", 1.0, 0.7, 0.3, 1.18, mat("roof_blue", ROOF_BLUE))


def farm_plot():
    box("soil", (1.9, 1.9, 0.08), (0, 0, 0.04), mat("soil", SOIL))
    for i in range(4):
        y = -0.7 + i * 0.47
        box(f"row{i}", (1.7, 0.18, 0.06), (0, y, 0.1), mat("soil_ridge", (0.36, 0.23, 0.12)))


def farm_crops():
    """Separate model shown on top of a farm plot while it grows (scaled by growth)."""
    for i in range(4):
        y = -0.7 + i * 0.47
        for j in range(5):
            x = -0.7 + j * 0.35
            cone(f"crop{i}_{j}", 0.1, 0.35, (x, y, 0.3), mat("crop", CROP), verts=6)


def house():
    box("walls", (1.6, 1.6, 1.0), (0, 0, 0.5), mat("wall", WALL))
    roof("roof", 1.9, 1.9, 0.75, 1.0, mat("roof_red", ROOF_RED))
    box("door", (0.35, 0.05, 0.6), (0, -0.81, 0.3), mat("wood", WOOD))
    box("window", (0.3, 0.05, 0.25), (0.5, -0.81, 0.62), mat("water", WATER, rough=0.2))


def storehouse():
    box("walls", (1.8, 1.8, 1.2), (0, 0, 0.6), mat("wood_light", WOOD_LIGHT))
    roof("roof", 2.0, 2.0, 0.6, 1.2, mat("roof_green", ROOF_GREEN))
    box("door", (0.8, 0.05, 0.9), (0, -0.91, 0.45), mat("wood", WOOD))


def workshop():
    box("walls", (1.8, 1.8, 1.0), (0, 0, 0.5), mat("stone", STONE))
    roof("roof", 2.0, 2.0, 0.55, 1.0, mat("wood", WOOD))
    box("chimney", (0.3, 0.3, 0.9), (0.55, 0.45, 1.5), mat("stone_dark", STONE_DARK))
    box("anvil", (0.4, 0.2, 0.25), (0.6, -1.15, 0.12), mat("stone_dark", STONE_DARK))


def kitchen():
    box("walls", (1.8, 1.8, 1.0), (0, 0, 0.5), mat("wall", WALL))
    roof("roof", 2.0, 2.0, 0.6, 1.0, mat("roof_gold", ROOF_GOLD))
    box("chimney", (0.3, 0.3, 0.9), (-0.55, 0.45, 1.5), mat("stone", STONE))
    cyl("pot", 0.22, 0.25, (0.55, -1.1, 0.15), mat("stone_dark", STONE_DARK), verts=12)


def town_hall():
    box("walls", (2.8, 2.8, 1.4), (0, 0, 0.7), mat("stone", STONE))
    roof("roof", 3.1, 3.1, 0.9, 1.4, mat("roof_blue", ROOF_BLUE))
    box("tower", (0.8, 0.8, 1.4), (0, 0, 2.3), mat("wall", WALL))
    pyramid("spire", 0.9, 0.9, 3.0, mat("roof_gold", ROOF_GOLD))
    box("door", (0.6, 0.05, 0.9), (0, -1.41, 0.45), mat("wood", WOOD))


def site_frame():
    """Scaffolding shown on a 1x1 construction site; Godot scales it to the footprint."""
    m = mat("wood_light", WOOD_LIGHT)
    for x in (-0.45, 0.45):
        for y in (-0.45, 0.45):
            box(f"post{x}{y}", (0.06, 0.06, 1.0), (x, y, 0.5), m)
    for z in (0.05, 0.95):
        for y in (-0.45, 0.45):
            box(f"beamx{z}{y}", (0.96, 0.05, 0.05), (0, y, z), m)
        for x in (-0.45, 0.45):
            box(f"beamy{z}{x}", (0.05, 0.96, 0.05), (x, 0, z), m)


def tree():
    cyl("trunk", 0.1, 0.5, (0, 0, 0.25), mat("wood", WOOD), verts=8)
    cone("leaves1", 0.45, 0.8, (0, 0, 0.8), mat("leaf", LEAF), verts=8)
    cone("leaves2", 0.33, 0.6, (0, 0, 1.2), mat("leaf_dark", LEAF_DARK), verts=8)


def rock():
    ico("rock1", 0.35, (0, 0, 0.18), mat("stone", STONE), sub=1, scale=(1.1, 0.9, 0.7))
    ico("rock2", 0.2, (0.22, 0.15, 0.12), mat("stone_dark", STONE_DARK), sub=1)


def berry_bush():
    for i, (x, y, r) in enumerate([(0, 0, 0.3), (0.18, 0.1, 0.22), (-0.15, 0.12, 0.2)]):
        sphere(f"bush{i}", r, (x, y, r), mat("leaf", LEAF), seg=10)
    for i in range(7):
        a = i / 7 * math.tau
        sphere(f"berry{i}", 0.05, (math.cos(a) * 0.27, math.sin(a) * 0.27, 0.32 + (i % 2) * 0.12), mat("berry", BERRY), seg=6)


MODELS = {
    "colonist": colonist, "campfire": campfire, "stockpile": stockpile, "well": well,
    "farm_plot": farm_plot, "farm_crops": farm_crops, "house": house, "storehouse": storehouse,
    "workshop": workshop, "kitchen": kitchen, "town_hall": town_hall, "site_frame": site_frame,
    "tree": tree, "rock": rock, "berry_bush": berry_bush,
}

for name, fn in MODELS.items():
    clear()
    fn()
    export(name)
print("done", len(MODELS), "models")
