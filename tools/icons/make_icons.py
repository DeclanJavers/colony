"""Write the colony's head-badge icons as SVG files (assets/icons/*.svg).

  python3 tools/icons/make_icons.py

One consistent style: a round badge in a soft category colour with a simple white glyph.
Categories: work (green), build/farm (amber), needs (blue), social (rose), thinking (grey),
danger (red). Godot imports the SVGs as textures.
"""
import os

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "assets", "icons")
os.makedirs(OUT, exist_ok=True)

COLORS = {
    "work": "#5f9e6e", "build": "#c9973f", "need": "#5a86c8",
    "social": "#c97597", "think": "#8b909a", "danger": "#d0504a",
}
W = 'stroke="#fff" stroke-width="9" stroke-linecap="round" stroke-linejoin="round" fill="none"'
F = 'fill="#fff"'

GLYPHS = {
    # gathering
    "axe": f'<line x1="44" y1="92" x2="80" y2="40" {W}/><path d="M70 30 Q92 30 94 52 L80 56 L68 42 Z" {F}/>',
    "pickaxe": f'<line x1="44" y1="94" x2="80" y2="44" {W}/><path d="M42 44 Q64 22 98 38" {W}/>',
    "berries": f'<circle cx="52" cy="72" r="13" {F}/><circle cx="76" cy="72" r="13" {F}/><circle cx="64" cy="52" r="13" {F}/>'
               f'<path d="M64 38 Q72 26 86 28" {W}/>',
    "bucket": f'<path d="M42 52 L86 52 L80 92 L48 92 Z" {F}/><path d="M44 52 Q64 22 84 52" {W}/>',
    # build / farm
    "hammer": f'<line x1="50" y1="94" x2="70" y2="54" {W}/><rect x="52" y="34" width="40" height="20" rx="4" transform="rotate(25 72 44)" {F}/>',
    "sprout": f'<line x1="64" y1="94" x2="64" y2="56" {W}/><path d="M64 62 Q42 60 40 40 Q60 40 64 62 Z" {F}/>'
              f'<path d="M64 56 Q86 54 90 32 Q68 32 64 56 Z" {F}/>',
    # needs
    "bowl": f'<path d="M36 62 L92 62 Q90 92 64 92 Q38 92 36 62 Z" {F}/><path d="M54 52 Q50 44 56 36" {W}/><path d="M72 52 Q68 44 74 36" {W}/>',
    "drop": f'<path d="M64 30 Q90 62 88 74 Q86 96 64 96 Q42 96 40 74 Q38 62 64 30 Z" {F}/>',
    "moon": f'<path d="M78 32 Q50 36 48 64 Q50 92 80 96 Q56 104 40 84 Q26 62 42 42 Q56 28 78 32 Z" {F}/>'
            f'<path d="M76 50 L92 50 L76 66 L92 66" {W}/>',
    "rest": f'<path d="M40 70 L88 70" {W}/><path d="M46 70 L46 90 M82 70 L82 90" {W}/><path d="M48 70 L48 44" {W}/>',
    # social
    "talk": f'<path d="M34 40 Q34 32 42 32 L86 32 Q94 32 94 40 L94 70 Q94 78 86 78 L56 78 L42 94 L44 78 L42 78 Q34 78 34 70 Z" {F}/>',
    "heart": f'<path d="M64 94 Q30 70 34 48 Q38 30 56 34 Q62 36 64 44 Q66 36 72 34 Q90 30 94 48 Q98 70 64 94 Z" {F}/>',
    # thinking
    "dots": f'<circle cx="42" cy="64" r="8" {F}/><circle cx="64" cy="64" r="8" {F}/><circle cx="86" cy="64" r="8" {F}/>',
}

# icon name -> (glyph, colour)
ICONS = {
    "gather_wood": ("axe", "work"), "gather_stone": ("pickaxe", "work"), "gather_food": ("berries", "work"),
    "fetch_water": ("bucket", "work"),
    "build": ("hammer", "build"), "farm": ("sprout", "build"),
    "eat": ("bowl", "need"), "drink": ("drop", "need"), "sleep": ("moon", "need"), "rest": ("rest", "need"),
    "socialize": ("talk", "social"), "have_child": ("heart", "social"),
    "thinking": ("dots", "think"),
    "danger_thirst": ("drop", "danger"), "danger_hunger": ("bowl", "danger"), "danger_energy": ("moon", "danger"),
}

for name, (glyph, col) in ICONS.items():
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">'
        f'<circle cx="64" cy="64" r="58" fill="{COLORS[col]}"/>'
        '<circle cx="64" cy="64" r="58" fill="none" stroke="#ffffff" stroke-opacity="0.85" stroke-width="5"/>'
        f'{GLYPHS[glyph]}</svg>'
    )
    with open(os.path.join(OUT, f"{name}.svg"), "w") as f:
        f.write(svg)
print("wrote", len(ICONS), "icons to", os.path.abspath(OUT))
