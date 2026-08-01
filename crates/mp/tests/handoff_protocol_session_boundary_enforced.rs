//! M141 AC-03: hand-off protocol session-boundary discipline is now
//! documented inline in `mp-flow`'s Hand-off protocol section. The
//! content-completeness check walks the four hand-off points and
//! asserts each one documents the three-section contract (data,
//! session-boundary, evidence) and the L5 cross-link to
//! `docs/code-review-lessons.md`. The session-boundary check is
//! content completeness — runtime harness-enforcement of "different
//! session ids across the boundary" is the harness's concern, out
//! of scope for mp.

mod common;

use std::fs;

use common::repo_root;

fn skill_path() -> std::path::PathBuf {
    repo_root().join("templates/skills/mp-flow/SKILL.md")
}

fn read_skill() -> String {
    fs::read_to_string(skill_path())
        .unwrap_or_else(|_| panic!("missing: {}", skill_path().display()))
}

/// AC-03: the Hand-off protocol section has the four hand-off points
/// (a-d). The section lives inline in `mp-flow/SKILL.md`; the
/// pre-M141 standalone `mp-handoff` skill is gone.
#[test]
fn handoff_protocol_has_four_handoff_points() {
    let content = read_skill();
    // Find the Hand-off protocol section and search within it.
    let needle = "## Hand-off protocol";
    let start = content
        .find(needle)
        .unwrap_or_else(|| panic!("mp-flow SKILL.md must contain the '{needle}' section"));
    let after = &content[start..];
    let body_end = after[2..]
        .find("\n## ")
        .map(|i| i + 2)
        .unwrap_or(after.len());
    let section = &after[..body_end];

    let required = [
        "Hand-off point (a)",
        "Hand-off point (b)",
        "Hand-off point (c)",
        "Hand-off point (d)",
    ];
    for term in required {
        assert!(
            section.contains(term),
            "Hand-off protocol section must contain '{}'; section was:\n{}",
            term,
            section
        );
    }
}

/// AC-03: each of the FOUR hand-off points (a-d) names (1) data,
/// (2) session-boundary, (3) evidence. Per-section assertion: the
/// four `## Hand-off point (a|b|c|d)` blocks must each carry the
/// three-section contract.
#[test]
fn handoff_protocol_three_section_contract() {
    let content = read_skill();

    let labels = ["(a)", "(b)", "(c)", "(d)"];
    for label in labels {
        let needle = format!("### Hand-off point {}", label);
        let start = content.find(&needle).unwrap_or_else(|| {
            panic!(
                "mp-flow SKILL.md is missing the '{}' section header",
                needle
            )
        });
        let after = &content[start..];
        // End of section: next `## ` or `### ` heading or EOF.
        let body_end = after[2..]
            .find("\n## ")
            .or_else(|| after[2..].find("\n### "))
            .map(|i| i + 2)
            .unwrap_or(after.len());
        let section = &after[..body_end];

        for field in &["**Data**", "**Session-boundary**", "**Evidence**"] {
            assert!(
                section.contains(field),
                "Hand-off point {} section must contain '{}' (per-section contract); section was:\n{}",
                label,
                field,
                section
            );
        }
    }
}

/// AC-03: SKILL.md explains the session-boundary discipline (author should
/// not be the only reviewer). Consumer-surface hygiene forbids internal
/// lesson codes (`L5`) and the dead `docs/code-review-lessons.md` path in
/// shipped skills — the discipline is stated in prose instead.
#[test]
fn handoff_protocol_cites_l5_session_boundary() {
    let content = read_skill();
    assert!(
        content.contains("session-boundary") || content.contains("session boundary"),
        "mp-flow SKILL.md must reference session-boundary discipline"
    );
    assert!(
        content.contains("author should not be the only")
            || content.contains("author-not-only-reviewer"),
        "mp-flow SKILL.md must state the author-not-only-reviewer rationale"
    );
}

/// AC-03: hand-off point (a) includes the AC verification integrity report.
#[test]
fn handoff_protocol_point_a_carries_verification_integrity_report() {
    let content = read_skill();

    assert!(
        content.contains("integrity report")
            || content.contains("integrity-report")
            || content.contains("verification integrity"),
        "mp-flow SKILL.md hand-off point (a) must reference the AC verification integrity report"
    );
    assert!(
        content.contains("mp plan verify-ac") || content.contains("verify-ac"),
        "mp-flow SKILL.md hand-off point (a) must reference 'mp plan verify-ac'"
    );
    assert!(
        content.contains("UNRESOLVABLE"),
        "mp-flow SKILL.md hand-off point (a) must name the runner-side UNRESOLVABLE rejection rule"
    );
}
