//! The pack interface (M10): hunting lives behind it, and works exactly as it did before.
//!
//! The check is a digest of what happens (every event, and each morning the stores, every
//! person's place, needs, carried things, skills and what they're doing, and every animal), for
//! colonies given spears at the start so that hunting starts at once. The golden numbers were
//! taken with hunting still built into the base, just before it moved behind the interface.

use sim::commands::Gift;
use sim::{Command, Sim};
use std::hash::{Hash, Hasher};

fn digest(seed: u64, days: u64) -> (u64, usize) {
    let mut sim = Sim::new(seed, 256);
    sim.command(Command::Gift(Gift::Items { item: "spear".into(), amount: 4 }));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut hunts = 0;
    let per_day = sim.content.calendar.ticks_per_day();
    for _ in 0..days {
        for _ in 0..per_day {
            sim.step();
            for e in sim.take_events() {
                if matches!(e, sim::Event::Hunted { .. }) {
                    hunts += 1;
                }
                // The gift event's wording changed after the goldens were taken (it gained a
                // "stores" flag): hash it as it read then, so the check stays about behaviour.
                let text = match &e {
                    sim::Event::Gifted { what, stores: true } => format!("Gifted {{ what: \"{what} in the stores\" }}"),
                    _ => format!("{e:?}"),
                };
                text.hash(&mut h);
            }
        }
        sim.camp.stores.hash(&mut h);
        for (_, p) in sim.people.iter() {
            p.name.hash(&mut h);
            p.alive.hash(&mut h);
            p.pos.0.to_bits().hash(&mut h);
            p.pos.1.to_bits().hash(&mut h);
            for v in p.needs.iter().chain(p.practice.iter()) {
                v.to_bits().hash(&mut h);
            }
            p.carry.hash(&mut h);
            p.action.as_ref().map(|a| a.label.clone()).hash(&mut h);
        }
        for (_, a) in sim.animals.iter() {
            a.pos.0.to_bits().hash(&mut h);
            a.pos.1.to_bits().hash(&mut h);
        }
    }
    (h.finish(), hunts)
}

#[test]
fn hunting_works_the_same_as_a_pack() {
    if cfg!(debug_assertions) {
        return;
    }
    // Taken on 2026-11-11 with hunting built into the base (2, 5 and 3 animals hunted).
    let golden = [(1u64, 0x3e895d0d5622ec06u64), (2, 0xb99079d9cac86a7c), (3, 0xf9b659b2c49a03d8)];
    for (seed, want) in golden {
        let (d, hunts) = digest(seed, 48);
        println!("packs: seed {seed}: digest {d:#018x}, {hunts} animals hunted");
        assert!(hunts > 0, "seed {seed}: nobody hunted, so the check proves nothing");
        assert_eq!(d, want, "seed {seed}: the colony went differently with hunting as a pack");
    }
}

#[test]
fn a_world_without_the_pack_has_no_hunting() {
    let mut sim = Sim::new(2, 256);
    sim.settings.packs.clear();
    sim.command(Command::Gift(Gift::Items { item: "spear".into(), amount: 4 }));
    let days = if cfg!(debug_assertions) { 3 } else { 24 };
    let mut acts = 0;
    for _ in 0..days * sim.content.calendar.ticks_per_day() {
        sim.step();
        for e in sim.take_events() {
            assert!(!matches!(e, sim::Event::Hunted { .. } | sim::Event::SpearMade { .. }), "{e:?}");
        }
    }
    for (_, p) in sim.people.iter() {
        for t in &p.traces {
            acts += t.request.options.iter().filter(|o| matches!(o.kind, sim::actions::Kind::Pack { .. })).count();
        }
    }
    assert_eq!(acts, 0, "nobody is offered a pack's actions when it's off");
    // And with it on (the default), they are.
    assert_eq!(sim::pack::default_ids(), vec!["hunting".to_string()]);
}
