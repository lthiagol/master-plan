mod complete;
mod io;
mod spec;

pub(crate) use complete::format_gate_errors;
pub use complete::{
    block_milestone, complete_milestone, criterion_fail, criterion_pass, defer_milestone,
    reopen_milestone, set_execution_status, unblock_milestone,
};
pub(crate) use complete::{collect_set_execution_status_gates, event_for_execution_status};

pub use io::{
    archive_milestone, delete_milestone, load_milestone_by_id, load_milestone_path,
    next_fragment_id, purge_archived_milestone, restore_archived_milestone,
    with_milestone_mut_unlocked, write_milestone_synced,
};
#[allow(unused_imports)]
pub(crate) use io::{
    strip_deferred_reason_from_path, strip_deferred_reason_in_plan, strip_dropped_keys_from_path,
    strip_dropped_keys_in_plan,
};

pub(crate) use spec::apply_migrate_raw;
pub(crate) use spec::apply_transition;
pub use spec::{
    add_depends_on, add_depends_on_with_graph, apply_spec_status, apply_spec_status_with_gates,
    approve_milestone, build_depends_on_graph, create_from_handoff, create_milestone,
    criterion_add, criterion_bulk_update, criterion_list, criterion_remove, criterion_show,
    criterion_update, depends_on_creates_cycle_in_graph, design_decision_add,
    design_decision_remove, design_decision_update, gate_errors_for_spec_status, question_add,
    question_resolve, read_create_input, read_update_input, remove_depends_on, set_lifecycle,
    set_lifecycle_preview, set_priority, set_priority_preview, set_target_version,
    spec_status_allows_steps, split_milestone, update_milestone, warn_dangling_deps,
    ApplySpecStatusResult, CreateAcceptanceCriterion, CreateMilestoneInput, UpdateMilestoneInput,
    DROPPED_CEREMONY_KEYS, SPEC_STATUSES, VALID_PRIORITIES,
};
