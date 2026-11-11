//! World generation: a pipeline of named stages run in order from the seed. Each stage reads and
//! writes the map being built. Expansions can insert their own stages before or after a named one.

use super::{Map, Node, stages};
use crate::content::{Content, GenParams};
use crate::rng::Rng;
use std::time::Instant;

/// One step of world generation.
#[derive(Clone, Copy)]
pub struct Stage {
    pub name: &'static str,
    pub run: fn(&mut GenCtx),
}

/// What a stage works on.
pub struct GenCtx<'a> {
    pub seed: u64,
    pub n: usize,
    pub content: &'a Content,
    pub p: &'a GenParams,
    pub map: Map,
    /// Nodes placed so far (None = removed again, e.g. cleared for the start site).
    pub nodes: Vec<Option<Node>>,
    /// Unit vector pointing out to sea, across the map.
    pub coast_dir: (f32, f32),
    /// Steps (4-neighbour) to the nearest sea tile, and to the nearest river or lake tile.
    pub dist_sea: Vec<u32>,
    pub dist_fresh: Vec<u32>,
    /// Steepest height difference to a neighbour, metres per tile.
    pub slope: Vec<f32>,
    pub report: GenReport,
}

impl GenCtx<'_> {
    /// The random stream for a stage.
    pub fn rng(&self, name: &str) -> Rng {
        Rng::stream(self.seed, name)
    }
}

/// What happened during generation, for maps, tests and tuning.
#[derive(Clone, Debug, Default)]
pub struct GenReport {
    /// (stage, milliseconds)
    pub stages: Vec<(String, f64)>,
    pub total_ms: f64,
    /// Rain-weighted flow above which a tile is a river.
    pub river_threshold: f32,
    /// 1 = start site met every target, 2 = relaxed, 3 = anywhere on land.
    pub start_tier: u8,
    pub notes: Vec<String>,
}

pub struct GenOutput {
    pub map: Map,
    pub nodes: Vec<Node>,
    pub report: GenReport,
}

pub struct WorldGen {
    pub stages: Vec<Stage>,
}

impl Default for WorldGen {
    /// The base game's pipeline (ARCHITECTURE.md section 3).
    fn default() -> Self {
        let s = |name: &'static str, run: fn(&mut GenCtx)| Stage { name, run };
        WorldGen {
            stages: vec![
                s("shape", stages::shape),
                s("mountains", stages::mountains),
                s("erosion", stages::erosion),
                s("coast", stages::coast),
                s("climate", stages::climate),
                s("hydrology", stages::hydrology),
                s("terrain", stages::terrain),
                s("biomes", stages::biomes),
                s("soil", stages::soil),
                s("vegetation", stages::vegetation),
                s("start_site", stages::start_site),
                s("finish", stages::finish),
            ],
        }
    }
}

impl WorldGen {
    /// Put a stage straight after the named one (an expansion's hook).
    pub fn insert_after(&mut self, after: &str, stage: Stage) {
        let i = self.stages.iter().position(|s| s.name == after).unwrap_or_else(|| panic!("no stage '{after}'"));
        self.stages.insert(i + 1, stage);
    }

    pub fn insert_before(&mut self, before: &str, stage: Stage) {
        let i = self.stages.iter().position(|s| s.name == before).unwrap_or_else(|| panic!("no stage '{before}'"));
        self.stages.insert(i, stage);
    }

    pub fn run(&self, seed: u64, size: u32, content: &Content) -> GenOutput {
        let n = size as usize;
        let mut ctx = GenCtx {
            seed,
            n,
            content,
            p: &content.worldgen,
            map: Map::new(size),
            nodes: Vec::new(),
            coast_dir: (1.0, 0.0),
            dist_sea: vec![u32::MAX; n * n],
            dist_fresh: vec![u32::MAX; n * n],
            slope: vec![0.0; n * n],
            report: GenReport::default(),
        };
        let all = Instant::now();
        for stage in &self.stages {
            let t = Instant::now();
            (stage.run)(&mut ctx);
            ctx.report.stages.push((stage.name.to_string(), t.elapsed().as_secs_f64() * 1000.0));
        }
        ctx.report.total_ms = all.elapsed().as_secs_f64() * 1000.0;
        GenOutput { map: ctx.map, nodes: ctx.nodes.into_iter().flatten().collect(), report: ctx.report }
    }
}
