//! M5 "done when" checks (docs/ROADMAP.md): children's traits follow inheritance over five
//! generations, a skill dies with its last holder and the chronicle says so, family records stay
//! consistent, and saving and loading a world with families changes nothing.

use sim::people::social::Stage;
use sim::people::{Person, Sex};
use sim::{Event, PersonId, Sim, save};

fn year(sim: &Sim) -> u64 {
    sim.content.calendar.ticks_per_day() * sim.content.calendar.days_per_year()
}

fn day(sim: &Sim) -> u64 {
    sim.content.calendar.ticks_per_day()
}

/// Least-squares slope of y on x, its standard error, and the correlation.
fn regress(xy: &[(f64, f64)]) -> (f64, f64, f64) {
    let n = xy.len() as f64;
    let (mx, my) = (xy.iter().map(|p| p.0).sum::<f64>() / n, xy.iter().map(|p| p.1).sum::<f64>() / n);
    let sxy: f64 = xy.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let sxx: f64 = xy.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let syy: f64 = xy.iter().map(|p| (p.1 - my).powi(2)).sum();
    let slope = sxy / sxx;
    let resid: f64 = xy.iter().map(|p| (p.1 - my - slope * (p.0 - mx)).powi(2)).sum();
    let se = (resid / (n - 2.0) / sxx).sqrt();
    (slope, se, sxy / (sxx * syy).sqrt())
}

/// One long run checks both measured M5 checks: five generations of inheritance, and a skill
/// lost when its last holder dies (written in the chronicle).
#[test]
fn five_generations_inherit_and_a_skill_is_lost() {
    if cfg!(debug_assertions) {
        return; // decades of a colony: run with cargo test --release
    }
    let mut sim = Sim::new(2, 256);
    let y = year(&sim);
    let mut lost: Vec<(String, String)> = Vec::new();
    let mut generations = 0;
    for _ in 0..40 {
        sim.run_ticks(y);
        for e in sim.take_events() {
            if let Event::SkillLost { skill, last } = e {
                lost.push((skill, last));
            }
        }
        generations = sim.people.iter().map(|(_, p)| p.family.generation).max().unwrap_or(0);
        assert!(sim.alive() > 0, "the colony died out");
        // Enough children for the statistics below to mean something.
        let born = sim.people.iter().filter(|(_, p)| p.family.mother.is_some()).count();
        if generations >= 5 && !lost.is_empty() && born >= 40 {
            break;
        }
    }
    assert!(generations >= 5, "only {generations} generations in 40 years");
    let c = &sim.content;
    let g = &c.genes;

    // 1. Every child's alleles came from their parents: one from the mother, one from the father
    //    (mutation flips about 1 in 1000).
    let people: Vec<(PersonId, &Person)> = sim.people.iter().collect();
    let get = |id: Option<PersonId>| id.and_then(|i| sim.people.get(i));
    let mut children = 0;
    let mut mismatches = 0;
    let mut alleles = 0;
    // For heterozygous parents: how often the child got the "1" allele (should be half).
    let (mut het, mut het_ones) = (0u32, 0u32);
    for (_, p) in &people {
        let (Some(m), Some(f)) = (get(p.family.mother), get(p.family.father)) else { continue };
        children += 1;
        for k in 0..p.genome.loci.len() {
            alleles += 2;
            mismatches += !m.genome.loci[k].contains(&p.genome.loci[k][0]) as u32 + !f.genome.loci[k].contains(&p.genome.loci[k][1]) as u32;
            for (parent, got) in [(m, p.genome.loci[k][0]), (f, p.genome.loci[k][1])] {
                if parent.genome.loci[k][0] != parent.genome.loci[k][1] {
                    het += 1;
                    het_ones += got as u32;
                }
            }
        }
    }
    assert!(children >= 40, "only {children} children born");
    let expected = alleles as f64 * 0.001;
    assert!((mismatches as f64) <= expected + 4.0 * expected.sqrt() + 2.0, "{mismatches} of {alleles} alleles didn't come from a parent (expect about {expected:.0} from mutation)");
    let share = het_ones as f64 / het as f64;
    let z = (share - 0.5) / (0.25 / het as f64).sqrt();
    assert!(z.abs() < 4.0, "heterozygous parents passed on the '1' allele {:.1}% of the time over {het} (z = {z:.1})", share * 100.0);

    // 2. Height follows the parents. A child's genetic height is on average their parents'
    //    average (slope 1). Their height (set at birth, reached when grown) tracks their parents'
    //    with slope h2 = genetic variance / all variance (the rest is upbringing and chance).
    //    Both are checked within 3 standard errors.
    let base = |s: Sex| if s == Sex::Female { g.base_height.0 } else { g.base_height.1 };
    let mut genetic = Vec::new();
    let mut grown = Vec::new();
    for (_, p) in &people {
        let (Some(m), Some(f)) = (get(p.family.mother), get(p.family.father)) else { continue };
        let score = |q: &Person| q.genome.score(g, "height") as f64;
        genetic.push(((score(m) + score(f)) / 2.0, score(p)));
        // Height above the sex's base, so men and women compare.
        let over = |q: &Person| (q.looks.height - base(q.sex)) as f64;
        grown.push(((over(m) + over(f)) / 2.0, over(p)));
    }
    let (gslope, gse, gr) = regress(&genetic);
    assert!((gslope - 1.0).abs() < 3.0 * gse, "genetic height: slope {gslope:.2} ± {gse:.2} (expect 1), r {gr:.2} over {} children", genetic.len());
    let height_loci: Vec<_> = g.loci.iter().filter(|l| l.trait_id == "height").collect();
    let var_g: f64 = height_loci.iter().map(|l| 2.0 * (l.effect as f64).powi(2) * l.freq as f64 * (1.0 - l.freq as f64)).sum();
    let var_e = (2.0 * g.height_noise as f64).powi(2) / 12.0;
    let h2 = var_g / (var_g + var_e);
    let (slope, se, r) = regress(&grown);
    assert!((slope - h2).abs() < 3.0 * se, "height: slope {slope:.2} ± {se:.2} (expect h2 {h2:.2}), r {r:.2} over {} children", grown.len());
    println!(
        "M5 inheritance: {generations} generations, {children} children; {mismatches} of {alleles} alleles from mutation (expect {expected:.0}); \
         heterozygous parents passed '1' {:.1}% of {het} times (z {z:.2}); genetic height slope {gslope:.2} ± {gse:.2} r {gr:.2} (n {}); \
         height slope {slope:.2} ± {se:.2} (h2 {h2:.2}) r {r:.2} (n {})",
        share * 100.0,
        genetic.len(),
        grown.len()
    );

    // 3. A recessive trait: two blue-eyed parents have blue-eyed children.
    let blue: Vec<&Person> = people
        .iter()
        .map(|(_, p)| *p)
        .filter(|p| get(p.family.mother).is_some_and(|m| m.looks.eye_word == "blue") && get(p.family.father).is_some_and(|f| f.looks.eye_word == "blue"))
        .collect();
    let blue_kids = blue.iter().filter(|p| p.looks.eye_word == "blue").count();
    assert!(blue_kids * 100 >= blue.len() * 90, "{blue_kids} of {} children of two blue-eyed parents have blue eyes", blue.len());

    // 4. A skill died with its last holder: nobody living knows it, and the chronicle says so.
    assert!(!lost.is_empty(), "no skill was lost in {generations} generations");
    println!("M5 skills lost: {lost:?}; blue-eyed parents: {blue_kids} of {} children blue-eyed", blue.len());
    for (skill, last) in &lost {
        let k = c.skills.skills.iter().position(|s| s.name.to_lowercase() == *skill).unwrap();
        assert!(sim.people.iter().all(|(_, p)| !p.alive || p.skill_level(k, c) == 0), "someone living still knows {skill}");
        assert!(
            sim.chronicle.entries.iter().any(|e| e.text.contains(skill.as_str()) && e.text.contains(last.as_str()) && e.text.contains("lost")),
            "the chronicle doesn't record {skill} being lost with {last}"
        );
        let holder = sim.people.iter().find(|(_, p)| p.name == *last && !p.alive).map(|(_, p)| p);
        assert!(holder.is_some_and(|h| h.skill_level(k, c) >= 1), "{last} was the last holder of {skill} but didn't know it");
    }
}

#[test]
fn family_records_hold_together() {
    let mut sim = Sim::new(3, 256);
    let days = if cfg!(debug_assertions) { 7 } else { 24 * 4 };
    sim.run_ticks(day(&sim) * days);
    let c = &sim.content;
    let mut babies = 0;
    let all = sim::family::seen(&sim.people, sim.tick, c);
    for (id, p) in sim.people.iter() {
        // The living have living partners (the dead keep theirs on record, as "late partner").
        if let Some(q) = p.family.partner
            && p.alive
        {
            let other = sim.people.get(q).unwrap();
            assert!(other.alive, "{} has a dead partner on record", p.name);
            assert_eq!(other.family.partner, Some(id), "{} and {} aren't each other's partners", p.name, other.name);
            assert!(!sim::family::close_kin(&all, id, q), "{} and {} are close kin", p.name, other.name);
        }
        for parent in [p.family.mother, p.family.father].into_iter().flatten() {
            let pa = sim.people.get(parent).unwrap();
            assert!(pa.family.children.contains(&id), "{} isn't listed among {}'s children", p.name, pa.name);
            assert!(pa.family.generation < p.family.generation, "{} isn't a generation after {}", p.name, pa.name);
            assert!(pa.born < p.born, "{} was born before their parent {}", p.name, pa.name);
        }
        if let (Some(m), Some(f)) = (p.family.mother, p.family.father) {
            let (m, f) = (sim.people.get(m).unwrap(), sim.people.get(f).unwrap());
            assert_eq!(p.family.generation, m.family.generation.max(f.family.generation) + 1);
            assert!(m.sex == Sex::Female && f.sex == Sex::Male);
        }
        // Babies are kept close to whoever looks after them.
        if p.alive && p.stage(sim.tick, c) == Stage::Baby {
            babies += 1;
            let me = sim::family::find(&all, id).unwrap();
            let carer = sim::family::carer(&all, me).and_then(|cid| sim.people.get(cid)).expect("every baby has a carer");
            let d = ((carer.pos.0 - p.pos.0).powi(2) + (carer.pos.1 - p.pos.1).powi(2)).sqrt();
            assert!(d < 3.0, "{} is {d:.1} tiles from {}", p.name, carer.name);
        }
        if p.alive {
            assert!(p.memories.len() <= c.life.keep_memories);
        }
    }
    if !cfg!(debug_assertions) {
        assert!(babies > 0 || sim.people.iter().filter(|(_, p)| p.family.mother.is_some()).count() > 0, "no children in four years");
    }
}

#[test]
fn saving_a_world_with_families_changes_nothing() {
    let mut a = Sim::new(4, 256);
    // Late enough for partners, pregnancies, babies and invitations.
    let days = if cfg!(debug_assertions) { 6 } else { 30 };
    a.run_ticks(day(&a) * days);
    let bytes = save::to_bytes(&a);
    let mut b = save::from_bytes(&bytes).expect("save loads");
    assert_eq!(a.state_hash(), b.state_hash(), "loaded world matches the saved one");
    assert_eq!(a.chronicle, b.chronicle, "the chronicle is saved");
    let d = day(&a);
    a.run_ticks(d);
    b.run_ticks(d);
    assert_eq!(a.state_hash(), b.state_hash(), "continuing after a load matches continuing without");
}
