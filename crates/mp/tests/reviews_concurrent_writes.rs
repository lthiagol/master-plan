//! M133 H2: `mp reviews comment add` and `mp reviews handoff` are
//! serialized through the plan-write advisory lock, so N parallel
//! invocations against the same plan must all land. Pre-M133 advisory-
//! lock wrap, the read-modify-write window in `add_comment` /
//! `record_handoff` (load_reviews → compute next_id → push → save) lost
//! the last writer. This test is the regression guard for the lock
//! wrap at `commands/reviews.rs::cmd_reviews` (mirroring
//! `commands/milestone.rs::cmd_milestone`'s M113 wrap).

mod common;

use crate::common::TestEnv;

fn write_milestone(env: &TestEnv, id: &str, slug: &str, title: &str) {
    use std::fs;
    let dir = env.tmp.path().join("master-plan/milestones");
    fs::create_dir_all(&dir).unwrap();
    let milestone = serde_json::json!({
        "milestone": {
            "id": id,
            "title": title,
            "slug": slug,
            "spec_status": "ready",
            "execution_status": "planned",
            "depends_on": [],
            "effort": "S",
            "risk": "low",
            "change_kind": "",
            "priority": "normal",
            "created": "2026-07-09",
            "updated": "2026-07-09",
            "blocked_at": "",
            "block_reason": "",
            "blocked_by": "",
        },
        "intent": { "outcome": "x" },
        "problem": { "description": "x" },
        "scope": { "in_scope": ["x"], "out_of_scope": ["a", "b"] },
        "acceptance_criteria": [{
            "id": "AC-01",
            "description": "x",
            "verification": "manual: test",
            "status": "pending",
            "evidence": "",
        }],
    });
    let json = serde_json::to_string_pretty(&milestone).unwrap();
    fs::write(dir.join(format!("{id}-{slug}.json")), format!("{json}\n")).unwrap();
    let out = env.run(&["sync", "--format", "json"]);
    assert!(
        out.status.success(),
        "sync failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn parallel_comment_adds_all_land() {
    // Fire N parallel `mp reviews comment add` invocations against one
    // milestone. The advisory lock around the entire `cmd_reviews`
    // dispatch (mirroring M113's `cmd_milestone` wrap) serializes
    // them; without the wrap, two processes would both load identical
    // ReviewsFile, both compute the same `next_comment_id`, and last
    // writer wins — silently losing the comment.
    const N: usize = 8;

    let env = TestEnv::new();
    write_milestone(
        &env,
        "133",
        "reviews-concurrent-comments",
        "Concurrent comments",
    );

    use std::process::Command;
    use std::thread;

    let mut handles = Vec::new();
    for n in 0..N {
        let cwd = env.tmp.path().to_path_buf();
        let author = format!("reviewer-{n}");
        let body = format!("concurrent comment from thread {n}");
        handles.push(thread::spawn(move || {
            let child = Command::new(common::mp_bin())
                .current_dir(cwd)
                .env("MP_HOME", common::repo_root())
                .args([
                    "reviews", "comment", "add", "133", "--author", &author, "--body", &body,
                    "--format", "json",
                ])
                .output();
            (n, child)
        }));
    }

    let mut failures: Vec<(usize, String)> = Vec::new();
    for handle in handles {
        let (i, child) = handle.join().expect("thread panic");
        let child = child.expect("spawn mp");
        if !child.status.success() {
            failures.push((
                i,
                format!(
                    "comment add failed: status={:?} stderr={}",
                    child.status,
                    String::from_utf8_lossy(&child.stderr)
                ),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "every parallel comment add must succeed; failures: {failures:?}"
    );

    // Read back: every reviewer-{n} author must appear exactly once.
    let list = env.run(&["reviews", "comment", "list", "133", "--format", "json"]);
    assert!(
        list.status.success(),
        "comment list failed: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let comments = v["comments"].as_array().expect("comments array");
    assert_eq!(
        comments.len(),
        N,
        "all N comments must land — pre-M133 advisory-lock wrap, last writer wins; landed: {comments:?}"
    );

    let authors: std::collections::HashSet<String> = comments
        .iter()
        .filter_map(|c| c["author"].as_str().map(|s| s.to_string()))
        .collect();
    for n in 0..N {
        let want = format!("reviewer-{n}");
        assert!(
            authors.contains(&want),
            "reviewer {n} must have landed; landed: {authors:?}"
        );
    }
}

#[test]
fn parallel_handoff_records_all_land() {
    // Same race window as comments, exercised for handoffs. Each
    // thread records a hand-off; the lock wrap ensures each gets a
    // unique H-NN id and none are silently overwritten.
    const N: usize = 8;

    let env = TestEnv::new();
    write_milestone(
        &env,
        "133",
        "reviews-concurrent-handoffs",
        "Concurrent handoffs",
    );

    use std::process::Command;
    use std::thread;

    let mut handles = Vec::new();
    for n in 0..N {
        let cwd = env.tmp.path().to_path_buf();
        let from = format!("role-{n}A");
        let to = format!("role-{n}B");
        let data = format!("concurrent hand-off payload {n}");
        handles.push(thread::spawn(move || {
            let child = Command::new(common::mp_bin())
                .current_dir(cwd)
                .env("MP_HOME", common::repo_root())
                .args([
                    "reviews",
                    "handoff",
                    "133",
                    "--from-session",
                    &from,
                    "--to-session",
                    &to,
                    "--data",
                    &data,
                    "--format",
                    "json",
                ])
                .output();
            (n, child)
        }));
    }

    let mut failures: Vec<(usize, String)> = Vec::new();
    for handle in handles {
        let (i, child) = handle.join().expect("thread panic");
        let child = child.expect("spawn mp");
        if !child.status.success() {
            failures.push((
                i,
                format!(
                    "handoff failed: status={:?} stderr={}",
                    child.status,
                    String::from_utf8_lossy(&child.stderr)
                ),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "every parallel handoff must succeed; failures: {failures:?}"
    );

    // The reviews.json file should hold N handoffs. Without the
    // advisory lock, two writers would land at the same H-NN id and
    // only N-1 (or fewer) would survive.
    let reviews_path = env.tmp.path().join("master-plan/reviews.json");
    let text = std::fs::read_to_string(&reviews_path).expect("read reviews.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse reviews.json");
    let handoffs = v["handoffs"].as_array().expect("handoffs array");
    assert_eq!(
        handoffs.len(),
        N,
        "all N handoffs must land — pre-M133 advisory-lock wrap, last writer wins; landed: {handoffs:?}"
    );
    let ids: std::collections::HashSet<String> = handoffs
        .iter()
        .filter_map(|h| h["id"].as_str().map(|s| s.to_string()))
        .collect();
    assert_eq!(
        ids.len(),
        N,
        "every handoff must have a unique H-NN id (advisory lock prevents id collision); ids: {ids:?}"
    );
}
