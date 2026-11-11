//! M7 "done when" checks (docs/ROADMAP.md): with "beauty matters" on, mood near beautiful things
//! is measurably higher; with it off, beauty has no effect on any number. Also: people take up
//! hobbies and make things, and saving a world with them changes nothing.

use sim::{Command, Sim, save};

fn day(sim: &Sim) -> u64 {
    sim.content.calendar.ticks_per_day()
}

fn year(sim: &Sim) -> u64 {
    day(sim) * sim.content.calendar.days_per_year()
}

/// Mood (and the beauty there) of every awake adult, sampled twice a day.
fn sample(sim: &mut Sim, days: u64, out: &mut Vec<(f32, i32)>) {
    let d = day(sim);
    for _ in 0..days * 2 {
        sim.run_ticks(d / 2);
        let c = &sim.content;
        let field = &sim.beauty;
        for (_, p) in sim.people.iter().filter(|(_, p)| p.alive && !p.asleep && p.grown(sim.tick, c)) {
            let b = sim::leisure::beauty_at(field, &sim.map, p.pos);
            let (score, _) = p.mood(c, false, false, sim.beauty_for(p.pos));
            out.push((b, score));
        }
    }
}

fn mean(xs: &[i32]) -> f64 {
    xs.iter().map(|&x| x as f64).sum::<f64>() / xs.len().max(1) as f64
}

fn se(xs: &[i32]) -> f64 {
    let m = mean(xs);
    let n = xs.len().max(2) as f64;
    (xs.iter().map(|&x| (x as f64 - m).powi(2)).sum::<f64>() / (n - 1.0) / n).sqrt()
}

#[test]
fn beauty_lifts_mood_when_it_matters() {
    if cfg!(debug_assertions) {
        return; // years of a colony: run with cargo test --release
    }
    // The same seed, with beauty mattering and not. Two years to make things, then a year of
    // samples: the mood of people somewhere beautiful (by the same field, which both worlds
    // have), on and off.
    let mut near = [Vec::new(), Vec::new()];
    for (k, on) in [true, false].into_iter().enumerate() {
        for seed in [3u64, 4] {
            let mut sim = Sim::new(seed, 256);
            sim.command(Command::SetBeautyMatters { on });
            let y = year(&sim);
            sim.run_ticks(2 * y);
            let mut s = Vec::new();
            let days = sim.content.calendar.days_per_year();
            sample(&mut sim, days, &mut s);
            near[k].extend(s.iter().filter(|(b, _)| *b >= 0.2).map(|(_, m)| *m));
            let works = sim.works.len();
            println!("M7 beauty {}: seed {seed}: {works} things made, {} mood samples somewhere beautiful", if on { "on" } else { "off" }, s.iter().filter(|(b, _)| *b >= 0.2).count());
        }
    }
    let (on, off) = (&near[0], &near[1]);
    let (m_on, m_off) = (mean(on), mean(off));
    let gap_se = (se(on).powi(2) + se(off).powi(2)).sqrt();
    println!("M7 beauty: mood somewhere beautiful {m_on:.2} ± {:.2} with beauty mattering (n {}), {m_off:.2} ± {:.2} without (n {})", se(on), on.len(), se(off), off.len());
    assert!(on.len() >= 100 && off.len() >= 100, "too few samples somewhere beautiful ({} on, {} off)", on.len(), off.len());
    assert!(m_on - m_off >= 1.0 && m_on - m_off > 4.0 * gap_se, "mood somewhere beautiful: {m_on:.2} on vs {m_off:.2} off (± {gap_se:.2})");
}

#[test]
fn with_beauty_off_it_changes_nothing() {
    // Two runs with beauty not mattering. In one, the beauty field is replaced with noise every
    // tick. If anything read it (mood, needs, choices, where people stroll), the worlds would part.
    let days = if cfg!(debug_assertions) { 3 } else { 40 };
    let run = |scramble: bool| {
        let mut sim = Sim::new(4, 256);
        sim.command(Command::SetBeautyMatters { on: false });
        let n = day(&sim) * days;
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..n {
            if scramble {
                for v in sim.beauty.iter_mut() {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    *v = (x % 1000) as f32 / 1000.0;
                }
            }
            sim.step();
        }
        (sim.state_hash(), sim.works.len(), sim.people.iter().map(|(_, p)| p.hobbies.iter().map(|h| h.times).sum::<u32>()).sum::<u32>())
    };
    let (a, works, acts) = run(false);
    let (b, _, _) = run(true);
    println!("M7 beauty off: {works} things made and {acts} free-time activities in {days} days; the same world with the field scrambled: {}", if a == b { "identical" } else { "different" });
    assert!(acts > 0, "nobody did anything in their free time");
    assert_eq!(a, b, "with beauty off, scrambling the beauty field changed the world");
}

#[test]
fn people_take_up_hobbies_and_make_things() {
    if cfg!(debug_assertions) {
        return;
    }
    let mut named = std::collections::BTreeSet::new();
    let mut with_hobby = 0;
    let mut grown = 0;
    let mut made = 0;
    for seed in 1..=3u64 {
        let mut sim = Sim::new(seed, 256);
        let y = year(&sim);
        sim.run_ticks(4 * y);
        let c = &sim.content;
        for (_, p) in sim.people.iter().filter(|(_, p)| p.alive && p.grown(sim.tick, c)) {
            grown += 1;
            let h = p.hobby_acts();
            if !h.is_empty() {
                with_hobby += 1;
            }
            for a in h {
                named.insert(sim::leisure::hobby_name(c, a));
            }
        }
        made += sim.works.len() + sim.structures.iter().map(|(_, s)| s.decor as usize).sum::<usize>();
    }
    println!("M7 hobbies: {with_hobby} of {grown} grown people have a hobby, {} kinds: {named:?}; {made} things made", named.len());
    assert!(with_hobby * 3 >= grown, "only {with_hobby} of {grown} grown people have a hobby");
    assert!(named.len() >= 3, "hobbies: {named:?}");
    assert!(made >= 15, "only {made} things made in 3 colonies over 4 years");
}

#[test]
fn saving_a_world_with_hobbies_changes_nothing() {
    let mut a = Sim::new(6, 256);
    let days = if cfg!(debug_assertions) { 4 } else { 30 };
    a.run_ticks(day(&a) * days);
    let bytes = save::to_bytes(&a);
    let mut b = save::from_bytes(&bytes).expect("save loads");
    assert_eq!(a.state_hash(), b.state_hash(), "loaded world matches the saved one");
    assert_eq!(a.works, b.works, "what people made is saved");
    let d = day(&a);
    a.run_ticks(d);
    b.run_ticks(d);
    assert_eq!(a.state_hash(), b.state_hash(), "continuing after a load matches continuing without");
    assert_eq!(a.beauty, b.beauty, "the beauty field is worked out the same after a load");
}
