//! M1 "done when" checks (docs/ROADMAP.md): determinism, saves, rivers, start site, speed.

use sim::save;
use sim::world::worldgen::{Stage, WorldGen};
use sim::{Command, Sim, Water};

const SIZE: u32 = 256;

fn day(sim: &Sim) -> u64 {
    sim.content.calendar.ticks_per_day()
}

/// A fixed script of commands, so two runs can be compared.
fn script(sim: &mut Sim, step: u64) {
    let (sx, sy) = sim.map.start;
    let rock = sim.content.node_kind("rock");
    match step % 3 {
        0 => sim.command(Command::PlaceNode { kind: rock, x: sx + 6 + (step % 5) as u32, y: sy }),
        1 => sim.command(Command::RemoveNode { x: sx + 6 + ((step + 2) % 5) as u32, y: sy }),
        _ => {}
    }
}

fn run_days(sim: &mut Sim, days: u64) {
    let per = day(sim);
    for d in 0..days {
        for h in 0..24 {
            script(sim, d * 24 + h);
            sim.run_ticks(per / 24);
        }
    }
}

#[test]
fn same_seed_same_world() {
    let a = Sim::new(7, SIZE);
    let b = Sim::new(7, SIZE);
    let c = Sim::new(8, SIZE);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_ne!(a.state_hash(), c.state_hash());
}

#[test]
fn same_commands_same_world_after_running() {
    let mut a = Sim::new(3, SIZE);
    let mut b = Sim::new(3, SIZE);
    run_days(&mut a, 2);
    run_days(&mut b, 2);
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(a.command_log.len() > 20, "the script should have applied commands");
}

#[test]
fn save_load_and_continue_matches() {
    let mut a = Sim::new(5, SIZE);
    run_days(&mut a, 1);
    let bytes = save::to_bytes(&a);
    let mut b = save::from_bytes(&bytes).expect("save loads");
    assert_eq!(a.state_hash(), b.state_hash(), "loaded world matches the saved one");
    run_days(&mut a, 1);
    run_days(&mut b, 1);
    assert_eq!(a.state_hash(), b.state_hash(), "continuing after a load matches continuing without");
}

#[test]
fn background_save_writes_a_loadable_file() {
    let sim = Sim::new(9, SIZE);
    let dir = std::env::temp_dir().join(format!("colony-test-{}", std::process::id()));
    let path = dir.join("world.save");
    save::save_background(&sim, &path).wait().expect("save writes");
    let back = save::load_from(&path).expect("file loads");
    assert_eq!(sim.state_hash(), back.state_hash());
    assert!(!path.with_extension("tmp").exists(), "no temp file left behind");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_river_reaches_a_lake_or_the_sea() {
    for seed in 1..=10 {
        let sim = Sim::new(seed, SIZE);
        let m = &sim.map;
        let nn = m.water.len();
        let rivers: Vec<usize> = (0..nn).filter(|&i| m.water[i] == Water::River).collect();
        assert!(rivers.len() > SIZE as usize, "seed {seed}: expected some rivers, got {}", rivers.len());
        for &start in &rivers {
            let mut i = start;
            let mut steps = 0;
            while !matches!(m.water[i], Water::Lake | Water::Sea) {
                i = m.downstream(i).unwrap_or_else(|| panic!("seed {seed}: water from {:?} stops at {:?}", m.xy(start), m.xy(i)));
                steps += 1;
                assert!(steps < nn, "seed {seed}: water from {:?} goes round in circles", m.xy(start));
            }
        }
    }
}

#[test]
fn start_site_has_water_food_and_wood_in_reach() {
    for seed in 1..=10 {
        let sim = Sim::new(seed, SIZE);
        let m = &sim.map;
        let (sx, sy) = m.start;
        assert!(!m.water[m.idx(sx, sy)].is_water(), "seed {seed}: start is in water");
        let berry = sim.content.node_kind("berry_bush");
        let wood = [sim.content.node_kind("tree"), sim.content.node_kind("pine")];
        let (mut fresh, mut berries, mut trees) = (false, 0, 0);
        let r = 40i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (sx as i32 + dx, sy as i32 + dy);
                if !m.in_bounds(x, y) || dx * dx + dy * dy > r * r {
                    continue;
                }
                let i = m.idx(x as u32, y as u32);
                fresh |= m.water[i].is_fresh();
                if let Some(id) = m.node_at[i].checked_sub(1).and_then(|s| sim.nodes.id_at(s)) {
                    let k = sim.nodes.get(id).unwrap().kind;
                    berries += (k == berry) as u32;
                    trees += wood.contains(&k) as u32;
                }
            }
        }
        let tier = sim.gen_report.as_ref().unwrap().start_tier;
        assert!(fresh, "seed {seed}: no fresh water within {r} tiles of the start");
        assert!(berries >= 3, "seed {seed}: only {berries} berry bushes near the start");
        assert!(trees >= 6, "seed {seed}: only {trees} trees near the start");
        assert!(tier <= 2, "seed {seed}: start site fell back to tier {tier}");
    }
}

#[test]
fn generation_is_fast() {
    if cfg!(debug_assertions) {
        return; // measured in release: cargo test --release
    }
    for seed in 1..=5 {
        let t = std::time::Instant::now();
        let _ = Sim::new(seed, SIZE);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert!(ms < 2000.0, "seed {seed}: {SIZE}x{SIZE} took {ms:.0} ms (budget 2000)");
    }
}

#[test]
fn packs_can_add_generation_stages() {
    fn mark(ctx: &mut sim::world::worldgen::GenCtx) {
        ctx.report.notes.push("pack stage ran".into());
    }
    let mut g = WorldGen::default();
    g.insert_after("hydrology", Stage { name: "test_pack", run: mark });
    let sim = Sim::with_worldgen(1, 128, &g);
    let r = sim.gen_report.unwrap();
    assert!(r.notes.iter().any(|n| n == "pack stage ran"));
    let names: Vec<&str> = r.stages.iter().map(|(n, _)| n.as_str()).collect();
    let h = names.iter().position(|n| *n == "hydrology").unwrap();
    assert_eq!(names[h + 1], "test_pack");
}
