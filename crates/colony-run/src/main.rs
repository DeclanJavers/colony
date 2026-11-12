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
        "bridge" => cmd_bridge(&a),
        "train" => cmd_train(&a),
        "arena" => cmd_arena(&a),
        "distill" => cmd_distill(&a),
        "text" => cmd_text(&a),
        "label" => cmd_label(&a),
        "speed" => cmd_speed(&a),
        "fork" => cmd_fork(&a),
        "export" => cmd_export(&a),
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
colony-run run   --seed N [--days 24] [--traces out.jsonl.zst] [--quiet]   live a colony headless
colony-run bridge --seed N [--days 3] [--dir runs/bridge-N] [--resume] [--window 15] [--batch 24]
                                                       a colony decided by Claude Haiku through the
                                                       subagent bridge (answer with tools/haiku_driver)
colony-run train --traces a.jsonl[,b.jsonl.zst...] | --examples dataset/features.jsonl [--teacher rules|subagent] [--epochs 30] [--out brain/models/scorer@v1.json]
                                                       train the scorer on a teacher's decisions
colony-run arena --seeds 1..5 [--days 48] --brains rules,scorer@v1 [--beauty-off] [--out runs/arena]
                                                       the same worlds with different brains, compared
colony-run label [--seeds 1..12] [--days 48] [--per 420] [--brain rules] [--shards 3] [--dir runs/label-1]
                 [--only act,act] [--some act --some-share 0.25] [--no-driver]
                                                       Claude Haiku labels decisions sampled from
                                                       colonies another brain runs (a dataset for
                                                       the text model and Laya; uses your plan)
colony-run speed --brain text-NAME [--seed 13] [--days 3] [--n 300]
                                                       how long the text model takes per decision,
                                                       on decisions from a rules colony
colony-run text --seed N [--day 2] [--person K]
                                                       the words a text brain sees for someone's
                                                       latest decision (Haiku, the text model)
colony-run distill [--seeds 1..12] [--days 12] [--rounds 3] [--keep 0.5] [--out brain/models/scorer@rules-v2.json]
                                                       distil the rules brain into a scorer, with rounds
                                                       of its own play relabelled by the rules brain
colony-run export --traces a.jsonl[,...] [--teacher subagent] --out brain/data/name
                                                       datasets from traces: features.jsonl (for the
                                                       scorer) and text.jsonl (for a text model),
                                                       each row marked train or held-out by seed
colony-run fork  --load a.colony --out b.colony [--branch 1] [--brain rules]
                                                       branch a saved world: its chance events go
                                                       their own way from here";

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
                    sim::Event::FieldSown { .. } => "sown",
                    sim::Event::Harvested { .. } => "harvests",
                    sim::Event::CropFailed { .. } => "crops_failed",
                    sim::Event::Hunted { .. } => "hunts",
                    sim::Event::SpearMade { .. } => "spears",
                    sim::Event::BuildingDone { .. } => "built",
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
            let built = |id: &str| sim.structures.iter().filter(|(_, s)| s.kind == c.building(id) && s.done()).count();
            let sites = sim.structures.iter().filter(|(_, s)| !s.done()).count();
            let harvested: u32 = sim.structures.iter().map(|(_, s)| s.harvested).sum();
            let beasts: Vec<String> = c.animals.iter().enumerate().map(|(k, a)| format!("{} {}", a.id, sim.animals.iter().filter(|(_, x)| x.kind == k as u16).count())).collect();
            let season = (c.calendar.day_index(sim.tick) / c.calendar.days_per_season as u64) as usize;
            let pace = sim.stats.pace(season.saturating_sub(1)).map_or("-".into(), |p| format!("{p:.1}"));
            let saving = sim.stats.trail_saving(season.saturating_sub(1)).map_or("-".into(), |s| format!("{:.0}%", s * 100.0));
            // Free time (M7): hobbies by name, and what people made.
            let mut hobbies: std::collections::BTreeMap<&str, u32> = Default::default();
            for p in &alive {
                for h in p.hobby_acts() {
                    *hobbies.entry(sim::leisure::hobby_name(&c, h)).or_default() += 1;
                }
            }
            let kind_count = |id: &str| { let k = c.node_kind(id); sim.nodes.iter().filter(|(_, n)| n.kind == k).count() };
            let decor: u32 = sim.structures.iter().map(|(_, s)| s.decor as u32).sum();
            let lovely = alive.iter().filter(|p| sim.beauty_for(p.pos) > 0.2).count();
            println!(
                "seed {seed} year {y:>3}: {:>3} alive ({kids} young, {pairs} couples, gen {generation}), food {:.0} days; {}; stone {}, spears {}; homes {}h {}s, store {}, fields {} (+{sites} sites), grain harvested {harvested}; {} (hunted {:?}); pace {pace} min/100m, trails save {saving}; hobbies {hobbies:?}; beds {} statues {} decor {decor}; {lovely} somewhere lovely",
                alive.len(),
                sim::actions::food_in(&c, &sim.camp.stores) / (alive.len().max(1) as f32 * sim::actions::daily_hunger(&c)),
                knowers.join(", "),
                sim.camp.stores[c.item("stone")],
                sim.camp.stores[c.item("spear")],
                built("house"),
                built("shelter"),
                built("storehouse"),
                built("field"),
                beasts.join(" "),
                sim.wildlife.hunted,
                kind_count("flower_bed"),
                kind_count("carving"),
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

/// A colony whose people are decided by the subagent bridge (ARCHITECTURE.md section 8), in
/// lockstep: whenever a batch of decisions is out, time waits for the answers. Answers come from
/// `tools/haiku_driver` (or a Claude Code session answering the files). If the driver stops at a
/// usage limit, the world is saved and `--resume` carries on from it.
fn cmd_bridge(a: &Args) {
    use std::io::Write;
    let seed = a.num("seed", 1u64);
    let days = a.num("days", 3u64);
    let dir = PathBuf::from(a.opts.get("dir").cloned().unwrap_or_else(|| format!("runs/bridge-{seed}")));
    let save = dir.join("world.colony");
    let mut sim = if a.opts.contains_key("resume") {
        sim::save::load_from(&save).unwrap_or_else(|e| die(&format!("can't resume from {}: {e}", save.display())))
    } else {
        let started = std::fs::read_dir(dir.join("bridge")).map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().starts_with("req-"))).unwrap_or(false);
        if save.exists() || started {
            die(&format!("{} already has a run in it: use --resume, or another --dir", dir.display()));
        }
        Sim::new(seed, a.num("size", 256u32))
    };
    sim.mode = "lockstep".into();
    let mut bridge = brains::SubagentBrain::new(&dir.join("bridge")).unwrap_or_else(|e| die(&format!("can't make {}: {e}", dir.display())));
    bridge.window = a.num("window", 15u32);
    bridge.max_items = a.num("batch", 24usize);
    sim.brains.map.insert("subagent".into(), Box::new(bridge));
    let traces = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("traces.jsonl")).unwrap_or_else(|e| die(&format!("can't write traces: {e}")));
    let traces = std::sync::Arc::new(std::sync::Mutex::new((std::io::BufWriter::new(traces), 0u64, 0u64)));
    let tr = traces.clone();
    sim.trace_sink = Some(Box::new(move |t: &sim::brain::Trace| {
        let mut g = tr.lock().unwrap();
        serde_json::to_writer(&mut g.0, t).expect("trace json");
        g.0.write_all(b"\n").expect("trace write");
        g.1 += 1;
        if t.fallback {
            g.2 += 1;
        }
    }));
    let per_day = sim.content.calendar.ticks_per_day();
    // --hours for a short run from now; else whole days.
    let target = match a.opts.get("hours") {
        Some(_) => sim.tick + a.num("hours", 1u64) * 60 / sim.content.calendar.minutes_per_tick as u64,
        None => (sim.tick / per_day + days) * per_day,
    };
    println!("bridge run: seed {seed}, until {} (lockstep); batches in {}", sim.content.calendar.describe(target), dir.join("bridge").display());
    println!("  answer them with: tools/haiku_driver {}", dir.join("bridge").display());
    let t0 = Instant::now();
    let mut waited_since: Option<Instant> = None;
    let mut last_report = sim.tick;
    while sim.tick < target {
        if let Some((brain, why)) = sim.brains_stopped() {
            sim::save::save_to(&sim, &save).unwrap_or_else(|e| die(&format!("can't save: {e}")));
            traces.lock().unwrap().0.flush().ok();
            println!("{brain} stopped: {why}. Saved {} at {}; carry on later with --resume.", save.display(), sim.describe_time());
            std::process::exit(3);
        }
        // Everyone (alive and grown enough to decide) goes to the bridge, newborns included.
        let ids: Vec<sim::PersonId> = sim.people.iter().filter(|(_, p)| p.alive && p.brain != "subagent").map(|(id, _)| id).collect();
        for id in ids {
            sim.set_brain(id, "subagent");
        }
        if sim.brains_in_flight() > 0 {
            // The sim pauses while Haiku decides.
            let since = *waited_since.get_or_insert_with(Instant::now);
            std::thread::sleep(std::time::Duration::from_millis(200));
            sim.poll_brains();
            if since.elapsed().as_secs() > 0 && since.elapsed().as_secs().is_multiple_of(60) && since.elapsed().subsec_millis() < 200 {
                println!("  ... still waiting for {} decisions ({} s)", sim.brains_in_flight(), since.elapsed().as_secs());
            }
            continue;
        }
        waited_since = None;
        // With everyone who decides waiting, there's no point holding the batch back.
        let deciders = sim.people.iter().filter(|(_, p)| p.alive && p.stage(sim.tick, &sim.content) != sim::people::social::Stage::Baby).count();
        if sim.brains_queued() > 0 && sim.waiting() >= deciders.max(1) {
            sim.flush_brains();
            continue;
        }
        sim.step();
        if sim.tick >= last_report + 120 {
            last_report = sim.tick;
            let g = traces.lock().unwrap();
            println!("{:<34} alive {:>2}  decisions {:>5} ({} fell back to rules)  {:.0} s", sim.describe_time(), sim.alive(), g.1, g.2, t0.elapsed().as_secs_f64());
        }
    }
    sim::save::save_to(&sim, &save).unwrap_or_else(|e| die(&format!("can't save: {e}")));
    let mut g = traces.lock().unwrap();
    g.0.flush().ok();
    let secs = t0.elapsed().as_secs_f64();
    println!("done: {} decisions traced ({} fell back to rules), {} alive, {:.0} s real time; saved {}", g.1, g.2, sim.alive(), secs, save.display());
}

/// Every decision trace in some files (`.jsonl`, or `.jsonl.zst` as `colony-run run --traces` writes).
fn read_traces(paths: &str) -> Vec<sim::brain::Trace> {
    use std::io::BufRead;
    let mut out = Vec::new();
    for path in paths.split(',').filter(|p| !p.is_empty()) {
        let f = std::fs::File::open(path).unwrap_or_else(|e| die(&format!("can't read {path}: {e}")));
        let reader: Box<dyn std::io::Read> = if path.ends_with(".zst") { Box::new(zstd::stream::read::Decoder::new(f).expect("zstd")) } else { Box::new(f) };
        for line in std::io::BufReader::new(reader).lines().map_while(Result::ok) {
            // Impossible options score minus infinity, which JSON writes as null: read those back.
            let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            for k in ["scores", "rules_scores"] {
                if let Some(arr) = v.get_mut(k).and_then(|x| x.as_array_mut()) {
                    for x in arr.iter_mut().filter(|x| x.is_null()) {
                        *x = serde_json::json!(-1.0e30);
                    }
                }
            }
            if let Ok(t) = serde_json::from_value::<sim::brain::Trace>(v) {
                out.push(t);
            }
        }
    }
    out
}

/// Train the scorer on a teacher's decisions (ARCHITECTURE.md section 9). Seeds divisible by 5 are
/// held out, for the agreement figure.
fn cmd_train(a: &Args) {
    use std::io::BufRead;
    let teacher = a.opts.get("teacher").cloned();
    let content = sim::content::Content::base();
    let act_of = |o: &sim::brain::Choice| o.kind.name(&content);
    // From traces, or from a dataset's features.jsonl (export or label: {"split", "example"}).
    let data: Vec<brains::scorer::Example> = if let Some(paths) = a.opts.get("examples") {
        let mut all = Vec::new();
        for path in paths.split(',').filter(|p| !p.is_empty()) {
            let f = std::fs::File::open(path).unwrap_or_else(|e| die(&format!("can't read {path}: {e}")));
            all.extend(
                std::io::BufReader::new(f)
                    .lines()
                    .map_while(Result::ok)
                    .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
                    .filter_map(|v| serde_json::from_value::<brains::scorer::Example>(v.get("example")?.clone()).ok()),
            );
        }
        all
    } else {
        let paths = a.opts.get("traces").cloned().unwrap_or_else(|| die("--traces (trace files) or --examples (a features.jsonl) is needed"));
        let traces = read_traces(&paths);
        traces.iter().filter(|t| teacher.as_ref().is_none_or(|w| &t.brain == w)).filter_map(|t| brains::scorer::Example::from_trace(t, &act_of)).collect()
    };
    if data.is_empty() {
        die("no decisions to learn from (check --teacher)");
    }
    let out = PathBuf::from(a.opts.get("out").cloned().unwrap_or_else(|| "brain/models/scorer@v1.json".into()));
    let name = out.file_stem().map_or("scorer".into(), |s| s.to_string_lossy().to_string());
    let t0 = Instant::now();
    let scorer = brains::scorer::train(&name, &data, a.num("epochs", 12usize));
    println!(
        "trained {name} on {} in {:.1} s: on held-out seeds it picks the teacher's option {:.1}% of the time, and the same kind of action {:.1}%",
        scorer.trained_on,
        t0.elapsed().as_secs_f64(),
        scorer.held_out_agreement * 100.0,
        scorer.held_out_kind_agreement * 100.0
    );
    scorer.save(&out).unwrap_or_else(|e| die(&format!("can't write {}: {e}", out.display())));
    println!("  wrote {} ({} option weights, {} situation weights)", out.display(), scorer.option_w.len(), scorer.state_w.values().map(|m| m.len()).sum::<usize>());
}

/// One colony's outcome under a brain.
#[derive(Default, Clone)]
struct Outcome {
    alive: f64,
    born: f64,
    died: f64,
    starved_or_cold: f64,
    homes: f64,
    grain: f64,
    made: f64,
    hobbies: f64,
    agree: f64,
    decisions: f64,
    micros: f64,
}

/// Run one colony for some days with everyone under one brain (the rules brain, or a scorer),
/// handing each finished decision to `sink`. Returns the world and the seconds it took.
fn run_colony(seed: u64, days: u64, size: u32, name: &str, scorer: Option<brains::Scorer>, beauty_off: bool, sink: sim::sim::TraceSink) -> (Sim, f64) {
    let mut sim = Sim::new(seed, size);
    sim.mode = "lockstep".into();
    // The beauty question: does a brain make beauty when it doesn't matter?
    if beauty_off {
        sim.command(sim::Command::SetBeautyMatters { on: false });
    }
    if let Some(s) = scorer {
        sim.brains.map.insert(name.to_string(), Box::new(brains::ScorerBrain::new(s)));
    }
    // The text model: "text-NAME" is brain/models/NAME.
    if let Some(m) = name.strip_prefix("text-") {
        let b = brains::TextBrain::load(name, &PathBuf::from(format!("brain/models/{m}"))).unwrap_or_else(|e| die(&format!("can't load the text model {m}: {e}")));
        sim.brains.map.insert(name.to_string(), Box::new(b));
    }
    // Laya answers through the bridge's files (tools/laya_driver, in v1's Python environment).
    let mut driver = None;
    if let Some(model) = laya_model(name) {
        let dir = PathBuf::from(format!("runs/arena-bridges/{name}-{seed}/bridge"));
        let _ = std::fs::remove_dir_all(&dir);
        let mut b = brains::SubagentBrain::new(&dir).unwrap_or_else(|e| die(&format!("bridge for {name}: {e}")));
        b.window = 1;
        sim.brains.map.insert(name.to_string(), Box::new(b));
        let log = std::fs::File::create(dir.with_file_name("driver.log")).expect("driver log");
        driver = Some(
            std::process::Command::new("archive/godot-v1/brain_server/.venv/bin/python")
                .args(["tools/laya_driver", &dir.to_string_lossy(), "--model", &model])
                .stdout(log.try_clone().expect("log"))
                .stderr(log)
                .spawn()
                .unwrap_or_else(|e| die(&format!("can't start tools/laya_driver: {e}"))),
        );
    }
    sim.trace_sink = Some(sink);
    let per_day = sim.content.calendar.ticks_per_day();
    let target = sim.tick + days * per_day;
    let t0 = Instant::now();
    while sim.tick < target {
        if name != "rules" {
            let ids: Vec<sim::PersonId> = sim.people.iter().filter(|(_, p)| p.alive && p.brain != name).map(|(id, _)| id).collect();
            for id in ids {
                sim.set_brain(id, name);
            }
        }
        if driver.is_some() {
            // Lockstep: the world waits for the answers.
            if sim.brains_in_flight() > 0 {
                std::thread::sleep(std::time::Duration::from_millis(5));
                sim.poll_brains();
                continue;
            }
            let deciders = sim.people.iter().filter(|(_, p)| p.alive && p.stage(sim.tick, &sim.content) != sim::people::social::Stage::Baby).count();
            if sim.brains_queued() > 0 && sim.waiting() >= deciders.max(1) {
                sim.flush_brains();
                continue;
            }
        }
        sim.step();
    }
    if let Some(mut d) = driver {
        let _ = d.kill();
        let _ = d.wait();
    }
    (sim, t0.elapsed().as_secs_f64())
}

/// The Laya checkpoint for a brain name: "laya" is the base model, "laya-v5" v1's fine-tune,
/// "laya-NAME" a checkpoint in brain/models/NAME.
fn laya_model(name: &str) -> Option<String> {
    match name {
        "laya" => Some("convaiinnovations/laya".into()),
        "laya-v5" => Some("archive/godot-v1/models/laya_colony_v5".into()),
        n => n.strip_prefix("laya-").map(|m| format!("brain/models/{m}")),
    }
}

/// How a colony ended up.
fn outcome(sim: &Sim) -> Outcome {
    let c = &sim.content;
    let mut o = Outcome { alive: sim.alive() as f64, ..Default::default() };
    for (_, p) in sim.people.iter() {
        if p.family.mother.is_some() {
            o.born += 1.0;
        }
        if let Some((_, cause)) = &p.died {
            o.died += 1.0;
            if matches!(cause.as_str(), "starvation" | "thirst" | "the cold") {
                o.starved_or_cold += 1.0;
            }
        }
        if p.alive {
            o.hobbies += p.hobby_acts().len() as f64;
        }
    }
    o.homes = sim.structures.iter().filter(|(_, s)| s.done() && c.buildings[s.kind as usize].sleeps > 0).count() as f64;
    o.grain = sim.structures.iter().map(|(_, s)| s.harvested as f64).sum();
    o.made = sim.works.len() as f64 + sim.structures.iter().map(|(_, s)| s.decor as f64).sum::<f64>();
    o
}

impl Outcome {
    fn add(&mut self, o: &Outcome) {
        for (t, v) in [(&mut self.alive, o.alive), (&mut self.born, o.born), (&mut self.died, o.died), (&mut self.starved_or_cold, o.starved_or_cold), (&mut self.homes, o.homes), (&mut self.grain, o.grain), (&mut self.made, o.made), (&mut self.hobbies, o.hobbies), (&mut self.agree, o.agree), (&mut self.decisions, o.decisions), (&mut self.micros, o.micros)] {
            *t += v;
        }
    }
}

/// How often a brain's choice was the rules brain's, and where they differed: (what rules would
/// have done, what the brain did) -> count, by kind of action.
#[derive(Default)]
struct Agreement {
    same: u64,
    all: u64,
    differ: Differ,
}

/// (what rules would have done, what the brain did) -> how often.
type Differ = BTreeMap<(String, String), u64>;

fn agreement_sink() -> (sim::sim::TraceSink, std::sync::Arc<std::sync::Mutex<Agreement>>) {
    let agree = std::sync::Arc::new(std::sync::Mutex::new(Agreement::default()));
    let ag = agree.clone();
    let content = sim::content::Content::base();
    let sink: sim::sim::TraceSink = Box::new(move |t: &sim::brain::Trace| {
        let mut g = ag.lock().unwrap();
        g.same += (t.choice == t.rules_choice) as u64;
        g.all += 1;
        let kind = |i: usize| t.request.options.get(i).map_or("?".to_string(), |o| o.kind.name(&content));
        let (r, b) = (kind(t.rules_choice), kind(t.choice));
        if r != b {
            *g.differ.entry((r, b)).or_default() += 1;
        }
    });
    (sink, agree)
}

/// The same worlds with different brains, compared (ARCHITECTURE.md section 9).
fn cmd_arena(a: &Args) {
    let seeds = a.range("seeds", (1, 3));
    let days = a.num("days", 48u64);
    let names: Vec<String> = a.opts.get("brains").cloned().unwrap_or_else(|| "rules".into()).split(',').map(|s| s.trim().to_string()).collect();
    let out = PathBuf::from(a.opts.get("out").cloned().unwrap_or_else(|| "runs/arena".into()));
    let beauty_off = a.opts.contains_key("beauty-off");
    let mut rows: Vec<(String, Outcome)> = Vec::new();
    let mut differs: Vec<(String, Differ)> = Vec::new();
    for name in &names {
        let mut total = Outcome::default();
        let mut differ: Differ = BTreeMap::new();
        for &seed in &seeds {
            let scorer = if name == "rules" || laya_model(name).is_some() || name.starts_with("text-") {
                None
            } else {
                let path = PathBuf::from(format!("brain/models/{name}.json"));
                Some(brains::Scorer::load(&path).unwrap_or_else(|e| die(&format!("can't load brain '{name}': {e}"))))
            };
            let (sink, agree) = agreement_sink();
            let (sim, secs) = run_colony(seed, days, a.num("size", 256u32), name, scorer, beauty_off, sink);
            let mut o = outcome(&sim);
            let g = agree.lock().unwrap();
            o.agree = g.same as f64;
            o.decisions = g.all as f64;
            o.micros = secs * 1e6;
            for (k, n) in &g.differ {
                *differ.entry(k.clone()).or_default() += n;
            }
            println!("  {name} seed {seed}: {} alive, {} died ({} of hunger, thirst or cold), {} homes, {} grain, {} made; {:.1} s", o.alive, o.died, o.starved_or_cold, o.homes, o.grain, o.made, secs);
            total.add(&o);
        }
        rows.push((name.clone(), total));
        differs.push((name.clone(), differ));
    }
    let n = seeds.len() as f64;
    let beauty = if beauty_off { " (beauty doesn't matter)" } else { "" };
    let list = if seeds.windows(2).all(|w| w[1] == w[0] + 1) { format!("seeds {}..{}", seeds[0], seeds[seeds.len() - 1]) } else { format!("seeds {}", seeds.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ")) };
    let mut report = format!("# Arena: {list}, {days} days each{beauty}\n\nMeans per colony.\n\n| Brain | Alive | Born | Died | Of hunger, thirst or cold | Homes | Grain | Things made | Hobbies | Agrees with rules |\n|---|---|---|---|---|---|---|---|---|---|\n");
    for (name, t) in &rows {
        report += &format!(
            "| {name} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.0} | {:.1} | {:.1} | {:.0}% |\n",
            t.alive / n,
            t.born / n,
            t.died / n,
            t.starved_or_cold / n,
            t.homes / n,
            t.grain / n,
            t.made / n,
            t.hobbies / n,
            100.0 * t.agree / t.decisions.max(1.0)
        );
    }
    // Where each brain parts from rules most often, by kind of action.
    for (name, differ) in differs.iter().filter(|(n, _)| n != "rules") {
        let all = rows.iter().find(|(r, _)| r == name).map_or(1.0, |(_, t)| t.decisions.max(1.0));
        let mut top: Vec<_> = differ.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        report += &format!("\n**Where {name} parts from rules most** (rules would have, it did instead; share of all decisions):\n\n");
        for ((r, b), k) in top.into_iter().take(8) {
            report += &format!("- {r} → {b}: {:.1}%\n", 100.0 * *k as f64 / all);
        }
    }
    std::fs::create_dir_all(&out).ok();
    let path = out.join("report.md");
    std::fs::write(&path, &report).unwrap_or_else(|e| die(&format!("can't write {}: {e}", path.display())));
    println!("{report}");
    println!("wrote {}", path.display());
}

/// One decision to be labelled: where it came from, what the person saw, and what the brain
/// running the colony (and the rules brain) chose.
#[derive(serde::Serialize, serde::Deserialize)]
struct ToLabel {
    seed: u64,
    request: sim::brain::DecisionRequest,
    /// Index into the request's options.
    chosen: usize,
    rules_choice: usize,
}

/// Claude Haiku labels decisions (M9). Rather than running Haiku colonies live (a few decisions
/// per call, and never past the first days), colonies run by another brain (rules, or a scorer)
/// for whole years give the situations: an even sample of their decisions goes to Haiku in full
/// batches through the bridge's files, answered by `tools/haiku_driver` (on your Claude plan).
/// The answers become a dataset in the export's format (features.jsonl, text.jsonl), with
/// teacher "haiku". It resumes: answered batches are kept, and a usage limit stops it cleanly.
fn cmd_label(a: &Args) {
    use std::io::{BufRead, Write};
    let seeds = a.range("seeds", (1, 12));
    let days = a.num("days", 48u64);
    let per = a.num("per", 420usize);
    let shards = a.num("shards", 3usize).max(1);
    let brain = a.opts.get("brain").cloned().unwrap_or_else(|| "rules".into());
    let dir = PathBuf::from(a.opts.get("dir").cloned().unwrap_or_else(|| "runs/label-1".into()));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("can't make {}: {e}", dir.display())));
    let content = sim::content::Content::base();
    // Relabelling a part of a sample: `--only a,b` keeps the decisions that offer any of those
    // actions, and `--some c --some-share 0.25` a share of those that offer c.
    let list = |k: &str| a.opts.get(k).map(|v| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>()).unwrap_or_default();
    let (only, some) = (list("only"), list("some"));
    let some_share = a.num("some-share", 0.25f64);
    let offers = |t: &ToLabel, acts: &[String]| t.request.options.iter().any(|o| o.possible && acts.contains(&o.kind.name(&content)));
    // 1. The decisions: the same arguments always pick the same ones.
    let reqs_path = dir.join("requests.jsonl");
    let todo: Vec<ToLabel> = if reqs_path.exists() {
        let f = std::fs::File::open(&reqs_path).expect("requests file");
        std::io::BufReader::new(f).lines().map_while(Result::ok).filter_map(|l| serde_json::from_str(&l).ok()).collect()
    } else {
        let mut all = Vec::new();
        for &seed in &seeds {
            let got: std::sync::Arc<std::sync::Mutex<Vec<(u64, ToLabel)>>> = Default::default();
            let g2 = got.clone();
            let sink: sim::sim::TraceSink = Box::new(move |t: &sim::brain::Trace| {
                if t.request.options.iter().filter(|o| o.possible).count() < 2 || t.fallback {
                    return;
                }
                let key = sim::hash::hash2(t.seed ^ 0x1ABE1, t.person.index as i32, t.tick as i32);
                g2.lock().unwrap().push((key, ToLabel { seed: t.seed, request: t.request.clone(), chosen: t.choice, rules_choice: t.rules_choice }));
            });
            // The text model and Laya load themselves in run_colony; a scorer loads here.
            let scorer = (brain != "rules" && !brain.starts_with("text-") && laya_model(&brain).is_none())
                .then(|| brains::Scorer::load(&PathBuf::from(format!("brain/models/{brain}.json"))).unwrap_or_else(|e| die(&format!("can't load brain '{brain}': {e}"))));
            let (sim, _) = run_colony(seed, days, a.num("size", 256u32), &brain, scorer, false, sink);
            let mut v = std::mem::take(&mut *got.lock().unwrap());
            let total = v.len();
            v.sort_by_key(|(k, _)| *k);
            v.truncate(per);
            if !only.is_empty() || !some.is_empty() {
                v.retain(|(k, t)| offers(t, &only) || (offers(t, &some) && (k % 10_000) as f64 / 10_000.0 < some_share));
            }
            println!("  seed {seed}: {} of {total} decisions kept ({} alive at the end)", v.len(), sim.alive());
            all.extend(v.into_iter().map(|(_, t)| t));
        }
        let mut f = std::io::BufWriter::new(std::fs::File::create(&reqs_path).expect("requests file"));
        for t in &all {
            serde_json::to_writer(&mut f, t).expect("json");
            f.write_all(b"\n").expect("write");
        }
        all
    };
    // 2. Batches of up to 24, dealt out to the shards (each answered by its own driver).
    let batch = a.num("batch", 24usize).max(1);
    let n_batches = todo.len().div_ceil(batch);
    let shard_dir = |k: usize| dir.join(format!("bridge-{}", k % shards));
    for k in 0..shards {
        std::fs::create_dir_all(shard_dir(k)).expect("shard dir");
        let _ = std::fs::remove_file(shard_dir(k).join("stop.json"));
    }
    let mut waiting = 0;
    for (n, chunk) in todo.chunks(batch).enumerate() {
        let d = shard_dir(n);
        if d.join(format!("res-{n}.json")).exists() {
            continue;
        }
        waiting += 1;
        let items: Vec<brains::bridge::BatchItem> = chunk
            .iter()
            .enumerate()
            .map(|(j, t)| {
                let person = t.request.text.split_once(',').map_or(String::new(), |(x, _)| x.trim_start_matches("You are ").to_string());
                brains::bridge::BatchItem { id: (n * batch + j) as u64, person, text: t.request.text.clone(), options: t.request.options.iter().filter(|o| o.possible).count() }
            })
            .collect();
        let b = brains::bridge::Batch { batch: n as u64, tick: 0, instructions: brains::bridge::INSTRUCTIONS.into(), items };
        let tmp = d.join(format!("req-{n}.json.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(&b).expect("batch")).and_then(|_| std::fs::rename(&tmp, d.join(format!("req-{n}.json")))).expect("write batch");
    }
    println!("label: {} decisions in {n_batches} batches ({waiting} to answer) over {shards} drivers, in {}", todo.len(), dir.display());
    // 3. The drivers, until every batch is answered (or the plan's usage limit stops them).
    let root = std::env::current_dir().unwrap_or_default();
    let mut kids = Vec::new();
    if waiting > 0 && !a.opts.contains_key("no-driver") {
        for k in 0..shards.min(waiting) {
            let log = std::fs::File::create(dir.join(format!("driver-{k}.log"))).expect("driver log");
            let child = std::process::Command::new(root.join("tools/haiku_driver")).arg(shard_dir(k)).stdout(log.try_clone().expect("log")).stderr(log).spawn().unwrap_or_else(|e| die(&format!("can't start tools/haiku_driver: {e}")));
            kids.push(child);
        }
    }
    let t0 = Instant::now();
    let mut last = usize::MAX;
    loop {
        let done = (0..n_batches).filter(|n| shard_dir(*n).join(format!("res-{n}.json")).exists()).count();
        if done != last {
            println!("  {done} of {n_batches} batches answered ({:.0} s)", t0.elapsed().as_secs_f64());
            last = done;
        }
        if done == n_batches {
            break;
        }
        if let Some(k) = (0..shards).find(|k| shard_dir(*k).join("stop.json").exists()) {
            let why = std::fs::read_to_string(shard_dir(k).join("stop.json")).unwrap_or_default();
            for mut c in kids {
                let _ = c.kill();
                let _ = c.wait();
            }
            println!("a driver stopped ({}): {done} of {n_batches} answered. Run the same command again later to carry on.", why.trim());
            std::process::exit(3);
        }
        if kids.is_empty() {
            println!("no drivers running (--no-driver): answer the batches in {}, then run this again", dir.display());
            std::process::exit(2);
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    for mut c in kids {
        let _ = c.kill();
        let _ = c.wait();
    }
    // 4. The dataset.
    let out = dir.join("dataset");
    std::fs::create_dir_all(&out).expect("dataset dir");
    let mut feats = std::io::BufWriter::new(std::fs::File::create(out.join("features.jsonl")).expect("features"));
    let mut text = std::io::BufWriter::new(std::fs::File::create(out.join("text.jsonl")).expect("text"));
    let (mut n, mut agree, mut bad, mut held) = (0usize, 0usize, 0usize, 0usize);
    for (b, chunk) in todo.chunks(batch).enumerate() {
        let bytes = std::fs::read(shard_dir(b).join(format!("res-{b}.json"))).unwrap_or_default();
        let answers: BTreeMap<u64, brains::bridge::ItemAnswer> = brains::bridge::parse_answer(&bytes).map(|p| p.answers.into_iter().map(|x| (x.id, x)).collect()).unwrap_or_default();
        for (j, t) in chunk.iter().enumerate() {
            let possible: Vec<usize> = t.request.options.iter().enumerate().filter(|(_, o)| o.possible).map(|(i, _)| i).collect();
            let Some(ans) = answers.get(&((b * batch + j) as u64)) else {
                bad += 1;
                continue;
            };
            let Some(k) = usize::try_from(ans.choice).ok().filter(|k| *k >= 1 && *k <= possible.len()) else {
                bad += 1;
                continue;
            };
            let rules_k = possible.iter().position(|&i| i == t.rules_choice);
            agree += (rules_k == Some(k - 1)) as usize;
            let e = brains::scorer::Example {
                seed: t.seed,
                teacher: "haiku".into(),
                state: t.request.state.clone(),
                options: possible.iter().map(|&i| (t.request.options[i].kind.name(&content), t.request.options[i].features.clone())).collect(),
                choice: k - 1,
            };
            let split = if t.seed % 5 == 0 { "held_out" } else { "train" };
            held += (split == "held_out") as usize;
            serde_json::to_writer(&mut feats, &serde_json::json!({ "split": split, "person": t.request.person.index, "tick": t.request.tick, "example": e })).expect("json");
            feats.write_all(b"\n").expect("write");
            serde_json::to_writer(&mut text, &serde_json::json!({ "split": split, "seed": t.seed, "person": t.request.person.index, "tick": t.request.tick, "teacher": "haiku", "prompt": t.request.text, "choice": k, "reason": ans.reason, "rules_choice": rules_k.map(|r| r + 1) })).expect("json");
            text.write_all(b"\n").expect("write");
            n += 1;
        }
    }
    println!(
        "dataset: {n} decisions labelled by Haiku ({held} held out), {bad} unusable answers; Haiku chose what rules would {:.0}% of the time. Wrote {}",
        100.0 * agree as f64 / n.max(1) as f64,
        out.display()
    );
}

/// Time the text model on real decisions (from a rules colony): the first time through (nothing
/// cached) and again (option lines cached, as in play).
fn cmd_speed(a: &Args) {
    use sim::brain::Brain;
    let brain = a.opts.get("brain").cloned().unwrap_or_else(|| "text-text_model_v1".into());
    let m = brain.strip_prefix("text-").unwrap_or_else(|| die("--brain text-NAME"));
    let n = a.num("n", 300usize);
    // `--data text.jsonl`: agreement with the teacher on a dataset's held-out decisions (the
    // check that the game reads the model exactly as training did).
    if let Some(path) = a.opts.get("data") {
        use std::io::BufRead;
        let r = std::fs::File::open(path).unwrap_or_else(|e| die(&format!("{path}: {e} (unzip a .gz first)")));
        let mut model = brains::text::TextModel::load(&PathBuf::from(format!("brain/models/{m}"))).unwrap_or_else(|e| die(&e));
        let (mut hit, mut all) = (0, 0);
        for line in std::io::BufReader::new(r).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            if v["split"] != "held_out" {
                continue;
            }
            let (situation, options) = brains::text::split(v["prompt"].as_str().unwrap_or(""));
            if options.len() < 2 {
                continue;
            }
            let s = model.score(&situation, &options).unwrap_or_else(|e| die(&e));
            let pick = (0..s.len()).max_by(|&x, &y| s[x].total_cmp(&s[y]).then(y.cmp(&x))).unwrap_or(0);
            hit += (pick as i64 + 1 == v["choice"].as_i64().unwrap_or(0)) as usize;
            all += 1;
        }
        println!("{brain}: picks Haiku's option on {hit} of {all} held-out decisions ({:.1}%)", 100.0 * hit as f64 / all.max(1) as f64);
        return;
    }
    let got: std::sync::Arc<std::sync::Mutex<Vec<sim::brain::Trace>>> = Default::default();
    let g2 = got.clone();
    let sink: sim::sim::TraceSink = Box::new(move |t: &sim::brain::Trace| g2.lock().unwrap().push(t.clone()));
    run_colony(a.num("seed", 13u64), a.num("days", 3u64), 256, "rules", None, false, sink);
    let traces: Vec<sim::brain::Trace> = got.lock().unwrap().iter().filter(|t| t.request.options.iter().filter(|o| o.possible).count() >= 2).take(n).cloned().collect();
    let mut b = brains::TextBrain::load(&brain, &PathBuf::from(format!("brain/models/{m}"))).unwrap_or_else(|e| die(&e));
    for pass in ["first (nothing cached)", "again (option lines cached)"] {
        let t0 = Instant::now();
        let mut agree = 0;
        for t in &traces {
            b.submit(t.request.clone());
            let (_, ans) = b.poll().pop().expect("an answer");
            agree += (ans.choice == t.choice) as usize;
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / traces.len().max(1) as f64;
        println!("{brain}, {pass}: {ms:.1} ms per decision over {} decisions; picks what rules did {:.0}% of the time; {} couldn't be scored", traces.len(), 100.0 * agree as f64 / traces.len().max(1) as f64, b.failed);
    }
}

/// Print the latest decision of someone, as a text brain sees it.
fn cmd_text(a: &Args) {
    let mut sim = Sim::new(a.num("seed", 2u64), a.num("size", 256u32));
    let per_day = sim.content.calendar.ticks_per_day();
    sim.run_ticks(a.num("day", 2u64).max(1) * per_day - sim.tick % per_day);
    let k = a.num("person", 0usize);
    let Some((_, p)) = sim.people.iter().filter(|(_, p)| p.alive && !p.traces.is_empty()).nth(k) else { die("nobody has decided anything yet") };
    let t = p.traces.back().expect("has traces");
    println!("{}", t.request.text);
}

/// Distil the rules brain into a scorer, with rounds of relabelled play (ARCHITECTURE.md
/// section 9). Round 0 learns from rules colonies. Each later round lets the scorer run the
/// colonies itself, and the rules brain says what it would have done in every situation the
/// scorer got into; those join the data and the scorer is trained again. A scorer trained only on
/// the teacher's own colonies never sees its own mistakes, so in play they compound.
fn cmd_distill(a: &Args) {
    let seeds = a.range("seeds", (1, 12));
    let days = a.num("days", 12u64);
    let rounds = a.num("rounds", 3usize);
    let epochs = a.num("epochs", 12usize);
    let size = a.num("size", 256u32);
    let out = PathBuf::from(a.opts.get("out").cloned().unwrap_or_else(|| "brain/models/scorer@rules-v2.json".into()));
    let name = out.file_stem().map_or("scorer".into(), |s| s.to_string_lossy().to_string());
    // Keep this share of decisions (picked by hash, so the same ones each run): two years of a
    // dozen colonies is a lot of decisions, and neighbouring ones are much alike.
    let keep = a.num("keep", 0.5f64);
    let content = sim::content::Content::base();
    // Decisions from one colony, labelled with the rules brain's choice.
    let collect = |seed: u64, scorer: Option<brains::Scorer>| -> (Vec<brains::scorer::Example>, Outcome) {
        let got = std::sync::Arc::new(std::sync::Mutex::new((Vec::new(), 0u64, 0u64)));
        let g2 = got.clone();
        let c = content.clone();
        let sink: sim::sim::TraceSink = Box::new(move |t: &sim::brain::Trace| {
            let mut g = g2.lock().unwrap();
            g.1 += (t.choice == t.rules_choice) as u64;
            g.2 += 1;
            if (sim::hash::hash2(t.seed ^ 0xD15, t.person.index as i32, t.tick as i32) % 10_000) as f64 >= keep * 10_000.0 {
                return;
            }
            let mut t = t.clone();
            t.choice = t.rules_choice;
            t.brain = "rules".into();
            t.fallback = false;
            if let Some(e) = brains::scorer::Example::from_trace(&t, &|o: &sim::brain::Choice| o.kind.name(&c)) {
                g.0.push(e);
            }
        });
        let brain = if scorer.is_some() { name.as_str() } else { "rules" };
        let (sim, _) = run_colony(seed, days, size, brain, scorer, false, sink);
        let mut o = outcome(&sim);
        let mut g = got.lock().unwrap();
        o.agree = g.1 as f64;
        o.decisions = g.2 as f64;
        (std::mem::take(&mut g.0), o)
    };
    let mut data = Vec::new();
    for &seed in &seeds {
        data.extend(collect(seed, None).0);
    }
    let t0 = Instant::now();
    // Each round's scorer is saved too (name-r0, -r1...), so the arena can say which plays best:
    // agreement on held-out decisions doesn't tell you that.
    let save_round = |s: &brains::Scorer, round: usize| {
        let path = out.with_file_name(format!("{name}-r{round}.json"));
        let mut s = s.clone();
        s.name = format!("{name}-r{round}");
        s.save(&path).unwrap_or_else(|e| die(&format!("can't write {}: {e}", path.display())));
    };
    let mut scorer = brains::scorer::train(&name, &data, epochs);
    save_round(&scorer, 0);
    println!("round 0: {} from rules colonies; held-out agreement {:.1}% ({:.0} s)", data.len(), scorer.held_out_agreement * 100.0, t0.elapsed().as_secs_f64());
    // Held-out seeds stay out of the scorer's own play, so the agreement figure stays honest.
    let play: Vec<u64> = seeds.iter().copied().filter(|s| s % 5 != 0).collect();
    for round in 1..=rounds {
        let mut total = Outcome::default();
        for &seed in &play {
            let (ex, o) = collect(seed, Some(scorer.clone()));
            data.extend(ex);
            total.add(&o);
        }
        let n = play.len() as f64;
        println!(
            "round {round}: the scorer played {} colonies: {:.1} alive, {:.1} died of hunger, thirst or cold, {:.0} grain, agreeing with rules {:.0}% of the time",
            play.len(),
            total.alive / n,
            total.starved_or_cold / n,
            total.grain / n,
            100.0 * total.agree / total.decisions.max(1.0)
        );
        scorer = brains::scorer::train(&name, &data, epochs);
        save_round(&scorer, round);
        println!("  retrained on {}: held-out agreement {:.1}%", data.len(), scorer.held_out_agreement * 100.0);
    }
    scorer.trained_on = format!("{} (rules colonies on seeds {}..{}, {days} days, then {rounds} rounds of its own play relabelled by rules)", scorer.trained_on, seeds[0], seeds[seeds.len() - 1]);
    scorer.save(&out).unwrap_or_else(|e| die(&format!("can't write {}: {e}", out.display())));
    println!("wrote {}", out.display());
}

/// Branch a saved world (M9): load it, give its chance events a new branch, optionally put
/// everyone under another brain, and save it as a new world. Both carry on from the same moment.
fn cmd_fork(a: &Args) {
    let from = PathBuf::from(a.opts.get("load").cloned().unwrap_or_else(|| die("--load needs a save")));
    let to = PathBuf::from(a.opts.get("out").cloned().unwrap_or_else(|| die("--out needs a path for the new world")));
    let mut sim = sim::save::load_from(&from).unwrap_or_else(|e| die(&format!("can't load {}: {e}", from.display())));
    let branch = a.num("branch", 1u64);
    sim.branch(branch);
    if let Some(brain) = a.opts.get("brain") {
        let ids: Vec<sim::PersonId> = sim.people.iter().filter(|(_, p)| p.alive).map(|(id, _)| id).collect();
        for id in ids {
            sim.set_brain(id, brain);
        }
    }
    sim::save::save_to(&sim, &to).unwrap_or_else(|e| die(&format!("can't save {}: {e}", to.display())));
    println!("forked {} at {} into {} (branch {branch})", from.display(), sim.describe_time(), to.display());
}

/// Datasets from traces (ARCHITECTURE.md section 9): the features of each decision with the
/// teacher's choice (for the scorer), and its text with the choice and reason (for a text model).
/// Seeds divisible by 5 are held out.
fn cmd_export(a: &Args) {
    use std::io::Write;
    let paths = a.opts.get("traces").cloned().unwrap_or_else(|| die("--traces needs one or more trace files"));
    let teacher = a.opts.get("teacher").cloned();
    let out = PathBuf::from(a.opts.get("out").cloned().unwrap_or_else(|| "brain/data/export".into()));
    std::fs::create_dir_all(&out).unwrap_or_else(|e| die(&format!("can't make {}: {e}", out.display())));
    let content = sim::content::Content::base();
    let act_of = |o: &sim::brain::Choice| o.kind.name(&content);
    let traces = read_traces(&paths);
    let mut feats = std::io::BufWriter::new(std::fs::File::create(out.join("features.jsonl")).expect("features file"));
    let mut text = std::io::BufWriter::new(std::fs::File::create(out.join("text.jsonl")).expect("text file"));
    let (mut n, mut held) = (0, 0);
    for t in traces.iter().filter(|t| teacher.as_ref().is_none_or(|w| &t.brain == w)) {
        let Some(e) = brains::scorer::Example::from_trace(t, &act_of) else { continue };
        let split = if t.seed % 5 == 0 { "held_out" } else { "train" };
        held += (split == "held_out") as u32;
        n += 1;
        serde_json::to_writer(&mut feats, &serde_json::json!({ "split": split, "example": e })).expect("json");
        feats.write_all(b"\n").expect("write");
        // The option number as the text lists them (possible options only, from 1).
        let k = e.choice + 1;
        serde_json::to_writer(&mut text, &serde_json::json!({ "split": split, "seed": t.seed, "teacher": t.brain, "prompt": t.request.text, "choice": k, "reason": t.reason })).expect("json");
        text.write_all(b"\n").expect("write");
    }
    println!("exported {n} decisions ({held} held out) to {}", out.display());
}
