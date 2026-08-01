#![allow(dead_code)]

//! Minimal test helpers for the mp-oracle crate.
//!
//! This is a deliberately small subset of `crates/mp/tests/common/mod.rs`.
//! The full common module pulls in a hardlink-snapshot retry layer
//! (M132) for `mp` subprocess spawns racing with cargo's rebuild. The
//! oracle suite runs only a handful of tests, so we accept the
//! simpler "spawn the workspace `target/debug/mp` directly" pattern.
//! If `mp` becomes a flaky spawn target from here, lift the snapshot
//! helpers from `crates/mp/tests/common/mod.rs` into this file.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Path to the workspace root (two levels up from `CARGO_MANIFEST_DIR`,
/// which is `crates/mp-oracle`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// Workspace-built `mp` binary path. Cached so callers don't re-walk the
/// path on every spawn.
///
/// This is intentionally simpler than `crates/mp/tests/common::mp_bin`:
/// no hardlink snapshot, no retry loop. The oracle suite is tiny
/// (≤2 tests) so the rebuild-race failure mode `mp_bin` guards against
/// is unlikely to surface here.
pub fn mp_bin() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| repo_root().join("target").join("debug").join("mp"))
}

/// Minimal test environment — wraps a tempdir plus a runner that spawns
/// the workspace `mp` binary with `MP_HOME` pointed at the real plan.
pub struct TestEnv {
    pub tmp: tempfile::TempDir,
}

impl TestEnv {
    pub fn new() -> Self {
        let env = Self::blank();
        assert!(
            env.run(&["init", "--profile", "full", "--format", "json"])
                .status
                .success(),
            "mp init --profile full failed"
        );
        env
    }

    pub fn blank() -> Self {
        Self {
            tmp: tempfile::TempDir::new().expect("temp"),
        }
    }

    pub fn run(&self, args: &[&str]) -> std::process::Output {
        let args = args.to_vec();
        Command::new(mp_bin())
            .current_dir(self.tmp.path())
            .env("MP_HOME", repo_root())
            .args(&args)
            .output()
            .expect("mp spawn")
    }
}
