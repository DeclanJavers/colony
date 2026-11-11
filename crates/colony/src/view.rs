//! How the game shows the sim: people and the camp for the renderer (positions smoothed between
//! ticks), picking people under the cursor, and the text of the hover card and inspector. Plain
//! for now (M3); the real UI kit arrives in M4.

use glam::{Vec2, Vec3};
use render::people::PersonDraw;
use render::{CampDraw, Renderer, linear};
use sim::actions::Anim;
use sim::people::{Sex, TRAITS};
use sim::{PersonId, Sim};
use std::collections::BTreeMap;

pub fn key(id: PersonId) -> u64 {
    ((id.index as u64) << 32) | id.generation as u64
}

/// Positions (x, z, facing) of everyone, to smooth movement between ticks.
pub type Poses = BTreeMap<u64, (f32, f32, f32)>;

pub fn poses(sim: &Sim) -> Poses {
    sim.people.iter().filter(|(_, p)| p.alive).map(|(id, p)| (key(id), (p.pos.0, p.pos.1, p.facing))).collect()
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
pub fn people(sim: &Sim, ground: &render::terrain::Ground, prev: &Poses, alpha: f32, selected: Option<PersonId>, ticks_per_second: f32, walk_ref: f32) -> Vec<PersonDraw> {
    let c = &sim.content;
    let mut out = Vec::new();
    for (id, p) in sim.people.iter() {
        if !p.alive {
            continue;
        }
        let k = key(id);
        let moved = prev.get(&k).map_or(0.0, |&(px, pz, _)| ((p.pos.0 - px).powi(2) + (p.pos.1 - pz).powi(2)).sqrt());
        let (x, z, f) = match prev.get(&k) {
            Some(&(px, pz, pf)) => (px + (p.pos.0 - px) * alpha, pz + (p.pos.1 - pz) * alpha, lerp_angle(pf, p.facing, alpha)),
            None => (p.pos.0, p.pos.1, p.facing),
        };
        let anim = if p.asleep { Anim::Sleep } else { p.action.as_ref().map_or(Anim::Idle, |a| a.anim) };
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
        if p.looks.beard {
            parts |= 1 << 3;
        }
        if p.sex == Sex::Female {
            parts |= 1 << 4;
        }
        out.push(PersonDraw {
            key: k,
            pos: Vec3::new(x, ground.height(x, z), z),
            facing: f,
            height: p.looks.height / 1.65,
            build: p.looks.build,
            skin: rgb(p.looks.skin),
            hair: rgb(p.looks.hair),
            clothes: rgb(p.looks.clothes),
            eyes: rgb(p.looks.eyes),
            parts,
            clip: anim.clip(),
            rate: if anim == Anim::Walk { (moved * ticks_per_second / (walk_ref * p.looks.height / 1.65)).clamp(0.15, 6.0) } else { 1.0 },
            carry: if anim == Anim::Sleep { None } else { carry },
            selected: Some(id) == selected,
        });
    }
    out
}

pub fn camp(sim: &Sim, ground: &render::terrain::Ground) -> CampDraw {
    let at = |t: (f32, f32)| Vec3::new(t.0, ground.height(t.0, t.1), t.1);
    CampDraw { stores: at(sim.camp.stores_pos()), fire: at(sim.camp.fire_pos()), lit: sim.camp.fire_lit }
}

/// The person under the cursor, if any (within 30 window pixels of their middle).
pub fn pick(r: &Renderer, sim: &Sim, draws: &[PersonDraw], cursor: Vec2, win: (u32, u32), scale: f32) -> Option<PersonId> {
    let mut best: Option<(f32, u64)> = None;
    for d in draws {
        let Some(s) = r.to_screen(d.pos + Vec3::new(0.0, 0.55, 0.0), win) else { continue };
        let dist = (s - cursor).length();
        if dist < 30.0 * scale && best.is_none_or(|(b, _)| dist < b) {
            best = Some((dist, d.key));
        }
    }
    let k = best?.1;
    sim.people.iter().map(|(id, _)| id).find(|id| key(*id) == k)
}

fn bar(v: f32) -> String {
    let n = (v.clamp(0.0, 1.0) * 10.0).round() as usize;
    format!("[{}{}]", "#".repeat(n), ".".repeat(10 - n))
}

/// A few lines about someone, for hovering.
pub fn hover(sim: &Sim, id: PersonId) -> Vec<String> {
    let Some(p) = sim.people.get(id) else { return Vec::new() };
    let c = &sim.content;
    let mut v = vec![format!("{}, {}", p.name, p.age_years(sim.tick, c) as u32)];
    v.push(if p.asleep { "asleep".to_string() } else { p.action.as_ref().map_or("thinking".to_string(), |a| a.label.clone()) });
    if let Some((i, val)) = p.pressing(c) {
        v.push(format!("most pressing: {}", c.needs[i].word(val)));
    }
    v.push("click to inspect".into());
    v
}

/// The plain inspector: everything about a person, in lines.
pub fn inspector(sim: &Sim, id: PersonId) -> Vec<String> {
    let Some(p) = sim.people.get(id) else { return Vec::new() };
    let c = &sim.content;
    let age = p.age_years(sim.tick, c) as u32;
    let sex = if p.sex == Sex::Female { "woman" } else { "man" };
    let mut v = vec![format!("{}, {age}, {sex}", p.name)];
    if !p.alive {
        if let Some((_, cause)) = &p.died {
            v.push(format!("died of {cause}"));
        }
        return v;
    }
    v.push(format!("now: {}", if p.asleep { "asleep".to_string() } else { p.action.as_ref().map_or("thinking".to_string(), |a| a.label.clone()) }));
    v.push(format!("\"{}\"", p.thought));
    v.push(format!("brain: {}, {} decisions", p.brain, p.tally.decisions));
    v.push(String::new());
    v.push("needs".into());
    for (i, n) in c.needs.iter().enumerate() {
        let sat = 1.0 - p.needs[i];
        v.push(format!("{:<7} {} {}", n.name.to_lowercase(), bar(sat), n.word(p.needs[i])));
    }
    v.push(format!("{:<7} {} {:.0}%", "health", bar(p.health), p.health * 100.0));
    let carried: Vec<String> = c.items.iter().zip(&p.carry).filter(|(_, n)| **n > 0).map(|(it, n)| format!("{n} {}", it.name.to_lowercase())).collect();
    if !carried.is_empty() {
        v.push(format!("carrying {}", carried.join(", ")));
    }
    v.push(String::new());
    v.push("body".into());
    let g = &c.genes;
    let build = if p.looks.build < 0.35 { "slight" } else if p.looks.build > 0.65 { "broad" } else { "medium" };
    v.push(format!("{:.2} m, {build} build", p.looks.height));
    v.push(format!("{} skin, {} hair", p.looks.skin_word, p.looks.hair_word));
    v.push(format!("{} eyes ({})", p.looks.eye_word, p.genome.allele_text(g, "eye_brown")));
    v.push(String::new());
    v.push("personality".into());
    let traits: Vec<String> = TRAITS.iter().zip(p.personality).map(|(t, x)| format!("{t} {:.0}", x * 10.0)).collect();
    v.push(traits[..3].join("  "));
    v.push(traits[3..].join("  "));
    v.push(String::new());
    v.push("skills".into());
    let skills: Vec<String> = c.skills.skills.iter().enumerate().map(|(i, s)| format!("{} {}", s.name.to_lowercase(), p.skill_level(i, c))).collect();
    v.push(skills[..3].join("  "));
    v.push(skills[3..].join("  "));
    v.push(String::new());
    v.push(format!(
        "knows {} places, {} water spots, {:.0}% explored",
        p.knowledge.nodes.len(),
        p.knowledge.water.len(),
        p.knowledge.explored_share() * 100.0
    ));
    if let Some(t) = p.open_trace.as_ref().or(p.traces.back()) {
        v.push(String::new());
        v.push("last decision".into());
        let chosen = &t.request.options[t.choice];
        v.push(format!("{} ({} options)", chosen.label.to_lowercase(), t.request.options.iter().filter(|o| o.possible).count()));
        if t.choice == t.rules_choice {
            v.push("rules agreed".into());
        } else {
            v.push(format!("rules picked: {}", t.request.options[t.rules_choice].label.to_lowercase()));
        }
        if let Some(o) = &t.outcome {
            let r = match &o.result {
                sim::brain::Ending::Done => "done".to_string(),
                sim::brain::Ending::Failed(w) => format!("failed: {w}"),
                sim::brain::Ending::Interrupted(w) => format!("stopped: {w}"),
            };
            v.push(format!("{r} in {} min", o.minutes));
        }
    }
    v
}
