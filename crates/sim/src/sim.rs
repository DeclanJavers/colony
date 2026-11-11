//! The simulation: world state, the tick, commands and events. Same seed, content and command log,
//! same world, every time.

use crate::actions::{self, Ctx};
use crate::brain::{self, Answer, Brains, DecisionRequest, Ending, Outcome, RulesBrain, Trace};
use crate::camp::Camp;
use crate::chronicle::Chronicle;
use crate::commands::Command;
use crate::content::Content;
use crate::events::Event;
use crate::ids::Table;
use crate::family;
use crate::life;
use crate::social;
use crate::path::Pathfinder;
use crate::people::{self, Person, PersonId};
use crate::rng::Rng;
use crate::world::worldgen::{GenReport, WorldGen};
use crate::world::{Map, Node};

/// Saved with the world: random streams that advance as it runs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Streams {
    pub life: Rng,
}

pub type TraceSink = Box<dyn FnMut(&Trace) + Send>;

pub struct Sim {
    pub seed: u64,
    pub content: Content,
    pub tick: u64,
    pub map: Map,
    pub nodes: Table<Node>,
    pub people: Table<Person>,
    pub camp: Camp,
    pub streams: Streams,
    /// History, written from events as they happen.
    pub chronicle: Chronicle,
    /// Every command applied so far, with the tick it was applied at.
    pub command_log: Vec<(u64, Command)>,
    pending: Vec<Command>,
    events: Vec<Event>,
    /// The tick each event happened on (parallel to `events`).
    event_ticks: Vec<u64>,
    /// How the world was generated (not saved).
    pub gen_report: Option<GenReport>,
    pub brains: Brains,
    /// "live" (the game) or "lockstep" (headless runs, the arena): written into traces.
    pub mode: String,
    /// Every finished trace goes here too, if set (headless runs write them to disk).
    pub trace_sink: Option<TraceSink>,
    path: Pathfinder,
}

impl Sim {
    /// A new world from a seed: `size` x `size` tiles.
    pub fn new(seed: u64, size: u32) -> Self {
        Self::with_worldgen(seed, size, &WorldGen::default())
    }

    pub fn with_worldgen(seed: u64, size: u32, pipeline: &WorldGen) -> Self {
        let content = Content::base();
        let out = pipeline.run(seed, size, &content);
        let start = out.map.start;
        let camp = Camp::new(start, content.items.len());
        let mut sim = Sim {
            seed,
            tick: content.calendar.start_tick(),
            camp,
            map: out.map,
            nodes: Table::new(),
            people: Table::new(),
            streams: Streams { life: Rng::stream(seed, "sim.life") },
            chronicle: Chronicle::default(),
            command_log: Vec::new(),
            pending: Vec::new(),
            events: Vec::new(),
            event_ticks: Vec::new(),
            gen_report: Some(out.report),
            brains: Brains::base(seed),
            mode: "live".into(),
            trace_sink: None,
            path: Pathfinder::default(),
            content,
        };
        sim.map.node_at.fill(0);
        for node in out.nodes {
            let i = sim.map.idx(node.x as u32, node.y as u32);
            let id = sim.nodes.insert(node);
            sim.map.node_at[i] = id.index + 1;
        }
        // The group arrives with a little food, water and firewood.
        for (item, n) in [("berries", 40), ("water", 10), ("sticks", 6), ("wood", 4)] {
            sim.camp.stores[sim.content.item(item)] = n;
        }
        let mut rng = Rng::stream(seed, "sim.founders");
        let c = &sim.content;
        for mut p in people::founders(c, c.people.start_group, start, sim.map.size, sim.tick, &mut rng) {
            life::perceive(&mut p, c, &sim.map, &sim.nodes, &sim.camp, sim.tick, c.people.sight + 10, true);
            sim.people.insert(p);
        }
        // Founders start out knowing each other a little.
        let ids: Vec<PersonId> = sim.people.iter().map(|(id, _)| id).collect();
        for &a in &ids {
            for &b in &ids {
                if a != b {
                    let r = sim.people.get_mut(a).unwrap().rel_mut(b);
                    r.familiarity = 0.2;
                    r.affection = 0.1;
                }
            }
        }
        let n = sim.people.len();
        sim.chronicle.add(sim.tick, crate::chronicle::Kind::World, format!("{n} foragers settled by fresh water and made camp."), ids, 1.0);
        sim
    }

    /// Rebuild from saved parts (see save.rs).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        seed: u64,
        tick: u64,
        map: Map,
        nodes: Table<Node>,
        mut people: Table<Person>,
        camp: Camp,
        streams: Streams,
        chronicle: Chronicle,
        command_log: Vec<(u64, Command)>,
    ) -> Self {
        // Brains aren't saved: anyone waiting for an answer decides again.
        let ids: Vec<PersonId> = people.iter().map(|(id, _)| id).collect();
        for id in ids {
            people.get_mut(id).unwrap().waiting = false;
        }
        Sim {
            seed,
            content: Content::base(),
            tick,
            map,
            nodes,
            people,
            camp,
            streams,
            chronicle,
            command_log,
            pending: Vec::new(),
            events: Vec::new(),
            event_ticks: Vec::new(),
            gen_report: None,
            brains: Brains::base(seed),
            mode: "live".into(),
            trace_sink: None,
            path: Pathfinder::default(),
        }
    }

    /// Queue a command for the start of the next tick.
    pub fn command(&mut self, c: Command) {
        self.pending.push(c);
    }

    /// Advance one tick.
    pub fn step(&mut self) {
        let first_event = self.events.len();
        for c in std::mem::take(&mut self.pending) {
            self.apply(&c);
            self.command_log.push((self.tick, c));
        }
        self.tick_people();
        self.tick_world();
        self.tick += 1;
        let cal = &self.content.calendar;
        if self.tick.is_multiple_of(cal.ticks_per_day()) {
            self.events.push(Event::DayStarted { day: cal.day_index(self.tick) });
            if cal.day_index(self.tick).is_multiple_of(cal.days_per_season as u64) {
                let (season, year) = (cal.season(self.tick).to_string(), cal.year(self.tick));
                if season == cal.seasons[0] && year > 1 {
                    let alive = self.alive();
                    self.chronicle.add(self.tick, crate::chronicle::Kind::World, format!("Year {year} began, with {alive} people in the camp."), Vec::new(), 0.4);
                }
                self.events.push(Event::SeasonStarted { season, year });
            }
            self.new_day();
        }
        for e in &self.events[first_event..] {
            self.chronicle.record(self.tick, e);
        }
        let at = self.tick - 1;
        self.event_ticks.resize(self.events.len(), at);
    }

    pub fn run_ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// Events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        self.event_ticks.clear();
        std::mem::take(&mut self.events)
    }

    /// Events since the last call, with the tick each happened on.
    pub fn take_timed_events(&mut self) -> Vec<(u64, Event)> {
        self.event_ticks.resize(self.events.len(), self.tick);
        let ticks = std::mem::take(&mut self.event_ticks);
        ticks.into_iter().zip(std::mem::take(&mut self.events)).collect()
    }

    pub fn describe_time(&self) -> String {
        self.content.calendar.describe(self.tick)
    }

    pub fn alive(&self) -> usize {
        self.people.iter().filter(|(_, p)| p.alive).count()
    }

    /// People waiting for a brain's answer (lockstep runners wait until this is zero).
    pub fn waiting(&self) -> usize {
        self.people.iter().filter(|(_, p)| p.waiting).count()
    }

    fn tick_people(&mut self) {
        let ids: Vec<PersonId> = self.people.iter().filter(|(_, p)| p.alive).map(|(id, _)| id).collect();
        let awake: Vec<(f32, f32)> = ids.iter().map(|id| self.people.get(*id).unwrap()).filter(|p| !p.asleep).map(|p| p.pos).collect();
        let alive = ids.len();
        let hour = self.content.calendar.minute_of_day(self.tick) / 60;
        let tm = self.content.calendar.minutes_per_tick as u64;
        let seen = family::seen(&self.people, self.tick, &self.content);
        let mut effects: Vec<social::Effect> = Vec::new();
        let mut deaths: Vec<(PersonId, String)> = Vec::new();
        let Sim { content: c, map, nodes, people, camp, streams, path, events, tick, brains, seed, mode, trace_sink, .. } = self;
        let mut requests: Vec<DecisionRequest> = Vec::new();
        let mut finished: Vec<(PersonId, Ending)> = Vec::new();
        // Places by the fire go to the youngest first (the closest, warmest places).
        let mut by_age: Vec<(i64, PersonId)> = ids.iter().map(|&id| (-self_born(people, id), id)).collect();
        by_age.sort();
        let fire_slot = |id: PersonId| by_age.iter().position(|&(_, i)| i == id).unwrap_or(0);
        for (slot, &id) in ids.iter().enumerate() {
            let baby = family::find(&seen, id).is_some_and(|s| s.stage == crate::people::social::Stage::Baby);
            let p = people.get_mut(id).unwrap();
            let reach = c.people.company_reach;
            let company = awake.iter().filter(|q| ((q.0 - p.pos.0).powi(2) + (q.1 - p.pos.1).powi(2)).sqrt() <= reach).count() > 1;
            let before = p.needs.clone();
            let died = life::body(p, c, map, camp, *tick, company);
            for (i, n) in c.needs.iter().enumerate() {
                if n.kind == crate::content::NeedKind::Body && before[i] < 0.85 && p.needs[i] >= 0.85 {
                    events.push(Event::NeedCritical { person: id, name: p.name.clone(), need: n.id.clone(), word: n.word(p.needs[i]).to_string() });
                }
            }
            if let Some(cause) = died {
                if p.action.is_some() {
                    finished.push((id, Ending::Interrupted(format!("{} died", p.name))));
                }
                deaths.push((id, cause));
                continue;
            }
            if (*tick + slot as u64).is_multiple_of(15) {
                let check_gone = (*tick + slot as u64).is_multiple_of(60);
                life::perceive(p, c, map, nodes, camp, *tick, c.people.sight, check_gone);
            }
            if baby {
                // Babies make no decisions: they're kept close and fed (family::babies).
                continue;
            }
            if p.asleep && (8..18).contains(&hour) {
                p.tally.day_sleep_minutes += tm;
            }
            if !p.asleep {
                p.tally.awake_minutes += tm;
            }
            let mut ctx = Ctx { c, map, nodes, camp, path, rng: &mut streams.life, tick: *tick, events, alive, slot: fire_slot(id), who: id, seen: &seen, effects: &mut effects };
            if let Some(mut a) = p.action.take() {
                let mut end = actions::step(p, &mut a, &mut ctx);
                if end.is_none() && (*tick + slot as u64).is_multiple_of(20) {
                    end = actions::should_interrupt(p, &a, c, ctx.camp).map(Ending::Interrupted);
                }
                if end.is_some() {
                    p.asleep = false;
                }
                p.action = Some(a);
                if let Some(e) = end {
                    finished.push((id, e));
                }
            } else if !p.waiting {
                requests.push(actions::request(p, id, &ctx));
                p.waiting = true;
            }
        }
        // Close finished actions' traces.
        for (id, ending) in finished {
            finish(people, id, ending, *tick, c, trace_sink);
        }
        for (id, cause) in deaths {
            family::die(people, id, cause, *tick, c, events);
        }
        // Changes to others from everyone's turn, in order; then births, babies and company.
        social::apply(effects, people, nodes, map, camp.fire_pos(), c, *tick, &mut streams.life, events);
        family::births(people, *tick, c, map.size, &mut streams.life, events);
        family::babies(people, &seen, *tick, c);
        if tick.is_multiple_of(60) {
            family::hourly(people, &seen, *tick, c);
        }
        // Hand requests to each person's brain, then take every answer that's ready.
        for req in requests {
            let brain = people.get(req.person).map(|p| p.brain.clone()).unwrap_or_default();
            match brains.map.get_mut(&brain) {
                Some(b) => b.submit(req),
                None => brains.map.get_mut("rules").unwrap().submit(req),
            }
        }
        let mut answers: Vec<(String, DecisionRequest, Answer)> = Vec::new();
        for (name, b) in brains.map.iter_mut() {
            for (req, ans) in b.poll() {
                answers.push((name.clone(), req, ans));
            }
        }
        answers.sort_by_key(|(_, r, _)| r.person);
        for (brain_id, req, ans) in answers {
            let Some(p) = people.get_mut(req.person) else { continue };
            p.waiting = false;
            if !p.alive {
                continue;
            }
            let rules_scores = RulesBrain::score(&req, *seed);
            let rules_choice = RulesBrain::pick(&rules_scores);
            let valid = ans.choice < req.options.len() && req.options[ans.choice].possible;
            let (choice, fallback) = if valid { (ans.choice, false) } else { (rules_choice, true) };
            let chosen = req.options[choice].clone();
            if let crate::actions::Kind::Answer { from, ask } = &chosen.kind {
                p.invites.retain(|i| !(i.from == *from && i.ask == *ask));
            }
            p.thought = actions::thought(&req, &chosen, c);
            p.action = Some(actions::start(p, &chosen, *tick));
            p.tally.decisions += 1;
            p.open_trace = Some(Trace {
                seed: *seed,
                tick: *tick,
                when: c.calendar.describe(*tick),
                person: req.person,
                name: p.name.clone(),
                brain: brain_id,
                mode: mode.clone(),
                scores: ans.scores,
                rules_scores,
                choice,
                rules_choice,
                reason: ans.reason,
                latency_us: 0,
                fallback,
                outcome: None,
                request: req,
            });
        }
    }

    fn tick_world(&mut self) {
        let c = &self.content;
        if self.camp.fire_lit {
            self.camp.fire_hours -= c.calendar.minutes_per_tick as f32 / 60.0;
            if self.camp.fire_hours <= 0.0 {
                self.camp.fire_hours = 0.0;
                self.camp.fire_lit = false;
                self.events.push(Event::FireOut);
            }
        }
    }

    fn new_day(&mut self) {
        let c = &self.content;
        let season = c.calendar.season_index(self.tick);
        let first_of_season = c.calendar.day_of_season(self.tick) == 1;
        let berry = c.node_kind("berry_bush");
        let ids: Vec<_> = self.nodes.iter().map(|(id, _)| id).collect();
        for id in ids {
            let n = self.nodes.get_mut(id).unwrap();
            let def = &c.nodes[n.kind as usize];
            if n.kind == berry {
                if season == 3 && first_of_season {
                    n.amount = (n.amount * (1.0 - c.actions.winter_rot)).floor();
                } else {
                    n.amount = (n.amount + c.actions.berry_regrow[season]).min(def.amount);
                }
            } else if def.regrow > 0.0 {
                n.amount = (n.amount + def.regrow).min(def.amount);
            }
        }
        for f in self.map.footfall.iter_mut() {
            // Trails fade unless walked (half gone in about two weeks).
            *f *= 0.95;
        }
        let Sim { people, content, tick, streams, events, .. } = self;
        family::daily(people, *tick, content, &mut streams.life, events);
        self.map.version += 1;
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

fn self_born(people: &Table<Person>, id: PersonId) -> i64 {
    people.get(id).map_or(0, |p| p.born)
}

/// End someone's current action: fill in the outcome of the decision that started it.
fn finish(people: &mut Table<Person>, id: PersonId, ending: Ending, tick: u64, c: &Content, sink: &mut Option<TraceSink>) {
    let Some(p) = people.get_mut(id) else { return };
    let Some(a) = p.action.take() else { return };
    if let Some(mut t) = p.open_trace.take() {
        t.outcome = Some(Outcome {
            result: ending,
            minutes: (tick - a.started) * c.calendar.minutes_per_tick as u64,
            need_change: p.needs.iter().zip(&a.needs_start).map(|(n, s)| n - s).collect(),
            stored: c.items.iter().zip(&a.stored).filter(|(_, n)| **n > 0).map(|(it, n)| (it.id.clone(), *n)).collect(),
        });
        if let Some(sink) = sink.as_mut() {
            sink(&t);
        }
        p.push_trace(t);
    }
}

/// Probabilities from scores, for showing how sure a brain was.
pub fn confidence(scores: &[f32]) -> Vec<f32> {
    brain::softmax(scores, 0.15)
}
