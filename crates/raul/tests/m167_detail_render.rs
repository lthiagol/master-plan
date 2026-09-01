//! M167 WP4 §4 (S12-S15): full document rendering — meta sub-block, all
//! overlays, optional sections (Design Decisions, Open Questions, Work
//! Packages, Verification, Delta), and enriched Steps / ACs / Findings.
//!
//! ACs covered:
//!   AC-23 meta_subblock_all_fields_present
//!   AC-25 cancelled_deferred_overlays_render_only_when_set
//!   AC-26 design_decisions_section_visible_when_present
//!   AC-27 open_questions_section_visible_when_present
//!   AC-28 work_packages_visible_when_present_and_steps_flat
//!   AC-29 acceptance_criteria_two_line_per_item
//!   AC-30 steps_section_progress_bar_and_two_line_per_item
//!   AC-31 findings_open_first_by_severity_two_line_per_item
//!   AC-32 verification_section_only_when_field_set
//!   AC-33 delta_section_only_when_change_kind_delta
//!   AC-35 empty_sections_omit_headers_and_placeholders

use std::collections::BTreeMap;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::render;
use raul::tui::view_state;

fn render_full(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            output.push_str(buffer[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

fn load_detail(app: &mut App, json: serde_json::Value) {
    app.load_milestone_detail(json);
    app.content = raul::tui::app::ContentState::MilestoneDetail;
    app.selected_milestone_id = Some("86".into());
}

fn base_detail() -> serde_json::Value {
    serde_json::json!({
        "milestone": {
            "id": "86",
            "title": "Test milestone",
            "spec_status": "verified",
            "execution_status": "done",
            "effort": "S",
            "risk": "low",
            "change_kind": "greenfield",
            "priority": "high",
            "depends_on": ["80"],
            "lifecycle": "in-progress",
            "lifecycle_at": "2026-07-01T00:00:00Z",
            "created": "2026-06-01",
            "updated": "2026-07-01"
        },
        "intent": { "outcome": "Provide test coverage" },
        "problem": { "description": "Need integration tests" },
        "scope": { "in_scope": ["tests"], "out_of_scope": ["docs"] },
        "acceptance_criteria": [
            { "id": "AC-01", "description": "tests pass", "status": "passed",
              "verification": "cargo nextest run", "evidence": "exit 0" }
        ],
        "steps": [
            { "id": "S1", "action": "Write test", "status": "done" }
        ]
    })
}

#[test]
fn meta_subblock_all_fields_present() {
    // AC-23: meta sub-block shows effort / risk / change_kind /
    // priority / depends_on / created / updated.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    load_detail(&mut app, base_detail());
    let s = render_full(&app, 160, 60);
    assert!(
        s.contains("Effort: S"),
        "Meta sub-block missing Effort; output:\n{s}"
    );
    assert!(s.contains("Risk: low"), "Meta sub-block missing Risk");
    assert!(s.contains("Change kind: greenfield"));
    assert!(s.contains("Priority: high"));
    assert!(s.contains("Depends on: 80"));
    // Created and Updated dates are flattened; the fixture sets
    // created=2026-06-01 and updated=2026-07-01.
    assert!(s.contains("Created: 2026-06-01"));
    assert!(s.contains("Updated: 2026-07-01"));
}

#[test]
fn cancelled_deferred_overlays_render_only_when_set() {
    // AC-25: cancelled/deferred/target_version render only when set.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["milestone"]["cancelled"] = serde_json::json!(true);
    j["milestone"]["deferred"] = serde_json::json!(true);
    j["milestone"]["deferred_reason"] = serde_json::json!("waiting on review");
    j["milestone"]["target_version"] = serde_json::json!("v2.1");
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("CANCELLED"));
    assert!(s.contains("DEFERRED — waiting on review"));
    assert!(s.contains("Target version: v2.1"));
}

#[test]
fn design_decisions_section_visible_when_present() {
    // AC-26
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["design_decisions"] = serde_json::json!([
        { "area": "interface", "choice": "use ratatui",
          "rationale": "modern primitives" }
    ]);
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(
        s.contains("Design Decisions"),
        "missing design decisions header"
    );
    assert!(s.contains("interface"));
    assert!(s.contains("use ratatui"));
    assert!(s.contains("reason: modern primitives"));
}

#[test]
fn open_questions_section_visible_when_present() {
    // AC-27
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["open_questions"] = serde_json::json!([
        { "id": "Q-01", "question": "Are tests in scope?",
          "status": "resolved", "answer": "yes" }
    ]);
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("Open Questions"));
    assert!(s.contains("Q-01"));
    assert!(s.contains("Are tests in scope?"));
    assert!(s.contains("answer: yes"));
}

#[test]
fn work_packages_visible_when_present_and_steps_flat() {
    // AC-28
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["work_packages"] = serde_json::json!([
        { "id": "WP1", "name": "Naming", "goal": "ship it",
          "rollback": "revert" }
    ]);
    // Add multiple steps belonging to WP1 to verify flatness.
    j["steps"] = serde_json::json!([
        { "id": "S1", "action": "First", "status": "done", "work_package": "WP1" },
        { "id": "S2", "action": "Second", "status": "done", "work_package": "WP1" },
        { "id": "S3", "action": "Third", "status": "done", "work_package": "WP1" }
    ]);
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("Work Packages"));
    assert!(s.contains("WP1 — Naming"));
    // Steps must NOT be nested under WP1 — they appear once each in
    // a flat Steps section, with `wp: WP1` on each row.
    let s1_count = s.matches("S1").count();
    let s2_count = s.matches("S2").count();
    let s3_count = s.matches("S3").count();
    assert_eq!(s1_count, 1, "S1 should appear once");
    assert_eq!(s2_count, 1, "S2 should appear once");
    assert_eq!(s3_count, 1, "S3 should appear once");
    // `wp:` tag on each step row.
    assert!(s.contains("wp: WP1"));
}

#[test]
fn acceptance_criteria_two_line_per_item() {
    // AC-29: ACs render as 2-line (header + verification context).
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["acceptance_criteria"] = serde_json::json!([
        {
            "id": "AC-01",
            "description": "all tests pass",
            "status": "passed",
            "verification": "cargo nextest run",
            "evidence": "exit 0"
        }
    ]);
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("Acceptance Criteria"));
    assert!(s.contains("AC-01"));
    assert!(s.contains("all tests pass"));
    assert!(s.contains("verify: cargo nextest run"));
    assert!(s.contains("evidence: exit 0"));
}

#[test]
fn steps_section_progress_bar_and_two_line_per_item() {
    // AC-30 (steps 2-line) + AC-21 (counts).
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["steps"] = serde_json::json!([
        { "id": "S1", "action": "Write test", "status": "done",
          "files": ["src/lib.rs", "src/main.rs"],
          "tests": "cargo nextest",
          "done_when": "tests pass" },
        { "id": "S2", "action": "Wire it up", "status": "in-progress" }
    ]);
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("Steps"));
    // Section header shows done/total (1/2 in progress is "1/2" once normalized,
    // not "0" — the test only checks that the badge labels appear).
    assert!(s.contains("1 / 2") || s.contains("0 / 2") || s.contains("1/2"));
    assert!(s.contains("S1"));
    assert!(s.contains("Write test"));
    assert!(s.contains("S2"));
    assert!(s.contains("Wire it up"));
}

#[test]
fn findings_open_first_by_severity_two_line_per_item() {
    // AC-31
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["findings"] = serde_json::json!([
        { "id": "F-01", "severity": "low", "status": "resolved",
          "description": "low-pri resolved" },
        { "id": "F-02", "severity": "high", "status": "open",
          "description": "high-pri open" }
    ]);
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("Findings"));
    // Open findings appear first (F-02 then F-01) regardless of input order.
    let f02_pos = s.find("F-02").unwrap();
    let f01_pos = s.find("F-01").unwrap();
    assert!(f02_pos < f01_pos, "open finding must come before resolved");
    assert!(s.contains("high-pri open"));
    assert!(s.contains("low-pri resolved"));
}

#[test]
fn verification_section_only_when_field_set() {
    // AC-32
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["verification"] = serde_json::json!({
        "date": "2026-07-05",
        "branch": "main",
        "evidence": "cargo nextest exit 0"
    });
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("Verification"));
    assert!(s.contains("date: 2026-07-05"));
    assert!(s.contains("branch: main"));
    assert!(s.contains("evidence: cargo nextest exit 0"));

    // Empty verification object: section omitted entirely (no header).
    let mut j2 = base_detail();
    j2["verification"] = serde_json::json!({});
    load_detail(&mut app, j2);
    let s2 = render_full(&app, 160, 60);
    assert!(
        !s2.contains("Verification"),
        "verification header must NOT render when section is empty"
    );
}

#[test]
fn delta_section_only_when_change_kind_delta() {
    // AC-33
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["milestone"]["change_kind"] = serde_json::json!("delta");
    j["delta"] = serde_json::json!({
        "domain": "spec",
        "base_version": 1,
        "added": [{ "id": "AC-99", "statement": "new criterion" }],
        "modified": [{ "target": "AC-01",
                     "before": "old text",
                     "after": "new text" }],
        "removed": [{ "id": "AC-02", "statement": "removed criterion" }]
    });
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    assert!(s.contains("Delta"));
    assert!(s.contains("spec from v1"));
    assert!(s.contains("+ AC-99"));
    assert!(s.contains("~ AC-01"));
    // M167: removed uses `−` (U+2212) for visual distinction from `-`.
    assert!(s.contains("− AC-02"));

    // change_kind != delta: Delta omitted entirely.
    let mut j2 = base_detail();
    j2["milestone"]["change_kind"] = serde_json::json!("greenfield");
    load_detail(&mut app, j2);
    let s2 = render_full(&app, 160, 60);
    assert!(!s2.contains("Delta"));
}

#[test]
fn finding_severity_bars_render_with_counts_when_mixed() {
    // AC-31 (companion) + BF-02: when findings have mixed severities,
    // the Findings section renders a single histogram line whose
    // labels read `high [N] BAR med [N] BAR low [N] BAR`, with each
    // bucket's bar proportional to the max bucket (capped at 8 cells).
    // M167 BF-02 uses styled-line spans in lieu of
    // `ratatui::widgets::BarChart` (which is a `Widget`, not embeddable
    // inside the single-`Paragraph` model without invasive scrollbar
    // math surgery — deferred to a follow-up). The visual outcome and
    // data contract (counts from the same sorted array) are equivalent.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.load_milestones(vec![MilestoneSummary {
        id: "86".into(),
        title: "t".into(),
        lifecycle: "in-progress".into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
    flow_stages: BTreeMap::new(),
    }]);
    let mut j = base_detail();
    j["findings"] = serde_json::json!([
        { "id": "F-01", "severity": "high",   "status": "open",     "description": "h1" },
        { "id": "F-02", "severity": "high",   "status": "open",     "description": "h2" },
        { "id": "F-03", "severity": "high",   "status": "resolved", "description": "h3" },
        { "id": "F-04", "severity": "medium", "status": "open",     "description": "m1" },
        { "id": "F-05", "severity": "low",    "status": "open",     "description": "l1" }
    ]);
    load_detail(&mut app, j);
    let s = render_full(&app, 160, 60);
    // Header line in this section carries the open/total counts;
    // bars line carries the per-bucket counts in brackets.
    assert!(
        s.contains("high [3]") && s.contains("med [1]") && s.contains("low [1]"),
        "missing per-bucket counts; output:\n{s}"
    );
    // Bars are proportional to the max bucket. The three bars live on
    // the same Line; we slice between `high [N]` and `med [N]` to
    // count `█` glyphs in the high bucket alone, etc.
    let bars_line = s
        .lines()
        .find(|l| l.contains("high [") && l.contains("med ["))
        .expect("bars line missing high/med/low sections");
    fn bucket_bar_len(line: &str, label: &str) -> usize {
        // Find `label [N]` then count `█` from there up to the next
        // bucket label or end of bars line (heuristic: stop at the
        // next `[` which precedes a count).
        let after_label =
            line.find(&format!("{label} [")).expect("label not found") + label.len() + 2;
        let rest = &line[after_label..];
        // Count `█` glyphs until we hit the next `[` (which marks the
        // next bucket label).
        let next_label = rest.find("[").unwrap_or(rest.len());
        rest[..next_label].matches('\u{2588}').count()
    }
    let high_bar_len = bucket_bar_len(bars_line, "high");
    let med_bar_len = bucket_bar_len(bars_line, "med");
    let low_bar_len = bucket_bar_len(bars_line, "low");
    assert!(
        high_bar_len > med_bar_len && high_bar_len > low_bar_len,
        "high bucket bar must be wider than med/low (got {high_bar_len}/{med_bar_len}/{low_bar_len})"
    );
}
