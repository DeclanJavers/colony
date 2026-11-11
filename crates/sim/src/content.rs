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
    pub food_target_days: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Names {
    pub female: Vec<String>,
    pub male: Vec<String>,
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

fn parse<T: for<'de> Deserialize<'de>>(name: &str, text: &str) -> T {
    ron::from_str(text).unwrap_or_else(|e| panic!("data/base/{name}: {e}"))
}

impl Content {
    /// The base game's content.
    pub fn base() -> Self {
        let mut all = Vec::new();
        for t in [CALENDAR, BIOMES, NODES, WORLDGEN, NEEDS, PEOPLE, GENES, SKILLS, ITEMS, ACTIONS, NAMES] {
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
