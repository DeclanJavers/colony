//! How the game shows the sim to the renderer: people and the camp (positions smoothed between
//! ticks, lens rings), and picking people under the cursor. The UI itself is in hud.rs.

use glam::{Vec2, Vec3};
use render::people::PersonDraw;
use render::{CampDraw, Renderer, linear};
use sim::actions::Anim;
use sim::people::Sex;
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
#[allow(clippy::too_many_arguments)]
pub fn people(sim: &Sim, ground: &render::terrain::Ground, prev: &Poses, alpha: f32, selected: Option<PersonId>, ticks_per_second: f32, walk_ref: f32, lens: crate::hud::Lens) -> Vec<PersonDraw> {
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
            ring: crate::hud::ring_colour(sim, p, lens),
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

