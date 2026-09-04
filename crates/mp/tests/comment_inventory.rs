mod common;

use std::fs;

use common::repo_root;

fn comment_paragraph<'a>(source: &'a str, needle: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = source.lines().collect();
    let index = lines
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("comment inventory needle missing: {needle}"));

    let mut start = index;
    while start > 0 && lines[start - 1].trim_start().starts_with("//") {
        start -= 1;
    }
    let mut end = index + 1;
    while end < lines.len() && lines[end].trim_start().starts_with("//") {
        end += 1;
    }
    lines[start..end].to_vec()
}

fn has_historical_tag(line: &str) -> bool {
    let bytes = line.as_bytes();
    (0..bytes.len()).any(|index| {
        let boundary_before = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
        if !boundary_before {
            return false;
        }

        let tagged_digits = |prefix: &[u8]| {
            bytes[index..].starts_with(prefix)
                && bytes
                    .get(index + prefix.len())
                    .is_some_and(u8::is_ascii_digit)
        };
        tagged_digits(b"M") || tagged_digits(b"S") || tagged_digits(b"F-") || tagged_digits(b"AC-")
    })
}

#[test]
fn keep_and_shorten_inventory_comments_are_tag_free() {
    let root = repo_root();
    let inventory = [
        (
            "crates/mp/src/activity.rs",
            &[
                "**Locking contract.** This primitive",
                "The prior-state read runs outside",
            ][..],
        ),
        (
            "crates/mp-model/src/milestone.rs",
            &["Empty-phase findings count as self-review"][..],
        ),
        (
            "crates/mp/src/autopilot/drive/run_state.rs",
            &["A schema-version mismatch on a *future*"][..],
        ),
        (
            "crates/raul/src/tui/modes/normal.rs",
            &["Modal menu keys must be handled"][..],
        ),
        (
            "crates/raul/src/tui/render/overlays.rs",
            &["Character-safe truncation"][..],
        ),
        (
            "crates/raul/src/tui/view_state.rs",
            &[
                "How far from the bottom",
                "Helper: compute the scroll offset",
                "Row height for an Overview inbox item",
            ][..],
        ),
        (
            "crates/raul/src/tui/status.rs",
            &["Mirrors `mp_model::MilestoneFile::effective_execution_status`"][..],
        ),
        (
            "crates/mp/src/milestone/complete.rs",
            &["Completion requires all self-review work"][..],
        ),
        (
            "crates/mp/src/milestone/io.rs",
            &["same-filesystem atomic rename"][..],
        ),
        (
            "crates/mp/src/milestone/spec.rs",
            &["Every AC id must exist before any write"][..],
        ),
        (
            "crates/raul/src/tui/app.rs",
            &["Per-lane TTL cache for `mp` subprocess payloads"][..],
        ),
        (
            "crates/raul/src/tui/render/scrollbar.rs",
            &["Measure the absolute extent"][..],
        ),
        (
            "crates/raul/src/tui/render/lane_lists.rs",
            &["Filtering can leave a stale selection index"][..],
        ),
        (
            "crates/raul/src/tui/progress.rs",
            &["`implemented` is the legacy spec equivalent"][..],
        ),
        (
            "crates/raul/src/tui/render/watch.rs",
            &["Log I/O belongs to the idle poller"][..],
        ),
    ];

    for (relative, needles) in inventory {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        for needle in needles {
            let paragraph = comment_paragraph(&source, needle);
            let violations: Vec<&str> = paragraph
                .iter()
                .copied()
                .filter(|line| has_historical_tag(line))
                .collect();
            assert!(
                violations.is_empty(),
                "{relative}: retained comment around {needle:?} contains historical tags: {violations:?}"
            );
        }
    }
}
