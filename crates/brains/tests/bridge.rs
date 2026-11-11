//! The subagent bridge's file protocol, answered by hand (no model): batches are written, good
//! answers are used, bad ones fall back to the rules brain and the trace says so, and the sim
//! waits while a batch is out.

use brains::SubagentBrain;
use sim::Sim;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("colony-bridge-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn batches_go_out_and_answers_come_back_checked() {
    let dir = temp_dir("answers");
    let mut sim = Sim::new(3, 256);
    sim.mode = "lockstep".into();
    let mut b = SubagentBrain::new(&dir).unwrap();
    b.window = 5;
    sim.brains.map.insert("subagent".into(), Box::new(b));
    let ids: Vec<_> = sim.people.iter().map(|(id, _)| id).collect();
    for id in &ids {
        sim.set_brain(*id, "subagent");
    }
    // Step until a batch goes out; the people wait (no actions) meanwhile.
    let mut n = 0;
    while sim.brains_in_flight() == 0 {
        sim.step();
        n += 1;
        assert!(n < 50, "no batch went out");
    }
    let req: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("req-0.json")).unwrap()).unwrap();
    let items = req["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items[0]["text"].as_str().unwrap().contains("What do you do next?"));
    // Answer: the first item well, the second with an impossible number, the rest not at all.
    let mut answers = vec![serde_json::json!({"id": items[0]["id"], "choice": 1, "reason": "I'm thirsty."})];
    if items.len() > 1 {
        answers.push(serde_json::json!({"id": items[1]["id"], "choice": 999, "reason": "?"}));
    }
    // Wrapped in words and a fence, as a model might.
    let text = format!("Here you go:\n```json\n{}\n```", serde_json::json!({ "answers": answers }));
    std::fs::write(dir.join("res-0.json"), text).unwrap();
    let before = sim.tick;
    sim.poll_brains();
    assert_eq!(sim.tick, before, "polling doesn't move time");
    assert_eq!(sim.brains_in_flight(), 0, "the answered batch is taken");
    // Each answered person has started something, and their decision is traced.
    let mut subagent = 0;
    let mut fallback = 0;
    for (_, p) in sim.people.iter() {
        if let Some(t) = &p.open_trace {
            assert_eq!(t.brain, "subagent");
            if t.fallback {
                fallback += 1;
                assert_eq!(t.choice, t.rules_choice, "a fallback takes the rules brain's choice");
            } else {
                subagent += 1;
                assert_eq!(t.reason.as_deref(), Some("I'm thirsty."));
                let possible: Vec<usize> = t.request.options.iter().enumerate().filter(|(_, o)| o.possible).map(|(i, _)| i).collect();
                assert_eq!(t.choice, possible[0], "choice 1 is the first possible option");
            }
        }
    }
    assert_eq!(subagent, 1);
    assert_eq!(fallback, items.len() - 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stop_file_stops_the_brain() {
    let dir = temp_dir("stop");
    let mut sim = Sim::new(3, 256);
    sim.brains.map.insert("subagent".into(), Box::new(SubagentBrain::new(&dir).unwrap()));
    std::fs::write(dir.join("stop.json"), r#"{"reason": "usage limit reached"}"#).unwrap();
    sim.step();
    assert_eq!(sim.brains_stopped(), Some(("subagent".into(), "usage limit reached".into())));
    let _ = std::fs::remove_dir_all(&dir);
}
