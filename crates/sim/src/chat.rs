//! What people say when they talk (M8, chat on the Low setting: code templates). A line is picked
//! from what's on the speaker's mind: the weather, food, the fire, family, their hobby, somewhere
//! lovely, the one they're talking to. It's for showing (a speech bubble, the person page); the
//! talk's effects on needs and relationships are worked out elsewhere (social.rs).

use crate::actions::Ctx;
use crate::family::Seen;
use crate::people::{Person, PersonId};

/// A line for `p` to say to `o` as they start talking.
pub fn line(p: &Person, id: PersonId, o: &Seen, ctx: &Ctx) -> String {
    let c = ctx.c;
    let need = |n: &str| p.needs[c.need(n)];
    let b = &p.knowledge.stores;
    let food_days = crate::actions::food_in(c, &b.items) / (ctx.alive.max(1) as f32 * crate::actions::daily_hunger(c));
    let hour = c.calendar.minute_of_day(ctx.tick) / 60;
    let w = &ctx.map.weather;
    let salt = crate::hash::hash2(0xC4A7, id.index as i32, (ctx.tick / 7) as i32);
    // One of a few ways of saying the same thing.
    let pick = |opts: &[&str]| opts[(salt as usize / 7) % opts.len()].to_string();
    let mut lines: Vec<(f32, String)> = Vec::new();
    // How pressing each topic is; the most pressing (with a little variety) is said.
    if food_days < 1.5 {
        lines.push((0.9, pick(&["We're nearly out of food.", "The stores are almost bare.", "We need more food in, and soon."])));
    } else if food_days > 6.0 {
        lines.push((0.3, pick(&["The stores are looking good.", "Plenty put by, for once.", "We'll eat well this week."])));
    }
    if need("hunger") > 0.6 {
        lines.push((0.8, pick(&["I'm starving. Is there anything to eat?", "My stomach's growling.", "I could eat a whole bush of berries."])));
    }
    if need("warmth") > 0.5 || w.snap_left > 0 {
        lines.push((0.7, pick(&["Bitter cold, isn't it?", "I can't feel my fingers.", "Stay close to the fire tonight."])));
    }
    if w.sky.word() == "rain" {
        lines.push((0.5, pick(&["This rain gets into everything.", "Soaked through again.", "Good for the fields, at least."])));
    } else if w.sky.word() == "snow" {
        lines.push((0.5, pick(&["Look at the snow coming down.", "Snow already.", "Everything's gone quiet under the snow."])));
    } else if w.sky.word() == "clear" && (9..17).contains(&hour) {
        lines.push((0.2, pick(&["Lovely clear day.", "Not a cloud.", "Good weather for working."])));
    }
    if !b.fire_lit {
        lines.push((0.6, pick(&["Someone should get the fire going.", "The fire's out again.", "Who let the fire die?"])));
    }
    if need("rest") > 0.6 {
        lines.push((0.5, pick(&["I'm worn out.", "My back aches.", "I could sleep standing up."])));
    }
    if need("social") > 0.6 {
        lines.push((0.45, pick(&["I've hardly spoken to anyone all day.", "Glad you came by.", "Sit with me a while?"])));
    }
    if let Some(&h) = p.hobby_acts().first() {
        let what = match crate::leisure::hobby_name(c, h) {
            "walking" => pick(&["I walked out past the trees this morning. Lovely out there.", "There's a quiet spot by the water I like.", "You should walk with me sometime."]),
            "singing" => pick(&["Sing with us tonight, by the fire?", "I've a new song stuck in my head.", "We should sing more."]),
            "gardening" => pick(&["The flowers are coming up nicely.", "I've planted some new flowers.", "The beds need weeding again."]),
            "carving" => pick(&["I'm carving something new. You'll see.", "I found a good bit of wood for carving.", "My hands know the knife now."]),
            _ => pick(&["I've been making the house look nice.", "Have you seen our door posts?", "Home's looking better."]),
        };
        lines.push((0.35, what));
    }
    // Only when beauty matters (otherwise the field is empty and this is 0).
    if crate::leisure::beauty_at(ctx.beauty, ctx.map, p.pos) > 0.3 {
        lines.push((0.4, pick(&["It's lovely here, isn't it?", "I love the flowers here.", "Someone's made this place pretty."])));
    }
    if p.family.partner == Some(o.id) {
        lines.push((0.45, pick(&["There you are. I was hoping to find you.", "Missed you today.", "Come and sit with me."])));
    }
    if let Some(k) = ctx.seen.iter().filter(|s| s.alive && (s.mother == Some(o.id) || s.father == Some(o.id)) && s.stage < crate::people::social::Stage::Adult).max_by(|a, b| b.age.total_cmp(&a.age)) {
        lines.push((0.3, format!("How's little {}?", k.name)));
    }
    let greeting = match hour {
        5..=10 => format!("Morning, {}.", o.name),
        11..=16 => format!("Hello, {}.", o.name),
        _ => format!("Evening, {}.", o.name),
    };
    lines.push((0.25, greeting));
    lines.push((0.2, pick(&["How are you keeping?", "What have you been up to?", "Busy day?"])));
    // A little variety, from who's talking and when (no random state used).
    lines
        .into_iter()
        .enumerate()
        .max_by(|a, b| {
            let jitter = |i: usize| ((salt >> (i % 16 * 4)) & 0xF) as f32 / 15.0 * 0.25;
            (a.1.0 + jitter(a.0)).total_cmp(&(b.1.0 + jitter(b.0)))
        })
        .map(|(_, l)| l.1)
        .unwrap_or_default()
}

/// A line of a song (someone singing by the fire). Our own words.
pub fn song(id: PersonId, tick: u64) -> String {
    const SONGS: [&str; 8] = [
        "Down by the water the reeds grow tall...",
        "Light the fire, the night is long...",
        "Over the hills where the deer run free...",
        "Bring the grain home, bring it home...",
        "Winter is coming, so gather the wood...",
        "Sleep, little one, the stars are near...",
        "We came from the far side of the sea...",
        "Berries are red and the river is wide...",
    ];
    let k = crate::hash::hash2(0x5096, id.index as i32, (tick / 60) as i32) as usize % SONGS.len();
    format!("\u{266A} {} \u{266A}", SONGS[k])
}
