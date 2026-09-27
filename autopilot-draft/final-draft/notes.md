# Final draft — choices, tradeoffs, and open questions

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

## Tradeoffs we accepted

- **Setup region is narrower** (40%). A 50/50 split is more balanced but
  the start button + summary + control row are tight at 50%.
- **Sidebar tab `1` / `2` / `3` may collide with terminal app shortcuts**.
  Most users can rebind their terminal app; raul doesn't try to be clever
  about detecting tmux.
- **Floating picker eats the central viewport**. On narrow terminals
  (< 100 cols), the picker can't fit, and we fall back to a modal picker
  (not yet designed — see open questions).
- **State.json schema versioning** is a maintenance burden. We accept it
  for now; a future version can drop v1 entirely.

## Open questions (for the milestone spec to resolve)

1. **Floating picker's narrow-terminal fallback** — does the modal picker
   reuse `floating-picker.md`'s sub-screen or is it a separate `subscreens.md`
   entry? Lean: same internal state, different renderer.
2. **Run-mode chip + detached confirmation** — is the confirmation modal
   mandatory on every Detached click, or does it auto-confirm after the
   first one with a `Don't ask again` checkbox?
3. **Conflict between `r` = Run mode and `r` = resume** — is the context
   switch (idle vs. running) explicit enough? Test once in the dogfood
   cycle and adjust.
4. **Sidebar split drag handles** — does the divider allow pushing the
   sidebar to 0% (collapsing it entirely) and a small arrow appears to
   bring it back? Or is there a minimum width?
5. **`recent runs` privacy** — keep per-user (`~/.ra_cache`) or per-plan
   (`.mp/...`)? current draft says per-user; revisit when M221's mouse
   support gets re-tested with a multi-user setup.
6. **Activity tail length** — default 8 lines in the Progress tab. Is
   that enough? Most runs have meaningful events at 1-3 per minute; 8 lines
   gives 2-3 minutes of context.
7. **Harness per-role enumerations** — currently `opencode / cursor / pi`
   per the R-late constraint. When a new harness lands in the registry,
   does the UI pick it up automatically? Lean: yes, scan
   `mp config schema get` for harness names.
8. **Topo id display in advanced override** — `three-agent-001` is the
   project's example topo id. Is this auto-derived or user-set?
9. **Sidebar tab availability during run** — State tab stays available
   during a run (read-only). Activity tab streams. Progress tab is the
   default. Should `State` be hidden or pinned during a run?
10. **`just-rendered, not-yet-run` state** — after Confirm in the picker,
    what happens visually? Does the Start button pulse / outline / pre-arm?

## What we deliberately skipped (for later)

- **Pause-and-edit** — re-editing config mid-run is a *follow-up*
  feature. Disabled in this draft; can be added as M242 or M243.
- **Per-role harness grid when uniform is off** — visually we already
  show three rows of three chips; advanced override handles the rest.
- **`recent runs` as a stat dashboard** — counts and trends are nice but
  not in this draft.
- **Telemetry history (sparkline)** — current draft shows numerical
  snapshot; a sparkline would be a `state tab` follow-up.

## Engineering-cost boundary

If we estimate by `crates/raul/src/tui/render/lane_lists.rs::render_autopilot_tab`
replacement scope:

- Setup region rewrite: ~300 LOC.
- Sidebar tabs: ~250 LOC.
- Floating picker: ~250 LOC.
- Sub-screens (advanced override, detached confirm, peek view, state inspector):
  ~400 LOC.
- State.json read/write: ~120 LOC.
- New keybinds + focus: ~80 LOC.

Total: roughly **1400 LOC** added; the existing autopilot tab's ~2,400 LOC
is reduced to about 600 LOC (the parts we keep: lane reducer, telemetry
collection, picker search backend, override panel).

## How to convert this draft into a milestone spec

When you greenlight, the natural split is:

- **M241** — Geometry: 40/60 split, sidebar tabs, control row, setup region.
  Touches `crates/raul/src/tui/{autopilot, dashboard, lane_lists, view_state}.rs`.
- **M242** — Floating picker: standalone overlay widget, multi-select
  backend, state persistence. Smaller scope, fewer touchpoints.

Both at priority `high`. M241 first because it's the larger surface; M242
depends on M241 because the picker's `[+ select]` button lives in M241's
setup region.

(We may merge into one M241 if the milestones feel too small individually;
the draft supports either ship.)
