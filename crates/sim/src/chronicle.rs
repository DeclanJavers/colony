//! The chronicle: what happened, written as history (ARCHITECTURE.md section 4). Built from
//! events as they happen; the History ledger shows it as a timeline. Saved with the world.

use crate::events::Event;
use crate::people::PersonId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Life,
    Social,
    Knowledge,
    World,
    Trouble,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub tick: u64,
    pub kind: Kind,
    pub text: String,
    pub people: Vec<PersonId>,
    /// 0..1: what a short history keeps.
    pub importance: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chronicle {
    pub entries: Vec<Entry>,
    /// Firsts already written ("the first fire", "the first child born here").
    firsts: Vec<String>,
}

impl Chronicle {
    pub fn add(&mut self, tick: u64, kind: Kind, text: String, people: Vec<PersonId>, importance: f32) {
        self.entries.push(Entry { tick, kind, text, people, importance });
    }

    fn first(&mut self, key: &str) -> bool {
        if self.firsts.iter().any(|f| f == key) {
            return false;
        }
        self.firsts.push(key.to_string());
        true
    }

    /// Write history from what just happened.
    pub fn record(&mut self, tick: u64, e: &Event) {
        match e {
            Event::Born { child, name, mother, father, parents } => {
                let first = self.first("birth");
                let text = if first { format!("{name} was born to {parents}, the first child born in the camp.") } else { format!("{name} was born to {parents}.") };
                self.add(tick, Kind::Life, text, vec![*child, *mother, *father], if first { 0.9 } else { 0.6 });
            }
            Event::PersonDied { person, name, cause } => {
                let first = self.first("death");
                let text = if first { format!("{name} died of {cause}, the first death in the camp.") } else { format!("{name} died of {cause}.") };
                self.add(tick, Kind::Life, text, vec![*person], 0.8);
            }
            Event::Partnered { a, b, names } => self.add(tick, Kind::Social, format!("{names} became partners."), vec![*a, *b], 0.6),
            Event::GrewUp { person, name } => self.add(tick, Kind::Life, format!("{name} came of age."), vec![*person], 0.3),
            Event::Argued { a, b, names } => self.add(tick, Kind::Trouble, format!("{names} quarrelled."), vec![*a, *b], 0.15),
            Event::Learned { learner, name, skill, teacher } => {
                self.add(tick, Kind::Knowledge, format!("{name} learned {skill} from {teacher}."), vec![*learner], 0.5);
            }
            Event::SkillLost { skill, last } => {
                self.add(tick, Kind::Knowledge, format!("With {last} gone, nobody living knows {skill}. It is lost."), Vec::new(), 1.0);
            }
            Event::Buried { person, name, by } => self.add(tick, Kind::Life, format!("{by} buried {name}."), vec![*person], 0.3),
            Event::FireLit { person, name }
                if self.first("fire") => {
                    self.add(tick, Kind::World, format!("{name} lit the camp's first fire."), vec![*person], 0.7);
                }
            // Settling down (M6): the firsts are history; the rest is everyday.
            Event::BuildingDone { kind, owner, .. } => {
                let key = format!("built {kind}");
                if self.first(&key) {
                    let text = match kind.as_str() {
                        "field" => "The first field was cleared for grain.".to_string(),
                        "storehouse" => "The camp built its first storehouse, to keep food from spoiling.".to_string(),
                        k => format!("The camp's first {k} was finished."),
                    };
                    self.add(tick, Kind::World, text, owner.iter().copied().collect(), 0.7);
                }
            }
            Event::FieldSown { person, name, .. } if self.first("sown") => {
                self.add(tick, Kind::World, format!("{name} sowed the first field of grain."), vec![*person], 0.8);
            }
            Event::Harvested { person, name, amount, .. } if self.first("harvest") => {
                self.add(tick, Kind::World, format!("{name} brought in the first harvest: {amount} grain."), vec![*person], 0.8);
            }
            Event::CropFailed { .. } if self.first("crop failed") => {
                self.add(tick, Kind::Trouble, "Frost killed a field of grain before it ripened.".to_string(), Vec::new(), 0.5);
            }
            Event::SpearMade { person, name } if self.first("spear") => {
                self.add(tick, Kind::Knowledge, format!("{name} made the camp's first spear."), vec![*person], 0.6);
            }
            Event::HobbyFound { person, name, hobby } => {
                self.add(tick, Kind::Life, format!("{name} took up {hobby}."), vec![*person], 0.35);
            }
            Event::Made { person, name, what } if what.contains("statue") && self.first("statue") => {
                self.add(tick, Kind::World, format!("{name} carved the camp's first statue."), vec![*person], 0.7);
            }
            Event::Made { person, name, what } if what.contains("flower bed") && self.first("flower bed") => {
                self.add(tick, Kind::World, format!("{name} planted the camp's first flower bed."), vec![*person], 0.5);
            }
            Event::SettingChanged { what } => self.add(tick, Kind::World, format!("{what}: the rules of the world changed."), Vec::new(), 0.6),
            Event::Hunted { person, name, animal, .. } if self.first(&format!("hunted {animal}")) => {
                self.add(tick, Kind::World, format!("{name} killed the camp's first {animal}."), vec![*person], 0.7);
            }
            _ => {}
        }
    }
}
