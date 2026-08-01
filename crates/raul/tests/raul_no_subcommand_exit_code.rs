//! M164 AC-06: unknown subcommand exits non-zero with migration sentinel.

use std::process::Command;

#[test]
fn unknown_subcommand_prints_sentinel_and_exits_nonzero() {
    let bin = env!("CARGO_BIN_EXE_raul");
    let out = Command::new(bin)
        .arg("status")
        .output()
        .expect("spawn raul");
    assert!(
        !out.status.success(),
        "raul status must fail after M164; got {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("subcommands removed in M164; launch the TUI"),
        "stderr must contain M164 sentinel; got: {stderr}"
    );
}

#[test]
fn help_still_exits_zero() {
    let bin = env!("CARGO_BIN_EXE_raul");
    let out = Command::new(bin).arg("--help").output().expect("spawn");
    assert!(out.status.success(), "raul --help should succeed");
}

/// Coverage gap (M164 review): a malformed *flag* (not a subcommand) must
/// reach clap's usage hint rather than the M164 sentinel, so users with
/// real flag typos see actionable feedback instead of "launch the TUI".
#[test]
fn malformed_flag_does_not_emit_migration_sentinel() {
    let bin = env!("CARGO_BIN_EXE_raul");
    let out = Command::new(bin)
        .arg("--bogus")
        .output()
        .expect("spawn raul");
    assert!(!out.status.success(), "raul --bogus must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("subcommands removed in M164"),
        "unknown flag must not emit the migration sentinel; got: {stderr}"
    );
}
