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
| `.mp/autopilot-state.last-run.json` | raul | Last completed run summary (sticky audit) | when a run reaches "completed" or "aborted" |
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
    "split_pct": 40
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
- **Recent runs** lives in `~/.ra_cache/autopilot-recent.json` (per-user, not
  per-plan, because "what I ran today" is a personal log).

## Persistence on close

When the user closes raul:
- The `.mp/autopilot-state.json` is written on every chip click already, so
  the file is up-to-date.
- The setup region returns to the main screen as if `q` had been pressed.
- The live runner (if detached) keeps running independently. Reopen raul →
  it reconnects and the sidebar Progress tab shows current state.

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

## Conflict resolution

If both `.mp/autopilot-state.json` and `master-plan/decisions/autopilot/state.json`
disagree, the **decisions/ file wins** for runtime semantics (it's the
authoritative source of "what will run"); the **state.json** wins for UI
display (it's the user's last interaction). The setup region shows a
`🛈 drift detected` banner if the two diverge and a `view diff` button to
open a side-by-side comparison.
