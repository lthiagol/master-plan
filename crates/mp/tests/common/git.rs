//! Shared git-init helpers for tests that need a repo on the test tmp.
//!
//! Previously duplicated in:
//!   - crates/mp/tests/suites/git_suggest.rs:6
//!   - crates/mp/tests/suites/p12_recommendation_batch.rs:5
//!
//! Both versions were byte-identical; both suites now
//! `use crate::common::git::init_git;`.

use std::process::Command;

use crate::common::TestEnv;

/// `git init` + `user.email` + `user.name` on `env.tmp.path()`.
///
/// Caller is responsible for any subsequent `git add` / `git commit` /
/// branch / remote setup; this helper only sets up the local repo +
/// identity so commands like `mp git status` have something to query.
pub fn init_git(env: &TestEnv) {
    let root = env.tmp.path();
    Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(root)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .expect("git config name");
}
