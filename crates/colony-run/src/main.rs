//! colony-run: the headless command-line runner.
//!
//!   colony-run map   --seed N [--size 256] [--out runs/maps]   PNG per layer + stats for one seed
//!   colony-run maps  --seeds 1..10 [--size 256] [--out ...]    contact sheet of many seeds
//!   colony-run bench [--size 256] [--seeds 1..5]               world generation time
//!   colony-run hash  --seed N [--days 2] [--size 256]          state hash after running N days

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
        _ => {
            println!("{}", HELP);
        }
    }
}

const HELP: &str = "colony-run map   --seed N [--size 256] [--out runs/maps]
colony-run maps  --seeds 1..10 [--size 256] [--out runs/maps]
colony-run bench [--size 256] [--seeds 1..5]
colony-run hash  --seed N [--days 2] [--size 256]
colony-run find  --seed N --kind wildflowers           nearest nodes of a kind to the start";

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

fn cmd_find(a: &Args) {
    let sim = Sim::new(a.num("seed", 1u64), a.num("size", 256u32));
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
