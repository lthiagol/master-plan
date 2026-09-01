//! M185 AC-05: title bar chip for Milestones filter.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, ContentState, Lane, MilestoneSummary};
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

    app.milestone_filter.insert("approved".into());
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
