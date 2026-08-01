//! M165: post-completion `verification` amend surface.
//!
//! AC-01: `mp milestone update 159 --verification "<text>"` exits 0 on a
//!        lifecycle=complete milestone, persists the new evidence, and leaves
//!        `lifecycle=complete`, `spec_status=verified`, `execution_status=done`
//!        unchanged.
//! AC-02: After the amend, `mp show milestone <id> --summary`'s
//!        `verification.force_bypassed` reflects the post-amend evidence
//!        (true iff `evidence.contains("[force-bypassed")`).
//! AC-03: `--verification-file`, `--verification-date`, `--verification-branch`
//!        are accepted; absent flags preserve the existing field.
//!
//! Test fixtures are copied into per-test temporary directories.

mod common;

use std::process::Command;

use crate::common::{repo_root, TestEnv};

fn mp_bin() -> &'static std::path::Path {
    common::mp_bin()
}

fn workspace_root() -> std::path::PathBuf {
    repo_root()
}

fn run_mp(env: &TestEnv, args: &[&str]) -> std::process::Output {
    Command::new(mp_bin())
        .current_dir(env.tmp.path())
        .env("MP_HOME", workspace_root())
        .args(args)
        .output()
        .expect("failed to run mp")
}

fn milestone_file_path(env: &TestEnv, id: &str) -> std::path::PathBuf {
    let plan_dir = env.tmp.path().join("master-plan/milestones");
    for entry in std::fs::read_dir(&plan_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if name.starts_with(&format!("{id}-")) {
            return entry.path();
        }
    }
    panic!("milestone file not found for id {id}");
}

fn read_milestone(env: &TestEnv, id: &str) -> serde_json::Value {
    let raw = std::fs::read_to_string(milestone_file_path(env, id)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

/// Patch a freshly-created milestone to the legacy-shape triple
/// (lifecycle=complete, spec_status=verified, execution_status=done) and
/// stamp `verification.evidence` with the supplied text. Mirrors the
/// `set_legacy_complete` helper in `lifecycle_complete_ceremony.rs` but
/// also writes a known verification string so post-amend assertions are
/// deterministic.
fn force_complete_with_evidence(env: &TestEnv, id: &str, evidence: &str) {
    let path = milestone_file_path(env, id);
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&raw).unwrap();
    m["milestone"]["lifecycle"] = serde_json::json!("complete");
    m["milestone"]["spec_status"] = serde_json::json!("verified");
    m["milestone"]["execution_status"] = serde_json::json!("done");
    m["verification"] = serde_json::json!({
        "date": "2026-07-13",
        "branch": "main",
        "evidence": evidence,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
}

fn fixture_id(env: &TestEnv, _slug: &str) -> String {
    // Discover the milestone file id from the copied fixture. The fixture
    // ships 01-foundation.json + 02-feature-alpha.json — both work for
    // these tests because we patch lifecycle directly to "complete".
    let plan_dir = env.tmp.path().join("master-plan/milestones");
    let mut entries: Vec<_> = std::fs::read_dir(&plan_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x == "json")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let entry = entries.first().expect("fixture has at least one milestone");
    let stem = entry
        .path()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    // stem looks like "01-foundation"; id is the leading number.
    stem.split('-').next().unwrap().to_string()
}

/// AC-01 + AC-02 (force_bypassed flip).
#[test]
fn milestone_update_amends_evidence() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env, "minimal-ready");
    force_complete_with_evidence(
        &env,
        &id,
        "[force-bypassed: prior AC-01 threshold unmet; tracked under B-83]",
    );

    // Sanity: summary reports force_bypassed=true before amend.
    let summary_before: serde_json::Value = serde_json::from_slice(
        &run_mp(
            &env,
            &["show", "milestone", &id, "--summary", "--format", "json"],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(
        summary_before["verification"]["force_bypassed"], true,
        "fixture should start with force_bypassed=true (the [force-bypassed marker is in evidence)"
    );

    // Run the new surface: --verification flag rewrites evidence.
    let amend = run_mp(
        &env,
        &[
            "milestone",
            "update",
            &id,
            "--verification",
            "evidence amended; no force-bypass marker",
        ],
    );
    assert!(
        amend.status.success(),
        "mp milestone update --verification failed: {}",
        String::from_utf8_lossy(&amend.stderr)
    );

    // AC-01: evidence updated; lifecycle / spec_status / execution_status
    // preserved; milestone.updated flips.
    let after = read_milestone(&env, &id);
    assert_eq!(
        after["verification"]["evidence"], "evidence amended; no force-bypass marker",
        "amend must persist the new evidence string"
    );
    assert_eq!(
        after["milestone"]["lifecycle"], "complete",
        "lifecycle must be preserved"
    );
    assert_eq!(
        after["milestone"]["spec_status"], "verified",
        "spec_status must be preserved"
    );
    assert_eq!(
        after["milestone"]["execution_status"], "done",
        "execution_status must be preserved"
    );
    assert!(
        after["milestone"]["updated"].as_str().is_some(),
        "milestone.updated should be a date string after the amend"
    );

    // AC-02: summary's force_bypassed flips to false (marker removed).
    let summary_after: serde_json::Value = serde_json::from_slice(
        &run_mp(
            &env,
            &["show", "milestone", &id, "--summary", "--format", "json"],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(
        summary_after["verification"]["force_bypassed"], false,
        "force_bypassed should flip to false after the marker is removed"
    );
}

/// AC-01 (lifecycle preservation) + AC-03 (absent --verification preserves value).
#[test]
fn verifies_unset_field_preserves_value() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env, "minimal-ready");
    force_complete_with_evidence(&env, &id, "preexisting evidence; do not clobber");

    let before = read_milestone(&env, &id);
    let title_before = before["milestone"]["title"].as_str().unwrap().to_string();
    let updated_before = before["milestone"]["updated"].as_str().unwrap().to_string();

    // Update a different field (--json with `title` only) — verification
    // must NOT be touched. The M165 surface added `--verification` as a
    // sibling to `--json`; absent here, the existing verification block
    // stays.
    let amend = run_mp(
        &env,
        &[
            "milestone",
            "update",
            &id,
            "--json",
            r#"{"title": "Title rewritten by verifies_unset_field_preserves_value"}"#,
        ],
    );
    assert!(
        amend.status.success(),
        "mp milestone update --json (no --verification) failed: {}",
        String::from_utf8_lossy(&amend.stderr)
    );

    let after = read_milestone(&env, &id);
    assert_eq!(
        after["verification"]["evidence"], "preexisting evidence; do not clobber",
        "absent --verification must preserve the existing evidence"
    );
    assert_eq!(
        after["verification"]["date"], "2026-07-13",
        "absent --verification-date must preserve the existing date"
    );
    assert_eq!(
        after["verification"]["branch"], "main",
        "absent --verification-branch must preserve the existing branch"
    );
    assert_eq!(
        after["milestone"]["title"], "Title rewritten by verifies_unset_field_preserves_value",
        "title should reflect the --json title field"
    );
    assert_ne!(
        after["milestone"]["updated"], updated_before,
        "milestone.updated should flip even when verification is untouched"
    );
    assert_ne!(
        after["milestone"]["title"], title_before,
        "control: title-before must differ from title-after; otherwise this test is not exercising the amend path"
    );
}

/// AC-03: `--verification-file <path>` reads the evidence text from disk.
/// (Long evidence values would otherwise need shell-escape gymnastics.)
#[test]
fn milestone_update_verification_file_reads_from_disk() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env, "minimal-ready");
    force_complete_with_evidence(&env, &id, "short");

    let evidence_path = env.tmp.path().join("long-evidence.txt");
    std::fs::write(
        &evidence_path,
        "long evidence string read from a file\nwith a newline and some surrounding text",
    )
    .unwrap();

    let amend = run_mp(
        &env,
        &[
            "milestone",
            "update",
            &id,
            "--verification-file",
            evidence_path.to_str().unwrap(),
        ],
    );
    assert!(
        amend.status.success(),
        "mp milestone update --verification-file failed: {}",
        String::from_utf8_lossy(&amend.stderr)
    );

    let after = read_milestone(&env, &id);
    assert_eq!(
        after["verification"]["evidence"],
        "long evidence string read from a file\nwith a newline and some surrounding text",
        "--verification-file should read the file contents into verification.evidence"
    );
}

/// AC-03: `--verification-date` and `--verification-branch` set the companion
/// fields independently of `--verification`.
#[test]
fn milestone_update_verification_date_and_branch_set_companions() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env, "minimal-ready");
    force_complete_with_evidence(&env, &id, "evidence text");

    let amend = run_mp(
        &env,
        &[
            "milestone",
            "update",
            &id,
            "--verification-date",
            "2026-07-14",
            "--verification-branch",
            "m165-fixup",
        ],
    );
    assert!(
        amend.status.success(),
        "mp milestone update --verification-date / --verification-branch failed: {}",
        String::from_utf8_lossy(&amend.stderr)
    );

    let after = read_milestone(&env, &id);
    assert_eq!(
        after["verification"]["date"], "2026-07-14",
        "--verification-date should write the supplied date"
    );
    assert_eq!(
        after["verification"]["branch"], "m165-fixup",
        "--verification-branch should write the supplied branch name"
    );
    assert_eq!(
        after["verification"]["evidence"], "evidence text",
        "absent --verification / --verification-file must preserve the existing evidence"
    );
}

/// AC-01 / AC-03 negative-path: an empty `--verification` value is rejected
/// before the marker-removal flip can be silently exercised. Pins the
/// guard at `crates/mp/src/commands/milestone.rs:419`.
#[test]
fn empty_verification_text_refuses_to_clobber() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env, "minimal-ready");
    force_complete_with_evidence(&env, &id, "[force-bypassed: legacy marker] preserve this");

    let amend = run_mp(&env, &["milestone", "update", &id, "--verification", ""]);
    assert!(
        !amend.status.success(),
        "empty --verification must fail; got {:?}",
        amend.status
    );
    let stderr = String::from_utf8_lossy(&amend.stderr);
    assert!(
        stderr.contains("--verification text is empty"),
        "expected structured error in stderr, got: {stderr}"
    );

    // Evidence must NOT have been clobbered.
    let after = read_milestone(&env, &id);
    assert_eq!(
        after["verification"]["evidence"], "[force-bypassed: legacy marker] preserve this",
        "empty --verification must leave the existing evidence untouched"
    );
}

/// AC-03 negative-path: an empty file passed to `--verification-file` is
/// rejected before the file is read into evidence. Pins the guard at
/// `crates/mp/src/commands/milestone.rs:435`.
#[test]
fn empty_verification_file_refuses_to_clobber() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env, "minimal-ready");
    force_complete_with_evidence(&env, &id, "preserve this");

    let evidence_path = env.tmp.path().join("empty-evidence.txt");
    std::fs::write(&evidence_path, "").unwrap();

    let amend = run_mp(
        &env,
        &[
            "milestone",
            "update",
            &id,
            "--verification-file",
            evidence_path.to_str().unwrap(),
        ],
    );
    assert!(
        !amend.status.success(),
        "empty --verification-file must fail; got {:?}",
        amend.status
    );
    let stderr = String::from_utf8_lossy(&amend.stderr);
    assert!(
        stderr.contains("is empty"),
        "expected structured error in stderr, got: {stderr}"
    );

    let after = read_milestone(&env, &id);
    assert_eq!(
        after["verification"]["evidence"], "preserve this",
        "empty --verification-file must leave the existing evidence untouched"
    );
}

/// AC-03 negative-path: an empty path passed to `--verification-file` is
/// rejected before the file is read. Pins the empty-path guard added by
/// the M165 ext-review.
#[test]
fn empty_verification_file_path_refuses_to_clobber() {
    let env = TestEnv::from_fixture("minimal-ready");
    let id = fixture_id(&env, "minimal-ready");
    force_complete_with_evidence(&env, &id, "preserve this");

    // Pass an explicit empty value via `--verification-file=`; clap parses
    // this to Some(PathBuf::from("")) in normal builds.
    let amend = run_mp(
        &env,
        &["milestone", "update", &id, "--verification-file", ""],
    );
    assert!(
        !amend.status.success(),
        "empty --verification-file path must fail; got {:?}",
        amend.status
    );
    let stderr = String::from_utf8_lossy(&amend.stderr);
    assert!(
        stderr.contains("--verification-file"),
        "expected structured error mentioning --verification-file, got: {stderr}"
    );

    let after = read_milestone(&env, &id);
    assert_eq!(
        after["verification"]["evidence"], "preserve this",
        "empty --verification-file path must leave the existing evidence untouched"
    );
}
