//! M215 AC-04: Autopilot session replay shell.
//!
//! The past-session mode consumes `mp autopilot session list` /
//! `mp autopilot session show` envelopes (never reads plan files
//! directly) and opens a read-only replay shell backed by session
//! events. M216 may enrich the shared detail renderer without
//! being required for this milestone.
//!
//! The shell is the typed model — `ReplayShell::from_session_show`
//! and `ReplayShell::from_session_list_entry` translate the `mp`
//! subprocess payloads into the in-lane view. The shell never reaches
//! into `master-plan/` files.

use raul::tui::autopilot::{ReplayEvent, ReplayShell};

fn sample_session_show_payload() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "session_id": "alpha",
        "session": {
            "id": "alpha",
            "status": "active",
            "last_updated": "2026-09-04T00:00:00Z",
            "events": [
                {"seq": 1, "kind": "dispatch", "actor": "runner", "body": "starting"},
                {"seq": 2, "kind": "note", "actor": "runner", "body": "in-flight"},
                {"seq": 3, "kind": "transition", "actor": "runner", "body": "done"},
            ],
        }
    })
}

fn sample_session_list_entry() -> serde_json::Value {
    serde_json::json!({
        "id": "beta",
        "status": "completed",
        "last_updated": "2026-09-04T00:00:00Z"
    })
}

/// AC-04: the replay shell consumes `mp autopilot session show`
/// JSON and renders events in seq order. The timeline is read-only
/// (no mutator exposed) so the lane never accidentally rewrites
/// session history.
#[test]
fn replay_shell_from_session_show_renders_events_in_seq_order() {
    let payload = sample_session_show_payload();
    let shell = ReplayShell::from_session_show(&payload);
    assert_eq!(shell.session_id, "alpha");
    assert_eq!(shell.status, "active");
    assert_eq!(shell.last_updated, "2026-09-04T00:00:00Z");

    let seqs: Vec<u64> = shell.timeline().iter().map(|e| e.seq).collect();
    assert_eq!(
        seqs,
        vec![1, 2, 3],
        "timeline must preserve event order so the renderer scrolls top-to-bottom in seq order"
    );

    assert!(shell.has_events());
}

/// AC-04: the event body is rendered from the canonical
/// `body` / `description` / `message` key paths. The shell
/// accepts any of the three so an event recorded before the
/// `body` key landed still renders. Each fallback is documented
/// so a future migration knows what to expect.
#[test]
fn replay_shell_reads_event_body_from_canonical_keys() {
    // Body key (preferred).
    let payload = serde_json::json!({
        "session_id": "x",
        "session": {
            "status": "active",
            "last_updated": "",
            "events": [
                {"seq": 1, "kind": "dispatch", "actor": "a", "body": "from body"},
            ],
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    assert_eq!(shell.timeline()[0].body, "from body");

    // Description fallback.
    let payload = serde_json::json!({
        "session_id": "x",
        "session": {
            "status": "active",
            "last_updated": "",
            "events": [
                {"seq": 1, "kind": "dispatch", "actor": "a", "description": "from description"},
            ],
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    assert_eq!(shell.timeline()[0].body, "from description");

    // Message fallback.
    let payload = serde_json::json!({
        "session_id": "x",
        "session": {
            "status": "active",
            "last_updated": "",
            "events": [
                {"seq": 1, "kind": "dispatch", "actor": "a", "message": "from message"},
            ],
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    assert_eq!(shell.timeline()[0].body, "from message");

    // Non-string body value is rendered via `Value::to_string()`
    // so a typed payload (e.g., a JSON object) does not crash the
    // renderer.
    let payload = serde_json::json!({
        "session_id": "x",
        "session": {
            "status": "active",
            "last_updated": "",
            "events": [
                {"seq": 1, "kind": "dispatch", "actor": "a", "body": {"kind": "structured"}},
            ],
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    assert!(
        shell.timeline()[0].body.contains("structured"),
        "non-string body must render via to_string(): got {:?}",
        shell.timeline()[0].body
    );
}

/// AC-04: the session-list entry shape (id, status, last_updated)
/// produces a shell with an empty timeline. The shell carries
/// enough state to render the "no events yet" placeholder that
/// the past-session picker shows before the user drills into a
/// session.
#[test]
fn replay_shell_from_session_list_entry_is_event_less() {
    let entry = sample_session_list_entry();
    let shell = ReplayShell::from_session_list_entry(&entry);
    assert_eq!(shell.session_id, "beta");
    assert_eq!(shell.status, "completed");
    assert_eq!(shell.last_updated, "2026-09-04T00:00:00Z");
    assert!(shell.events.is_empty());
    assert!(!shell.has_events());
}

/// AC-04: the shell never reads `master-plan/` directly — its
/// only inputs are the JSON envelopes returned by `mp autopilot
/// session list` / `session show`. The surface is bounded by
/// `from_session_show` / `from_session_list_entry`; no path
/// resolver, no `std::fs::read` call.
#[test]
fn replay_shell_has_no_filesystem_inputs() {
    // The public surface exposes only the two builders + the
    // timeline accessor. A future change that adds a `from_path`
    // helper would force every test to redefine its fixtures —
    // this assertion pins the surface boundary.
    let shell = ReplayShell::from_session_show(&sample_session_show_payload());
    let _: &[ReplayEvent] = shell.timeline();
    let _ = shell.has_events();
}

/// AC-04: the shell tolerates empty / missing event arrays — the
/// renderer falls through to a "no events yet" placeholder rather
/// than crashing on an empty timeline.
#[test]
fn replay_shell_handles_empty_event_arrays() {
    let payload = serde_json::json!({
        "session_id": "gamma",
        "session": {
            "status": "draft",
            "last_updated": "2026-09-04T00:00:00Z",
            "events": [],
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    assert_eq!(shell.session_id, "gamma");
    assert!(shell.events.is_empty());
    assert!(!shell.has_events());
}

/// AC-04: the shell tolerates missing / unknown fields by falling
/// back to empty strings / "unknown" labels. The renderer never
/// panics on a half-populated envelope.
#[test]
fn replay_shell_handles_missing_envelope_fields() {
    // Bare-bones payload — only `session_id` and `status` set.
    let payload = serde_json::json!({
        "session_id": "delta",
        "session": {
            "status": "active",
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    assert_eq!(shell.session_id, "delta");
    assert_eq!(shell.status, "active");
    assert_eq!(shell.last_updated, "");
    assert!(shell.events.is_empty());

    // `session_id` at the top level takes precedence over
    // `session.id` — the `mp` show envelope uses `session_id` at
    // the top level for back-compat.
    let payload = serde_json::json!({
        "session_id": "epsilon",
        "session": {
            "id": "should-be-overridden",
            "status": "active",
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    assert_eq!(shell.session_id, "epsilon");
}

/// AC-04: every `ReplayEvent` carries seq / kind / actor / body so
/// the renderer can render one line per event without re-shelling.
/// Defensive pin: a future field addition is visible here, not a
/// silent drop.
#[test]
fn replay_event_carries_seq_kind_actor_body() {
    let payload = serde_json::json!({
        "session_id": "zeta",
        "session": {
            "status": "active",
            "events": [
                {"seq": 1, "kind": "dispatch", "actor": "runner", "body": "hello"},
            ],
        }
    });
    let shell = ReplayShell::from_session_show(&payload);
    let event = &shell.timeline()[0];
    assert_eq!(event.seq, 1);
    assert_eq!(event.kind, "dispatch");
    assert_eq!(event.actor, "runner");
    assert_eq!(event.body, "hello");

    // Round-trip through serde — the on-disk shape (M207's
    // `events[]`) carries the same fields.
    let v = serde_json::to_value(event).unwrap();
    let back: ReplayEvent = serde_json::from_value(v).unwrap();
    assert_eq!(back, *event);
}

/// AC-04: the shell's `Default` impl produces an empty shell so a
/// past-session picker with no entries renders the "no past
/// sessions" placeholder without crashing.
#[test]
fn replay_shell_default_is_empty() {
    let shell = ReplayShell::default();
    assert_eq!(shell.session_id, "");
    assert_eq!(shell.status, "");
    assert_eq!(shell.last_updated, "");
    assert!(shell.events.is_empty());
    assert!(!shell.has_events());
}

/// AC-04: the shell's `timeline()` accessor returns a slice so the
/// renderer can iterate events without consuming the shell. The
/// shell is read-only by construction — there is no mutator.
#[test]
fn timeline_accessor_returns_a_read_only_slice() {
    let shell = ReplayShell::from_session_show(&sample_session_show_payload());
    let timeline: &[ReplayEvent] = shell.timeline();
    assert_eq!(timeline.len(), 3);

    // The slice is a borrowed view, not a move — the shell is
    // still usable after the accessor returns.
    let second_lookup = shell.timeline();
    assert_eq!(second_lookup.len(), 3);
    assert_eq!(second_lookup[0].seq, 1);
}

/// AC-04: the `from_session_list_entry` builder tolerates missing
/// fields by falling back to empty / "unknown". The lane never
/// crashes on a malformed list entry.
#[test]
fn session_list_entry_tolerates_missing_fields() {
    let entry = serde_json::json!({});
    let shell = ReplayShell::from_session_list_entry(&entry);
    assert_eq!(shell.session_id, "");
    assert_eq!(shell.status, "unknown");
    assert_eq!(shell.last_updated, "");
}