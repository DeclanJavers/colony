//! M3 "done when" checks (docs/ROADMAP.md): survival, speed, the body clock, and decision traces.

use sim::Sim;
use sim::brain::Ending;

fn days(sim: &Sim, d: u64) -> u64 {
    sim.content.calendar.ticks_per_day() * d
}

#[test]
fn a_rules_colony_survives_a_year_on_ten_seeds() {
    if cfg!(debug_assertions) {
        return; // a year per seed: run with cargo test --release
    }
    let mut lost = Vec::new();
    for seed in 1..=10 {
        let mut sim = Sim::new(seed, 256);
        let year = days(&sim, sim.content.calendar.days_per_year());
        let t = std::time::Instant::now();
        sim.run_ticks(year);
        let secs = t.elapsed().as_secs_f64();
        assert!(secs < 60.0, "seed {seed}: a year took {secs:.1} s (budget 60)");
        let alive = sim.alive();
        if alive < sim.people.len() {
            lost.push(format!("seed {seed}: {} of {} died", sim.people.len() - alive, sim.people.len()));
        }
    }
    assert!(lost.is_empty(), "{}", lost.join("; "));
}

#[test]
fn nobody_sleeps_through_the_day() {
    for seed in [2, 5] {
        let mut sim = Sim::new(seed, 256);
        let n = 4;
        sim.run_ticks(days(&sim, n));
        for (_, p) in sim.people.iter() {
            let share = p.tally.day_sleep_minutes as f64 / (n as f64 * 600.0);
            assert!(share < 0.05, "seed {seed}: {} slept {:.1}% of 08:00-18:00", p.name, share * 100.0);
        }
    }
}

#[test]
fn every_decision_leaves_a_full_trace() {
    let mut sim = Sim::new(3, 256);
    sim.run_ticks(days(&sim, 1));
    let mut total = 0;
    for (_, p) in sim.people.iter() {
        assert!(!p.traces.is_empty(), "{} made no decisions", p.name);
        for t in &p.traces {
            total += 1;
            let r = &t.request;
            assert_eq!(t.scores.len(), r.options.len());
            assert_eq!(t.rules_scores.len(), r.options.len());
            assert!(r.options[t.choice].possible, "{} chose an impossible option", p.name);
            assert!(!r.text.is_empty() && !r.state.is_empty());
            assert!(r.options.iter().all(|o| o.possible || o.why_not.is_some()), "impossible options say why");
            assert!(r.options.iter().filter(|o| o.possible).all(|o| !o.features.is_empty()));
            let out = t.outcome.as_ref().expect("finished decisions have an outcome");
            if let Ending::Failed(w) | Ending::Interrupted(w) = &out.result {
                assert!(!w.is_empty());
            }
        }
    }
    assert!(total > 50, "only {total} decisions in a day");
}

#[test]
fn people_are_born_with_looks_from_their_dna() {
    let sim = Sim::new(4, 256);
    let heights: Vec<f32> = sim.people.iter().map(|(_, p)| p.looks.height).collect();
    assert!(heights.iter().all(|h| (1.35..2.0).contains(h)));
    let distinct: std::collections::BTreeSet<String> = sim.people.iter().map(|(_, p)| format!("{:?}", p.looks.skin)).collect();
    assert!(distinct.len() > 3, "founders should look different");
}
