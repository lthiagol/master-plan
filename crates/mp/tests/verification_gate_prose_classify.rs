//! M177 S4: regression pin — previously shell-parse-broken AC verifications
//! in M135 / M138 (and a sample of other milestones) classify as Manual.

use mp::ac_verify::{classify_with, looks_like_prose, Kind};

#[test]
fn verification_gate_prose_classify_m135_ac02() {
    let v = "crates/raul/tests/tui_view_state.rs (grep-based test)";
    assert!(looks_like_prose(v));
    assert_eq!(classify_with(v, false), Kind::Manual);
}

#[test]
fn verification_gate_prose_classify_m138_ac03() {
    let v = "crates/raul/tests/keybinds.rs (load from JSON then assert default on missing entries)";
    assert!(looks_like_prose(v));
    assert_eq!(classify_with(v, false), Kind::Manual);
}

#[test]
fn verification_gate_prose_classify_m138_ac05() {
    let v = "crates/raul/tests/keybinds.rs + rg for hardcoded key legends in crates/raul/src/tui/render/";
    assert!(looks_like_prose(v));
    assert_eq!(classify_with(v, false), Kind::Manual);
}

#[test]
fn verification_gate_prose_classify_sample_parenthetical() {
    let samples = [
        "crates/mp/tests/foo.rs (integration harness check)",
        "mp validate (full plan) and inspect warnings",
        "run the suite and confirm green and file the note",
        "cargo nextest run -p mp ; all green ; expected zero failures",
        "src/lib.rs + grep for unwrap in public API",
    ];
    for v in samples {
        assert_eq!(
            classify_with(v, false),
            Kind::Manual,
            "expected Manual for {v:?}"
        );
    }
}

#[test]
fn verification_gate_prose_classify_runnable_negatives() {
    let samples = [
        "cargo test -p mp",
        "make test",
        "mp validate",
        "crates/mp/tests/workflow_gates.rs",
        "(cd dir && cargo test -p mp)",
        "cd crates/mp; cargo test -p mp",
        "rg something",
        "./scripts/audit-step-tests.sh",
        // M177 F-07/F-08: repo-canonical nextest filters + quoted parens.
        "cargo nextest run -p mp -E 'test(/foo-bar/)' --no-fail-fast",
        "cargo nextest run -p mp -E 'test(/strip_deferred_reason/)' --no-fail-fast",
        "echo 'hello (world with spaces)'",
        r#"printf "ok (pre-m154)\n""#,
    ];
    for v in samples {
        assert_eq!(
            classify_with(v, false),
            Kind::Runnable,
            "expected Runnable for {v:?}"
        );
    }
}
