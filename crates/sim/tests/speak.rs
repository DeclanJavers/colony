//! M8: speaking to someone (god tool "speak", read by keyword on the Low setting) changes what
//! they do, and the trace shows why.

use sim::{Command, Event, Sim};

#[test]
fn words_are_read_by_keyword() {
    let sim = Sim::new(1, 64);
    let c = &sim.content;
    let (acts, read) = sim::speak::read("Could you fetch some firewood, and maybe sing later?", c);
    assert!(acts.contains(&"chop".to_string()) && acts.contains(&"sing".to_string()), "{acts:?}");
    assert_eq!(read, vec!["fetch wood".to_string(), "sing".to_string()]);
    assert!(sim::speak::read("hello there", c).0.is_empty());
}

#[test]
fn asking_someone_for_wood_changes_what_they_do() {
    // Over several people and seeds: after being asked, they fetch wood far more often than the
    // same people do unasked, and every such decision's trace shows the request.
    let (mut asked_wood, mut asked_n, mut plain_wood, mut plain_n) = (0, 0, 0, 0);
    for seed in 1..=4u64 {
        for ask in [true, false] {
            let mut sim = Sim::new(seed, 256);
            // Mid-morning on the second day (not just before bed, when sleep wins).
            let cal = &sim.content.calendar;
            let target = cal.ticks_per_day() + 10 * 60 / cal.minutes_per_tick as u64;
            sim.run_ticks(target - sim.tick % cal.ticks_per_day().max(1) - (sim.tick / cal.ticks_per_day()) * cal.ticks_per_day());
            let people: Vec<_> = sim.people.iter().filter(|(_, p)| p.alive && p.grown(sim.tick, &sim.content)).map(|(id, _)| id).take(4).collect();
            let asked_at = sim.tick;
            if ask {
                for id in &people {
                    sim.command(Command::Speak { person: *id, words: "Please fetch some wood for the fire.".into() });
                }
            }
            sim.run_ticks(4 * 60);
            if ask {
                let heard = sim.take_events().into_iter().filter(|e| matches!(e, Event::Spoken { .. })).count();
                assert_eq!(heard, people.len());
            }
            for (k, id) in people.iter().enumerate() {
                let p = sim.people.get(*id).unwrap();
                // Decisions made after the words were heard.
                let fresh: Vec<_> = p.traces.iter().chain(p.open_trace.iter()).filter(|t| t.tick >= asked_at).collect();
                let _ = k;
                let wood = fresh.iter().take(3).any(|t| matches!(t.request.options[t.choice].kind.name(&sim.content).as_str(), "chop" | "chop_pine"));
                if ask {
                    asked_n += 1;
                    asked_wood += wood as u32;
                    // The decision that answered the request shows it: the feature and the words.
                    if let Some(t) = fresh.iter().find(|t| t.request.options[t.choice].f("asked") > 0.0) {
                        assert!(t.request.text.contains("A voice from the sky said to you"), "the text doesn't show what they heard");
                    }
                } else {
                    plain_n += 1;
                    plain_wood += wood as u32;
                }
            }
        }
    }
    println!("M8 speak: asked for wood, {asked_wood} of {asked_n} fetched it within their next 3 decisions; unasked, {plain_wood} of {plain_n}");
    assert!(asked_wood * 4 >= asked_n * 3, "only {asked_wood} of {asked_n} asked people fetched wood");
    assert!(asked_wood >= plain_wood + asked_n / 3, "asking made little difference: {asked_wood} vs {plain_wood}");
}

