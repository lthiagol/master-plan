//! M185 AC-05: title bar chip for Milestones filter.
//! M204 AC-02: debounced sort/filter writes (test names pinned
//! here per the S2 step's `tests` field).

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, ContentState, Lane, MilestoneSummary, SortKey};
use raul::tui::render;
use raul::tui::view_state;
use serde_json::json;
use std::collections::BTreeMap;

fn render_header(app: &App) -> String {
    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut row = String::new();
    for x in 0..buf.area().width {
        row.push_str(buf[(x, 0)].symbol());
    }
    row
}

#[test]
fn chip_all_and_filtered() {
    let mut app = App::new();
    app.load_milestones(vec![
        MilestoneSummary {
            id: "01".into(),
            title: "a".into(),
            lifecycle: "approved".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
        MilestoneSummary {
            id: "02".into(),
            title: "b".into(),
            lifecycle: "in-progress".into(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".into(),
            updated: String::new(),
            created: String::new(),
            cancelled: false,
            cancelled_at: None,
            cancel_reason: None,
            flow_stages: BTreeMap::new(),
        },
    ]);
    app.select_lane(Lane::Milestones);

    let header = render_header(&app);
    assert!(
        header.contains("All (2)") || header.contains("All (2"),
        "empty filter chip; got {header:?}"
    );

    app.set_lifecycle_filter(["approved".to_string()].into_iter().collect());
    let header = render_header(&app);
    assert!(
        header.contains("approved") && header.contains("(1)"),
        "filtered chip; got {header:?}"
    );
}

// ─── M202 S17 + S18 + S19: MilestoneDetail Stages section ────────────────
//
// AC-14: Stages section lists all 12 mp-flow stages in canonical
// order with status icon, label, and timestamp.
// AC-15: overlay sub-line appears for cancelled/blocked/remediation.
// AC-14 / S19: detail header shows the Stage cell.

#[test]
fn stages_section_renders_twelve_rows() {
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Sample".to_string(),
        lifecycle: "complete".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    // Inject a synthetic detail with an empty flow_stages so the
    // Stages section renders the legacy "pending (unknown)" path.
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01",
            "title": "Sample",
            "lifecycle": "complete",
            "spec_status": "verified",
            "execution_status": "done",
            "blocked": false,
            "cancelled": false,
            "remediation_pre_state": null,
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "flow_stages": {}
        },
        "intent": {"outcome": "x"},
        "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [],
        "design_decisions": [],
        "open_questions": [],
        "work_packages": [],
        "steps": [],
        "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    // Section header must render.
    assert!(
        flat.contains("Stages"),
        "Stages section header must render; got: {flat}"
    );
    // Every 12 stage slugs must appear.
    for slug in [
        "draft",
        "groom",
        "specify",
        "approve",
        "execute",
        "self-review",
        "complete",
        "external-review",
        "remediate",
        "re-review",
        "document",
        "hand-off",
    ] {
        assert!(
            flat.contains(slug),
            "Stages section must include {slug}; got: {flat}"
        );
    }
}

#[test]
fn stages_section_order_is_canonical() {
    // The 12 stages must appear in canonical order (draft first,
    // hand-off last). Verifying the first occurrence of each slug
    // in the rendered buffer confirms the canonical-order contract.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Sample".to_string(),
        lifecycle: "complete".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01",
            "title": "Sample",
            "lifecycle": "complete",
            "spec_status": "verified",
            "execution_status": "done",
            "blocked": false,
            "cancelled": false,
            "remediation_pre_state": null,
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "flow_stages": {}
        },
        "intent": {"outcome": "x"},
        "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [],
        "design_decisions": [],
        "open_questions": [],
        "work_packages": [],
        "steps": [],
        "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    let slugs = [
        "draft",
        "groom",
        "specify",
        "approve",
        "execute",
        "self-review",
        "complete",
        "external-review",
        "remediate",
        "re-review",
        "document",
        "hand-off",
    ];
    let mut last_pos: Option<usize> = None;
    for slug in slugs {
        let pos = flat.find(slug).unwrap_or_else(|| panic!("{slug} missing"));
        if let Some(p) = last_pos {
            assert!(
                pos > p,
                "{slug} must come after the previous stage in canonical order; got pos={pos} last_pos={p}"
            );
        }
        last_pos = Some(pos);
    }
}

#[test]
fn stages_section_sits_between_meta_and_g14() {
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Sample".to_string(),
        lifecycle: "complete".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Sample", "lifecycle": "complete",
            "spec_status": "verified", "execution_status": "done",
            "blocked": false, "cancelled": false,
            "remediation_pre_state": null,
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {}
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    // Find first "Meta" (the section header) and "Stages" and
    // "G14" positions; Stages must sit between Meta and G14.
    let meta_pos = flat.find("Meta").expect("Meta header");
    let stages_pos = flat.find("Stages").expect("Stages header");
    let g14_pos = flat.find("G14").expect("G14 line");
    assert!(meta_pos < stages_pos, "Meta must come before Stages");
    assert!(stages_pos < g14_pos, "Stages must come before G14");
}

#[test]
fn status_icons_match_enum() {
    // Pin the icon-to-status mapping the S17 renderer emits.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Sample".to_string(),
        lifecycle: "complete".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    // flow_stages with a mix of statuses to assert each icon.
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Sample", "lifecycle": "complete",
            "spec_status": "verified", "execution_status": "done",
            "blocked": false, "cancelled": false,
            "remediation_pre_state": null,
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {
                "draft": {"status": "done", "at": "2026-08-01T00:00:00Z"},
                "groom": {"status": "done", "at": "2026-08-02T00:00:00Z"},
                "specify": {"status": "done", "at": "2026-08-03T00:00:00Z"},
                "approve": {"status": "done", "at": "2026-08-04T00:00:00Z"},
                "execute": {"status": "done", "at": "2026-08-05T00:00:00Z"},
                "self-review": {"status": "in_progress", "at": "2026-08-06T00:00:00Z"},
                "complete": {"status": "pending"},
                "external-review": {"status": "pending"},
                "remediate": {"status": "skipped"},
                "re-review": {"status": "pending"},
                "document": {"status": "pending"},
                "hand-off": {"status": "pending"}
            }
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    // The done stages (draft, groom, specify, approve, execute)
    // must carry a ✓ icon; the in_progress stage (self-review)
    // must carry a ● icon; the skipped stage (remediate) must
    // carry a ⊘ icon. Each icon appears in the rendered buffer.
    assert!(flat.contains('✓'), "done icon ✓ must render");
    assert!(flat.contains('●'), "in_progress icon ● must render");
    assert!(flat.contains('⊘'), "skipped icon ⊘ must render");
}

#[test]
fn skipped_icon_for_cancelled_stages() {
    // AC-15: cancelled milestone → all non-done stages show ⊘.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Cancelled".to_string(),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: true,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Cancelled", "lifecycle": "approved",
            "spec_status": "ready", "execution_status": "planned",
            "blocked": false, "cancelled": true,
            "remediation_pre_state": null,
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {
                "draft": {"status": "done"},
                "groom": {"status": "skipped"},
                "specify": {"status": "skipped"}
            }
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    assert!(
        flat.contains('⊘'),
        "cancelled milestone stages must show ⊘; got: {flat}"
    );
}

#[test]
fn cancelled_overlay_subline_appears() {
    // AC-15: a cancelled milestone renders an overlay sub-line
    // `└─ lifecycle overlay: cancelled` under the current-stage row.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Cancelled".to_string(),
        lifecycle: "approved".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: true,
        cancelled_at: None,
        cancel_reason: Some("work shipped via different design".to_string()),
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Cancelled", "lifecycle": "approved",
            "spec_status": "ready", "execution_status": "planned",
            "blocked": false, "cancelled": true,
            "remediation_pre_state": null,
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {
                "draft": {"status": "done"}
            }
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    assert!(
        flat.contains("└─ lifecycle overlay: cancelled"),
        "cancelled overlay sub-line must render; got: {flat}"
    );
    // F-08: the sub-line must sit DIRECTLY under the current-stage
    // row (groom — the first non-done stage after draft), not after
    // the whole 12-row section. Verify the sub-line appears before
    // the "specify" stage row.
    let subline_pos = flat.find("└─ lifecycle overlay: cancelled").unwrap();
    let specify_pos = flat.find("specify").unwrap();
    assert!(
        subline_pos < specify_pos,
        "overlay sub-line must render under the current-stage row (before the next stage row); \
         sub-line at {subline_pos}, specify row at {specify_pos}"
    );
    // And the sub-line must come AFTER the current-stage row (groom).
    let groom_pos = flat.find("groom").unwrap();
    assert!(
        groom_pos < subline_pos,
        "overlay sub-line must render after the current-stage (groom) row; \
         groom at {groom_pos}, sub-line at {subline_pos}"
    );
}

#[test]
fn blocked_overlay_subline_appears() {
    // AC-15: blocked overlay renders a sub-line too.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Blocked".to_string(),
        lifecycle: "in-progress".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Blocked", "lifecycle": "in-progress",
            "spec_status": "ready", "execution_status": "blocked",
            "blocked": true, "cancelled": false,
            "remediation_pre_state": null,
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {
                "draft": {"status": "done"},
                "groom": {"status": "done"},
                "specify": {"status": "done"},
                "approve": {"status": "done"},
                "execute": {"status": "in_progress"}
            }
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    assert!(
        flat.contains("└─ lifecycle overlay: blocked"),
        "blocked overlay sub-line must render; got: {flat}"
    );
}

#[test]
fn remediation_overlay_subline_appears() {
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Remediation".to_string(),
        lifecycle: "remediation".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Remediation", "lifecycle": "remediation",
            "spec_status": "implemented", "execution_status": "done",
            "blocked": false, "cancelled": false,
            "remediation_pre_state": "complete",
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {
                "draft": {"status": "done"},
                "groom": {"status": "done"},
                "specify": {"status": "done"},
                "approve": {"status": "done"},
                "execute": {"status": "done"},
                "self-review": {"status": "done"},
                "complete": {"status": "done"},
                "external-review": {"status": "done"},
                "remediate": {"status": "in_progress"}
            }
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    assert!(
        flat.contains("└─ lifecycle overlay: remediation"),
        "remediation overlay sub-line must render; got: {flat}"
    );
}

#[test]
fn no_overlay_no_subline() {
    // AC-15 negative: a normal milestone (no overlay) shows no
    // overlay sub-line.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Normal".to_string(),
        lifecycle: "in-progress".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Normal", "lifecycle": "in-progress",
            "spec_status": "ready", "execution_status": "in-progress",
            "blocked": false, "cancelled": false,
            "remediation_pre_state": null,
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {
                "draft": {"status": "done"},
                "groom": {"status": "done"},
                "specify": {"status": "done"},
                "approve": {"status": "done"},
                "execute": {"status": "in_progress"}
            }
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    assert!(
        !flat.contains("└─ lifecycle overlay:"),
        "no-overlay milestone must NOT show an overlay sub-line; got: {flat}"
    );
}

#[test]
fn detail_header_shows_stage_cell_not_lifecycle_badge() {
    // S19: header shows the Stage cell `<N>/12 · <Label>` rather
    // than a legacy lifecycle badge.
    let mut app = App::new();
    app.load_milestones(vec![MilestoneSummary {
        id: "01".to_string(),
        title: "Sample".to_string(),
        lifecycle: "complete".to_string(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".to_string(),
        updated: String::new(),
        created: String::new(),
        cancelled: false,
        cancelled_at: None,
        cancel_reason: None,
        flow_stages: BTreeMap::new(),
    }]);
    app.selected_milestone_id = Some("01".to_string());
    app.content = ContentState::MilestoneDetail;
    app.milestone_detail = Some(json!({
        "milestone": {
            "id": "01", "title": "Sample", "lifecycle": "complete",
            "spec_status": "verified", "execution_status": "done",
            "blocked": false, "cancelled": false,
            "remediation_pre_state": null,
            "depends_on": [], "effort": "S", "risk": "low",
            "flow_stages": {
                "draft": {"status": "done"},
                "groom": {"status": "done"},
                "specify": {"status": "done"},
                "approve": {"status": "done"},
                "execute": {"status": "done"},
                "self-review": {"status": "done"},
                "complete": {"status": "done"},
                "external-review": {"status": "done"},
                "remediate": {"status": "done"},
                "re-review": {"status": "done"},
                "document": {"status": "done"}
            }
        },
        "intent": {"outcome": "x"}, "problem": {"description": "y"},
        "scope": {"in_scope": [], "out_of_scope": []},
        "acceptance_criteria": [], "design_decisions": [],
        "open_questions": [], "work_packages": [], "steps": [], "findings": []
    }));
    let backend = TestBackend::new(140, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    // Every stage done → 12/12 · Hand-off sentinel.
    assert!(
        flat.contains("12/12") && flat.contains("Hand-off"),
        "header must show Stage cell 12/12 · Hand-off; got: {flat}"
    );
}

// ── M203 S5: 2-line backlog rows with preview ─────────────────────────────
//
// AC-04 contracts:
//   * Each logical Backlog row renders as 2 visual lines.
//   * Title bold on line 1, preview dim with `↳` prefix on line 2.
//   * Selected-row highlight spans both visual lines.
//   * Selected-index addresses logical rows (not visual rows).

fn render_backlog(app: &App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut flat = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            flat.push_str(buf[(x, y)].symbol());
        }
        flat.push('\n');
    }
    flat
}

#[test]
fn backlog_row_renders_two_visual_lines() {
    let mut app = App::new();
    app.load_backlog(vec![
        raul::tui::app::BacklogLine {
            id: "BL-01".to_string(),
            title: "Refactor parser".to_string(),
            priority: "high".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "Continuation detail here".to_string(),
            ..Default::default()
        },
        raul::tui::app::BacklogLine {
            id: "BL-02".to_string(),
            title: "Single line row".to_string(),
            priority: "medium".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "".to_string(),
            ..Default::default()
        },
    ]);
    app.select_lane(Lane::Backlog);
    let flat = render_backlog(&app, 140, 20);

    // Title appears on a visual row (line N) immediately above a
    // visual row that starts with the `↳` arrow prefix. Title row
    // N+1 should contain `Continuation detail here` (after the prefix).
    let lines: Vec<&str> = flat.lines().collect();
    let title_row = lines
        .iter()
        .position(|l| l.contains("Refactor parser"))
        .expect("title row must appear");
    // Within the next 2 visual rows below the title, the preview
    // must appear. The 2-line layout means the preview row is
    // immediately under the title row.
    assert!(
        lines
            .iter()
            .skip(title_row + 1)
            .take(2)
            .any(|l| l.contains("Continuation detail here")),
        "preview row must appear within 2 visual lines of the title; got flat:\n{flat}"
    );
    // The preview row must carry the `↳` arrow prefix (M203 AC-04).
    assert!(
        lines
            .iter()
            .skip(title_row + 1)
            .take(2)
            .any(|l| l.contains('↳')),
        "preview row must carry the `↳` arrow prefix; got flat:\n{flat}"
    );
}

#[test]
fn preview_line_is_dim_with_arrow_prefix() {
    // The preview line is dim (lower-priority style) AND prefixed
    // with `↳ `. Empty previews render as an empty visual line.
    let mut app = App::new();
    app.load_backlog(vec![raul::tui::app::BacklogLine {
        id: "BL-01".to_string(),
        title: "Refactor parser".to_string(),
        priority: "high".to_string(),
        status: "open".to_string(),
        resolution: "".to_string(),
        preview: "some continuation text".to_string(),
        ..Default::default()
    }]);
    app.select_lane(Lane::Backlog);

    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Find the title row, then read the next row's prefix cell.
    let mut title_y: Option<u16> = None;
    for y in 0..buf.area().height {
        let row: String = (0..buf.area().width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        if row.contains("Refactor parser") {
            title_y = Some(y);
            break;
        }
    }
    let title_y = title_y.expect("title row must exist");
    let preview_y = title_y + 1;

    // The `↳` glyph must land on the preview row.
    let preview_row: String = (0..buf.area().width)
        .map(|x| buf[(x, preview_y)].symbol().to_string())
        .collect();
    assert!(
        preview_row.contains('↳'),
        "preview row must contain `↳` glyph at row {preview_y}; got: {preview_row:?}"
    );
    // The preview text follows the prefix.
    assert!(
        preview_row.contains("some continuation text"),
        "preview row must contain the continuation text; got: {preview_row:?}"
    );
}

#[test]
fn selected_row_highlight_spans_both_lines() {
    // When a row is selected, BOTH visual rows (title + preview)
    // share the same background highlight.
    let mut app = App::new();
    app.load_backlog(vec![
        raul::tui::app::BacklogLine {
            id: "BL-01".to_string(),
            title: "Refactor parser".to_string(),
            priority: "high".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "Continuation detail".to_string(),
            ..Default::default()
        },
        raul::tui::app::BacklogLine {
            id: "BL-02".to_string(),
            title: "Other row".to_string(),
            priority: "low".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "Other continuation".to_string(),
            ..Default::default()
        },
    ]);
    app.select_lane(Lane::Backlog);
    app.selected_index = 0;

    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();

    // Find the title and preview rows for the SELECTED row (BL-01).
    let mut sel_title_y: Option<u16> = None;
    for y in 0..buf.area().height {
        let row: String = (0..buf.area().width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        if row.contains("Refactor parser") {
            sel_title_y = Some(y);
            break;
        }
    }
    let sel_title_y = sel_title_y.expect("selected title row present");
    let sel_preview_y = sel_title_y + 1;

    // Sample the bg style of a cell on the title row + preview row
    // for the SELECTED item, AND for the UNSELECTED item (BL-02).
    let sample_bg = |y: u16, contains: &str| -> Option<ratatui::style::Color> {
        let needle = contains.chars().next().unwrap_or(' ');
        for x in 0..buf.area().width {
            let cell = &buf[(x, y)];
            if cell.symbol().chars().next().unwrap_or(' ') == needle {
                return Some(cell.style().bg.unwrap_or(ratatui::style::Color::Reset));
            }
        }
        None
    };

    // Title row bg vs preview row bg for the SELECTED row: both must
    // be the accent background (not the default panel bg).
    let sel_title_bg = sample_bg(sel_title_y, "R")
        .or_else(|| sample_bg(sel_title_y, "C"))
        .expect("selected title cell");
    let sel_preview_bg = sample_bg(sel_preview_y, "↳").expect("selected preview cell");
    assert_eq!(
        sel_title_bg, sel_preview_bg,
        "selected row highlight must span both visual rows; got title_bg={sel_title_bg:?} preview_bg={sel_preview_bg:?}"
    );

    // The preview row must NOT carry the unselected-row dim fg.
    let preview_cell = (0..buf.area().width)
        .map(|x| buf[(x, sel_preview_y)].clone())
        .find(|c| c.symbol() == "↳")
        .expect("preview arrow cell");
    let preview_fg = preview_cell
        .style()
        .fg
        .unwrap_or(ratatui::style::Color::Reset);
    // Selected preview fg must differ from the unselected dim color
    // (we don't pin the exact RGB; just assert it's NOT the dim
    // color used for unselected previews).
    assert_ne!(
        preview_fg,
        app.effective_palette().dim,
        "selected preview cell must not render with the unselected dim color"
    );
}

#[test]
fn selected_index_addresses_logical_rows() {
    // The selected_index points at logical rows (not visual lines).
    // With 2 visible rows, indices 0 and 1 must address distinct
    // logical rows. Selecting index 1 must NOT scroll the cursor
    // into the preview sub-line of row 0.
    let mut app = App::new();
    app.load_backlog(vec![
        raul::tui::app::BacklogLine {
            id: "BL-01".to_string(),
            title: "First row".to_string(),
            priority: "high".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "first preview".to_string(),
            ..Default::default()
        },
        raul::tui::app::BacklogLine {
            id: "BL-02".to_string(),
            title: "Second row".to_string(),
            priority: "low".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "second preview".to_string(),
            ..Default::default()
        },
    ]);
    app.select_lane(Lane::Backlog);
    app.selected_index = 1;

    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();

    // Locate row 0 title + preview and row 1 title + preview.
    let mut r0_title_y: Option<u16> = None;
    let mut r1_title_y: Option<u16> = None;
    for y in 0..buf.area().height {
        let row: String = (0..buf.area().width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        if row.contains("First row") {
            r0_title_y = Some(y);
        }
        if row.contains("Second row") {
            r1_title_y = Some(y);
        }
    }
    let r0_title_y = r0_title_y.expect("first title row present");
    let r1_title_y = r1_title_y.expect("second title row present");
    // Logical rows must NOT share a visual row (they are stacked).
    assert!(
        r1_title_y > r0_title_y,
        "logical rows must occupy distinct visual rows; got r0={r0_title_y} r1={r1_title_y}"
    );
    // And they must be 2 visual rows apart (one logical row = 2
    // visual lines).
    assert_eq!(
        r1_title_y - r0_title_y,
        2,
        "logical rows must be 2 visual lines apart; got delta={}",
        r1_title_y - r0_title_y
    );

    // The selected row (index 1) must carry the highlight. The
    // `First row` (index 0) must NOT carry the highlight.
    let r0_cell = (0..buf.area().width)
        .map(|x| buf[(x, r0_title_y)].clone())
        .find(|c| c.symbol() == "F")
        .expect("first-row F cell");
    let r1_cell = (0..buf.area().width)
        .map(|x| buf[(x, r1_title_y)].clone())
        .find(|c| c.symbol() == "S")
        .expect("second-row S cell");
    let r0_bg = r0_cell.style().bg.unwrap_or(ratatui::style::Color::Reset);
    let r1_bg = r1_cell.style().bg.unwrap_or(ratatui::style::Color::Reset);
    // The selected row's bg must equal the accent; the unselected
    // row's bg must be the default panel bg (Reset).
    assert_eq!(
        r1_bg,
        app.effective_palette().accent,
        "row at selected_index=1 must be highlighted"
    );
    assert_ne!(
        r0_bg, r1_bg,
        "row at selected_index=0 must NOT be highlighted; got bg={r0_bg:?}"
    );
}

// ── M203 S6: compact-mode (narrow terminal) 2-line rows ───────────────────
//
// AC-05 contracts:
//   * Compact-mode (narrow terminal) renders the same 2-line layout
//     with reduced column widths.
//   * Preview content unchanged; truncation may kick in earlier due
//     to narrower title column.

#[test]
fn backlog_compact_row_renders_two_visual_lines() {
    // Compact: pane width small enough that the side columns
    // dominate. Title column shrinks but the row layout must still
    // be 2 visual lines per logical row, and the `↳` arrow must
    // still appear under the title.
    let mut app = App::new();
    app.load_backlog(vec![raul::tui::app::BacklogLine {
        id: "BL-01".to_string(),
        title: "Refactor parser".to_string(),
        priority: "high".to_string(),
        status: "open".to_string(),
        resolution: "".to_string(),
        preview: "A continuation text".to_string(),
        ..Default::default()
    }]);
    app.select_lane(Lane::Backlog);

    // Narrow pane — typical compact-mode width.
    let backend = TestBackend::new(60, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut title_y: Option<u16> = None;
    for y in 0..buf.area().height {
        let row: String = (0..buf.area().width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        if row.contains("Refactor parser") {
            title_y = Some(y);
            break;
        }
    }
    let title_y = title_y.expect("title row must exist even in compact mode");
    let preview_row: String = (0..buf.area().width)
        .map(|x| buf[(x, title_y + 1)].symbol().to_string())
        .collect();
    assert!(
        preview_row.contains('↳'),
        "compact mode must still render 2-line rows with the ↳ arrow on line 2; got row: {preview_row:?}"
    );
    assert!(
        preview_row.contains("continuation"),
        "compact preview must contain the continuation text; got: {preview_row:?}"
    );
}

#[test]
fn preview_truncates_earlier_in_compact_mode() {
    // The preview truncates to its title-column budget. On a wide
    // pane the budget is ~80 chars; on a narrow pane the title
    // column shrinks and the preview gets clipped earlier. The same
    // payload preview should render fewer characters when the pane
    // is narrower.
    let long_preview: String = "a".repeat(120);
    let wide = render_preview_visible_chars(&long_preview, 140, 24);
    let narrow = render_preview_visible_chars(&long_preview, 60, 12);
    let wide_chars = wide.chars().filter(|c| *c == 'a').count();
    let narrow_chars = narrow.chars().filter(|c| *c == 'a').count();
    assert!(
        wide_chars > narrow_chars,
        "compact mode must truncate the preview earlier than wide mode; wide={wide_chars} narrow={narrow_chars}"
    );
    // Compact mode budget: title_w = inner - (id_w + pri_w + status_w
    // + 3 spacing) = (60-2) - (10+10+12+3) = 23. Preview budget =
    // 23 - 2 (prefix) = 21 chars. So we expect ~21 chars of preview
    // text in narrow mode (the ellipsis adds 3 more chars).
    assert!(
        narrow_chars <= 23,
        "compact preview should fit within ~21-23 a-chars; got {narrow_chars}"
    );
    assert!(
        narrow_chars >= 8,
        "compact preview should still hold meaningful text; got {narrow_chars}"
    );
}

// ── M203 S7: sort / search / filter behavior unchanged after the
// 2-line refactor. The preview field is render-only; it MUST NOT
// influence sort keys, search needles, or hide-done filters.

fn seed_backlog(app: &mut App) {
    app.load_backlog(vec![
        raul::tui::app::BacklogLine {
            id: "BL-01".to_string(),
            title: "Zeta".to_string(),
            priority: "high".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "z-preview".to_string(),
            ..Default::default()
        },
        raul::tui::app::BacklogLine {
            id: "BL-02".to_string(),
            title: "alpha".to_string(),
            priority: "low".to_string(),
            status: "resolved".to_string(),
            resolution: "shipped".to_string(),
            preview: "alpha-preview".to_string(),
            ..Default::default()
        },
        raul::tui::app::BacklogLine {
            id: "BL-03".to_string(),
            title: "Mu".to_string(),
            priority: "medium".to_string(),
            status: "open".to_string(),
            resolution: "".to_string(),
            preview: "mu-preview".to_string(),
            ..Default::default()
        },
        raul::tui::app::BacklogLine {
            id: "BL-04".to_string(),
            title: "Beta".to_string(),
            priority: "high".to_string(),
            status: "resolved".to_string(),
            resolution: "wont-fix".to_string(),
            preview: "beta-preview".to_string(),
            ..Default::default()
        },
    ]);
}

#[test]
fn sort_by_id_unchanged() {
    let mut app = App::new();
    seed_backlog(&mut app);
    app.select_lane(Lane::Backlog);
    app.lane_sort_key.insert(Lane::Backlog, SortKey::Id);
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids, vec!["BL-01", "BL-02", "BL-03", "BL-04"]);
}

#[test]
fn sort_by_title_unchanged() {
    let mut app = App::new();
    seed_backlog(&mut app);
    app.select_lane(Lane::Backlog);
    app.lane_sort_key.insert(Lane::Backlog, SortKey::Title);
    // Title sort is case-insensitive ascending. The preview field
    // MUST NOT participate — these titles are alphabetical with
    // upper vs lower ordering (alpha < Beta < Mu < Zeta).
    let titles: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.title.as_str())
        .collect();
    assert_eq!(
        titles,
        vec!["alpha", "Beta", "Mu", "Zeta"],
        "title sort unchanged after preview refactor; got {titles:?}"
    );
}

#[test]
fn sort_by_priority_unchanged() {
    let mut app = App::new();
    seed_backlog(&mut app);
    app.select_lane(Lane::Backlog);
    app.lane_sort_key.insert(Lane::Backlog, SortKey::Priority);
    // Priority rank: high > medium > low. Ties break on id ascending.
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["BL-01", "BL-04", "BL-03", "BL-02"],
        "priority sort unchanged after preview refactor; got {ids:?}"
    );
}

#[test]
fn sort_by_status_unchanged() {
    let mut app = App::new();
    seed_backlog(&mut app);
    app.select_lane(Lane::Backlog);
    app.lane_sort_key.insert(Lane::Backlog, SortKey::Status);
    // Status rank: open > resolved. Ties break on id ascending.
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["BL-01", "BL-03", "BL-02", "BL-04"],
        "status sort unchanged after preview refactor; got {ids:?}"
    );
}

#[test]
fn search_matches_title_not_preview() {
    let mut app = App::new();
    seed_backlog(&mut app);
    app.select_lane(Lane::Backlog);
    // Search term `alpha-preview` matches ONLY the preview text,
    // not the title. The filter must NOT pick this row up.
    app.lane_search
        .insert(Lane::Backlog, "alpha-preview".to_string());
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert!(
        ids.is_empty(),
        "search must NOT match against preview text; got {ids:?}"
    );
    // Search term `alpha` matches the title of BL-02 only.
    app.lane_search.insert(Lane::Backlog, "alpha".to_string());
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["BL-02"],
        "search must match title-only; got {ids:?}"
    );
    // And `mu-preview` (preview-only) must not match either.
    app.lane_search
        .insert(Lane::Backlog, "mu-preview".to_string());
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert!(
        ids.is_empty(),
        "search must not match preview-only terms; got {ids:?}"
    );
}

#[test]
fn hide_done_respects_status() {
    let mut app = App::new();
    seed_backlog(&mut app);
    app.select_lane(Lane::Backlog);
    app.hide_done = true;
    let ids: Vec<&str> = app
        .visible_backlog()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    // Resolved rows (BL-02, BL-04) must be filtered out; open rows
    // (BL-01, BL-03) remain.
    assert_eq!(
        ids,
        vec!["BL-01", "BL-03"],
        "hide_done must filter by status, not by preview; got {ids:?}"
    );
}

/// Helper: render the backlog list at the given pane size and return
/// just the preview substring (everything after `↳ ` on the preview
/// row of the first backlog item).
fn render_preview_visible_chars(preview: &str, w: u16, h: u16) -> String {
    let mut app = App::new();
    app.load_backlog(vec![raul::tui::app::BacklogLine {
        id: "BL-01".to_string(),
        title: "Title".to_string(),
        priority: "high".to_string(),
        status: "open".to_string(),
        resolution: "".to_string(),
        preview: preview.to_string(),
        ..Default::default()
    }]);
    app.select_lane(Lane::Backlog);
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Find the row that contains BOTH "BL-01" and "Title" (the data
    // row, not the column header which has "Title" alone). That row
    // is the logical row's title line; the preview is on the next
    // row.
    let mut title_y: Option<u16> = None;
    for y in 0..buf.area().height {
        let row: String = (0..buf.area().width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        if row.contains("BL-01") && row.contains("Title") {
            title_y = Some(y);
            break;
        }
    }
    let title_y = title_y.unwrap_or_else(|| panic!("BL-01 data row not found for w={w} h={h}"));
    // Preview is on the row immediately below the title row.
    let preview_row: String = (0..buf.area().width)
        .map(|x| buf[(x, title_y + 1)].symbol().to_string())
        .collect();
    preview_row
}

// ─── M204 AC-02: debounced sort/filter writes ────────────────────────────────

/// M204 AC-02: a single sort/filter mutation reaches disk within 1s
/// of the mutation, not on the next on_idle tick alone. The
/// debounce window is 500ms; the test pins the seam
/// `flush_pending_writes_if_due` with an injected clock.
#[test]
fn debounced_save_writes_within_one_second() {
    let mut app = App::new();
    let baseline = std::time::Instant::now();
    app.mark_sort_dirty(Lane::Milestones, "priority".to_string());
    assert!(
        app.last_pending_change_at.is_some(),
        "mark_sort_dirty must stamp last_pending_change_at"
    );

    // Within the debounce window — must NOT fire.
    let within = baseline
        .checked_add(std::time::Duration::from_millis(100))
        .unwrap();
    let calls = std::cell::RefCell::new(Vec::<(String, String)>::new());
    let fired = app
        .flush_pending_writes_if_due(
            std::time::Duration::from_millis(500),
            within,
            |key, value| {
                calls
                    .borrow_mut()
                    .push((key.to_string(), value.to_string()));
                Ok::<(), ()>(())
            },
        )
        .unwrap();
    assert!(!fired, "writes must not fire before debounce elapses");
    assert!(calls.borrow().is_empty(), "no writes yet");

    // After the debounce elapses — must fire exactly once.
    let after = baseline
        .checked_add(std::time::Duration::from_millis(600))
        .unwrap();
    let fired = app
        .flush_pending_writes_if_due(
            std::time::Duration::from_millis(500),
            after,
            |key, value| {
                calls
                    .borrow_mut()
                    .push((key.to_string(), value.to_string()));
                Ok::<(), ()>(())
            },
        )
        .unwrap();
    assert!(fired, "writes must fire after debounce elapses");
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1, "exactly one write per flush");
    assert_eq!(calls[0].0, "sort.milestones");
    assert_eq!(calls[0].1, "priority");
    assert_eq!(app.debounced_write_count, 1, "write counter must increment");
}

/// M204 AC-02: 100 rapid filter mutations within the debounce
/// window coalesce into a single disk write. Per the spec:
/// "100 rapid filter toggles → 1 disk write". The
/// `mark_filter_dirty` call snapshots the current `lane_filters`
/// into `pending_filter_writes`; repeated calls within the window
/// overwrite, not accumulate, so the on-disk JSON reflects the
/// most recent user selection.
#[test]
fn rapid_filter_toggles_coalesce_to_single_write() {
    let mut app = App::new();
    // Seed a small lane_filter so the mark-dirty call has something
    // to persist.
    let mut dims: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    dims.insert(
        "lifecycle".to_string(),
        std::collections::BTreeSet::from(["approved".to_string()]),
    );
    app.lane_filters.insert(Lane::Milestones, dims);

    // 100 rapid filter mutations, all within the debounce window.
    let t0 = std::time::Instant::now();
    for i in 0..100 {
        // Mutate the in-memory filter (toggling a value), then mark
        // dirty. The last mark wins; the debounce coalesces all 100
        // into one write.
        if i % 2 == 0 {
            let entry = app
                .lane_filters
                .entry(Lane::Milestones)
                .or_default()
                .entry("lifecycle".to_string())
                .or_default();
            entry.insert("in-progress".to_string());
        } else {
            let entry = app
                .lane_filters
                .entry(Lane::Milestones)
                .or_default()
                .entry("lifecycle".to_string())
                .or_default();
            entry.remove("in-progress");
        }
        app.mark_filter_dirty(Lane::Milestones);
    }
    // Sanity: the pending write state has a single Milestones entry
    // (the 100 marks coalesce in-memory too — `mark_filter_dirty`
    // overwrites per-lane with the most recent snapshot).
    assert_eq!(app.pending_filter_writes.len(), 1);
    assert!(app.pending_filter_writes.contains_key("milestones"));

    // Try to flush before the window — must not fire.
    let within = t0
        .checked_add(std::time::Duration::from_millis(50))
        .unwrap();
    let calls = std::cell::RefCell::new(Vec::<(String, String)>::new());
    let _ = app
        .flush_pending_writes_if_due(
            std::time::Duration::from_millis(500),
            within,
            |key, value| {
                calls
                    .borrow_mut()
                    .push((key.to_string(), value.to_string()));
                Ok::<(), ()>(())
            },
        )
        .unwrap();
    assert!(
        calls.borrow().is_empty(),
        "100 mutations must not fire before debounce"
    );

    // After the window — exactly one disk write.
    let after = t0
        .checked_add(std::time::Duration::from_millis(700))
        .unwrap();
    let fired = app
        .flush_pending_writes_if_due(
            std::time::Duration::from_millis(500),
            after,
            |key, value| {
                calls
                    .borrow_mut()
                    .push((key.to_string(), value.to_string()));
                Ok::<(), ()>(())
            },
        )
        .unwrap();
    assert!(fired);
    let calls = calls.borrow();
    assert_eq!(
        calls.len(),
        1,
        "100 rapid filter mutations must coalesce into 1 disk write; got {:?}",
        calls.clone()
    );
    assert_eq!(calls[0].0, "filter.milestones");
    let parsed: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    let lc = parsed["lifecycle"]
        .as_array()
        .expect("lifecycle must be a JSON array");
    assert!(lc.iter().any(|v| v.as_str() == Some("approved")));
}
