//! M233 / AC-06: negative integration tests driving the `?`-propagated
//! error paths in `crates/mp/src/commands/archive.rs` to non-zero exits.
//!
//! Three tests covering three distinct production bail sites:
//!
//!   1. `archive_track_item_unknown_id_exits_with_not_found_message`
//!      — `mp track archive track-item tweak nonexistent` hits the
//!      `.with_context(|| format!("item {id} not found"))?` at line 26
//!      (the `Item` not-found in the TrackItem arm).
//!   2. `restore_archived_unknown_entity_type_exits_with_unknown_type_message`
//!      — `mp restore archived bogus-type id` hits the
//!      `bail!("unknown type {entity_type}")` at line 87.
//!   3. `purge_archived_without_confirm_flag_exits_with_requires_confirm_message`
//!      — `mp purge archived --type milestone --id 1` (without
//!      `--confirm`) hits the `bail!("purge requires --confirm")` at
//!      line 106.
//!
//! **No production-code changes** — these tests only assert on the
//! already-existing error branches.

mod common;

use crate::common::TestEnv;

#[test]
fn archive_track_item_unknown_id_exits_with_not_found_message() {
    let env = TestEnv::new();
    let out = env.run(&[
        "track",
        "archive",
        "track-item",
        "tweak",
        "definitely-not-a-real-id-xyz",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "archive of an unknown track item must exit non-zero; stderr={stderr}"
    );
    // The exact wording the production code uses at archive.rs:26.
    assert!(
        stderr.contains("not found"),
        "stderr must surface the 'not found' wording; got: {stderr}"
    );
    // The offending id surfaces so operators know which item was bad.
    assert!(
        stderr.contains("definitely-not-a-real-id-xyz"),
        "stderr must echo the offending id; got: {stderr}"
    );
}

#[test]
fn restore_archived_unknown_entity_type_exits_with_unknown_type_message() {
    let env = TestEnv::new();
    // `restore` and `purge` live under `mp track` (per
    // crates/mp/src/cli/track.rs:48-50) — `mp restore` /
    // `mp purge` are not registered.
    let out = env.run(&[
        "track",
        "restore",
        "archived",
        "bogus-type",
        "some-id",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "restore of an unknown entity type must exit non-zero; stderr={stderr}"
    );
    // The literal wording at archive.rs:87.
    assert!(
        stderr.contains("unknown type"),
        "stderr must surface 'unknown type' wording; got: {stderr}"
    );
    assert!(
        stderr.contains("bogus-type"),
        "stderr must echo the offending entity_type; got: {stderr}"
    );
}

#[test]
fn purge_archived_without_confirm_flag_exits_with_requires_confirm_message() {
    let env = TestEnv::new();
    // `mp track purge archived` takes positional args (entity_type,
    // id), not `--type` / `--id` flags.
    let out = env.run(&[
        "track",
        "purge",
        "archived",
        "milestone",
        "01",
        "--format",
        "json",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "purge without --confirm must exit non-zero; stderr={stderr}"
    );
    // The literal wording at archive.rs:106.
    assert!(
        stderr.contains("purge requires --confirm"),
        "stderr must surface the --confirm requirement; got: {stderr}"
    );
}
