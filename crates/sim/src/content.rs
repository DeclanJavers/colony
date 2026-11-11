//! Content: the data files in data/base/. They're built into the binary, so a world always gets
//! the same content as the code it was made with. Their hash goes into every save.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Calendar {
    pub minutes_per_tick: u32,
    pub days_per_season: u32,
    pub seasons: Vec<String>,
    pub start_hour: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BiomeDef {
    pub id: String,
    pub name: String,
    pub colour: String,
    pub fertility: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NodeDef {
    pub id: String,
    pub regrow: f32,
    pub name: String,
    pub amount: f32,
    pub variants: u8,
    pub blocks: bool,
    pub clump: f32,
    pub near_water: f32,
    pub colour: String,
    pub densities: Vec<(String, f32)>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GenParams {
    pub sea_fraction: (f32, f32),
    pub inland_rise: f32,
    pub coast_noise_scale: f32,
    pub coast_roughness: f32,
    pub hills_scale: f32,
    pub hills_height: f32,
    pub detail_scale: f32,
    pub detail_height: f32,
    pub mountain_chance: f32,
    pub mountain_height: (f32, f32),
    pub mountain_scale: f32,
    pub mountain_elevation: f32,
    pub erosion_passes: u32,
    pub talus: f32,
    pub river_threshold: f32,
    pub river_share_max: u32,
    pub river_wander: f32,
    pub lake_min_depth: f32,
    pub lake_min_area: u32,
    pub max_lakes: u32,
    pub base_temperature: (f32, f32),
    pub lapse_per_m: f32,
    pub base_rain: (f32, f32),
    pub start_margin: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
pub enum NeedKind {
    Body,
    Mind,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NeedDef {
    pub id: String,
    pub name: String,
    pub kind: NeedKind,
    pub awake_per_hour: f32,
    pub asleep_per_hour: f32,
    pub damage_per_hour: f32,
    pub at: Vec<f32>,
    pub words: Vec<String>,
}

impl NeedDef {
    /// The word for a level, e.g. "hungry".
    pub fn word(&self, v: f32) -> &str {
        let k = self.at.iter().filter(|&&a| v >= a).count();
        &self.words[k.min(self.words.len() - 1)]
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PeopleParams {
    pub start_group: u32,
    pub start_age: (u32, u32),
    pub walk_speed: f32,
    pub carry: u32,
    pub sight: u32,
    pub heal_per_hour: f32,
    pub comfort_temp: f32,
    pub huddle: f32,
    pub cold_per_hour: f32,
    pub fire_reach: f32,
    pub fire_warmth_per_hour: f32,
    pub company_reach: f32,
    pub company_per_hour: f32,
    pub sleep_hour: f32,
    pub wake_hour: f32,
    pub chronotype_spread: f32,
    pub season_temp: Vec<f32>,
    pub day_night_swing: f32,
    pub clothes: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum LocusKind {
    Additive,
    Recessive,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LocusDef {
    pub id: String,
    pub trait_id: String,
    pub kind: LocusKind,
    pub effect: f32,
    pub freq: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EyeColours {
    pub blue: String,
    pub green: String,
    pub hazel: String,
    pub brown: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GeneParams {
    pub loci: Vec<LocusDef>,
    pub personality_heritability: f32,
    pub base_height: (f32, f32),
    pub height_noise: f32,
    pub skin: Vec<String>,
    pub hair: Vec<String>,
    pub hair_red: String,
    pub eyes: EyeColours,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    /// Known only from level 1, learned only from someone who knows it, lost with the last.
    pub taught: bool,
    pub founders_know: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SkillParams {
    pub minutes_per_level_sq: f32,
    pub skills: Vec<SkillDef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub food: f32,
    pub drink: f32,
    pub fuel: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GatherDef {
    pub id: String,
    pub name: String,
    pub node: String,
    pub item: String,
    pub skill: String,
    pub minutes: u32,
    pub yield_per: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FetchWater {
    pub minutes: u32,
    pub amount: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LightFire {
    pub minutes: u32,
    pub sticks: u32,
    pub wood: u32,
    pub chance: f32,
    pub chance_per_level: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RestDef {
    pub minutes: u32,
    pub fun_per_hour: f32,
    pub rest_per_hour: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpearDef {
    pub minutes: u32,
    pub wood: u32,
    pub stone: u32,
    pub breaks: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActionParams {
    pub gather: Vec<GatherDef>,
    pub drink_minutes: u32,
    pub fetch_water: FetchWater,
    pub eat_minutes: u32,
    pub light_fire: LightFire,
    pub tend_fire_minutes: u32,
    pub rest: RestDef,
    pub explore_minutes: u32,
    pub check_stores_minutes: u32,
    pub berry_regrow: Vec<f32>,
    pub winter_rot: f32,
    pub spear: SpearDef,
    pub hunt_minutes: u32,
    pub lost_at: f32,
    pub food_target_days: Vec<f32>,
}

/// Weather (data/base/weather.ron).
#[derive(Clone, Debug, Deserialize)]
pub struct WeatherParams {
    pub wet_chance: Vec<f32>,
    pub cloud_chance: Vec<f32>,
    pub clear_night: f32,
    pub cloudy: f32,
    pub drift: f32,
    pub snap_chance: f32,
    pub snap_cold: f32,
    pub snap_days: u32,
    pub rain_wet: f32,
    pub dry_base: f32,
    pub dry_per_c: f32,
    pub damp_near_water: f32,
    pub snow_fall: f32,
    pub melt_per_c: f32,
    pub wet_chill: f32,
}

/// A kind of building (data/base/buildings.ron).
#[derive(Clone, Debug, Deserialize)]
pub struct BuildingDef {
    pub id: String,
    pub name: String,
    pub w: u16,
    pub h: u16,
    pub blocks: bool,
    pub cost: Vec<(String, u32)>,
    pub minutes: u32,
    pub sleeps: u32,
    pub chill: f32,
    pub model: String,
}

/// Farming (data/base/farming.ron).
#[derive(Clone, Debug, Deserialize)]
pub struct FarmParams {
    pub crop: String,
    pub seed: u32,
    pub harvest: u32,
    pub days_to_ripe: f32,
    pub sow: Vec<bool>,
    pub best_temp: (f32, f32),
    pub best_moisture: (f32, f32),
    pub frost: f32,
    pub weeds_per_day: f32,
    pub sow_minutes: u32,
    pub tend_minutes: u32,
    pub harvest_minutes: u32,
    pub overripe_loss: f32,
    /// From autumn, a field this far grown can be cut early (for that share of the grain)
    /// rather than lost to the frost.
    pub early_harvest: f32,
    pub spoil: Vec<(String, f32, f32)>,
}

/// A kind of animal (data/base/animals.ron).
#[derive(Clone, Debug, Deserialize)]
pub struct AnimalDef {
    pub id: String,
    pub name: String,
    pub herd: (u32, u32),
    pub speed: f32,
    pub flee: f32,
    pub wary: f32,
    pub range: f32,
    pub lifespan_days: u32,
    pub births: f32,
    pub per_1000_tiles: f32,
    pub meat: u32,
    pub hit: f32,
    pub model: String,
    pub habitats: Vec<String>,
}

/// Families and society (data/base/life.ron).
#[derive(Clone, Debug, Deserialize)]
pub struct LifeParams {
    pub age_per_year: f32,
    pub child_age: f32,
    pub adult_age: f32,
    pub elder_age: f32,
    pub help_age: f32,
    pub old_age_hazard: f32,
    pub old_age_scale: f32,
    pub elder_weakening: f32,
    pub fertile_female: (f32, f32),
    pub fertile_male: (f32, f32),
    pub pregnancy_years: f32,
    pub conceive_chance: f32,
    pub birth_gap_years: f32,
    pub childbirth_death: f32,
    pub nursing_age: f32,
    pub birth_height: f32,
    pub drawn_to_other: f32,
    pub court_age_gap: f32,
    pub propose_romance: f32,
    pub familiarity_per_hour: f32,
    pub affection_fade: f32,
    pub romance_fade: f32,
    pub familiarity_fade: f32,
    pub talk_minutes: u32,
    pub argue_chance: f32,
    pub news_places: u32,
    pub court_minutes: u32,
    pub ask_minutes: u32,
    pub lesson_minutes: u32,
    pub watch_minutes: u32,
    pub play_minutes: u32,
    pub feed_minutes: u32,
    pub bury_minutes: u32,
    pub together_minutes: u32,
    pub lesson_rate: f32,
    pub watch_rate: f32,
    pub learn_until: f32,
    pub invite_hours: f32,
    pub memory_fade: f32,
    pub forget: f32,
    pub keep_memories: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Names {
    pub female: Vec<String>,
    pub male: Vec<String>,
}

/// A free-time activity (data/base/leisure.ron).
#[derive(Clone, Debug, Deserialize)]
pub struct LeisureDef {
    pub id: String,
    pub name: String,
    pub minutes: u32,
    /// Fun (and social) eased by doing it once.
    pub fun: f32,
    pub social: f32,
    pub skill: Option<String>,
    /// Personality traits that make it appeal, and how much.
    pub traits: Vec<(String, f32)>,
    /// Materials taken from the stores.
    pub wood: u32,
    pub stone: u32,
    /// Seasons it can be done in (spring, summer, autumn, winter).
    pub seasons: Vec<bool>,
    /// Enjoyment past which it becomes someone's hobby.
    pub hobby_joy: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BeautyParams {
    /// Node kind, beauty at the source, radius (tiles), beauty in winter.
    pub sources: Vec<(String, f32, f32, f32)>,
    pub statue_per_level: f32,
    pub decor: Vec<f32>,
    pub decor_radius: f32,
    pub mood: f32,
    pub fun_per_hour: f32,
}

/// Free time, hobbies and beauty (data/base/leisure.ron).
#[derive(Clone, Debug, Deserialize)]
pub struct LeisureParams {
    pub acts: Vec<LeisureDef>,
    pub max_hobbies: usize,
    pub hobby_appeal: f32,
    pub hobby_fun: f32,
    pub statue_level: u32,
    pub place_radius: u16,
    pub beauty: BeautyParams,
}

impl LeisureParams {
    pub fn act(&self, id: &str) -> u16 {
        self.acts.iter().position(|a| a.id == id).unwrap_or_else(|| panic!("no free-time activity '{id}'")) as u16
    }
}

/// Speaking to someone (data/base/speak.ron): words to the actions they ask for.
#[derive(Clone, Debug, Deserialize)]
pub struct SpeakTopic {
    pub words: Vec<String>,
    pub acts: Vec<String>,
    pub read: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpeakParams {
    pub hours: f32,
    pub topics: Vec<SpeakTopic>,
}

#[derive(Clone, Debug)]
pub struct Content {
    pub calendar: Calendar,
    pub biomes: Vec<BiomeDef>,
    pub nodes: Vec<NodeDef>,
    pub worldgen: GenParams,
    pub needs: Vec<NeedDef>,
    pub people: PeopleParams,
    pub genes: GeneParams,
    pub skills: SkillParams,
    pub items: Vec<ItemDef>,
    pub actions: ActionParams,
    pub names: Names,
    pub life: LifeParams,
    pub weather: WeatherParams,
    pub buildings: Vec<BuildingDef>,
    pub farming: FarmParams,
    pub animals: Vec<AnimalDef>,
    pub leisure: LeisureParams,
    pub speak: SpeakParams,
    pub hash: u64,
}

const CALENDAR: &str = include_str!("../../../data/base/calendar.ron");
const BIOMES: &str = include_str!("../../../data/base/biomes.ron");
const NODES: &str = include_str!("../../../data/base/nodes.ron");
const WORLDGEN: &str = include_str!("../../../data/base/worldgen.ron");
const NEEDS: &str = include_str!("../../../data/base/needs.ron");
const PEOPLE: &str = include_str!("../../../data/base/people.ron");
const GENES: &str = include_str!("../../../data/base/genes.ron");
const SKILLS: &str = include_str!("../../../data/base/skills.ron");
const ITEMS: &str = include_str!("../../../data/base/items.ron");
const ACTIONS: &str = include_str!("../../../data/base/actions.ron");
const NAMES: &str = include_str!("../../../data/base/names.ron");
const LIFE: &str = include_str!("../../../data/base/life.ron");
const WEATHER: &str = include_str!("../../../data/base/weather.ron");
const BUILDINGS: &str = include_str!("../../../data/base/buildings.ron");
const FARMING: &str = include_str!("../../../data/base/farming.ron");
const ANIMALS: &str = include_str!("../../../data/base/animals.ron");
const LEISURE: &str = include_str!("../../../data/base/leisure.ron");
const SPEAK: &str = include_str!("../../../data/base/speak.ron");

fn parse<T: for<'de> Deserialize<'de>>(name: &str, text: &str) -> T {
    ron::from_str(text).unwrap_or_else(|e| panic!("data/base/{name}: {e}"))
}

impl Content {
    /// The base game's content.
    pub fn base() -> Self {
        let mut all = Vec::new();
        for t in [CALENDAR, BIOMES, NODES, WORLDGEN, NEEDS, PEOPLE, GENES, SKILLS, ITEMS, ACTIONS, NAMES, LIFE, WEATHER, BUILDINGS, FARMING, ANIMALS, LEISURE, SPEAK] {
            all.extend_from_slice(t.as_bytes());
        }
        Content {
            calendar: parse("calendar.ron", CALENDAR),
            biomes: parse("biomes.ron", BIOMES),
            nodes: parse("nodes.ron", NODES),
            worldgen: parse("worldgen.ron", WORLDGEN),
            needs: parse("needs.ron", NEEDS),
            people: parse("people.ron", PEOPLE),
            genes: parse("genes.ron", GENES),
            skills: parse("skills.ron", SKILLS),
            items: parse("items.ron", ITEMS),
            actions: parse("actions.ron", ACTIONS),
            names: parse("names.ron", NAMES),
            life: parse("life.ron", LIFE),
            weather: parse("weather.ron", WEATHER),
            buildings: parse("buildings.ron", BUILDINGS),
            farming: parse("farming.ron", FARMING),
            animals: parse("animals.ron", ANIMALS),
            leisure: parse("leisure.ron", LEISURE),
            speak: parse("speak.ron", SPEAK),
            hash: crate::hash::fnv64(&all),
        }
    }

    pub fn biome(&self, id: &str) -> u8 {
        self.biomes.iter().position(|b| b.id == id).unwrap_or_else(|| panic!("no biome '{id}'")) as u8
    }

    pub fn need(&self, id: &str) -> usize {
        self.needs.iter().position(|n| n.id == id).unwrap_or_else(|| panic!("no need '{id}'"))
    }

    pub fn item(&self, id: &str) -> usize {
        self.items.iter().position(|n| n.id == id).unwrap_or_else(|| panic!("no item '{id}'"))
    }

    pub fn skill(&self, id: &str) -> usize {
        self.skills.skills.iter().position(|n| n.id == id).unwrap_or_else(|| panic!("no skill '{id}'"))
    }

    /// Ticks in one year of someone's age.
    pub fn ticks_per_age_year(&self) -> f64 {
        (self.calendar.ticks_per_day() * self.calendar.days_per_year()) as f64 / self.life.age_per_year as f64
    }

    pub fn building(&self, id: &str) -> u16 {
        self.buildings.iter().position(|b| b.id == id).unwrap_or_else(|| panic!("no building '{id}'")) as u16
    }

    pub fn animal(&self, id: &str) -> u16 {
        self.animals.iter().position(|a| a.id == id).unwrap_or_else(|| panic!("no animal '{id}'")) as u16
    }

    /// The share of a stored item that spoils per day: (in the open, in a storehouse).
    pub fn spoil(&self, item: usize) -> (f32, f32) {
        let id = &self.items[item].id;
        self.farming.spoil.iter().find(|(i, _, _)| i == id).map_or((0.0, 0.0), |(_, a, b)| (*a, *b))
    }

    pub fn node_kind(&self, id: &str) -> u16 {
        self.nodes.iter().position(|n| n.id == id).unwrap_or_else(|| panic!("no node kind '{id}'")) as u16
    }
}

/// "#RRGGBB" to bytes.
pub fn hex_colour(s: &str) -> [u8; 3] {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xFF00FF);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

#[cfg(test)]
mod tests {
    #[test]
    fn base_content_parses() {
        let c = super::Content::base();
        assert!(c.biomes.len() >= 10);
        assert!(c.nodes.iter().all(|n| n.densities.iter().all(|(b, _)| c.biomes.iter().any(|x| &x.id == b))));
        for g in &c.actions.gather {
            c.node_kind(&g.node);
            c.item(&g.item);
            c.skill(&g.skill);
        }
        assert_eq!(c.needs.len(), 6);
    }
}
