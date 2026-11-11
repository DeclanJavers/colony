//! Typed, generational ids into tables. A stale id (its slot reused since) finds nothing instead of
//! the wrong thing. Free slots are reused last-freed-first, so the order is deterministic.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Id<T> {
    pub index: u32,
    pub generation: u32,
    #[serde(skip)]
    _t: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    fn new(index: u32, generation: u32) -> Self {
        Id { index, generation, _t: PhantomData }
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> {
    fn eq(&self, o: &Self) -> bool {
        self.index == o.index && self.generation == o.generation
    }
}
impl<T> Eq for Id<T> {}
impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl<T> Ord for Id<T> {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        (self.index, self.generation).cmp(&(o.index, o.generation))
    }
}
impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.index.hash(h);
        self.generation.hash(h);
    }
}
impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}v{}", self.index, self.generation)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: u32,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Table { slots: Vec::new(), free: Vec::new(), len: 0 }
    }
}

impl<T> Table<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, value: T) -> Id<T> {
        self.len += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.generation += 1;
            slot.value = Some(value);
            Id::new(index, slot.generation)
        } else {
            self.slots.push(Slot { generation: 0, value: Some(value) });
            Id::new(self.slots.len() as u32 - 1, 0)
        }
    }

    pub fn remove(&mut self, id: Id<T>) -> Option<T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation || slot.value.is_none() {
            return None;
        }
        self.len -= 1;
        self.free.push(id.index);
        slot.value.take()
    }

    pub fn get(&self, id: Id<T>) -> Option<&T> {
        self.slots.get(id.index as usize).filter(|s| s.generation == id.generation)?.value.as_ref()
    }

    pub fn get_mut(&mut self, id: Id<T>) -> Option<&mut T> {
        self.slots.get_mut(id.index as usize).filter(|s| s.generation == id.generation)?.value.as_mut()
    }

    /// The live id at a slot index, if any.
    pub fn id_at(&self, index: u32) -> Option<Id<T>> {
        let slot = self.slots.get(index as usize)?;
        slot.value.as_ref().map(|_| Id::new(index, slot.generation))
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Live entries in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.value.as_ref().map(|v| (Id::new(i as u32, s.generation), v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_ids_find_nothing() {
        let mut t: Table<&str> = Table::new();
        let a = t.insert("a");
        assert_eq!(t.remove(a), Some("a"));
        let b = t.insert("b");
        assert_eq!(a.index, b.index);
        assert_eq!(t.get(a), None);
        assert_eq!(t.get(b), Some(&"b"));
        assert_eq!(t.len(), 1);
    }
}
