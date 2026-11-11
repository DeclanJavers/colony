//! The full-screen ledgers (M10, ARCHITECTURE.md section 12): Society, Economy, Knowledge and
//! Map, beside History and the Arena. One page with tabs along the top; each tab is drawn from the
//! sim as it is now (and, for the economy, from the stock kept each morning).

use super::*;
use sim::people::social::Stage;

/// Which ledger is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ledger {
    Society,
    Economy,
    Knowledge,
    Map,
    History,
    Arena,
}

impl Ledger {
    pub fn name(self) -> &'static str {
        match self {
            Ledger::Society => "Society",
            Ledger::Economy => "Economy",
            Ledger::Knowledge => "Knowledge",
            Ledger::Map => "Map",
            Ledger::History => "History",
            Ledger::Arena => "Arena",
        }
    }

    pub fn from_name(s: &str) -> Option<Ledger> {
        [Ledger::Society, Ledger::Economy, Ledger::Knowledge, Ledger::Map, Ledger::History, Ledger::Arena].into_iter().find(|l| l.name().eq_ignore_ascii_case(s))
    }

    fn tip(self) -> &'static str {
        match self {
            Ledger::Society => "Households, families, bonds and moods.",
            Ledger::Economy => "What's in the stores, how it's changed, and the buildings and fields.",
            Ledger::Knowledge => "Who knows what, and which knowledge is at risk of being lost.",
            Ledger::Map => "The whole region: what's known, where everyone is.",
            Ledger::History => "The chronicle: everything that has happened.",
            Ledger::Arena => "The same worlds with different brains, compared.",
        }
    }
}

/// A run of names that wrap, each clickable. Returns the height used and the one clicked.
fn names(ui: &mut Ui, x: f32, y: f32, w: f32, list: &[(PersonId, String, bool)]) -> (f32, Option<PersonId>) {
    let live = t(Font::Sans, 12.5, GOLD);
    let dead = t(Font::Sans, 12.5, FAINT);
    let sep = t(Font::Sans, 12.5, MUTED);
    let lh = ui.line_height(live) + ui.px(2.0);
    let (mut cx, mut cy) = (x, y);
    let mut hit = None;
    for (k, (id, name, alive)) in list.iter().enumerate() {
        let label = if *alive { name.clone() } else { format!("{name} †") };
        let tt = if *alive { live } else { dead };
        let nw = ui.measure(&label, tt);
        if cx > x && cx + nw > x + w {
            cx = x;
            cy += lh;
        }
        let r = Rect::new(cx - ui.px(2.0), cy - ui.px(1.0), nw + ui.px(4.0), lh);
        if ui.hovered(r) {
            ui.rect(r, CHIP, ui.px(4.0));
            if ui.clicked(r) {
                hit = Some(*id);
            }
        }
        ui.text(cx, cy, &label, tt);
        cx += nw;
        if k + 1 < list.len() {
            cx += ui.text(cx, cy, ", ", sep);
        }
    }
    (cy - y + lh, hit)
}

fn person_entry(sim: &Sim, id: PersonId) -> (PersonId, String, bool) {
    let p = sim.people.get(id);
    (id, p.map_or("?".into(), |p| p.name.clone()), p.is_some_and(|p| p.alive))
}

/// A small line chart of values (no axes), for stock over the mornings.
fn sparkline(ui: &mut Ui, r: Rect, values: &[f32], colour: [f32; 4]) {
    ui.rect(r, hex_a("#000000", 0.18), ui.px(3.0));
    if values.len() < 2 {
        return;
    }
    let max = values.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    let n = values.len();
    let bw = (r.w / n as f32).max(1.0);
    for (k, v) in values.iter().enumerate() {
        let h = (v / max * (r.h - ui.px(2.0))).max(if *v > 0.0 { ui.px(1.0) } else { 0.0 });
        let x = r.x + k as f32 * r.w / n as f32;
        ui.rect(Rect::new(x, r.bottom() - h - ui.px(1.0), (bw - ui.px(0.5)).max(1.0), h), colour, 0.0);
    }
}

impl Hud {
    /// Open a ledger (or close the page with None).
    pub fn open_ledger(&mut self, l: Option<Ledger>) {
        if l == Some(Ledger::Arena) {
            self.arena = Some(std::fs::read_to_string(self.root.join("runs/arena/report.md")).unwrap_or_default());
        }
        if l == Some(Ledger::History) && self.history.is_none() {
            self.history = Some(0);
        }
        if let Some(l) = l {
            self.last_ledger = l;
        }
        self.ledger = l;
        self.trace = None;
        self.gifting = false;
    }

    /// The page behind every ledger: the back button and the tabs. Returns the y below them, or
    /// None if the page was closed.
    fn ledger_header(&mut self) -> Option<f32> {
        let open = self.ledger?;
        let dev = self.dev;
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let top = ui.px(44.0);
        let page = Rect::new(0.0, top, sw, sh - top);
        ui.rect(page, hex_c("#1E1A1F"), 0.0);
        ui.block(page);
        let pad = ui.px(24.0);
        let y = top + ui.px(18.0);
        let bh = ui.px(30.0);
        let back = "< Back to the camp";
        let bw = ui.button_width(back);
        let mut choice = None;
        let close = ui.button(Rect::new(pad, y, bw, bh), back, false);
        let mut x = pad + bw + ui.px(18.0);
        let tabs = [Ledger::Society, Ledger::Economy, Ledger::Knowledge, Ledger::Map, Ledger::History, Ledger::Arena];
        for l in tabs.into_iter().filter(|l| *l != Ledger::Arena || dev || open == Ledger::Arena) {
            let w = ui.button_width(l.name()) + ui.px(8.0);
            let r = Rect::new(x, y, w, bh);
            if ui.button(r, l.name(), l == open) && l != open {
                choice = Some(l);
            }
            if ui.hovered(r) {
                ui.tooltip(vec![l.tip().into()]);
            }
            x += w + ui.px(6.0);
        }
        if let Some(l) = choice {
            self.open_ledger(Some(l));
        }
        if close {
            self.open_ledger(None);
            return None;
        }
        Some(y + bh + ui_px(&self.ui, 16.0))
    }

    pub(super) fn ledger_view(&mut self, sim: &Sim, acts: &mut Vec<Action>) {
        let Some(y) = self.ledger_header() else { return };
        match self.ledger {
            Some(Ledger::Society) => self.society(sim, y, acts),
            Some(Ledger::Economy) => self.economy(sim, y),
            Some(Ledger::Knowledge) => self.knowledge(sim, y, acts),
            Some(Ledger::Map) => self.map_ledger(sim, y, acts),
            Some(Ledger::History) => self.history_view(sim, y, acts),
            Some(Ledger::Arena) => self.arena_view(y),
            None => {}
        }
    }

    /// Someone's name was clicked on a ledger: show them in the camp.
    fn go_to_person(&mut self, id: PersonId, acts: &mut Vec<Action>) {
        acts.push(Action::Select(Some(Pick::Person(id))));
        self.open_ledger(None);
        self.tab = Tab::Life;
    }

    // -----------------------------------------------------------------------------------------
    // Society

    fn society(&mut self, sim: &Sim, y: f32, acts: &mut Vec<Action>) {
        let c = &sim.content;
        let living: Vec<(PersonId, &Person)> = sim.people.iter().filter(|(_, p)| p.alive).collect();
        let count = |s: Stage| living.iter().filter(|(_, p)| p.stage(sim.tick, c) == s).count();
        let (babies, kids, adults, elders) = (count(Stage::Baby), count(Stage::Child), count(Stage::Adult), count(Stage::Elder));
        let couples = living.iter().filter(|(id, p)| p.family.partner.is_some_and(|q| q > *id && sim.people.get(q).is_some_and(|q| q.alive))).count();
        let (house, shelter) = (c.building("house"), c.building("shelter"));
        let homes: Vec<(sim::buildings::StructureId, &sim::buildings::Structure)> = sim.structures.iter().filter(|(_, s)| (s.kind == house || s.kind == shelter) && s.done()).collect();
        let gens = sim.people.iter().map(|(_, p)| p.family.generation).max().unwrap_or(0) + 1;
        let mut clicked = None;
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let pad = ui.px(24.0);
        let summary = format!(
            "{} living: {adults} adults, {} young, {elders} elders · {couples} couples · {} homes · {gens} generations",
            living.len(),
            babies + kids,
            homes.len()
        );
        ui.text(pad, y, &summary, t(Font::Sans, 13.0, MUTED));
        let y = y + ui.px(28.0);
        let gap = ui.px(16.0);
        let cw = ((sw - 2.0 * pad - 2.0 * gap) / 3.0).floor();
        let ph = sh - y - ui.px(20.0);
        let head = t(Font::Sans, 15.0, GOLD);
        let small = t(Font::Sans, 11.5, FAINT);
        let body = t(Font::Sans, 12.5, TEXT);
        let ip = ui.px(16.0);

        // Households: each home and who lives there; then whoever has none.
        let p1 = Rect::new(pad, y, cw, ph);
        ui.panel(p1);
        ui.text(p1.x + ip, p1.y + ip, "Households", head);
        let mut yy = p1.y + ip + ui.px(30.0);
        let tw = cw - 2.0 * ip;
        let mut housed: Vec<PersonId> = Vec::new();
        for (sid, s) in &homes {
            if yy > p1.bottom() - ui.px(60.0) {
                ui.text(p1.x + ip, yy, "and more…", small);
                break;
            }
            let kind = &c.buildings[s.kind as usize].name;
            let members: Vec<PersonId> = living.iter().filter(|(id, p)| sim::settle::home_of(p, *id, false, &sim.structures, c) == Some(*sid)).map(|(id, _)| *id).collect();
            housed.extend(&members);
            let title = match s.owner.and_then(|o| sim.people.get(o)) {
                Some(o) => format!("{}'s {}", o.name, kind.to_lowercase()),
                None => format!("An empty {}", kind.to_lowercase()),
            };
            ui.text(p1.x + ip, yy, &title, body);
            yy += ui.px(19.0);
            if members.is_empty() {
                ui.text(p1.x + ip + ui.px(10.0), yy, "nobody lives here", small);
                yy += ui.px(18.0);
            } else {
                let list: Vec<_> = members.iter().map(|id| person_entry(sim, *id)).collect();
                let (h, hit) = names(ui, p1.x + ip + ui.px(10.0), yy, tw - ui.px(10.0), &list);
                clicked = clicked.or(hit);
                yy += h;
            }
            yy += ui.px(8.0);
        }
        let homeless: Vec<_> = living.iter().filter(|(id, _)| !housed.contains(id)).map(|(id, _)| person_entry(sim, *id)).collect();
        if !homeless.is_empty() && yy < p1.bottom() - ui.px(40.0) {
            ui.text(p1.x + ip, yy, if homes.is_empty() { "Everyone sleeps by the fire" } else { "No home yet (they sleep by the fire)" }, body);
            yy += ui.px(19.0);
            let (_, hit) = names(ui, p1.x + ip + ui.px(10.0), yy, tw - ui.px(10.0), &homeless);
            clicked = clicked.or(hit);
        }

        // Generations: founders first, the dead marked.
        let p2 = Rect::new(pad + cw + gap, y, cw, ph);
        ui.panel(p2);
        ui.text(p2.x + ip, p2.y + ip, "Generations", head);
        let mut yy = p2.y + ip + ui.px(30.0);
        for g in 0..gens {
            if yy > p2.bottom() - ui.px(50.0) {
                break;
            }
            let list: Vec<_> = sim.people.iter().filter(|(_, p)| p.family.generation == g).map(|(id, _)| person_entry(sim, id)).collect();
            let alive = list.iter().filter(|e| e.2).count();
            let label = if g == 0 { "Founders".to_string() } else { format!("Generation {}", g + 1) };
            ui.text(p2.x + ip, yy, &format!("{label} · {alive} of {} living", list.len()), body);
            yy += ui.px(19.0);
            let (h, hit) = names(ui, p2.x + ip + ui.px(10.0), yy, tw - ui.px(10.0), &list);
            clicked = clicked.or(hit);
            yy += h + ui.px(10.0);
        }

        // Bonds: the closest pairs and the worst, by how both sides feel; then the camp's mood.
        let p3 = Rect::new(pad + 2.0 * (cw + gap), y, cw, ph);
        ui.panel(p3);
        ui.text(p3.x + ip, p3.y + ip, "Bonds", head);
        let mut pairs: Vec<(f32, PersonId, PersonId, bool)> = Vec::new();
        for (a, pa) in &living {
            for (b, ra) in &pa.rels {
                if b <= a {
                    continue;
                }
                let Some(pb) = sim.people.get(*b).filter(|p| p.alive) else { continue };
                let rb = pb.rels.get(a).map_or(0.0, |r| r.affection);
                let partners = pa.family.partner == Some(*b);
                pairs.push(((ra.affection + rb) * 0.5, *a, *b, partners));
            }
        }
        pairs.sort_by(|x, y| y.0.total_cmp(&x.0));
        let mut yy = p3.y + ip + ui.px(30.0);
        let pair_word = |v: f32, partners: bool| {
            if partners {
                "partners"
            } else if v > 0.6 {
                "close friends"
            } else if v > 0.3 {
                "friends"
            } else if v < -0.4 {
                "can't stand each other"
            } else {
                "don't get on"
            }
        };
        let show = |ui: &mut Ui, yy: &mut f32, list: &[(f32, PersonId, PersonId, bool)]| -> Option<PersonId> {
            let mut hit = None;
            for (v, a, b, partners) in list {
                let entry = [person_entry(sim, *a), person_entry(sim, *b)];
                let (_, h) = names(ui, p3.x + ip, *yy, tw * 0.6, &entry);
                hit = hit.or(h);
                let word = pair_word(*v, *partners);
                let ww = ui.measure(word, small);
                ui.text(p3.right() - ip - ww, *yy + ui.px(1.0), word, small);
                *yy += ui.px(20.0);
            }
            hit
        };
        let close: Vec<_> = pairs.iter().filter(|p| p.0 > 0.3 || p.3).take(7).cloned().collect();
        ui.text(p3.x + ip, yy, "Closest", t(Font::Sans, 12.0, MUTED));
        yy += ui.px(20.0);
        if close.is_empty() {
            ui.text(p3.x + ip, yy, "No close bonds yet.", small);
            yy += ui.px(20.0);
        }
        clicked = clicked.or(show(ui, &mut yy, &close));
        let bad: Vec<_> = pairs.iter().rev().filter(|p| p.0 < -0.15).take(4).cloned().collect();
        yy += ui.px(10.0);
        ui.text(p3.x + ip, yy, "Tensions", t(Font::Sans, 12.0, MUTED));
        yy += ui.px(20.0);
        if bad.is_empty() {
            ui.text(p3.x + ip, yy, "Nobody is at odds.", small);
            yy += ui.px(20.0);
        }
        clicked = clicked.or(show(ui, &mut yy, &bad));
        // Moods: how many are content, so-so or low.
        yy += ui.px(14.0);
        if yy < p3.bottom() - ui.px(70.0) {
            ui.text(p3.x + ip, yy, "Mood", t(Font::Sans, 12.0, MUTED));
            yy += ui.px(22.0);
            let mut bands = [0usize; 3];
            for (_, p) in &living {
                let (score, _) = p.mood(c, near_others(sim, p), by_fire(sim, p), sim.beauty_for(p.pos));
                bands[if score >= 3 { 0 } else if score >= -2 { 1 } else { 2 }] += 1;
            }
            let total = living.len().max(1) as f32;
            let cols = [hex_c("#8CC56A"), hex_c("#E8C468"), hex_c("#F0997B")];
            let words = ["content", "so-so", "low"];
            let mut bx = p3.x + ip;
            for k in 0..3 {
                let w = (tw * bands[k] as f32 / total).floor();
                if w > 0.0 {
                    ui.rect(Rect::new(bx, yy, w, ui.px(10.0)), cols[k], ui.px(2.0));
                }
                bx += w;
            }
            yy += ui.px(18.0);
            let line = (0..3).map(|k| format!("{} {}", bands[k], words[k])).collect::<Vec<_>>().join(" · ");
            ui.text(p3.x + ip, yy, &line, small);
        }
        if let Some(id) = clicked {
            self.go_to_person(id, acts);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Economy (stock only)

    fn economy(&mut self, sim: &Sim, y: f32) {
        let c = &sim.content;
        let days = &sim.stats.days;
        let stored = sim::buildings::has_storehouse(&sim.structures, c);
        let alive = sim.alive().max(1) as f32;
        let food: f32 = c.items.iter().enumerate().map(|(i, d)| sim.camp.stores[i] as f32 * d.food).sum();
        let daily = sim::actions::daily_hunger(c);
        let food_days = food / (alive * daily.max(1e-6));
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let pad = ui.px(24.0);
        let summary = format!(
            "Food for about {} for {} people · {}",
            plural(food_days.floor(), "days"),
            alive as u32,
            if stored { "a storehouse keeps food from spoiling" } else { "no storehouse: food spoils in the open" }
        );
        ui.text(pad, y, &summary, t(Font::Sans, 13.0, MUTED));
        let mut y = y + ui.px(28.0);
        // One card per item: now, the last 24 mornings, the change over six days, what spoiled.
        let cols = 4usize;
        let gap = ui.px(14.0);
        let cw = ((sw - 2.0 * pad - gap * (cols - 1) as f32) / cols as f32).floor();
        let ch = ui.px(108.0);
        let head = t(Font::Sans, 14.0, TEXT);
        let small = t(Font::Sans, 11.5, FAINT);
        for (i, d) in c.items.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let r = Rect::new(pad + col as f32 * (cw + gap), y + row as f32 * (ch + gap), cw, ch);
            ui.panel(r);
            let ip = ui.px(14.0);
            let now = sim.camp.stores[i];
            ui.text(r.x + ip, r.y + ip, &d.name, head);
            let n = now.to_string();
            let nt = t(Font::Sans, 20.0, GOLD);
            let nw = ui.measure(&n, nt);
            ui.text(r.right() - ip - nw, r.y + ip - ui.px(4.0), &n, nt);
            let hist: Vec<f32> = days.iter().rev().take(24).rev().map(|s| *s.stores.get(i).unwrap_or(&0) as f32).collect();
            sparkline(ui, Rect::new(r.x + ip, r.y + ip + ui.px(28.0), r.w - 2.0 * ip, ui.px(30.0)), &hist, hex_a("#E8C468", 0.8));
            let week = days.iter().rev().nth(6).map(|s| *s.stores.get(i).unwrap_or(&0) as i64);
            let spoiled: u32 = days.iter().rev().take(6).map(|s| *s.spoiled.get(i).unwrap_or(&0)).sum();
            let mut line = match week {
                Some(w) if now as i64 > w => format!("up {} in 6 days", now as i64 - w),
                Some(w) if (now as i64) < w => format!("down {} in 6 days", w - now as i64),
                Some(_) => "the same as 6 days ago".to_string(),
                None => "kept since the first morning".to_string(),
            };
            if spoiled > 0 {
                line += &format!(" · {spoiled} spoiled");
            }
            ui.text(r.x + ip, r.bottom() - ip - ui.px(14.0), &line, small);
            if ui.hovered(r) {
                let mut tip = Vec::new();
                if d.food > 0.0 {
                    tip.push(format!("Food: one {} feeds {:.0}% of a person's day.", d.name.to_lowercase(), 100.0 * d.food / daily.max(1e-6)));
                }
                if d.drink > 0.0 {
                    tip.push("Drink: people drink it from the stores.".into());
                }
                if d.fuel > 0.0 {
                    tip.push(format!("Fuel: each one keeps the fire going {:.1} hours.", d.fuel));
                }
                let (open, kept) = c.spoil(i);
                if open > 0.0 {
                    tip.push(format!("Spoils {:.0}% a day in the open, {:.0}% in a storehouse.", open * 100.0, kept * 100.0));
                }
                tip.push(format!("The bars: the stores each morning, the last {} mornings.", hist.len()));
                ui.tooltip(tip);
            }
        }
        y += (c.items.len().div_ceil(cols)) as f32 * (ch + gap) + ui.px(6.0);
        let body = t(Font::Sans, 12.5, TEXT);
        let mut lines: Vec<String> = Vec::new();
        for (k, d) in c.buildings.iter().enumerate() {
            let all: Vec<&sim::buildings::Structure> = sim.structures.iter().map(|(_, s)| s).filter(|s| s.kind as usize == k).collect();
            if all.is_empty() {
                continue;
            }
            let done = all.iter().filter(|s| s.done()).count();
            let building: Vec<String> = all.iter().filter(|s| !s.done()).map(|s| format!("{:.0}%", s.progress * 100.0)).collect();
            let mut l = format!("{}: {done} standing", d.name);
            if !building.is_empty() {
                l += &format!(", {} going up ({})", building.len(), building.join(", "));
            }
            if d.id == "field" {
                let growing: Vec<String> = all
                    .iter()
                    .filter_map(|s| s.crop.as_ref())
                    .map(|k| if k.dead { "killed by frost".to_string() } else if k.growth >= 1.0 { "ripe".to_string() } else { format!("{:.0}% grown", k.growth * 100.0) })
                    .collect();
                let harvested: u32 = all.iter().map(|s| s.harvested).sum();
                if !growing.is_empty() {
                    l += &format!(" · crops: {}", growing.join(", "));
                }
                l += &format!(" · {harvested} grain harvested in all");
            }
            lines.push(l);
        }
        if lines.is_empty() {
            lines.push("Nothing built yet: they live in the open, around the fire.".into());
        }
        // Buildings and fields, in a panel as tall as its lines.
        let ip = ui.px(16.0);
        let w = sw - 2.0 * pad;
        let text_h: f32 = lines.iter().map(|l| ui.wrapped_height(w - 2.0 * ip, l, body) + ui.px(6.0)).sum();
        let r = Rect::new(pad, y, w, (ip * 2.0 + ui.px(28.0) + text_h).min(sh - y - ui.px(20.0)));
        ui.panel(r);
        ui.text(r.x + ip, r.y + ip, "Buildings and fields", t(Font::Sans, 15.0, GOLD));
        let mut yy = r.y + ip + ui.px(28.0);
        for l in lines {
            if yy > r.bottom() - ip - ui.px(16.0) {
                break;
            }
            yy += ui.wrapped(r.x + ip, yy, r.w - 2.0 * ip, &l, body) + ui.px(6.0);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Knowledge

    fn knowledge(&mut self, sim: &Sim, y: f32, acts: &mut Vec<Action>) {
        let c = &sim.content;
        let living: Vec<(PersonId, &Person)> = sim.people.iter().filter(|(_, p)| p.alive).collect();
        // What the camp knows of the land, all minds together.
        let cells = sim.map.size.div_ceil(sim::people::knowledge::CELL) as usize;
        let mut explored = vec![false; cells * cells];
        let mut nodes = std::collections::BTreeSet::new();
        for (_, p) in &living {
            for (k, e) in p.knowledge.explored.iter().enumerate() {
                explored[k] |= *e;
            }
            nodes.extend(p.knowledge.nodes.keys().copied());
        }
        let land_share = explored.iter().filter(|e| **e).count() as f32 / explored.len().max(1) as f32;
        let mut clicked = None;
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let pad = ui.px(24.0);
        let summary = format!("They know {:.0}% of the region and {} places to gather from", land_share * 100.0, nodes.len());
        ui.text(pad, y, &summary, t(Font::Sans, 13.0, MUTED));
        let y = y + ui.px(28.0);
        let r = Rect::new(pad, y, sw - 2.0 * pad, sh - y - ui.px(20.0));
        ui.panel(r);
        let ip = ui.px(18.0);
        let head = t(Font::Sans, 12.0, MUTED);
        let body = t(Font::Sans, 13.0, TEXT);
        let small = t(Font::Sans, 11.5, FAINT);
        let (x_name, x_know, x_bar, x_best, x_who) = (r.x + ip, r.x + ip + ui.px(150.0), r.x + ip + ui.px(250.0), r.x + ip + ui.px(420.0), r.x + ip + ui.px(600.0));
        let mut yy = r.y + ip;
        ui.text(x_name, yy, "Skill", head);
        ui.text(x_know, yy, "Who knows it", head);
        ui.text(x_best, yy, "The best", head);
        ui.text(x_who, yy, "The others who know it", head);
        yy += ui.px(24.0);
        ui.rect(Rect::new(r.x + ip, yy - ui.px(4.0), r.w - 2.0 * ip, 1.0), BORDER, 0.0);
        let n = living.len().max(1) as f32;
        for (k, s) in c.skills.skills.iter().enumerate() {
            if yy > r.bottom() - ip - ui.px(40.0) {
                break;
            }
            let mut who: Vec<(u32, PersonId, &str)> = living.iter().map(|(id, p)| (p.skill_level(k, c), *id, p.name.as_str())).filter(|w| w.0 >= 1).collect();
            who.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(b.2)));
            let at_risk = s.taught && who.len() == 1;
            let lost = s.taught && who.is_empty();
            ui.text(x_name, yy, &s.name, body);
            let count = if lost { "nobody: lost".to_string() } else { format!("{} of {}", who.len(), living.len()) };
            ui.text(x_know, yy, &count, t(Font::Sans, 12.5, if lost || at_risk { K_CONFLICT } else { TEXT }));
            ui.bar(Rect::new(x_bar, yy + ui.px(5.0), ui.px(150.0), ui.px(8.0)), who.len() as f32 / n, if at_risk || lost { K_CONFLICT } else { K_KNOWLEDGE });
            if let Some((lv, id, name)) = who.first() {
                let label = format!("{name}, level {lv}");
                let w = ui.measure(&label, t(Font::Sans, 12.5, GOLD));
                let br = Rect::new(x_best - ui.px(2.0), yy - ui.px(1.0), w + ui.px(4.0), ui.px(18.0));
                if ui.hovered(br) {
                    ui.rect(br, CHIP, ui.px(4.0));
                    if ui.clicked(br) {
                        clicked = Some(*id);
                    }
                }
                ui.text(x_best, yy, &label, t(Font::Sans, 12.5, GOLD));
            }
            let list: Vec<_> = who.iter().skip(1).map(|w| person_entry(sim, w.1)).collect();
            let (h, hit) = names(ui, x_who, yy, r.right() - ip - x_who, &list);
            clicked = clicked.or(hit);
            let row_h = h.max(ui.px(20.0));
            let row = Rect::new(r.x + ip, yy - ui.px(2.0), x_who - r.x - ip, row_h);
            if ui.hovered(row) {
                let mut tip = vec![if s.taught { "Learned only from someone who knows it; lost with the last who does.".to_string() } else { "Anyone can pick it up by doing it.".to_string() }];
                if at_risk {
                    tip.push("Only one person knows it: if they die before teaching it, it's gone.".into());
                }
                ui.tooltip(tip);
            }
            yy += row_h + ui.px(6.0);
        }
        let note = "Coral: knowledge at risk (one knower) or lost. People teach what's useful when asked; watching helps too.";
        if yy < r.bottom() - ip - ui.px(14.0) {
            ui.text(r.x + ip, r.bottom() - ip - ui.px(14.0), note, small);
        }
        if let Some(id) = clicked {
            self.go_to_person(id, acts);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Map

    fn map_ledger(&mut self, sim: &Sim, y: f32, acts: &mut Vec<Action>) {
        let c = &sim.content;
        let m = &sim.map;
        let n = m.size as usize;
        let cells = m.size.div_ceil(sim::people::knowledge::CELL) as usize;
        let mut explored = vec![false; cells * cells];
        for (_, p) in sim.people.iter().filter(|(_, p)| p.alive) {
            for (k, e) in p.knowledge.explored.iter().enumerate() {
                explored[k] |= *e;
            }
        }
        let ui = &mut self.ui;
        let (sw, sh) = ui.size;
        let pad = ui.px(24.0);
        ui.text(pad, y, "The region, as it is. Darker ground is land nobody alive has seen. Click to look there.", t(Font::Sans, 13.0, MUTED));
        let y = y + ui.px(28.0);
        let legend_w = ui.px(220.0);
        let side = (sh - y - ui.px(24.0)).min(sw - 2.0 * pad - legend_w - ui.px(20.0)).max(ui.px(100.0));
        let cell = (side / n as f32).max(0.5);
        let side = cell * n as f32;
        let r = Rect::new(pad, y, side, side);
        ui.rect(Rect::new(r.x - ui.px(4.0), r.y - ui.px(4.0), r.w + ui.px(8.0), r.h + ui.px(8.0)), hex_c("#141114"), ui.px(6.0));
        // The ground's colour runs along each row, worked out again only when the map or what's
        // been explored changes (it's 65,000 tiles).
        let key = (m.version, explored.iter().filter(|e| **e).count(), n);
        if self.map_runs.as_ref().is_none_or(|(k, _)| *k != key) {
            self.map_runs = Some((key, map_runs(sim, &explored, cells)));
        }
        let ui = &mut self.ui;
        if let Some((_, runs)) = &self.map_runs {
            for &(ty, x0, len, col) in runs {
                ui.rect(Rect::new(r.x + x0 as f32 * cell, r.y + ty as f32 * cell, len as f32 * cell + 0.5, cell + 0.5), col, 0.0);
            }
        }
        let at = |x: f32, z: f32| (r.x + x * cell, r.y + z * cell);
        // Buildings and fields, the camp, the herds people have seen, and the people.
        for (_, s) in sim.structures.iter() {
            let d = &c.buildings[s.kind as usize];
            let (x, z) = at(s.x as f32, s.y as f32);
            let col = if d.id == "field" { hex_c("#B07A3C") } else if s.done() { hex_c("#E8C468") } else { hex_a("#E8C468", 0.5) };
            ui.rect(Rect::new(x, z, (d.w as f32 * cell).max(ui.px(3.0)), (d.h as f32 * cell).max(ui.px(3.0))), col, 0.0);
        }
        let mut seen: std::collections::BTreeMap<sim::animals::AnimalId, (f32, f32, u64)> = Default::default();
        for (_, p) in sim.people.iter().filter(|(_, p)| p.alive) {
            for (id, a) in &p.knowledge.animals {
                let e = seen.entry(*id).or_insert((a.x, a.y, a.when));
                if a.when > e.2 {
                    *e = (a.x, a.y, a.when);
                }
            }
        }
        let dot = ui.px(5.0);
        for (x, z, _) in seen.values() {
            let (px, pz) = at(*x, *z);
            ui.rect(Rect::new(px - dot * 0.5, pz - dot * 0.5, dot, dot), hex_c("#8A5A35"), dot * 0.5);
        }
        let (fx, fz) = sim.camp.fire_pos();
        let (px, pz) = at(fx, fz);
        let ring = ui.px(9.0);
        ui.rect(Rect::new(px - ring * 0.5, pz - ring * 0.5, ring, ring), if sim.camp.fire_lit { hex_c("#F0A040") } else { hex_c("#8A8070") }, ring * 0.5);
        let pd = ui.px(4.0);
        for (_, p) in sim.people.iter().filter(|(_, p)| p.alive) {
            let (px, pz) = at(p.pos.0, p.pos.1);
            ui.rect(Rect::new(px - pd * 0.5, pz - pd * 0.5, pd, pd), hex_c("#F4EFE6"), pd * 0.5);
        }
        if ui.hovered(r) {
            let (cx, cy) = ui.input.cursor;
            let (tx, tz) = (((cx - r.x) / cell).floor().clamp(0.0, n as f32 - 1.0) as usize, ((cy - r.y) / cell).floor().clamp(0.0, n as f32 - 1.0) as usize);
            let i = tz * n + tx;
            let ground = match m.water[i] {
                sim::Water::Sea => "Sea".to_string(),
                sim::Water::None => c.biomes.get(m.biome[i] as usize).map_or("Land".into(), |b| b.name.clone()),
                _ => "Fresh water".into(),
            };
            let (dx, dz) = (tx as f32 + 0.5 - fx, tz as f32 + 0.5 - fz);
            let dist = (dx * dx + dz * dz).sqrt();
            let known = explored[(tz / sim::people::knowledge::CELL as usize) * cells + tx / sim::people::knowledge::CELL as usize];
            ui.tooltip(vec![ground, format!("{:.0} m from the camp", dist), if known { "Someone alive has been here.".into() } else { "Nobody alive has seen it.".into() }]);
            if ui.clicked(r) {
                acts.push(Action::Look((tx as f32 + 0.5, tz as f32 + 0.5)));
                self.open_ledger(None);
                return;
            }
        }
        // Legend.
        let lx = r.right() + ui.px(28.0);
        let mut ly = r.y + ui.px(4.0);
        let lt = t(Font::Sans, 12.5, TEXT);
        let items: [([f32; 4], &str); 7] = [
            (hex_c("#F0A040"), "the camp fire"),
            (hex_c("#E8C468"), "homes and storehouses"),
            (hex_c("#B07A3C"), "fields"),
            (hex_c("#F4EFE6"), "people"),
            (hex_c("#8A5A35"), "animals, where last seen"),
            (hex_c("#C9A26B"), "trails"),
            (hex_c("#3E6FA8"), "sea; lighter blue is fresh water"),
        ];
        for (col, label) in items {
            let d = ui.px(10.0);
            ui.rect(Rect::new(lx, ly + ui.px(3.0), d, d), col, d * 0.3);
            ui.text(lx + d + ui.px(8.0), ly, label, lt);
            ly += ui.px(22.0);
        }
    }
}

fn ui_px(ui: &Ui, v: f32) -> f32 {
    ui.px(v)
}

/// The Map ledger's ground: (row, first tile, run length, colour), one per run of a colour.
fn map_runs(sim: &Sim, explored: &[bool], cells: usize) -> Vec<(u16, u16, u16, [f32; 4])> {
let c = &sim.content;
let m = &sim.map;
let n = m.size as usize;
let biome: Vec<[f32; 4]> = c.biomes.iter().map(|b| hex_c(&b.colour)).collect();
    let colour = |i: usize| -> [f32; 4] {
        let x = i % n;
        let mut col = match m.water[i] {
            sim::Water::Sea => hex_c("#3E6FA8"),
            sim::Water::None => *biome.get(m.biome[i] as usize).unwrap_or(&hex_c("#5C9439")),
            _ => hex_c("#5A92CC"),
        };
        if !m.water[i].is_water() {
            // Light from the north-west, in steps, so hills read.
            let w = if x > 0 { m.elevation[i - 1] } else { m.elevation[i] };
            let up = if i >= n { m.elevation[i - n] } else { m.elevation[i] };
            let slope = ((m.elevation[i] - w) + (m.elevation[i] - up)) * 0.5;
            let shade = (1.0 + (slope * 0.08).clamp(-0.25, 0.25) * 2.0).clamp(0.6, 1.3);
            let shade = (shade * 8.0).round() / 8.0;
            for ch in col.iter_mut().take(3) {
                *ch *= shade;
            }
            if m.footfall[i] > 0.4 {
                col = [col[0] * 0.75 + 0.18, col[1] * 0.75 + 0.13, col[2] * 0.75 + 0.06, 1.0];
            }
        }
        let (cx, cy) = (x / sim::people::knowledge::CELL as usize, (i / n) / sim::people::knowledge::CELL as usize);
        if !explored[cy * cells + cx] {
            for ch in col.iter_mut().take(3) {
                *ch *= 0.45;
            }
        }
        col
    };
    let mut out = Vec::new();
    for ty in 0..n {
        let mut run = 0;
        let mut cur = colour(ty * n);
        for tx in 1..=n {
            let next = if tx < n { colour(ty * n + tx) } else { [-1.0; 4] };
            if next != cur {
                out.push((ty as u16, run as u16, (tx - run) as u16, cur));
                run = tx;
                cur = next;
            }
        }
    }
    out
}

