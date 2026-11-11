//! The scorer (M9): it learns a teacher's choices, and decides in well under a millisecond.

use brains::scorer::{Example, train};
use brains::{Scorer, ScorerBrain};
use sim::Sim;
use sim::brain::Brain;

/// Decisions from a short rules run, as examples.
fn rules_examples(seed: u64, days: u64) -> Vec<Example> {
    let mut sim = Sim::new(seed, 256);
    sim.run_ticks(sim.content.calendar.ticks_per_day() * days);
    let c = sim.content.clone();
    let act_of = |o: &sim::brain::Choice| o.kind.name(&c);
    sim.people.iter().flat_map(|(_, p)| p.traces.iter().filter_map(|t| Example::from_trace(t, &act_of)).collect::<Vec<_>>()).collect()
}

#[test]
fn it_learns_the_rules_brain() {
    if cfg!(debug_assertions) {
        return;
    }
    // Seeds 1..4 to learn from; seed 5 is held out by the trainer.
    let mut data = Vec::new();
    for seed in 1..=5 {
        data.extend(rules_examples(seed, 6));
    }
    let s = train("test", &data, 12);
    println!("M9 scorer: {}; held-out agreement {:.1}% (kind {:.1}%)", s.trained_on, s.held_out_agreement * 100.0, s.held_out_kind_agreement * 100.0);
    assert!(s.held_out_agreement > 0.55, "agreement {:.2}", s.held_out_agreement);
}

#[test]
fn it_decides_in_well_under_a_millisecond() {
    let data = rules_examples(2, if cfg!(debug_assertions) { 1 } else { 3 });
    let s: Scorer = train("speed", &data, 2);
    let mut sim = Sim::new(3, 256);
    sim.run_ticks(sim.content.calendar.ticks_per_day());
    let reqs: Vec<_> = sim.people.iter().flat_map(|(_, p)| p.traces.iter().map(|t| t.request.clone())).collect();
    let mut b = ScorerBrain::new(s);
    let t0 = std::time::Instant::now();
    for r in &reqs {
        b.submit(r.clone());
    }
    let answers = b.poll();
    let per = t0.elapsed().as_secs_f64() * 1e6 / reqs.len().max(1) as f64;
    println!("M9 scorer: {:.1} µs per decision over {} decisions", per, reqs.len());
    assert_eq!(answers.len(), reqs.len());
    if !cfg!(debug_assertions) {
        assert!(per < 1000.0, "{per:.0} µs per decision");
    }
}
