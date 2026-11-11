//! colony-run: the headless command-line runner.
//!
//!   colony-run map   --seed N [--size 256] [--out runs/maps]   PNG per layer + stats for one seed
//!   colony-run maps  --seeds 1..10 [--size 256] [--out ...]    contact sheet of many seeds
//!   colony-run bench [--size 256] [--seeds 1..5]               world generation time
//!   colony-run hash  --seed N [--days 2] [--size 256]          state hash after running N days
//!   colony-run life  --seeds 1..3 [--years 10] [--size 256] [--chronicle]
//!                                                             families over the years: people, births,
//!                                                             deaths, partners, skills, generations

mod render;

use sim::Sim;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

struct Args {
    cmd: String,
    opts: BTreeMap<String, String>,
}

impl Args {
    fn parse() -> Args {
        let mut it = std::env::args().skip(1);
        let cmd = it.next().unwrap_or_else(|| "help".into());
        let mut opts = BTreeMap::new();
        let rest: Vec<String> = it.collect();
        let mut i = 0;
        while i < rest.len() {
            let k = rest[i].trim_start_matches("--").to_string();
            let v = rest.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
            i += if v.is_some() { 2 } else { 1 };
            opts.insert(k, v.unwrap_or_else(|| "true".into()));
        }
        Args { cmd, opts }
    }

    fn num<T: std::str::FromStr>(&self, k: &str, default: T) -> T {
        self.opts.get(k).map(|v| v.parse().unwrap_or_else(|_| die(&format!("--{k} needs a number, got '{v}'")))).unwrap_or(default)
    }

    fn range(&self, k: &str, default: (u64, u64)) -> Vec<u64> {
        let (a, b) = match self.opts.get(k) {
            Some(v) => match v.split_once("..") {
                Some((a, b)) => (a.parse().unwrap_or_else(|_| die("bad range")), b.parse().unwrap_or_else(|_| die("bad range"))),
                None => {
                    let x = v.parse().unwrap_or_else(|_| die("bad seed"));
                    (x, x)
                }
            },
            None => default,
        };
        (a..=b).collect()
    }

    fn out(&self) -> PathBuf {
        PathBuf::from(self.opts.get("out").cloned().unwrap_or_else(|| "runs/maps".into()))
    }
}

fn die(msg: &str) -> ! {
    eprintln!("colony-run: {msg}");
    std::process::exit(2)
}

fn main() {
    let a = Args::parse();
    match a.cmd.as_str() {
        "map" => cmd_map(&a),
        "maps" => cmd_maps(&a),
        "bench" => cmd_bench(&a),
        "hash" => cmd_hash(&a),
        "find" => cmd_find(&a),
        "run" => cmd_run(&a),
        "water" => cmd_water(&a),
        "tiles" => cmd_tiles(&a),
        "life" => cmd_life(&a),
        _ => {
            println!("{}", HELP);
        }
    }
}

const HELP: &str = "colony-run map   --seed N [--size 256] [--out runs/maps]
colony-run maps  --seeds 1..10 [--size 256] [--out runs/maps]
colony-run bench [--size 256] [--seeds 1..5]
colony-run hash  --seed N [--days 2] [--size 256]
colony-run find  --seed N --kind wildflowers [--days N] nearest nodes of a kind to the start
colony-run water --seed N                              river mouths, lake inlets, confluences (for QA shots)
colony-run run   --seed N [--days 24] [--traces out.jsonl.zst] [--quiet]   live a colony headless";

fn cmd_map(a: &Args) {
    let seed = a.num("seed", 1u64);
    let size = a.num("size", 256u32);
    let sim = Sim::new(seed, size);
    let dir = a.out().join(format!("seed-{seed}"));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("can't make {}: {e}", dir.display())));
    for (name, img) in render::layers(&sim) {
        let path = dir.join(format!("{name}.png"));
        render::write_png(&path, &img);
    }
    print_stats(&sim);
    println!("wrote {}", dir.display());
}

fn cmd_maps(a: &Args) {
    let size = a.num("size", 256u32);
    let seeds = a.range("seeds", (1, 10));
    let mut tiles = Vec::new();
    for &s in &seeds {
        let sim = Sim::new(s, size);
        let r = sim.gen_report.as_ref().unwrap();
        println!("seed {s:>4}: {:.0} ms, start tier {}, {}", r.total_ms, r.start_tier, r.notes.join("; "));
        tiles.push((s, render::overview(&sim, 1)));
    }
    let sheet = render::contact_sheet(&tiles, 5);
    let dir = a.out();
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("can't make {}: {e}", dir.display())));
    let path = dir.join(format!("sheet-{}-{}.png", seeds[0], seeds[seeds.len() - 1]));
    render::write_png(&path, &sheet);
    println!("wrote {}", path.display());
}

fn cmd_bench(a: &Args) {
    let size = a.num("size", 256u32);
    let seeds = a.range("seeds", (1, 5));
    let mut worst = 0.0f64;
    for &s in &seeds {
        let t = Instant::now();
        let sim = Sim::new(s, size);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        worst = worst.max(ms);
        let r = sim.gen_report.as_ref().unwrap();
        let stages: Vec<String> = r.stages.iter().map(|(n, ms)| format!("{n} {ms:.0}")).collect();
        println!("seed {s:>4}: {ms:>6.0} ms  ({})", stages.join(", "));
    }
    println!("slowest: {worst:.0} ms for {size}x{size}");
}

fn cmd_hash(a: &Args) {
    let seed = a.num("seed", 1u64);
    let size = a.num("size", 256u32);
    let days = a.num("days", 2u64);
    let mut sim = Sim::new(seed, size);
    let t = Instant::now();
    sim.run_ticks(days * sim.content.calendar.ticks_per_day());
    println!("{:016x}  ({} after {days} days, {:.0} ms)", sim.state_hash(), sim.describe_time(), t.elapsed().as_secs_f64() * 1000.0);
}

#[derive(Default)]
struct RunStats {
    actions: BTreeMap<String, (u32, u32)>,
    why: BTreeMap<String, u32>,
    out: Option<zstd::stream::write::Encoder<'static, std::fs::File>>,
    written: u64,
}

fn cmd_run(a: &Args) {
    use std::sync::{Arc, Mutex};
    let seed = a.num("seed", 1u64);
    let days = a.num("days", 24u64);
    let quiet = a.opts.contains_key("quiet");
    let mut sim = Sim::new(seed, a.num("size", 256u32));
    sim.mode = "lockstep".into();
    let stats = Arc::new(Mutex::new(RunStats::default()));
    if let Some(path) = a.opts.get("traces") {
        let f = std::fs::File::create(path).unwrap_or_else(|e| die(&format!("can't write {path}: {e}")));
        stats.lock().unwrap().out = Some(zstd::stream::write::Encoder::new(f, 3).expect("zstd"));
    }
    let content = sim.content.clone();
    let st = stats.clone();
    sim.trace_sink = Some(Box::new(move |t: &sim::brain::Trace| {
        let mut s = st.lock().unwrap();
        let name = t.request.options[t.choice].kind.name(&content);
        let e = s.actions.entry(name.clone()).or_default();
        e.0 += 1;
        match t.outcome.as_ref().map(|o| &o.result) {
            Some(sim::brain::Ending::Done) | None => {}
            Some(sim::brain::Ending::Failed(w)) | Some(sim::brain::Ending::Interrupted(w)) => {
                e.1 += 1;
                let w = w.split_once(" was ").map(|(_, r)| format!("was {r}")).unwrap_or(w.clone());
                *s.why.entry(format!("{name}: {w}")).or_default() += 1;
            }
        }
        if let Some(out) = s.out.as_mut() {
            use std::io::Write;
            serde_json::to_writer(&mut *out, t).expect("trace json");
            out.write_all(b"\n").expect("trace write");
        }
        s.written += 1;
    }));
    let per_day = sim.content.calendar.ticks_per_day();
    let t0 = Instant::now();
    let mut deaths = Vec::new();
    for d in 0..days {
        sim.run_ticks(per_day);
        while sim.waiting() > 0 {
            sim.step();
        }
        for e in sim.take_events() {
            if let sim::Event::PersonDied { name, cause, person } = e {
                let age = sim.people.get(person).map_or(0.0, |p| p.age_years(sim.tick, &sim.content));
                deaths.push(format!("day {}: {name} ({age:.0}) died of {cause}", d + 1));
            }
        }
        if !quiet {
            let c = &sim.content;
            let st = |id: &str| sim.camp.stores[c.item(id)];
            let alive: Vec<&sim::Person> = sim.people.iter().map(|(_, p)| p).filter(|p| p.alive).collect();
            let avg = |n: &str| alive.iter().map(|p| p.needs[c.need(n)]).sum::<f32>() / alive.len().max(1) as f32;
            println!(
                "{:<34} alive {:>2}  berries {:>4} water {:>3} wood {:>3} sticks {:>3} stone {:>3}  fire {:<9} hunger {:.2} thirst {:.2} cold {:.2} tired {:.2}",
                sim.describe_time(),
                alive.len(),
                st("berries"),
                st("water"),
                st("wood"),
                st("sticks"),
                st("stone"),
                if sim.camp.fire_lit { format!("lit {:.0}h", sim.camp.fire_hours) } else { "out".into() },
                avg("hunger"),
                avg("thirst"),
                avg("warmth"),
                avg("rest"),
            );
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    if let Some(out) = stats.lock().unwrap().out.take() {
        out.finish().expect("zstd finish");
    }
    let s = stats.lock().unwrap();
    let alive = sim.alive();
    println!("seed {seed}: {alive} of {} alive after {days} days, {:.1} s", sim.people.len(), secs);
    for d in &deaths {
        println!("  {d}");
    }
    let day_minutes = days as f64 * 600.0;
    let worst = sim.people.iter().map(|(_, p)| (p.tally.day_sleep_minutes as f64 / day_minutes, p.name.clone())).max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((share, name)) = worst {
        println!("  most daytime sleep: {name}, {:.1}% of 08:00-18:00", share * 100.0);
    }
    let mix: Vec<String> = s.actions.iter().map(|(k, (n, f))| if *f > 0 { format!("{k} {n} ({f} not done)") } else { format!("{k} {n}") }).collect();
    println!("  decisions: {}", mix.join(", "));
    let mut why: Vec<(&String, &u32)> = s.why.iter().collect();
    why.sort_by(|a, b| b.1.cmp(a.1));
    let top: Vec<String> = why.iter().take(8).map(|(k, n)| format!("{k} ({n})")).collect();
    if !top.is_empty() {
        println!("  not done because: {}", top.join("; "));
    }
    if s.written > 0 && a.opts.contains_key("traces") {
        println!("  wrote {} traces to {}", s.written, a.opts["traces"]);
    }
}

fn cmd_water(a: &Args) {
    use sim::Water;
    let sim = Sim::new(a.num("seed", 1u64), a.num("size", 256u32));
    let m = &sim.map;
    let n = m.size as i32;
    let mut mouths = Vec::new();
    let mut lakes = Vec::new();
    let mut joins = Vec::new();
    for y in 1..n - 1 {
        for x in 1..n - 1 {
            let i = m.idx(x as u32, y as u32);
            if m.water[i] != Water::River {
                continue;
            }
            let Some(j) = m.downstream(i) else { continue };
            match m.water[j] {
                Water::Sea => mouths.push((x, y)),
                Water::Lake => lakes.push((x, y)),
                _ => {}
            }
            // Confluence: two or more river tiles that aren't neighbours of each other flow in here.
            let ins: Vec<usize> = (0..8)
                .filter_map(|d| {
                    let (dx, dy) = sim::world::DIRS[d];
                    let k = m.idx((x + dx) as u32, (y + dy) as u32);
                    (m.water[k] == Water::River && m.downstream(k) == Some(i)).then_some(k)
                })
                .collect();
            if ins.len() >= 2 && m.flow[ins[0]].min(m.flow[ins[1]]) > m.flow[i] * 0.2 {
                joins.push((x, y));
            }
        }
    }
    let show = |name: &str, v: &[(i32, i32)]| {
        let mut picked: Vec<(i32, i32)> = Vec::new();
        for &p in v {
            if picked.iter().all(|q| (q.0 - p.0).abs() + (q.1 - p.1).abs() > 25) {
                picked.push(p);
            }
        }
        let list: Vec<String> = picked.iter().take(4).map(|(x, y)| format!("{x},{y}")).collect();
        println!("{name}: {}", if list.is_empty() { "none".into() } else { list.join("  ") });
    };
    show("river mouths", &mouths);
    show("lake inlets", &lakes);
    show("confluences", &joins);
}

/// Print the tiles around a spot: water kind, ground height, water level, flow direction.
fn cmd_tiles(a: &Args) {
    use sim::Water;
    let sim = Sim::new(a.num("seed", 1u64), a.num("size", 256u32));
    let m = &sim.map;
    let (cx, cy) = (a.num("x", 128i32), a.num("y", 128i32));
    let r = a.num("r", 4i32);
    for y in cy - r..=cy + r {
        let mut line = format!("{y:>4} ");
        for x in cx - r..=cx + r {
            if !m.in_bounds(x, y) {
                continue;
            }
            let i = m.idx(x as u32, y as u32);
            let k = match m.water[i] {
                Water::None => '.',
                Water::Sea => 'S',
                Water::Lake => 'L',
                Water::River => 'R',
            };
            let lvl = if m.water[i].is_water() { format!("{:+.1}", m.water_level[i]) } else { "    ".into() };
            line += &format!("{k}{:+5.1}{lvl:>5} ", m.elevation[i]);
        }
        println!("{line}");
    }
}

fn cmd_find(a: &Args) {
    let mut sim = Sim::new(a.num("seed", 1u64), a.num("size", 256u32));
    // --days N: look after the colony has lived a while (graves appear with time).
    let days: u64 = a.num("days", 0);
    sim.run_ticks(days * sim.content.calendar.ticks_per_day());
    let kind = a.opts.get("kind").map(|s| s.as_str()).unwrap_or("wildflowers");
    let k = sim.content.node_kind(kind);
    let (sx, sy) = sim.map.start;
    let mut hits: Vec<(i64, u16, u16)> = sim
        .nodes
        .iter()
        .filter(|(_, n)| n.kind == k)
        .map(|(_, n)| ((n.x as i64 - sx as i64).pow(2) + (n.y as i64 - sy as i64).pow(2), n.x, n.y))
        .collect();
    hits.sort();
    for (d2, x, y) in hits.iter().take(5) {
        println!("{kind} at ({x}, {y}), {:.0} tiles from the start", (*d2 as f64).sqrt());
    }
}

fn print_stats(sim: &Sim) {
    let m = &sim.map;
    let c = &sim.content;
    let n = m.water.len() as f32;
    let r = sim.gen_report.as_ref().unwrap();
    println!("seed {}  {}x{}  generated in {:.0} ms", sim.seed, m.size, m.size, r.total_ms);
    for (name, ms) in &r.stages {
        print!("  {name} {ms:.0}");
    }
    println!();
    for note in &r.notes {
        println!("  {note}");
    }
    let mut counts = vec![0u32; c.biomes.len()];
    for &b in &m.biome {
        counts[b as usize] += 1;
    }
    let biomes: Vec<String> = c.biomes.iter().zip(&counts).filter(|(_, n)| **n > 0).map(|(b, k)| format!("{} {:.1}%", b.id, *k as f32 / n * 100.0)).collect();
    println!("  biomes: {}", biomes.join(", "));
    let mut nodes = vec![0u32; c.nodes.len()];
    for (_, nd) in sim.nodes.iter() {
        nodes[nd.kind as usize] += 1;
    }
    let ns: Vec<String> = c.nodes.iter().zip(&nodes).map(|(d, k)| format!("{} {k}", d.id)).collect();
    println!("  nodes: {}", ns.join(", "));
    let (sx, sy) = m.start;
    let si = m.idx(sx, sy);
    println!(
        "  start: ({sx}, {sy}) tier {}, {} at {:.1} m, fertility {:.2}",
        r.start_tier,
        c.biomes[m.biome[si] as usize].name,
        m.elevation[si],
        m.fertility[si]
    );
}

/// Run colonies for years and report on their families (M5).
fn cmd_life(a: &Args) {
    let years: u64 = a.num("years", 10);
    let size = a.num("size", 256u32);
    for seed in a.range("seeds", (1, 3)) {
        let mut sim = Sim::new(seed, size);
        let c = sim.content.clone();
        let year = c.calendar.ticks_per_day() * c.calendar.days_per_year();
        let t = Instant::now();
        let mut causes: BTreeMap<String, u32> = BTreeMap::new();
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for y in 1..=years {
            for _ in 0..year {
                sim.step();
            }
            for e in sim.take_events() {
                let k = match &e {
                    sim::Event::Born { .. } => "births",
                    sim::Event::PersonDied { cause, .. } => {
                        *causes.entry(cause.clone()).or_default() += 1;
                        "deaths"
                    }
                    sim::Event::Partnered { .. } => "partnerships",
                    sim::Event::Argued { .. } => "arguments",
                    sim::Event::Learned { .. } => "learned",
                    sim::Event::SkillLost { .. } => "skills_lost",
                    sim::Event::Buried { .. } => "burials",
                    sim::Event::Shared { .. } => "shared",
                    sim::Event::Expecting { .. } => "conceived",
                    _ => continue,
                };
                *counts.entry(k).or_default() += 1;
            }
            let alive: Vec<&sim::Person> = sim.people.iter().map(|(_, p)| p).filter(|p| p.alive).collect();
            let generation = alive.iter().map(|p| p.family.generation).max().unwrap_or(0);
            let knowers: Vec<String> = c
                .skills
                .skills
                .iter()
                .enumerate()
                .filter(|(_, s)| s.taught)
                .map(|(k, s)| format!("{} {}", s.id, alive.iter().filter(|p| p.skill_level(k, &c) >= 1).count()))
                .collect();
            let kids = alive.iter().filter(|p| !p.grown(sim.tick, &c)).count();
            let pairs = alive.iter().filter(|p| p.family.partner.is_some()).count() / 2;
            println!(
                "seed {seed} year {y:>3}: {:>3} alive ({kids} young, {pairs} couples, gen {generation}), stores {} berries; {}",
                alive.len(),
                sim.camp.stores[c.item("berries")],
                knowers.join(", ")
            );
            if alive.is_empty() {
                break;
            }
        }
        println!("seed {seed}: {:.1} s; {:?}; deaths by {:?}", t.elapsed().as_secs_f64(), counts, causes);
        if a.opts.contains_key("chronicle") {
            for e in &sim.chronicle.entries {
                if e.importance >= 0.3 {
                    println!("  {:<24} {}", c.calendar.describe(e.tick), e.text);
                }
            }
        }
    }
}
