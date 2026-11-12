//! Brains that live outside the sim (ARCHITECTURE.md section 6). The sim always has the rules
//! brain; these plug in beside it.
//!
//! - `bridge`: the subagent bridge (section 8). Decisions go out as files, Claude Haiku answers
//!   them (as Claude Code subagents, or through `tools/haiku_driver`), and the answers come back
//!   as files. No API key: it runs on the user's Claude plan.
//! - `scorer`: a small model over the decision's named numbers, trained on a teacher's choices
//!   (Haiku or rules), run natively.
//! - `text`: the text model, reading the decision's words like Haiku does, run through ONNX
//!   Runtime.

pub mod bridge;
pub mod scorer;
pub mod text;

pub use bridge::SubagentBrain;
pub use scorer::{Scorer, ScorerBrain};
pub use text::TextBrain;
