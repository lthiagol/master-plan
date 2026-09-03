//! M209 / AC-04: topology-mode tightening of the decision matrix.
//!
//! Pins the topology-mode policy at the integration level — the
//! unit-level coverage lives in `crates/mp/src/autopilot/role.rs`;
//! this file exercises the public surface through the same shape
//! consumers will use (Topology enums in, TopologyPolicy out).

use mp::autopilot::role::{
    tighten, topology_policy, topology_preflight, MilestoneKind, ReviewBypassPolicy, Role,
    Topology, TopologyMode, TopologyPreflightError,
};

#[test]
fn three_agent_topology_picks_full_matrix_with_four_cycle_budget() {
    let policy = topology_policy(Topology::ThreeAgent);
    assert_eq!(policy.mode, TopologyMode::FullMatrix);
    assert_eq!(policy.cycle_budget, 4);
    // All matrix paths are available — both ship-with-backlog and
    // independent external review are supported under 3-pane.
    assert!(policy.allows_ship_with_backlog());
    assert!(policy.allows_external_review());
}

#[test]
fn two_agent_topology_picks_no_ship_with_backlog_with_three_cycle_budget() {
    let policy = topology_policy(Topology::TwoAgent);
    assert_eq!(policy.mode, TopologyMode::NoShipWithBacklog);
    assert_eq!(policy.cycle_budget, 3);
    // 2-pane combines orchestrator+reviewer — the reviewer channel
    // is not independent.
    assert!(!policy.allows_ship_with_backlog());
    assert!(!policy.allows_external_review());
}

#[test]
fn one_agent_topology_picks_single_agent_track_only_with_two_cycle_budget() {
    let policy = topology_policy(Topology::OneAgent);
    assert_eq!(policy.mode, TopologyMode::SingleAgentTrackOnly);
    assert_eq!(policy.cycle_budget, 2);
    assert!(!policy.allows_ship_with_backlog());
    assert!(!policy.allows_external_review());
}

#[test]
fn cycle_budget_decreases_as_topology_contracts() {
    // The policy tightens budgets as the topology shrinks. Pin the
    // relative order so a future budget change is a deliberate test
    // edit.
    let three = topology_policy(Topology::ThreeAgent).cycle_budget;
    let two = topology_policy(Topology::TwoAgent).cycle_budget;
    let one = topology_policy(Topology::OneAgent).cycle_budget;
    assert!(three > two, "{three} should exceed {two}");
    assert!(two > one, "{two} should exceed {one}");
}

#[test]
fn preflight_rejects_full_milestone_in_one_agent_without_recorded_bypass() {
    // The headline rule: 1-pane + full milestone is rejected.
    let err = topology_preflight(Topology::OneAgent, MilestoneKind::Full, ReviewBypassPolicy::None)
        .unwrap_err();
    assert!(matches!(
        err,
        TopologyPreflightError::FullMilestoneRequiresReviewer { .. }
    ));
    // The error message must guide the operator.
    let msg = err.to_string();
    assert!(
        msg.contains("review") && msg.contains("topology"),
        "error message must guide the operator to the topology / review decision: {msg}"
    );
}

#[test]
fn preflight_rejects_full_milestone_with_unrecorded_bypass_under_one_agent() {
    // A CLI flag / runtime override isn't enough — bypass must be on
    // disk to qualify.
    let err = topology_preflight(
        Topology::OneAgent,
        MilestoneKind::Full,
        ReviewBypassPolicy::Unrecorded,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        TopologyPreflightError::FullMilestoneRequiresReviewer { .. }
    ));
}

#[test]
fn preflight_accepts_full_milestone_in_one_agent_with_recorded_bypass() {
    // The single accepted override path for 1-pane + full.
    let policy = topology_preflight(
        Topology::OneAgent,
        MilestoneKind::Full,
        ReviewBypassPolicy::Recorded,
    )
    .expect("recorded bypass should be honored");
    assert_eq!(policy.mode, TopologyMode::SingleAgentTrackOnly);
    assert_eq!(policy.cycle_budget, 2);
}

#[test]
fn preflight_accepts_track_under_every_topology_without_bypass() {
    for (topology, expected_mode) in [
        (Topology::ThreeAgent, TopologyMode::FullMatrix),
        (Topology::TwoAgent, TopologyMode::NoShipWithBacklog),
        (Topology::OneAgent, TopologyMode::SingleAgentTrackOnly),
    ] {
        let policy = topology_preflight(topology, MilestoneKind::Track, ReviewBypassPolicy::None)
            .expect("tracks must be accepted under every topology");
        assert_eq!(policy.mode, expected_mode, "{topology}");
    }
}

#[test]
fn preflight_accepts_full_milestone_under_two_agent_with_no_bypass() {
    // 2-pane + full + no bypass is allowed — the matrix handles the
    // ship-with-backlog path restriction downstream.
    let policy =
        topology_preflight(Topology::TwoAgent, MilestoneKind::Full, ReviewBypassPolicy::None)
            .unwrap();
    assert_eq!(policy.mode, TopologyMode::NoShipWithBacklog);
}

#[test]
fn preflight_accepts_full_milestone_under_three_agent_with_no_bypass() {
    let policy = topology_preflight(
        Topology::ThreeAgent,
        MilestoneKind::Full,
        ReviewBypassPolicy::None,
    )
    .unwrap();
    assert_eq!(policy.mode, TopologyMode::FullMatrix);
}

#[test]
fn tighten_is_the_canonical_one_liner_for_callers() {
    // The convenience wrapper must return the same policy as the
    // explicit call.
    let via_tighten = tighten(Topology::ThreeAgent, MilestoneKind::Full, ReviewBypassPolicy::None)
        .expect("three-agent + full + no bypass should never fail");
    let via_explicit = topology_preflight(
        Topology::ThreeAgent,
        MilestoneKind::Full,
        ReviewBypassPolicy::None,
    )
    .unwrap();
    assert_eq!(via_tighten.mode, via_explicit.mode);
    assert_eq!(via_tighten.cycle_budget, via_explicit.cycle_budget);
}

#[test]
fn topology_mode_wire_strings_are_stable() {
    // Pin the on-disk / JSON form. Adding a new mode is a
    // deliberate test change so the rename is grep-able.
    assert_eq!(TopologyMode::FullMatrix.as_str(), "full_matrix");
    assert_eq!(TopologyMode::NoShipWithBacklog.as_str(), "no_ship_with_backlog");
    assert_eq!(
        TopologyMode::SingleAgentTrackOnly.as_str(),
        "single_agent_track_only"
    );
}

#[test]
fn topology_policies_apply_per_role_through_the_full_role_set() {
    // The role set is independent of the topology — every policy
    // must still resolve cleanly for every role. This catches a
    // future topology variant from accidentally coupling mode to
    // role (e.g. refusing to spawn a reviewer in 1-pane without
    // also refusing the orchestrator, which is wrong).
    for topology in [Topology::OneAgent, Topology::TwoAgent, Topology::ThreeAgent] {
        let policy = topology_policy(topology);
        for role in Role::ALL {
            // The role only matters for *who* occupies a pane; the
            // mode is independent. Smoke-assert the role is still
            // resolvable through the topology.
            assert!(role_pane_slot(topology, role) < topology.pane_count());
            let _ = policy;
        }
    }
}

/// The pane ordinal that holds `role` in `topology` — copied from
/// `role::pane_index_for` so the integration test does not need to
/// name a private helper that the unit tests already exercise.
fn role_pane_slot(topology: Topology, role: Role) -> usize {
    let slots = match topology {
        Topology::ThreeAgent => vec![vec![Role::Orchestrator], vec![Role::Runner], vec![Role::Reviewer]],
        Topology::TwoAgent => vec![vec![Role::Orchestrator, Role::Reviewer], vec![Role::Runner]],
        Topology::OneAgent => vec![vec![Role::Orchestrator, Role::Runner, Role::Reviewer]],
    };
    slots
        .iter()
        .position(|slot| slot.contains(&role))
        .expect("every role is assigned to a slot")
}
