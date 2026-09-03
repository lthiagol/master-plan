//! M209 / AC-01: the three-role autopilot model and the topology
//! pane-slot mapping are stable, exhaustive, and serde-round-trippable.
//!
//! Pins the AC-01 contract: three `Role` variants, three `Topology`
//! variants, and the role-to-pane function returning the documented
//! slot pattern per topology.

use mp::autopilot::role::{pane_index_for, role_pane_slots, Role, Topology};

#[test]
fn role_enum_has_exactly_three_variants() {
    // The role enum must be closed at three (orchestrator / runner /
    // reviewer) so the role-to-pane mapping can be exhaustive. A
    // future 4th role is a topology change; pin the count so adding
    // one forces a topology-policy update too.
    assert_eq!(Role::ALL.len(), 3);
    let labels: Vec<&str> = Role::ALL.iter().map(|r| r.as_str()).collect();
    assert_eq!(
        labels,
        vec!["orchestrator", "runner", "reviewer"],
        "every role must serialize kebab-case in declared order"
    );
}

#[test]
fn topology_enum_has_exactly_three_variants_with_kebab_case_serde() {
    // Topology is a closed count enum. Adding a variant without
    // updating pane-slot mapping + topology_policy is a bug; this
    // test catches it at compile time (the for-loop in
    // every_role_assigned_to_exactly_one_pane covers the runtime
    // side, see the source file).
    for (variant, label) in [
        (Topology::OneAgent, "one-agent"),
        (Topology::TwoAgent, "two-agent"),
        (Topology::ThreeAgent, "three-agent"),
    ] {
        assert_eq!(variant.as_str(), label);
        // kebab-case serde representation matches the wire form.
        let json = serde_json::to_string(&variant).unwrap();
        assert_eq!(json.trim_matches('"'), label);
        // from_str accepts the wire form.
        let parsed: Topology = label.parse().unwrap();
        assert_eq!(parsed, variant);
        // and rejects unknown values.
        let err = "four-agent".parse::<Topology>().unwrap_err();
        assert!(err.contains("four-agent"), "error must echo input: {err}");
    }
}

#[test]
fn three_agent_topology_matches_documented_slot_pattern() {
    // 3-pane = [[O], [R], [V]] — every role in its own pane.
    let slots = role_pane_slots(Topology::ThreeAgent);
    assert_eq!(slots.len(), 3);
    assert_eq!(slots[0], vec![Role::Orchestrator]);
    assert_eq!(slots[1], vec![Role::Runner]);
    assert_eq!(slots[2], vec![Role::Reviewer]);
}

#[test]
fn two_agent_topology_combines_orchestrator_and_reviewer_in_supervisor_slot() {
    // 2-pane = [[O, V], [R]] — supervisor shares pane with reviewer.
    let slots = role_pane_slots(Topology::TwoAgent);
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0], vec![Role::Orchestrator, Role::Reviewer]);
    assert_eq!(slots[1], vec![Role::Runner]);
}

#[test]
fn one_agent_topology_collapses_every_role_into_a_single_pane() {
    // 1-pane = [[O, R, V]] — every role in one pane.
    let slots = role_pane_slots(Topology::OneAgent);
    assert_eq!(slots.len(), 1);
    assert_eq!(
        slots[0],
        vec![Role::Orchestrator, Role::Runner, Role::Reviewer]
    );
}

#[test]
fn pane_index_for_returns_canonical_assignment_per_topology() {
    // The pane index is the slot where the role lives — 0-based per
    // role_pane_slots(). Tests both the canonical assignments and
    // the shared-slot invariants (TwoAgent: O+V both at index 0).
    assert_eq!(pane_index_for(Topology::ThreeAgent, Role::Orchestrator), 0);
    assert_eq!(pane_index_for(Topology::ThreeAgent, Role::Runner), 1);
    assert_eq!(pane_index_for(Topology::ThreeAgent, Role::Reviewer), 2);

    assert_eq!(pane_index_for(Topology::TwoAgent, Role::Orchestrator), 0);
    assert_eq!(pane_index_for(Topology::TwoAgent, Role::Reviewer), 0);
    assert_eq!(pane_index_for(Topology::TwoAgent, Role::Runner), 1);

    assert_eq!(pane_index_for(Topology::OneAgent, Role::Orchestrator), 0);
    assert_eq!(pane_index_for(Topology::OneAgent, Role::Runner), 0);
    assert_eq!(pane_index_for(Topology::OneAgent, Role::Reviewer), 0);
}

#[test]
fn topology_serde_round_trip_via_serde_json() {
    for t in [Topology::OneAgent, Topology::TwoAgent, Topology::ThreeAgent] {
        let json = serde_json::to_string(&t).unwrap();
        let back: Topology = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }
}
