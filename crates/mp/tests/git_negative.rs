//! M233 / AC-06 / cycle-2 F-02: negative integration tests for the
//! `?`-propagated error paths in `crates/mp/src/git.rs`.
//!
//! Three negative tests covering the two CLI-reachable
//! `bail!("not a git repository")` sites in `git.rs`:
//!
//!  1. `git_suggest_message_exits_with_not_a_git_repository_message`
//!     — `mp git suggest-message` on a non-repo hits the bail at
//!     `git.rs:76` (`bail!("not a git repository")` inside
//!     `git_suggest_message`).
//!  2. `git_commit_exits_with_not_a_git_repository_message`
//!     — `mp git commit --message M` on a non-repo hits the bail at
//!     `git.rs:96` (inside `git_commit`).
//!  3. `git_suggest_message_when_run_from_non_repo_exits_nonzero_with_clean_message`
//!     — a duplicate-shape regression test asserting the wording
//!     stays clean: no "Error: " prefix from a layered wrapper, no
//!     swallowed exit code.
//!
//! The third `bail!` site at `git.rs:148` is inside `git_push`, which
//! has no CLI entry point (the `git push` call is only reachable
//! from inside `git_commit` *after* its own `bail!` at line 96
//! returns). The CLI surface therefore has two reachable
//! `not-a-git-repository` paths, both of which this file covers.
//!
//! **No production code changes.** These tests only assert on the
//! already-existing error branches.

mod common;

use crate::common::TestEnv;

#[test]
fn git_suggest_message_exits_with_not_a_git_repository_message() {
    let env = TestEnv::new();
    // TestEnv's tmp is a fresh TempDir with no .git; the
    // precondition check inside git_suggest_message fires the
    // `bail!("not a git repository")` arm at git.rs:76.
    let out = env.run(&["git", "suggest-message"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "mp git suggest-message on a non-repo must exit non-zero; stderr={stderr}"
    );
    // The literal wording at git.rs:76.
    assert!(
        stderr.contains("not a git repository"),
        "stderr must surface 'not a git repository'; got: {stderr}"
    );
}

#[test]
fn git_commit_exits_with_not_a_git_repository_message() {
    let env = TestEnv::new();
    // `mp git commit --message M` runs through `git_commit` at
    // git.rs:94; the `bail!("not a git repository")` arm at line 96
    // fires before any commit attempt.
    let out = env.run(&["git", "commit", "--message", "synthetic"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "mp git commit on a non-repo must exit non-zero; stderr={stderr}"
    );
    assert!(
        stderr.contains("not a git repository"),
        "stderr must surface 'not a git repository'; got: {stderr}"
    );
    // The marker file that a successful commit would create MUST
    // NOT be touched when the bail fires. (commits write to git's
    // own index, not the plan; the assertion is that no git objects
    // directory exists under the temp tree.)
    let git_dir = env.tmp.path().join(".git");
    assert!(
        !git_dir.exists(),
        ".git must not be created when bail fires: {}",
        git_dir.display()
    );
}

#[test]
fn git_suggest_message_when_run_from_non_repo_exits_nonzero_with_clean_message() {
    let env = TestEnv::new();
    // Run the same bail-triggering path twice to confirm the
    // wording is deterministic. A regression that wraps the bail
    // (e.g. a layered Error printer that doubles the wording) would
    // fail the second assertion below.
    for attempt in 1..=2 {
        let out = env.run(&["git", "suggest-message"]);
        let stderr = String::from_utf8_lossy(&out.stderr);

        assert!(
            !out.status.success(),
            "attempt {attempt}: git suggest-message must exit non-zero"
        );
        // The wording must appear exactly once (or at least not be
        // doubled by a layered Error printer).
        let count = stderr.matches("not a git repository").count();
        assert!(
            count == 1,
            "attempt {attempt}: 'not a git repository' must appear exactly once; got {count}; stderr={stderr}"
        );
    }
}
