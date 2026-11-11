//! Saves: the world state written with postcard and compressed with zstd. Files are written to a
//! temporary name and renamed when complete, so a crash never leaves half a save. Background saves
//! run on their own thread and are always waited for (dropping the job waits too).

use crate::commands::Command;
use crate::ids::Table;
use crate::sim::Sim;
use crate::world::{Map, Node};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

/// Bumped when the save layout changes.
pub const FORMAT: u32 = 1;

#[derive(Serialize)]
struct SaveRef<'a> {
    format: u32,
    seed: u64,
    content_hash: u64,
    tick: u64,
    map: &'a Map,
    nodes: &'a Table<Node>,
    command_log: &'a [(u64, Command)],
}

#[derive(Deserialize)]
struct SaveOwned {
    format: u32,
    seed: u64,
    content_hash: u64,
    tick: u64,
    map: Map,
    nodes: Table<Node>,
    command_log: Vec<(u64, Command)>,
}

fn snapshot(sim: &Sim) -> SaveRef<'_> {
    SaveRef {
        format: FORMAT,
        seed: sim.seed,
        content_hash: sim.content.hash,
        tick: sim.tick,
        map: &sim.map,
        nodes: &sim.nodes,
        command_log: &sim.command_log,
    }
}

/// The uncompressed world state (also what the state hash is taken over).
pub fn state_bytes(sim: &Sim) -> Vec<u8> {
    postcard::to_allocvec(&snapshot(sim)).expect("world state serialises")
}

/// A complete save file's bytes.
pub fn to_bytes(sim: &Sim) -> Vec<u8> {
    compress(&state_bytes(sim))
}

fn compress(raw: &[u8]) -> Vec<u8> {
    zstd::encode_all(raw, 3).expect("zstd compresses in memory")
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Corrupt(String),
    WrongFormat(u32),
    /// The save was made with different content (data files changed since).
    ContentChanged,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "couldn't read the save: {e}"),
            LoadError::Corrupt(e) => write!(f, "the save is damaged: {e}"),
            LoadError::WrongFormat(v) => write!(f, "the save is format {v}; this build reads format {FORMAT}"),
            LoadError::ContentChanged => write!(f, "the save was made with different game data"),
        }
    }
}

pub fn from_bytes(bytes: &[u8]) -> Result<Sim, LoadError> {
    let raw = zstd::decode_all(bytes).map_err(|e| LoadError::Corrupt(e.to_string()))?;
    let s: SaveOwned = postcard::from_bytes(&raw).map_err(|e| LoadError::Corrupt(e.to_string()))?;
    if s.format != FORMAT {
        return Err(LoadError::WrongFormat(s.format));
    }
    let sim = Sim::from_parts(s.seed, s.tick, s.map, s.nodes, s.command_log);
    if sim.content.hash != s.content_hash {
        return Err(LoadError::ContentChanged);
    }
    Ok(sim)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

pub fn save_to(sim: &Sim, path: &Path) -> io::Result<()> {
    write_atomic(path, &to_bytes(sim))
}

pub fn load_from(path: &Path) -> Result<Sim, LoadError> {
    from_bytes(&std::fs::read(path).map_err(LoadError::Io)?)
}

/// A save in progress on a background thread. `wait` for the result; dropping it waits as well, so
/// quitting never cuts a save short.
pub struct SaveJob {
    handle: Option<JoinHandle<io::Result<()>>>,
    pub path: PathBuf,
}

impl SaveJob {
    pub fn is_done(&self) -> bool {
        self.handle.as_ref().is_none_or(|h| h.is_finished())
    }

    pub fn wait(mut self) -> io::Result<()> {
        self.join()
    }

    fn join(&mut self) -> io::Result<()> {
        match self.handle.take() {
            Some(h) => h.join().unwrap_or_else(|_| Err(io::Error::other("save thread panicked"))),
            None => Ok(()),
        }
    }
}

impl Drop for SaveJob {
    fn drop(&mut self) {
        let _ = self.join();
    }
}

/// Take the state now (fast) and compress and write it on another thread.
pub fn save_background(sim: &Sim, path: &Path) -> SaveJob {
    let raw = state_bytes(sim);
    let p = path.to_path_buf();
    let handle = std::thread::spawn(move || write_atomic(&p, &compress(&raw)));
    SaveJob { handle: Some(handle), path: path.to_path_buf() }
}
