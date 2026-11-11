//! M9: branching a saved world. A fork with branch 0 is the same world; with another branch its
//! chance events go their own way from the moment it was forked, and it stays deterministic.

use sim::{Sim, save};

#[test]
fn a_branched_world_goes_its_own_way() {
    let mut a = Sim::new(5, 256);
    let d = a.content.calendar.ticks_per_day();
    a.run_ticks(d * if cfg!(debug_assertions) { 1 } else { 3 });
    let bytes = save::to_bytes(&a);
    let mut same = save::from_bytes(&bytes).unwrap();
    same.branch(0);
    let mut b1 = save::from_bytes(&bytes).unwrap();
    b1.branch(1);
    let mut b1_again = save::from_bytes(&bytes).unwrap();
    b1_again.branch(1);
    for s in [&mut a, &mut same, &mut b1, &mut b1_again] {
        s.run_ticks(d);
    }
    assert_eq!(a.state_hash(), same.state_hash(), "branch 0 changes nothing");
    assert_ne!(a.state_hash(), b1.state_hash(), "a branch goes its own way");
    assert_eq!(b1.state_hash(), b1_again.state_hash(), "the same branch twice is the same world");
}
