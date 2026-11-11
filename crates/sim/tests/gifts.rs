//! The gift god tool (M10): food and materials into the stores, and finished buildings.

use sim::commands::Gift;
use sim::{Command, Event, Sim};

fn gift(sim: &mut Sim, g: Gift) -> Vec<Event> {
    sim.take_events();
    sim.command(Command::Gift(g));
    sim.step();
    sim.take_events()
}

#[test]
fn food_and_materials_go_into_the_stores() {
    let mut sim = Sim::new(3, 256);
    sim.run_ticks(60);
    let berries = sim.content.item("berries");
    let before = sim.camp.stores[berries];
    let ev = gift(&mut sim, Gift::Items { item: "berries".into(), amount: 30 });
    // People may eat in the same tick, so allow a little to go.
    let after = sim.camp.stores[berries];
    assert!(after + 3 >= before + 30 && after <= before + 30, "berries {before} -> {after}");
    assert!(ev.iter().any(|e| matches!(e, Event::Gifted { what, stores: true } if what == "30 berries")), "{ev:?}");
    // Nonsense is ignored, not a crash.
    let ev = gift(&mut sim, Gift::Items { item: "gold".into(), amount: 5 });
    assert!(!ev.iter().any(|e| matches!(e, Event::Gifted { .. })));
}

#[test]
fn a_gifted_house_is_finished_and_someone_moves_in() {
    let mut sim = Sim::new(3, 256);
    sim.run_ticks(60);
    let house = sim.content.building("house");
    let homes_before = sim.structures.iter().filter(|(_, s)| s.kind == house).count();
    let ev = gift(&mut sim, Gift::Building { kind: "house".into() });
    assert!(ev.iter().any(|e| matches!(e, Event::Gifted { what, stores: false } if what == "a house")), "{ev:?}");
    let gifted: Vec<_> = sim.structures.iter().filter(|(_, s)| s.kind == house && s.done()).map(|(id, _)| id).collect();
    assert_eq!(sim.structures.iter().filter(|(_, s)| s.kind == house).count(), homes_before + 1);
    assert_eq!(gifted.len(), 1, "the gifted house is finished");
    // Homes are handed out each morning: by then a household lives in it.
    sim.run_ticks(sim.content.calendar.ticks_per_day());
    let s = sim.structures.get(gifted[0]).expect("still standing");
    assert!(s.owner.is_some(), "nobody moved into the gifted house");
}

#[test]
fn a_gifted_storehouse_keeps_food() {
    let mut sim = Sim::new(4, 256);
    sim.run_ticks(60);
    assert!(!sim::buildings::has_storehouse(&sim.structures, &sim.content));
    gift(&mut sim, Gift::Building { kind: "storehouse".into() });
    assert!(sim::buildings::has_storehouse(&sim.structures, &sim.content));
}

#[test]
fn gifts_replay_from_the_command_log() {
    let run = || {
        let mut sim = Sim::new(5, 256);
        sim.run_ticks(100);
        sim.command(Command::Gift(Gift::Items { item: "wood".into(), amount: 20 }));
        sim.command(Command::Gift(Gift::Building { kind: "shelter".into() }));
        sim.run_ticks(sim.content.calendar.ticks_per_day());
        sim.state_hash()
    };
    assert_eq!(run(), run());
}
