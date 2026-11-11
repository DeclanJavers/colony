//! How the game shows the sim to the renderer: people, animals, buildings, fields and the camp
//! (positions smoothed between ticks, lens rings), and picking things under the cursor. The UI
//! itself is in hud.rs.

use glam::{Mat4, Vec2, Vec3};
use render::people::PersonDraw;
use render::{AnimalDraw, CampDraw, Renderer, ThingDraw, linear};
use sim::actions::Anim;
use sim::people::Sex;
use sim::{PersonId, Sim};
use std::collections::BTreeMap;

pub fn key(id: PersonId) -> u64 {
    ((id.index as u64) << 32) | id.generation as u64
}

/// An animal's key (kept apart from people's by the top bit).
pub fn animal_key(id: sim::animals::AnimalId) -> u64 {
    (1 << 63) | ((id.index as u64) << 32) | id.generation as u64
}

/// Positions (x, z, facing) of everyone and every animal, to smooth movement between ticks.
pub type Poses = BTreeMap<u64, (f32, f32, f32)>;

pub fn poses(sim: &Sim) -> Poses {
    let mut out: Poses = sim.people.iter().filter(|(_, p)| p.alive).map(|(id, p)| (key(id), (p.pos.0, p.pos.1, p.facing))).collect();
    out.extend(sim.animals.iter().map(|(id, a)| (animal_key(id), (a.pos.0, a.pos.1, a.facing))));
    out
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let d = (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    a + d * t
}

fn rgb(c: [u8; 3]) -> [f32; 3] {
    linear(&format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
}

/// What to draw for each living person. `prev` is where they were a tick ago; `alpha` how far
/// into the next tick we are.
/// `ticks_per_second` (real time) and `walk_ref` (the walk clip's no-slide speed) set how fast
/// each walk cycle plays.
#[allow(clippy::too_many_arguments)]
/// `side` is the camera's right on the ground: toddlers walk beside their carer that way, so
/// they never stand in front of them on screen.
pub fn people(sim: &Sim, ground: &render::terrain::Ground, prev: &Poses, alpha: f32, selected: Option<PersonId>, ticks_per_second: f32, walk_ref: f32, lens: crate::hud::Lens, side: (f32, f32)) -> Vec<PersonDraw> {
    let c = &sim.content;
    let mut out = Vec::new();
    let seen = sim::family::seen(&sim.people, sim.tick, c);
    for (id, p) in sim.people.iter() {
        // The dead lie where they fell until someone buries them. Whoever sleeps at home is
        // inside.
        if (!p.alive && p.buried) || (p.alive && p.asleep && p.indoors.is_some()) {
            continue;
        }
        let k = key(id);
        let moved = prev.get(&k).map_or(0.0, |&(px, pz, _)| ((p.pos.0 - px).powi(2) + (p.pos.1 - pz).powi(2)).sqrt());
        let (mut x, mut z, mut f) = match prev.get(&k) {
            Some(&(px, pz, pf)) => (px + (p.pos.0 - px) * alpha, pz + (p.pos.1 - pz) * alpha, lerp_angle(pf, p.facing, alpha)),
            None => (p.pos.0, p.pos.1, p.facing),
        };
        // Babies: an infant in its carer's arms isn't drawn while they're up and about (a tiny
        // figure on top of theirs only blurs both). When the carer sits or sleeps the baby is
        // beside them; toddlers walk beside them. "Beside" is to the side on screen.
        let mut baby_clip = None;
        if p.alive
            && let Some(me) = sim::family::find(&seen, id).filter(|s| s.stage == sim::people::social::Stage::Baby)
            && let Some(cid) = sim::family::carer(&seen, me)
            && let Some(carer) = sim.people.get(cid)
        {
            let ck = key(cid);
            let (cx, cz, cf) = match prev.get(&ck) {
                Some(&(px, pz, pf)) => (px + (carer.pos.0 - px) * alpha, pz + (carer.pos.1 - pz) * alpha, lerp_angle(pf, carer.facing, alpha)),
                None => (carer.pos.0, carer.pos.1, carer.facing),
            };
            if carer.asleep && carer.indoors.is_some() {
                continue;
            }
            let sitting = carer.action.as_ref().is_some_and(|a| a.anim == Anim::Sit);
            if me.age < c.life.nursing_age && !carer.asleep && !sitting {
                continue;
            }
            if me.age < c.life.nursing_age && sitting {
                baby_clip = Some("sit");
            }
            let gap = if carer.asleep || sitting { 0.55 } else { 0.7 };
            (x, z, f) = (cx + side.0 * gap, cz + side.1 * gap, cf);
            let _ = ck;
        }
        let anim = if p.asleep || !p.alive { Anim::Sleep } else { p.action.as_ref().map_or(Anim::Idle, |a| a.anim) };
        // Children grow: a baby is about a third of their grown height.
        let l = &c.life;
        let age = p.age_years(sim.tick, c);
        let growth = if age >= l.adult_age { 1.0 } else { l.birth_height + (1.0 - l.birth_height) * (age / l.adult_age).clamp(0.0, 1.0).powf(0.7) };
        let dim = |col: [f32; 3]| if p.alive { col } else { [col[0] * 0.55, col[1] * 0.55, col[2] * 0.6] };
        let carry = c
            .items
            .iter()
            .zip(&p.carry)
            .filter(|(_, n)| **n > 0)
            .max_by_key(|(_, n)| **n)
            .map(|(it, _)| match it.id.as_str() {
                "berries" => "berry_bush",
                "water" => "prop_barrel",
                "stone" => "pebbles",
                _ => "sticks",
            });
        let mut parts = if p.looks.hair_style == 0 { 1 << 1 } else { 1 << 2 };
        if p.looks.beard && age >= l.adult_age {
            parts |= 1 << 3;
        }
        // The skirt is rigid (it moves with the hips), so it would stick out through bent legs:
        // it shows only standing and walking.
        let clip = baby_clip.unwrap_or(anim.clip());
        if p.sex == Sex::Female && !matches!(clip, "sit" | "lie_down") {
            parts |= 1 << 4;
        }
        out.push(PersonDraw {
            key: k,
            pos: Vec3::new(x, ground.height(x, z), z),
            facing: f,
            height: p.looks.height / 1.65,
            build: p.looks.build,
            size: growth,
            skin: dim(rgb(p.looks.skin)),
            hair: dim(rgb(p.looks.hair)),
            clothes: dim(rgb(p.looks.clothes)),
            eyes: dim(rgb(p.looks.eyes)),
            parts,
            clip,
            rate: if anim == Anim::Walk { (moved * ticks_per_second / (walk_ref * p.looks.height / 1.65 * growth)).clamp(0.15, 6.0) } else { 1.0 },
            carry: if anim == Anim::Sleep || age < l.child_age { None } else { carry },
            selected: Some(id) == selected,
            ring: if p.alive { crate::hud::ring_colour(sim, p, lens) } else { None },
        });
    }
    personal_space(&mut out, ground);
    out
}

/// Model names by variant (the renderer says how many of each there are).
const VARIANTS: &[(&str, [&str; 4])] = &[
    ("house", ["house", "house_2", "house_3", "house_4"]),
    ("shelter", ["shelter", "shelter_2", "shelter_3", "shelter"]),
    ("storehouse", ["storehouse", "storehouse", "storehouse", "storehouse"]),
];

fn variant(r: &Renderer, base: &str, pick: u64) -> Option<&'static str> {
    let names = VARIANTS.iter().find(|(b, _)| *b == base)?.1;
    let n = r.model_variants(base).clamp(1, 4);
    Some(names[pick as usize % n])
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Buildings, building sites and fields. `hour` and `night` light the windows of homes where
/// someone lives, in the evening and before dawn.
pub fn things(sim: &Sim, ground: &render::terrain::Ground, r: &Renderer, hour: f32, night: f32) -> Vec<ThingDraw> {
    let c = &sim.content;
    let mut out = Vec::new();
    let evening = night > 0.15 && ((16.5..22.5).contains(&hour) || (5.0..7.5).contains(&hour));
    for (id, s) in sim.structures.iter() {
        let def = &c.buildings[s.kind as usize];
        let (cx, cz) = s.centre(c);
        // Stand it on the lowest corner so it never floats (the footing hides the rest).
        let corners = [(s.x as f32, s.y as f32), (s.x as f32 + def.w as f32, s.y as f32), (s.x as f32, s.y as f32 + def.h as f32), (s.x as f32 + def.w as f32, s.y as f32 + def.h as f32)];
        let y = corners.iter().map(|&(x, z)| ground.height(x, z)).fold(ground.height(cx, cz), f32::min);
        let at = Mat4::from_translation(Vec3::new(cx, y, cz));
        let pick = sim::hash::hash2(0xB11D, s.x as i32, s.y as i32);
        if def.id == "field" {
            // The terrain draws the dug soil; the grain stands on each tile, following the ground.
            let Some(crop) = s.crop.as_ref().filter(|_| s.done()) else { continue };
            // Green shoots that grow tall, then turn gold as they ripen; a dead crop lies brown.
            let (h, tint) = if crop.dead {
                (0.3, linear("#8A5A35"))
            } else {
                let g = crop.growth.clamp(0.0, 1.0);
                (0.12 + 0.88 * g, mix(linear("#7DB040"), linear("#F2C640"), ((g - 0.7) / 0.3).clamp(0.0, 1.0)))
            };
            for ty in s.y..s.y + def.h {
                for tx in s.x..s.x + def.w {
                    let (x, z) = (tx as f32 + 0.5, ty as f32 + 0.5);
                    let t = Mat4::from_translation(Vec3::new(x, ground.height(x, z), z)) * Mat4::from_scale(Vec3::new(1.0, h, 1.0));
                    out.push(ThingDraw { model: "crop_tile", transform: t, tint: Some(tint), lit: false });
                }
            }
            continue;
        }
        let base = def.id.as_str();
        let Some(model) = variant(r, base, pick) else { continue };
        if !s.done() {
            // A site: scaffolding over the footprint; once half built, the building stands inside it.
            let frame = Mat4::from_scale(Vec3::new(def.w as f32 * 0.98, 1.0 + 0.2 * (def.w as f32 - 1.0), def.h as f32 * 0.98));
            out.push(ThingDraw { model: "site_frame", transform: at * frame, tint: None, lit: false });
            if s.progress >= 0.5 {
                out.push(ThingDraw { model, transform: at, tint: None, lit: false });
            }
            continue;
        }
        let lived_in = def.sleeps > 0 && s.owner.and_then(|o| sim.people.get(o)).is_some_and(|o| o.alive);
        let _ = id;
        out.push(ThingDraw { model, transform: at, tint: None, lit: lived_in && evening });
        // A decorated house: stones to the door, flower boxes, carved posts (M7).
        if s.decor > 0 && def.id == "house" {
            let decor = ["house_decor_1", "house_decor_2", "house_decor_3"][(s.decor as usize - 1).min(2)];
            out.push(ThingDraw { model: decor, transform: at, tint: None, lit: false });
        }
    }
    out
}

/// Every animal, smoothed between ticks like people. Rigged kinds play a clip at the speed
/// their feet need; whole models hop.
pub fn animals(sim: &Sim, ground: &render::terrain::Ground, r: &Renderer, prev: &Poses, alpha: f32, ticks_per_second: f32) -> Vec<AnimalDraw> {
    let c = &sim.content;
    let mut out = Vec::with_capacity(sim.animals.len());
    for (id, a) in sim.animals.iter() {
        let k = animal_key(id);
        let (x, z, f, moved) = match prev.get(&k) {
            Some(&(px, pz, pf)) => (px + (a.pos.0 - px) * alpha, pz + (a.pos.1 - pz) * alpha, lerp_angle(pf, a.facing, alpha), ((a.pos.0 - px).powi(2) + (a.pos.1 - pz).powi(2)).sqrt()),
            None => (a.pos.0, a.pos.1, a.facing, 0.0),
        };
        let kind = a.kind as usize;
        let def = &c.animals[kind];
        // Young animals are smaller for their first season.
        let age_days = sim.tick.saturating_sub(a.born) as f32 / c.calendar.ticks_per_day() as f32;
        let size = 0.6 + 0.4 * (age_days / (c.calendar.days_per_season as f32)).min(1.0);
        let clip = if a.state == sim::animals::State::Flee || moved > def.speed * 1.5 {
            "run"
        } else if moved > 0.002 {
            "walk"
        } else if a.state == sim::animals::State::Graze {
            "graze"
        } else {
            "idle"
        };
        let speed = moved * ticks_per_second;
        let rate = if matches!(clip, "walk" | "run") { (speed / (r.animal_clip_speed(kind, clip) * size).max(0.05)).clamp(0.2, 5.0) } else { 1.0 };
        // Deer: the stag (the first model) is male, the does (the others) female.
        let n = r.animal_variants(kind);
        let variant = if n >= 2 && def.id == "deer" { if a.female { 1 + id.index as usize % (n - 1) } else { 0 } } else { id.index as usize % n };
        out.push(AnimalDraw { key: k, kind, variant, pos: Vec3::new(x, ground.height(x, z), z), facing: f, size, clip, rate });
    }
    out
}

/// People sharing a spot (two drinking at the same bank, a crowd at the fire) would be drawn
/// inside each other. Nudge anyone closer than arm's length apart, a little, for drawing only.
fn personal_space(people: &mut [PersonDraw], ground: &render::terrain::Ground) {
    const GAP: f32 = 0.55;
    for _ in 0..3 {
        let mut push = vec![Vec2::ZERO; people.len()];
        for i in 0..people.len() {
            for j in i + 1..people.len() {
                let (a, b) = (&people[i], &people[j]);
                let d = Vec2::new(b.pos.x - a.pos.x, b.pos.z - a.pos.z);
                let reach = GAP * 0.5 * (a.size + b.size).max(1.0);
                let len = d.length();
                if len >= reach {
                    continue;
                }
                // Exactly on top of each other: part them along a direction fixed by who they are.
                let dir = if len > 1e-3 { d / len } else { Vec2::from_angle((a.key ^ b.key) as f32 * 0.618) };
                let amount = (reach - len) * 0.5;
                push[i] -= dir * amount;
                push[j] += dir * amount;
            }
        }
        for (p, d) in people.iter_mut().zip(&push) {
            p.pos.x += d.x;
            p.pos.z += d.y;
        }
    }
    for p in people.iter_mut() {
        p.pos.y = ground.height(p.pos.x, p.pos.z);
    }
}

pub fn camp(sim: &Sim, ground: &render::terrain::Ground) -> CampDraw {
    let at = |t: (f32, f32)| Vec3::new(t.0, ground.height(t.0, t.1), t.1);
    CampDraw { stores: at(sim.camp.stores_pos()), fire: at(sim.camp.fire_pos()), lit: sim.camp.fire_lit }
}

/// Anything the player can click: a person, an animal, a building or field, a resource node or
/// grave, or the camp's things.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Person(PersonId),
    Animal(sim::animals::AnimalId),
    Structure(sim::buildings::StructureId),
    Node(sim::ids::Id<sim::Node>),
    Fire,
    Stores,
}

impl Pick {
    pub fn person(self) -> Option<PersonId> {
        if let Pick::Person(p) = self { Some(p) } else { None }
    }
}

/// What's under the cursor: a person if one is close (they come first), else the nearest
/// thing whose shape on screen the cursor is over.
#[allow(clippy::too_many_arguments)]
pub fn pick(r: &Renderer, sim: &Sim, ground: &render::terrain::Ground, cam: &render::Camera, draws: &[PersonDraw], cursor: Vec2, win: (u32, u32), scale: f32) -> Option<Pick> {
    let mut best: Option<(f32, u64)> = None;
    for d in draws {
        let Some(s) = r.to_screen(d.pos + Vec3::new(0.0, 0.55 * d.size, 0.0), win) else { continue };
        let dist = (s - cursor).length();
        if dist < 30.0 * scale && best.is_none_or(|(b, _)| dist < b) {
            best = Some((dist, d.key));
        }
    }
    if let Some((_, k)) = best {
        return sim.people.iter().map(|(id, _)| id).find(|id| key(*id) == k).map(Pick::Person);
    }
    // Screen pixels per metre, to size each thing's shape.
    let f = cam.focus;
    let ppm = match (r.to_screen(f, win), r.to_screen(f + cam.right(), win)) {
        (Some(a), Some(b)) => (b - a).length(),
        _ => return None,
    };
    // A thing is its upright shape on screen: from its base up to its height, about `w` wide.
    let near = |base: Vec3, height: f32, w: f32| -> Option<f32> {
        let a = r.to_screen(base, win)?;
        let b = r.to_screen(base + Vec3::new(0.0, height.max(0.2), 0.0), win)?;
        let ab = b - a;
        let t = ((cursor - a).dot(ab) / ab.length_squared().max(1e-3)).clamp(0.0, 1.0);
        let d = (cursor - (a + ab * t)).length();
        let reach = (w * 0.5 * ppm).max(10.0 * scale);
        (d <= reach).then_some(d / reach)
    };
    let mut best: Option<(f32, Pick)> = None;
    let mut offer = |d: Option<f32>, p: Pick| {
        if let Some(d) = d
            && best.is_none_or(|b| d < b.0)
        {
            best = Some((d, p));
        }
    };
    let at = |x: f32, z: f32| Vec3::new(x, ground.height(x, z), z);
    let (fx, fz) = sim.camp.fire_pos();
    offer(near(at(fx, fz), 0.6, 1.1), Pick::Fire);
    let (sx, sz) = sim.camp.stores_pos();
    offer(near(at(sx, sz), 0.4, 1.8), Pick::Stores);
    // Only things near the middle of the view (the camera shows about `size` metres).
    let reach = cam.size * 1.2;
    let near_view = |x: f32, z: f32| (x - f.x).abs() <= reach * 1.6 && (z - f.z).abs() <= reach * 1.6;
    for (id, a) in sim.animals.iter() {
        if !near_view(a.pos.0, a.pos.1) {
            continue;
        }
        let big = sim.content.animals[a.kind as usize].id == "deer";
        // Animals are small and move: a little more reach, and they beat what they stand on.
        offer(near(at(a.pos.0, a.pos.1), if big { 1.1 } else { 0.3 }, if big { 1.0 } else { 0.6 }).map(|d| d * 0.8), Pick::Animal(id));
    }
    for (id, s) in sim.structures.iter() {
        let def = &sim.content.buildings[s.kind as usize];
        let (x, z) = s.centre(&sim.content);
        if !near_view(x, z) {
            continue;
        }
        let h = if def.id == "field" { 0.3 } else if def.w >= 2 { 2.0 } else { 1.1 };
        let w = def.w.max(def.h) as f32 * if def.id == "field" { 0.9 } else { 0.8 };
        // Fields lie flat: anywhere over the soil counts, but whatever stands on them comes first.
        offer(near(at(x, z), h, w).map(|d| if def.id == "field" { 0.6 + d * 0.4 } else { d }), Pick::Structure(id));
    }
    for (id, n) in sim.nodes.iter() {
        let (x, z) = (n.x as f32 + 0.5, n.y as f32 + 0.5);
        if !near_view(x, z) {
            continue;
        }
        let def = &sim.content.nodes[n.kind as usize];
        let h = r.kind_height(n.kind);
        let w = if def.blocks { 0.9 } else { 0.7 };
        offer(near(at(x, z), h, w), Pick::Node(id));
    }
    best.map(|b| b.1)
}

/// Where to draw the selection ring for a thing, and how wide.
pub fn marker(sim: &Sim, ground: &render::terrain::Ground, pick: Pick) -> Option<render::Marker> {
    use render::Marker::Ring;
    let at = |x: f32, z: f32| Vec3::new(x, ground.height(x, z) + 0.02, z);
    match pick {
        Pick::Person(_) => None,
        Pick::Fire => {
            let (x, z) = sim.camp.fire_pos();
            Some(Ring(at(x, z), 1.1))
        }
        Pick::Stores => {
            let (x, z) = sim.camp.stores_pos();
            Some(Ring(at(x, z), 2.0))
        }
        Pick::Animal(id) => sim.animals.get(id).map(|a| {
            let big = sim.content.animals[a.kind as usize].id == "deer";
            Ring(at(a.pos.0, a.pos.1), if big { 1.3 } else { 0.6 })
        }),
        Pick::Structure(id) => sim.structures.get(id).map(|s| {
            let def = &sim.content.buildings[s.kind as usize];
            if !def.blocks {
                // A field lies flat on the ground: outline its tiles instead of ringing it.
                return render::Marker::Area([s.x as f32, s.y as f32, (s.x + def.w) as f32, (s.y + def.h) as f32]);
            }
            let (x, z) = s.centre(&sim.content);
            Ring(at(x, z), def.w.max(def.h) as f32 * 1.15 + 0.3)
        }),
        Pick::Node(id) => sim.nodes.get(id).map(|n| {
            let blocks = sim.content.nodes[n.kind as usize].blocks;
            Ring(at(n.x as f32 + 0.5, n.y as f32 + 0.5), if blocks { 1.1 } else { 0.9 })
        }),
    }
}

