//! M6 "done when" checks (docs/ROADMAP.md): farming starts on its own in most seeds by the end of
//! year 2, trails form and trip times fall, overhunting drops deer numbers and they recover, and
//! saving and loading a settled world changes nothing.

use sim::{Event, Sim, save};

fn day(sim: &Sim) -> u64 {
    sim.content.calendar.ticks_per_day()
}

fn year(sim: &Sim) -> u64 {
    day(sim) * sim.content.calendar.days_per_year()
}

fn deer(sim: &Sim) -> usize {
    let k = sim.content.animal("deer");
    sim.animals.iter().filter(|(_, a)| a.kind == k).count()
}

#[test]
fn farming_starts_by_the_end_of_year_two() {
    if cfg!(debug_assertions) {
        return; // years of a colony: run with cargo test --release
    }
    let seeds = 1..=8u64;
    let mut farmed = Vec::new();
    for seed in seeds.clone() {
        let mut sim = Sim::new(seed, 256);
        let y = year(&sim);
        let mut sown = None;
        for d in 0..2 * sim.content.calendar.days_per_year() {
            sim.run_ticks(day(&sim));
            if sown.is_none() && sim.take_events().iter().any(|e| matches!(e, Event::FieldSown { .. })) {
                sown = Some(d);
            }
        }
        let field = sim.content.building("field");
        let harvested: u32 = sim.structures.iter().filter(|(_, s)| s.kind == field).map(|(_, s)| s.harvested).sum();
        println!("M6 farming: seed {seed}: first field sown on day {sown:?}, {harvested} grain harvested by day {}", 2 * y / day(&sim));
        if sown.is_some() && harvested > 0 {
            farmed.push(seed);
        }
    }
    let n = seeds.count();
    assert!(farmed.len() * 4 >= n * 3, "only seeds {farmed:?} of {n} were farming (sown and harvested) by the end of year 2");
}

#[test]
fn trails_form_and_trip_times_fall() {
    if cfg!(debug_assertions) {
        return;
    }
    for seed in 1..=3u64 {
        let mut sim = Sim::new(seed, 256);
        let y = year(&sim);
        sim.run_ticks(3 * y);
        let st = &sim.stats;
        // The first season against the third year (all four seasons, so the seasons' own ground
        // doesn't skew it).
        let first_saving = st.trail_saving(0).unwrap();
        let first_pace = st.pace(0).unwrap();
        let late: Vec<usize> = (8..12).collect();
        let late_saving = late.iter().map(|&k| st.trail_saving(k).unwrap()).sum::<f32>() / 4.0;
        let late_pace = late.iter().map(|&k| st.pace(k).unwrap()).sum::<f32>() / 4.0;
        let trails = sim.map.footfall.iter().filter(|&&f| f >= sim::path::TRAIL_STEPS * 0.5).count();
        println!(
            "M6 trails: seed {seed}: trails save {:.0}% of walking time in the first season, {:.0}% in year 3; pace {first_pace:.0} -> {late_pace:.0} min/100 m; {trails} trail tiles",
            first_saving * 100.0,
            late_saving * 100.0
        );
        assert!(trails >= 60, "seed {seed}: only {trails} tiles worn into trails");
        assert!(late_saving >= first_saving + 0.12, "seed {seed}: trails save {late_saving:.2} of walking time in year 3, {first_saving:.2} at first");
        assert!(late_pace <= first_pace * 0.9, "seed {seed}: pace {first_pace:.0} -> {late_pace:.0} min/100 m");
    }
}

#[test]
fn overhunting_drops_deer_and_they_recover() {
    if cfg!(debug_assertions) {
        return;
    }
    // People do hunt deer on their own.
    let mut kills = 0;
    for seed in 1..=4u64 {
        let mut sim = Sim::new(seed, 256);
        let y = year(&sim);
        sim.run_ticks(4 * y);
        kills += sim.wildlife.hunted[sim.content.animal("deer") as usize];
    }
    assert!(kills >= 4, "only {kills} deer hunted in 4 colonies over 4 years");

    // A season of overhunting: most of the deer are killed (the way hunts kill them, scaring the
    // ground), then the hunting stops. Numbers fall, then recover from births and herds moving in.
    let mut sim = Sim::new(2, 256);
    let y = year(&sim);
    sim.run_ticks(y);
    let before = deer(&sim);
    let k = sim.content.animal("deer");
    let ids: Vec<_> = sim.animals.iter().filter(|(_, a)| a.kind == k).map(|(id, _)| id).collect();
    for id in ids.iter().take(ids.len() * 3 / 4) {
        let at = sim.animals.get(*id).unwrap().pos;
        sim.animals.remove(*id);
        sim.wildlife.scare(&sim.map, at.0, at.1, 0.3);
    }
    let after = deer(&sim);
    let mut by_year = Vec::new();
    for _ in 0..4 {
        sim.run_ticks(y);
        by_year.push(deer(&sim));
    }
    let cap = sim.wildlife.capacity[k as usize];
    println!("M6 deer: {before} before the overhunting, {after} after; then by year {by_year:?} (the land holds {cap})");
    assert!(after * 2 < before, "the overhunting didn't drop the deer: {before} -> {after}");
    let recovered = *by_year.last().unwrap();
    assert!(recovered as f32 >= before as f32 * 0.8, "deer didn't recover: {before} -> {after} -> {by_year:?}");
}

#[test]
fn saving_a_settled_world_changes_nothing() {
    let mut a = Sim::new(5, 256);
    // Late enough for homes, a field and the herds' first moves.
    let days = if cfg!(debug_assertions) { 5 } else { 30 };
    a.run_ticks(day(&a) * days);
    assert!(a.structures.iter().count() > 0, "nothing built in {days} days");
    let bytes = save::to_bytes(&a);
    let mut b = save::from_bytes(&bytes).expect("save loads");
    assert_eq!(a.state_hash(), b.state_hash(), "loaded world matches the saved one");
    assert_eq!(a.stats, b.stats, "the stats are saved");
    let d = day(&a);
    a.run_ticks(d);
    b.run_ticks(d);
    assert_eq!(a.state_hash(), b.state_hash(), "continuing after a load matches continuing without");
}
