//! The body and the senses, every tick: needs rise, warmth follows the air and the fire, company
//! eases loneliness, health follows the body's needs, and people notice what's around them.

use crate::actions::air_temp;
use crate::camp::Camp;
use crate::content::{Content, NeedKind};
use crate::ids::Table;
use crate::people::Person;
use crate::world::{Map, Node};

/// Needs and health for one minute (or tick). Returns the cause if this kills them.
/// `beauty` is the beauty where they are, as it counts (0 when beauty doesn't matter).
pub fn body(p: &mut Person, c: &Content, map: &Map, camp: &Camp, tick: u64, company: bool, beauty: f32) -> Option<String> {
    let tm = c.calendar.minutes_per_tick as f32 / 60.0;
    let pp = &c.people;
    for (i, n) in c.needs.iter().enumerate() {
        let rate = if p.asleep { n.asleep_per_hour } else { n.awake_per_hour };
        p.needs[i] += rate * tm;
    }
    // Warmth: the fire warms, the cold chills, a warm day slowly warms.
    let w = c.need("warmth");
    let temp = air_temp(c, map, p.pos.0 as u32, p.pos.1 as u32, tick);
    let (fx, fy) = camp.fire_pos();
    let by_fire = camp.fire_lit && ((p.pos.0 - fx).powi(2) + (p.pos.1 - fy).powi(2)).sqrt() <= pp.fire_reach;
    // Under a roof the night's chill is cut (a house more than a shelter), and rain can't reach.
    let roof = p.indoors.map_or(1.0, |k| c.buildings[k as usize].chill);
    if by_fire {
        p.needs[w] -= pp.fire_warmth_per_hour * tm;
    } else if temp < pp.comfort_temp {
        let huddle = if p.asleep && company { pp.huddle } else { 1.0 };
        p.needs[w] += pp.cold_per_hour * (pp.comfort_temp - temp) * tm * huddle * roof;
    } else {
        p.needs[w] -= 0.1 * tm;
    }
    if map.weather.sky.wet() && p.indoors.is_none() && !by_fire {
        p.needs[w] += c.weather.wet_chill * tm;
    }
    // Company
    let s = c.need("social");
    if company && !p.asleep {
        p.needs[s] -= pp.company_per_hour * (0.5 + p.personality[2]) * tm;
    }
    // Somewhere beautiful slowly eases boredom.
    if beauty > 0.0 && !p.asleep {
        let f = c.need("fun");
        p.needs[f] -= c.leisure.beauty.fun_per_hour * beauty * tm;
    }
    for v in p.needs.iter_mut() {
        *v = v.clamp(0.0, 1.0);
    }
    // Health
    let mut hurting = false;
    let mut worst = (0usize, 0.0f32);
    for (i, n) in c.needs.iter().enumerate() {
        if n.kind != NeedKind::Body {
            continue;
        }
        if p.needs[i] > worst.1 {
            worst = (i, p.needs[i]);
        }
        if p.needs[i] >= 0.999 && n.damage_per_hour > 0.0 {
            p.health -= n.damage_per_hour * tm;
            hurting = true;
        }
    }
    if !hurting && worst.1 < 0.5 {
        p.health = (p.health + pp.heal_per_hour * tm).min(1.0);
    }
    if p.health <= 0.0 {
        let cause = match c.needs[worst.0].id.as_str() {
            "hunger" => "starvation",
            "thirst" => "thirst",
            "warmth" => "the cold",
            _ => "exhaustion",
        };
        return Some(cause.to_string());
    }
    None
}

/// Animals within sight: remember where they were seen.
pub fn see_animals(p: &mut Person, animals: &Table<crate::animals::Animal>, tick: u64, radius: f32) {
    for (id, a) in animals.iter() {
        if (a.pos.0 - p.pos.0).abs() <= radius && (a.pos.1 - p.pos.1).abs() <= radius && ((a.pos.0 - p.pos.0).powi(2) + (a.pos.1 - p.pos.1).powi(2)).sqrt() <= radius {
            p.knowledge.see_animal(id, a.kind, a.pos, tick);
        }
    }
    // Forget what's long gone.
    // Herds keep to a range, so a sighting is worth remembering for a few days.
    p.knowledge.animals.retain(|_, k| tick.saturating_sub(k.when) < 3 * 24 * 60);
}

/// Look around: resource nodes, fresh water, the camp. What they see replaces what they believed.
#[allow(clippy::too_many_arguments)]
pub fn perceive(p: &mut Person, c: &Content, map: &Map, nodes: &Table<Node>, camp: &Camp, tick: u64, radius: u32, check_gone: bool) {
    let n = map.size as i32;
    let r = radius as i32;
    let (px, py) = (p.pos.0 as i32, p.pos.1 as i32);
    let mut water: Option<(i32, i32, i32)> = None;
    for y in (py - r).max(0)..(py + r + 1).min(n) {
        for x in (px - r).max(0)..(px + r + 1).min(n) {
            let d2 = (x - px).pow(2) + (y - py).pow(2);
            if d2 > r * r {
                continue;
            }
            let i = map.idx(x as u32, y as u32);
            let slot = map.node_at[i];
            if slot > 0
                && let Some(id) = nodes.id_at(slot - 1)
            {
                let node = nodes.get(id).unwrap();
                if c.actions.gather.iter().any(|g| c.nodes[node.kind as usize].id == g.node) {
                    p.knowledge.see_node(id, node, tick);
                }
            }
            if crate::actions::is_fresh(map, i) && water.is_none_or(|w| d2 < w.2) {
                water = Some((x, y, d2));
            }
        }
    }
    if let Some((x, y, _)) = water {
        p.knowledge.see_water(x as u16, y as u16, tick, (camp.stores_at.0 as u32, camp.stores_at.1 as u32));
    }
    // Things they remember here that are gone (checked less often: it's a scan of everything
    // they know, and things rarely vanish).
    let gone: Vec<_> = if !check_gone { Vec::new() } else { p
        .knowledge
        .nodes
        .iter()
        .filter(|(id, k)| (k.x as i32 - px).pow(2) + (k.y as i32 - py).pow(2) <= r * r && nodes.get(**id).is_none())
        .map(|(id, _)| *id)
        .collect() };
    for id in gone {
        p.knowledge.forget_node(id);
    }
    let (sx, sy) = camp.stores_pos();
    if ((p.pos.0 - sx).powi(2) + (p.pos.1 - sy).powi(2)).sqrt() < 6.0 {
        let b = &mut p.knowledge.stores;
        b.items.clone_from(&camp.stores);
        b.fire_lit = camp.fire_lit;
        b.fire_hours = camp.fire_hours;
        b.when = tick;
    }
    p.knowledge.mark_explored(map.size, p.pos.0, p.pos.1, radius as f32);
}
