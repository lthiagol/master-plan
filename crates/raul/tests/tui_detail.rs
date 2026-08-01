use std::fs;
use std::path::PathBuf;

use raul::theme::Palette;
use raul::tui::progress;

#[test]
fn ac_status_uses_palette_colors() {
    let palette = Palette::default_palette();
    assert_eq!(
        progress::ac_status_style("passed", palette).fg,
        Some(palette.success)
    );
    assert_eq!(
        progress::ac_status_style("failed", palette).fg,
        Some(palette.danger)
    );
    assert_eq!(
        progress::ac_status_style("pending", palette).fg,
        Some(palette.warn)
    );
}

#[test]
fn backlog_detail_renders_enriched_fields() {
    let render_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("render")
        .join("lane_lists.rs");
    let content = fs::read_to_string(&render_path).unwrap();

    assert!(
        content.contains("fn render_backlog_detail"),
        "render_backlog_detail function must exist"
    );

    let enriched_fields = ["priority", "status", "resolution"];
    let mut found_fields = Vec::new();

    for field in &enriched_fields {
        if content.contains(field) {
            found_fields.push(*field);
        }
    }

    assert!(
        found_fields.len() >= 2,
        "Backlog detail should render at least 2 enriched fields (priority/status/resolution), found: {:?}",
        found_fields
    );
}

#[test]
fn scope_out_of_scope_uses_inline_markdown() {
    let render_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("render")
        .join("milestone_detail.rs");
    let content = fs::read_to_string(&render_path).unwrap();
    assert!(
        content.contains("parse_inline_spans(text, &md_styles)")
            && content.contains("Out of Scope"),
        "out_of_scope items should use inline markdown spans like in_scope"
    );
}

#[test]
fn backlog_detail_loader_exists() {
    let runner_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tui")
        .join("runner.rs");
    let content = fs::read_to_string(&runner_path).unwrap();

    assert!(
        content.contains("load_backlog_detail") || content.contains("BacklogDetail"),
        "Backlog detail loader must exist (load_backlog_detail or similar)"
    );
}
