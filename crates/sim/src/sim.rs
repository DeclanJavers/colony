//! The simulation: world state, the tick, commands and events. Same seed, content and command log,
//! same world, every time.

use crate::commands::Command;
use crate::content::Content;
use crate::events::Event;
use crate::ids::Table;
use crate::world::worldgen::{GenReport, WorldGen};
use crate::world::{Map, Node};

pub struct Sim {
    pub seed: u64,
    pub content: Content,
    pub tick: u64,
    pub map: Map,
    pub nodes: Table<Node>,
    /// Every command applied so far, with the tick it was applied at.
    pub command_log: Vec<(u64, Command)>,
    pending: Vec<Command>,
    events: Vec<Event>,
    /// How the world was generated (not saved).
    pub gen_report: Option<GenReport>,
}

impl Sim {
    /// A new world from a seed: `size` x `size` tiles.
    pub fn new(seed: u64, size: u32) -> Self {
        Self::with_worldgen(seed, size, &WorldGen::default())
    }

    pub fn with_worldgen(seed: u64, size: u32, pipeline: &WorldGen) -> Self {
        let content = Content::base();
        let out = pipeline.run(seed, size, &content);
        let mut sim = Sim {
            seed,
            tick: content.calendar.start_tick(),
            content,
            map: out.map,
            nodes: Table::new(),
            command_log: Vec::new(),
            pending: Vec::new(),
            events: Vec::new(),
            gen_report: Some(out.report),
        };
        sim.map.node_at.fill(0);
        for node in out.nodes {
            let i = sim.map.idx(node.x as u32, node.y as u32);
            let id = sim.nodes.insert(node);
            sim.map.node_at[i] = id.index + 1;
        }
        sim
    }

    /// Rebuild from saved parts (see save.rs).
    pub(crate) fn from_parts(seed: u64, tick: u64, map: Map, nodes: Table<Node>, command_log: Vec<(u64, Command)>) -> Self {
        Sim { seed, content: Content::base(), tick, map, nodes, command_log, pending: Vec::new(), events: Vec::new(), gen_report: None }
    }

    /// Queue a command for the start of the next tick.
    pub fn command(&mut self, c: Command) {
        self.pending.push(c);
    }

    /// Advance one tick.
    pub fn step(&mut self) {
        for c in std::mem::take(&mut self.pending) {
            self.apply(&c);
            self.command_log.push((self.tick, c));
        }
        self.tick += 1;
        let cal = &self.content.calendar;
        if self.tick.is_multiple_of(cal.ticks_per_day()) {
            self.events.push(Event::DayStarted { day: cal.day_index(self.tick) });
            if cal.day_index(self.tick).is_multiple_of(cal.days_per_season as u64) {
                self.events.push(Event::SeasonStarted { season: cal.season(self.tick).to_string(), year: cal.year(self.tick) });
            }
        }
    }

    pub fn run_ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// Events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn describe_time(&self) -> String {
        self.content.calendar.describe(self.tick)
    }

    fn apply(&mut self, c: &Command) {
        match *c {
            Command::PlaceNode { kind, x, y } => {
                if x >= self.map.size || y >= self.map.size || kind as usize >= self.content.nodes.len() {
                    return;
                }
                let i = self.map.idx(x, y);
                if self.map.node_at[i] != 0 || self.map.water[i].is_water() {
                    return;
                }
                let def = &self.content.nodes[kind as usize];
                let id = self.nodes.insert(Node { kind, x: x as u16, y: y as u16, variant: 0, turn: 0, amount: def.amount });
                self.map.node_at[i] = id.index + 1;
                self.map.version += 1;
                self.events.push(Event::NodePlaced { kind, x, y });
            }
            Command::RemoveNode { x, y } => {
                if x >= self.map.size || y >= self.map.size {
                    return;
                }
                let i = self.map.idx(x, y);
                let slot = self.map.node_at[i];
                if slot == 0 {
                    return;
                }
                if let Some(id) = self.nodes.id_at(slot - 1)
                    && let Some(node) = self.nodes.remove(id)
                {
                    self.events.push(Event::NodeRemoved { kind: node.kind, x, y });
                }
                self.map.node_at[i] = 0;
                self.map.version += 1;
            }
        }
    }

    /// Hash of the whole world state. Same seed, content and commands give the same hash.
    pub fn state_hash(&self) -> u64 {
        crate::hash::fnv64(&crate::save::state_bytes(self))
    }
}
