//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/decompose_depends.rs"]
mod decompose_depends;

#[path = "suites/execution_batch.rs"]
mod execution_batch;

#[path = "suites/execution_report.rs"]
mod execution_report;

#[path = "suites/execution_start_gate.rs"]
mod execution_start_gate;

#[path = "suites/hybrid_session_path.rs"]
mod hybrid_session_path;

#[path = "suites/path_flags.rs"]
mod path_flags;

#[path = "suites/plan_diff.rs"]
mod plan_diff;

#[path = "suites/plan_principles.rs"]
mod plan_principles;

#[path = "suites/plan_relocate.rs"]
mod plan_relocate;

#[path = "suites/session_branch.rs"]
mod session_branch;

#[path = "suites/session_focus.rs"]
mod session_focus;

#[path = "suites/session_focus_path.rs"]
mod session_focus_path;

#[path = "suites/workflow_gates.rs"]
mod workflow_gates;
