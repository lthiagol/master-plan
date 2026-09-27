# State and persistence

## State taxonomy

Two kinds of state on the autopilot tab:

1. **Configuration** — what the user *chose* (topology, harness, milestones,
   commit policy, run mode). Survives a refresh; survives a session restart.
2. **Live run state** — what's *currently happening* (the runner's view of
   the active session, telemetry counters, activity tail). Survives a tab
   refresh only.

Each lives in a different file. See the table.

## File map

| File | Owner | Purpose | Writes when |
|---|---|---|---|
| `.mp/autopilot-state.json` | raul (this tab) | Setup choices + UI state (sidebar tab, picker open, drawer open, last selected milestones) | on every chip click; on focus change; on each cycle boundary |
| `.mp/autopilot-recent.json` | raul | Recent runs history (per Q5: per-repo, lives in `.mp/`) | when a run reaches "completed" or "aborted" |
| `master-plan/decisions/autopilot/state.json` | raul → mp | Cross-session config the runner reads at Start. Includes topology, harness per role, extras. | on Start; on Stop (rollback); never silently |
| `master-plan/autopilot/<session-id>.json` | mp / herdr | Live run state (pane count, active run id, last cycle, role assignments) | every cycle; every 250ms by telemetry worker |
| `master-plan/activity.json` | mp | Audit trail of plan events (also touched by `mp milestone`, `mp reviews`, etc.) | every lifecycle event from any source |

## What's in `.mp/autopilot-state.json`

```json
{
  "version": 2,
  "last_run": {
    "id": "M236-...-M240",
    "completed_at": "2026-09-27T11:25:24Z",
    "exit": "complete",
    "milestones": ["M236", "M238", "M234"]
  },
  "setup": {
    "topology": "3-agent",
    "harness_uniform": true,
    "harness": {
      "orchestrator": "opencode",
      "runner": "opencode",
      "verifier": "opencode"
    },
    "milestones": ["M236", "M238", "M234"],
    "commit_policy": "per-finding",
    "run_mode": "detached"
  },
  "ui": {
    "sidebar_tab": "progress",
    "picker_open": false,
    "sidebar_visible": true,
    "split_pct": 40,
    "while_executing_last_seen": "2026-09-27T11:14:23Z"
  }
}
```

## What's NOT in this file (and where it lives instead)

- **Commit policy** also lives in `master-plan/decisions/autopilot/commit.json`
  (a per-project setting the runner reads). The tab UI mirrors it; if the
  file says "per-finding" but the UI says "per-step", the user sees a
  "configuration drift" warning at the top of the setup region.
- **Harness per role** also lives in `master-plan/decisions/autopilot/harness.json`
  (same mirror pattern).
- **Recent runs** lives in **`.mp/autopilot-recent.json`** (per-repo,
  per Q5). Same `.mp/` that holds session.json; gitignored; lives at
  the repo root, *sibling* of `master-plan/`.

## Harness enumeration (per Q7)

The harness chips read from a hardcoded list in the raul source (per Q7):

```rust
const ALLOWED_HARNESSES: &[&str] = &["opencode", "cursor", "pi"];
```

When a new harness lands in the runtime, a new raul release is required
to expose it in the UI. Tracking new harnesses is a release-driven
process; the harness names match those in `master-plan/decisions/autopilot/harness.json`.

## Topo id display (per Q8)

The advanced-override panel's `Topo id` is **auto-derived** from the
chosen topology + a per-plan counter:

- Pick `1-agent` → `one-agent-001`
- Pick `2-agent` → `two-agent-001`
- Pick `3-agent` → `three-agent-001`

The field is read-only — the user sees the id but cannot edit it. The
counter increments once per plan, stored in
`.mp/autopilot-state.json::setup.topo_counter`.

## Activity tail length (per Q6)

The activity tail uses an **auto-sized** height:
- Always renders at least 8 lines.
- Grows to fill the remaining vertical space in the sidebar (Progress tab
  in idle state) or in the While Executing full screen.
- Cap at 30 lines on tall terminals (prevents the activity tail from
  pushing the controls off-screen).

## Persistence on close

When the user closes raul:
- The `.mp/autopilot-state.json` is written on every chip click already, so
  the file is up-to-date.
- The setup region returns to the main screen as if `q` had been pressed.
- The live runner (if detached) keeps running independently. Reopen raul →
  it reconnects and the While Executing view shows current state.

## Migration / schema versioning

`.mp/autopilot-state.json` carries a `version` field. On load, if version is
older than the current schema, migrate forward:
- v1 → v2: rename `harness_global` to `harness_uniform` + add nested
  `harness.{role}` object.
- Drop older versions after a single minor release.

## What we explicitly do NOT persist

- The picker's filter / search state — it's an in-progress selection, not a
  user choice.
- The `Esc to close` timer on accidental outside-clicks — ephemeral.
- The mouse drag offset from the split divider — only the final `split_pct`
  value is persisted (drags are debounced).
- The 100ms Start-button pulse animation per Q10 — ephemeral.

## Conflict resolution

If both `.mp/autopilot-state.json` and `master-plan/decisions/autopilot/state.json`
disagree, the **decisions/ file wins** for runtime semantics (it's the
authoritative source of "what will run"); the **state.json** wins for UI
display (it's the user's last interaction). The setup region shows a
`🛈 drift detected` banner if the two diverge and a `view diff` button to
open a side-by-side comparison.
