//! M211 / AC-01: deterministic task-assignment rendering.
//!
//! The renderer is the single read path for orchestrator-to-runner /
//! orchestrator-to-reviewer dispatch. Its output (an argv vector) is
//! what gets handed to `herdr agent prompt`, so the wire shape MUST
//! be deterministic — the same [`TaskAssignment`] must always produce
//! the same argv, byte for byte.
//!
//! This test file pins the wire shape so a future refactor of
//! `render_task_text` cannot silently change how the runner or
//! reviewer receives its task description. The "golden" vector
//! below is the contract; the renderer is asserted to match it.

use mp::autopilot::task_assign::{
    build_assignment_argv, render_task_text, RoleDirection, TaskAssignment,
};

/// Minimal payload — every required field set, no optional
/// evidence refs / boundary reminders.
fn minimal_payload() -> TaskAssignment {
    TaskAssignment::new(
        "sess-alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToRunner,
        "%2",
        "execute cycle 1",
    )
}

fn reviewer_payload() -> TaskAssignment {
    TaskAssignment::new(
        "sess-alpha",
        "M211",
        1,
        RoleDirection::OrchestratorToReviewer,
        "%3",
        "review cycle 1",
    )
    .with_evidence_ref("cargo nextest run -p mp --test foo")
    .with_evidence_ref("cargo clippy --all-targets -- -D warnings")
    .with_boundary_reminder("report via mp autopilot session transition")
}

#[test]
fn golden_argv_shape_for_runner_dispatch() {
    let argv = build_assignment_argv(&minimal_payload());
    // Wire shape: [agent, prompt, <target_pane>, <task_text>]
    // herdr's `agent prompt <target> <text>` takes the target first
    // and the text as the final argv element. The renderer must
    // never reorder these positions; downstream consumers (and
    // logs) rely on argv[2] being the target pane.
    assert_eq!(argv.len(), 4, "argv must be exactly 4 elements");
    assert_eq!(argv[0], "agent", "argv[0] must be the herdr subcommand root");
    assert_eq!(argv[1], "prompt", "argv[1] must select prompt subcommand");
    assert_eq!(
        argv[2], "%2",
        "argv[2] must be the target pane id (runner pane)"
    );
    assert!(
        argv[3].contains("session=sess-alpha"),
        "argv[3] must include session id, got {:?}",
        argv[3]
    );
    assert!(
        argv[3].contains("milestone=M211"),
        "argv[3] must include milestone id, got {:?}",
        argv[3]
    );
    assert!(
        argv[3].contains("cycle=1"),
        "argv[3] must include cycle, got {:?}",
        argv[3]
    );
    assert!(
        argv[3].contains("direction=orchestrator-to-runner"),
        "argv[3] must include direction, got {:?}",
        argv[3]
    );
    assert!(
        argv[3].ends_with("execute cycle 1"),
        "argv[3] must end with the task body, got {:?}",
        argv[3]
    );
}

#[test]
fn golden_argv_shape_for_reviewer_dispatch() {
    let argv = build_assignment_argv(&reviewer_payload());
    assert_eq!(argv.len(), 4);
    assert_eq!(argv[0], "agent");
    assert_eq!(argv[1], "prompt");
    assert_eq!(
        argv[2], "%3",
        "argv[2] must be the reviewer pane id"
    );
    let text = &argv[3];
    assert!(text.contains("session=sess-alpha"));
    assert!(text.contains("direction=orchestrator-to-reviewer"));
    // Optional sections, when populated, must appear in the
    // rendered text.
    assert!(text.contains("evidence_refs:"));
    assert!(text.contains("cargo nextest run -p mp --test foo"));
    assert!(text.contains("cargo clippy --all-targets -- -D warnings"));
    assert!(text.contains("boundary_reminders:"));
    assert!(text.contains("mp autopilot session transition"));
}

#[test]
fn golden_argv_is_deterministic_across_calls() {
    let p = minimal_payload();
    let a = build_assignment_argv(&p);
    let b = build_assignment_argv(&p);
    let c = build_assignment_argv(&p);
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn golden_argv_changes_when_payload_changes() {
    // Sanity: the renderer is not a constant function. Two
    // different payloads must produce two different argvs.
    let runner = build_assignment_argv(&minimal_payload());
    let reviewer = build_assignment_argv(&reviewer_payload());
    assert_ne!(
        runner, reviewer,
        "different payloads must produce different argvs"
    );
}

#[test]
fn golden_argv_carries_no_shell_metacharacters_as_separators() {
    // argv is passed via Command::args, not through a shell, so no
    // quoting is required. The pin is that argv elements are
    // opaque strings — the renderer does NOT inject `&&`, `;`,
    // `|`, etc. as separators. The presence of a metachar in the
    // rendered text is fine if the payload itself contained one
    // (which validation rejects upstream), but the renderer
    // never concatenates argv elements.
    let argv = build_assignment_argv(&minimal_payload());
    for (i, elem) in argv.iter().enumerate() {
        // argv[3] is the rendered text and may contain arbitrary
        // characters from the task body — but it must be a single
        // argv element (one entry in the vector), not a
        // shell-concatenated string.
        if i == 3 {
            continue;
        }
        for c in elem.chars() {
            assert!(
                !matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<'),
                "argv[{i}] contains shell separator {c:?}: {elem:?}"
            );
        }
    }
}

#[test]
fn golden_render_task_text_is_byte_stable() {
    // Pin the byte-level output of render_task_text for the minimal
    // payload. Future refactors that change the format will need
    // to update this golden string — making the change explicit.
    let text = render_task_text(&minimal_payload());
    let expected = "session=sess-alpha milestone=M211 cycle=1 direction=orchestrator-to-runner\nexecute cycle 1";
    assert_eq!(
        text, expected,
        "render_task_text output drifted from the golden wire shape"
    );
}

#[test]
fn golden_render_task_text_appends_evidence_and_reminders() {
    let p = reviewer_payload();
    let text = render_task_text(&p);
    // Header is byte-identical to the minimal case (modulo
    // direction + task body).
    assert!(text.starts_with(
        "session=sess-alpha milestone=M211 cycle=1 direction=orchestrator-to-reviewer\nreview cycle 1"
    ));
    // Then a blank line, the evidence refs section, a blank line,
    // the boundary reminders section.
    assert!(text.contains("\n\nevidence_refs:\n"));
    assert!(text.contains("- cargo nextest run -p mp --test foo\n"));
    assert!(text.contains("- cargo clippy --all-targets -- -D warnings\n"));
    assert!(text.contains("\nboundary_reminders:\n"));
    assert!(text.contains("- report via mp autopilot session transition\n"));
}

#[test]
fn golden_render_task_text_omits_empty_optional_sections() {
    // When the payload has no evidence refs and no boundary
    // reminders, the rendered text must NOT contain those section
    // headers — they would add noise to the agent's prompt.
    let text = render_task_text(&minimal_payload());
    assert!(
        !text.contains("evidence_refs:"),
        "empty evidence_refs section must not be rendered"
    );
    assert!(
        !text.contains("boundary_reminders:"),
        "empty boundary_reminders section must not be rendered"
    );
}

#[test]
fn golden_role_direction_serialization_matches_wire_form() {
    // The wire form of RoleDirection is kebab-case; the goldens
    // above rely on this. Pin it so a future serde rename does
    // not break the contract silently.
    let json = serde_json::to_string(&RoleDirection::OrchestratorToRunner).unwrap();
    assert_eq!(json, "\"orchestrator-to-runner\"");
    let json = serde_json::to_string(&RoleDirection::OrchestratorToReviewer).unwrap();
    assert_eq!(json, "\"orchestrator-to-reviewer\"");
    let back: RoleDirection = serde_json::from_str("\"orchestrator-to-reviewer\"").unwrap();
    assert_eq!(back, RoleDirection::OrchestratorToReviewer);
}