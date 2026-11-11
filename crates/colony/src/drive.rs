//! Driving the real game from outside (M10): `colony --drive DIR` reads command files from a
//! folder and plays them through the same input paths as the mouse and keyboard. It's how an
//! agent plays the game blind, and how the menus and saves are tested in the real window.
//!
//! **The protocol.** Write `DIR/cmd-0001.txt` (then `cmd-0002.txt`...), one command per line.
//! Write it to a temporary name and rename it, so the game never reads half a file. When every
//! line has run, the game writes `DIR/done-0001.txt` with one line per command ("ok" or what
//! went wrong). Coordinates are window points from the top left (the size `info` reports).
//!
//! **Commands.**
//! - `click X Y`, `move X Y` (hover), `drag X1 Y1 X2 Y2`, `scroll DY` (over a panel it scrolls;
//!   over the world it zooms, + in, - out)
//! - `key NAME` (Escape, Enter, Space, Backspace, Tab, F, L, T, Y, 1, 2, 3, F3, Left, Right, Up,
//!   Down, W, A, S, D, Z, X), held for one frame
//! - `hold NAME SECONDS` (a key held down: panning with W/A/S/D or the arrows, zooming with Z/X)
//! - `type TEXT` (into whatever is taking text: the seed, or words to someone)
//! - `wait SECONDS` (real time; the game runs on)
//! - `shot PATH` (a PNG of the next frame at window-point size, so a pixel in it is a point you
//!   can click; end the path in "@2x.png" for the full resolution)
//! - `info` (the window size, the date, the speed and who's selected, into the done file)
//! - `quit`

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

pub struct Drive {
    pub dir: PathBuf,
    next: u32,
    queue: VecDeque<String>,
    /// The file being run, and what each command said.
    current: Option<(u32, Vec<String>)>,
    pub until: Option<Instant>,
    /// Frames to let pass before the next command (a click needs one to land).
    pub frames: u32,
    /// A key being held, and when to let go.
    pub held: Option<(KeyCode, Instant)>,
}

/// What the app should do for one command.
pub enum Step {
    Click(f32, f32),
    Move(f32, f32),
    Drag(f32, f32, f32, f32),
    Scroll(f32),
    Key(KeyCode),
    Hold(KeyCode, f32),
    Type(String),
    Shot(PathBuf),
    Info,
    Quit,
}

pub fn key_code(name: &str) -> Option<KeyCode> {
    Some(match name.to_ascii_lowercase().as_str() {
        "escape" | "esc" => KeyCode::Escape,
        "enter" | "return" => KeyCode::Enter,
        "space" => KeyCode::Space,
        "backspace" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "f" => KeyCode::KeyF,
        "l" => KeyCode::KeyL,
        "t" => KeyCode::KeyT,
        "y" => KeyCode::KeyY,
        "w" => KeyCode::KeyW,
        "a" => KeyCode::KeyA,
        "s" => KeyCode::KeyS,
        "d" => KeyCode::KeyD,
        "z" => KeyCode::KeyZ,
        "x" => KeyCode::KeyX,
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        "f3" => KeyCode::F3,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        _ => return None,
    })
}

impl Drive {
    pub fn new(dir: PathBuf) -> Drive {
        let _ = std::fs::create_dir_all(&dir);
        // Carry on after the last file done (a restarted game doesn't redo old commands).
        let mut next = 1;
        while dir.join(format!("done-{next:04}.txt")).exists() {
            next += 1;
        }
        Drive { dir, next, queue: VecDeque::new(), current: None, until: None, frames: 0, held: None }
    }

    /// Note how the last command went.
    pub fn report(&mut self, line: impl Into<String>) {
        if let Some((_, log)) = self.current.as_mut() {
            log.push(line.into());
        }
    }

    /// Write out the file being run (before quitting, so its done file says what happened).
    pub fn flush(&mut self) {
        if let Some((n, log)) = self.current.take() {
            let _ = std::fs::write(self.dir.join(format!("done-{n:04}.txt")), log.join("\n") + "\n");
        }
    }

    /// The next command to run now, if any (None while waiting, or with nothing to do).
    pub fn step(&mut self) -> Option<Step> {
        let now = Instant::now();
        if let Some((_, until)) = self.held
            && now < until
        {
            return None;
        }
        if self.until.is_some_and(|u| now < u) {
            return None;
        }
        self.until = None;
        if self.frames > 0 {
            self.frames -= 1;
            return None;
        }
        loop {
            if self.queue.is_empty() {
                if let Some((n, log)) = self.current.take() {
                    let _ = std::fs::write(self.dir.join(format!("done-{n:04}.txt")), log.join("\n") + "\n");
                    self.next = n + 1;
                }
                let path = self.dir.join(format!("cmd-{:04}.txt", self.next));
                let text = std::fs::read_to_string(&path).ok()?;
                self.queue = text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
                self.current = Some((self.next, Vec::new()));
                if self.queue.is_empty() {
                    continue;
                }
            }
            let line = self.queue.pop_front()?;
            let mut it = line.split_whitespace();
            let cmd = it.next().unwrap_or("").to_ascii_lowercase();
            let nums: Vec<f32> = line.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
            let rest = line.split_once(' ').map_or("", |(_, r)| r).to_string();
            let step = match (cmd.as_str(), nums.len()) {
                ("click", 2) => Step::Click(nums[0], nums[1]),
                ("move", 2) => Step::Move(nums[0], nums[1]),
                ("drag", 4) => Step::Drag(nums[0], nums[1], nums[2], nums[3]),
                ("scroll", 1) => Step::Scroll(nums[0]),
                ("wait", 1) => {
                    self.until = Some(now + Duration::from_secs_f32(nums[0].clamp(0.0, 600.0)));
                    self.report("ok");
                    return None;
                }
                ("key", _) => match key_code(rest.trim()) {
                    Some(k) => Step::Key(k),
                    None => {
                        self.report(format!("unknown key '{}'", rest.trim()));
                        continue;
                    }
                },
                ("hold", _) => {
                    let mut parts = rest.split_whitespace();
                    match (parts.next().and_then(key_code), parts.next().and_then(|v| v.parse::<f32>().ok())) {
                        (Some(k), Some(secs)) => Step::Hold(k, secs.clamp(0.0, 30.0)),
                        _ => {
                            self.report(format!("hold needs a key and seconds: '{line}'"));
                            continue;
                        }
                    }
                }
                ("type", _) => Step::Type(rest),
                ("shot", _) if !rest.trim().is_empty() => Step::Shot(PathBuf::from(rest.trim())),
                ("info", _) => Step::Info,
                ("quit", _) => Step::Quit,
                _ => {
                    self.report(format!("didn't understand '{line}'"));
                    continue;
                }
            };
            return Some(step);
        }
    }
}
