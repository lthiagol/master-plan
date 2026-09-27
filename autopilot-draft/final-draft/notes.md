# Final draft — choices, tradeoffs, and resolutions

## What we chose (and why)

### Why option-03-style split geometry

You're reading setup and progress at the same time. That's the win of a
split. Sidebar tabs (option 09) add the *granularity* — within the right
half, you flip between progress / activity / state without losing the
setup region.

### Why option-09-style tabbed sidebar (not a single progress pane)

The Progress tab is the default. Activity / State tabs are *side-channels*
that aren't always relevant. Tabs let them live without forcing the user to
scroll or visit a separate screen.

### Why Model B (free click + focus)

You asked for "click on them using the mouse" first. Model B is closer to
that mental model than Model A (focus + click). Model A would have you
describing "click here" as a sequence "Tab to focus this, then click" which
contradicts what most users expect from a clickable button.

### Why a floating picker (not a modal picker)

A modal would take over the whole screen. The floating picker covers only
the middle 2/3 of the screen and dims the surroundings. The user can still
see their setup context (especially the priority column) while selecting.
That's an improvement on option-04's modal.

### Why the picker is checkbox-style (not radio)

Multi-select is the requirement. There's no "preselected bundle of
milestones" — the user picks. Checkbox is the natural fit.

### Why the control row lives under the Start button (not in the sidebar)

Pause / Stop / Resume are *primary* actions during a run. Putting them in
a sidebar tab would be a regression. The control row stays visible below
the Start button, dimmed when there's no live run.

### Why a `state.json` separate from `decisions/autopilot/state.json`

The first is the user's UI choices; the second is what the runner
actually uses. Two files means the runner doesn't need to read the UI
state, and the UI can show drift without affecting execution.

### Why a While-Executing full-screen takeover

When the run is live, the split view's setup region becomes irrelevant —
you can't edit config mid-run (the Pause + Edit flow is the supported
editing path). The While-Executing screen gives live state the full
vertical space, including per-milestone intent blurbs, cycle counters,
progress bars, and the persistent Pause / Stop / Resume row. The State
tab (idle-only) disappears during a run; per-milestone detail is the
peek view.

## Tradeoffs we accepted

- **Setup region is narrower** (40%). A 50/50 split is more balanced but
  the start button + summary + control row are tight at 50%.
- **Sidebar tab `1` / `2` / `3` may collide with terminal app shortcuts**.
  Most users can rebind their terminal app; raul doesn't try to be clever
  about detecting tmux.
- **Floating picker eats the central viewport**. On narrow terminals
  (< 100 cols), the picker can't fit, and we fall back to a modal picker
  (Q1).
- **State.json schema versioning** is a maintenance burden. We accept it
  for now; a future version can drop v1 entirely.
- **While-Executing screen is a full takeover** — users going back to
  setup during a run must Pause + Edit (with confirmation). This is the
  supported mid-run edit protocol.
- **Harnesses are hardcoded** in raul (per Q7). New harnesses need a new
  raul release to appear in the UI. We accept this for v1.
- **Recent runs is per-repo** (per Q5) — `.mp/autopilot-recent.json` lives
  at the repo root, sibling of `master-plan/`. Trade-off: if you have
  multiple repos, each gets its own list. Not shared across machines.
- **Detached-mode confirmation modal pops every click** (per Q2) — the
  safest default. Once a user has detached 5 times, the modal might
  annoy them; we can revisit with a "Don't ask again" after we have
  usage data.
- **Activity tail auto-sizes** (per Q6) — at least 8 lines, grows to fill,
  caps at 30. Sometimes short, sometimes long; users don't pin it.

## Resolutions (all 10 open questions, locked-in)

| # | Question | Resolution |
|---|---|---|
| Q1 | Floating picker's narrow-terminal fallback | Same internal state, modal renderer (~50 LOC extra) |
| Q2 | Detached-mode confirmation pattern | Confirm modal every click (safest default) |
| Q3 | `r` ambiguity (Run mode vs. resume) | Context-switch the binding (dynamic per state) |
| Q4 | Sidebar split bounds | Min 25%, max 75% |
| Q5 | Recent runs location | Per-repo (`.mp/autopilot-recent.json`) |
| Q6 | Activity tail length | Auto-sized (≥8 lines, fills, capped at 30) |
| Q7 | Harness enumeration | Hardcoded in raul (per release) |
| Q8 | Topo id display | Auto-derive from topology + per-plan counter |
| Q9 | State tab during a run + dedicated run screen | State tab hidden; new "While Executing" full-screen takeover |
| Q10 | Start-button feedback | Subtle 100ms cyan pulse on Confirm |

Q9 also adds a new design element: the **While Executing** full-screen
view (see `subscreens.md` §1) that replaces the split when a run is live.

## What we deliberately skipped (for later)

- **Pause-and-edit fully wired** — the popup is here, but the mid-run
  edit protocol beyond pre-fill isn't (the user can Pause + edit but the
  "Apply" path is staged for M242/M243).
- **`recent runs` as a stat dashboard** — counts and trends, future.
- **Telemetry history sparkline** — current draft shows numerical snapshot.
- **Harness plug-in enumeration** — confirmed per Q7 to be hardcoded; an
  extension point exists for a future auto-scan (deferred).
- **Workspace-scoped recent runs** — confirmed per Q5 to be per-repo;
  add a workspace mode later if multi-monorepo adoption shows up.

## Engineering-cost boundary

If we estimate by `crates/raul/src/tui/render/lane_lists.rs::render_autopilot_tab`
replacement scope:

- Setup region rewrite: ~300 LOC.
- Sidebar tabs: ~250 LOC.
- Floating picker (with narrow-terminal modal fallback): ~280 LOC.
- While Executing full-screen takeover: ~350 LOC.
- Other sub-screens (advanced override, detached confirm, peek view,
  activity log, state inspector): ~400 LOC.
- State.json read/write: ~120 LOC.
- New keybinds + dynamic context-switch: ~80 LOC.
- Start-button 100ms pulse animation: ~30 LOC.

Total: roughly **1810 LOC** added; the existing autopilot tab's ~2,400
LOC is reduced to about 600 LOC (the parts we keep: lane reducer,
telemetry collection, picker search backend, override panel reducer).

## How to convert this draft into milestone(s)

When the user greenlights, the natural split is:

- **M241 — Geometry & While-Executing takeover.** 40/60 split, sidebar
  tabs, setup region, control row, full-screen While-Executing view.
  ~1200 LOC across `crates/raul/src/tui/{autopilot, dashboard,
  lane_lists, view_state, modes}.rs`.
- **M242 — Floating milestone picker.** Floating overlay widget, narrow-
  terminal modal fallback, multi-select backend, state persistence, 100ms
  Start-pulse animation. ~360 LOC in `crates/raul/src/tui/`.

Both at priority `high`. M241 first (larger surface, gates M242's
`[+ select]` button placement). We can merge into one M241 if you want
them shipped together — the draft supports either ship.

### AC verification commands (preview)

For M241:
```
cargo nextest run -p raul --no-fail-fast -E 'test(/autopilot_(idle|active|sidebar|control_row)/)'
make lint
```

For M242:
```
cargo nextest run -p raul --no-fail-fast -E 'test(/autopilot_(picker|floating|modal_fallback|pulse)/)'
cargo nextest run -p raul --no-fail-fast -E 'test(/tui_state_persistence/)' (M241+M242 shared)
```
