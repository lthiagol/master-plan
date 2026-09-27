# Floating milestone picker — sub-screen

The picker is a **floating overlay** — a popup that covers the central
2/3 of the screen. It does *not* take over the full screen: the underlying
setup + sidebar regions are still visible at the edges, dimmed by 30%.

## Layout (when active)

```
┌─ autopilot ──────────────────────────────┬──────────────────────────────────────────────────────┐
│ (dimmed setup region)                     │  (dimmed sidebar tabs region)                      │
│                                          │                                                      │
│                                          │                                                      │
│         ╔═════════════════════════════════════════════════════════════════════╗    │
│         ║                                                                     ║    │
│         ║  ◀  Pick milestones to run                                  ✕ close  ║    │
│         ║  ─────────────────────────────────────────────────────────────────  ║    │
│         ║                                                                     ║    │
│         ║  filter:  ▶ all   ● ready-only   ○ in-progress   ○ complete        ║    │
│         ║           /  search · · · · · · · · · · · · · · · · · · · · · ·   ║    │
│         ║                                                                     ║    │
│         ║    ☐  M236  ⓗ M202 doc-overindented-list-items clippy lint fix   ║    │
│         ║    ☐  M238  ⓗ M211 stale file slug + parse_rfc3339 dedup         ║    │
│         ║    ☑  M234   fixture/scenario hygiene                           ║    │
│         ║    ☑  M231   mp-flow skill table-vs-stages drift lint          ║    │
│         ║    ☐  M235   consumer-surface hygiene                            ║    │
│         ║    ☐  M230   post-M229 raul TUI Watch lane collapse              ║    │
│         ║    ☐  M225   * in-progress · handled-by-M207                    ║    │
│         ║    ☐  M224   ✓ complete · not runnable                          ║    │
│         ║    ─                                                              ║    │
│         ║    M229 onwards  ⏷  load more                                   ║    │
│         ║                                                                     ║    │
│         ║  selection summary                                              ║    │
│         ║    2 selected · ~12 min est. · uniform harness                   ║    │
│         ║                                                                     ║    │
│         ║            ┌─────────────────┐    ┌──────────┐                  ║    │
│         ║            │   ▶ Confirm     │    │  Cancel  │                  ║    │
│         ║            └─────────────────┘    └──────────┘                  ║    │
│         ║                                                                     ║    │
│         ╚═════════════════════════════════════════════════════════════════════╝    │
│                                          │                                                      │
│                                          │                                                      │
└──────────────────────────────────────────┴──────────────────────────────────────────────────────┘
```

The picker is **anchored to the middle 66% of the screen** — both horizontally
and vertically — so the dimmed setup region is visible on the left and the
dimmed sidebar is visible on the right.

## List columns

Each row in the picker is a milestone candidate:

| Column | What it shows | Width |
|---|---|---|
| Checkbox | ☐ / ☑ (filled on toggle) | 1 col |
| ID | `M###` | 5 cols |
| Status | ⓗ high · ⓝ normal · ⓛ low · ✓ complete · * in-progress | 2 cols |
| Title | first 60 chars of the milestone title | rest |
| Cycle hint | (optional) — current cycle count if already in motion | 8 cols |

Color rule:
- ✓ complete items are dimmed.
- * in-progress items are highlighted but the checkbox is replaced by `↳`
  (the milestone is mid-flight and can't be re-picked).
- high-priority items have a thin colored bar on the left edge.

## Filters (top of the picker)

Four chip-filters, mutually exclusive (radio):
- `all` — every milestone regardless of state.
- `ready-only` — only milestones in `execution_status: planned` +
  `lifecycle: approved`.
- `in-progress` — only cycles currently running.
- `complete` — already-finished (for reference; can't re-pick).

Plus a `/` search box that filters by ID prefix or title substring.

## Multi-select semantics

- Toggle is a checkbox, not a radio. You can pick any number from 0 to N.
- Selection persists across filter changes (a milestone selected under
  `ready-only` stays selected when you switch back to `all`).
- The summary line at the bottom shows count + estimated time + a note if
  no milestones are selected.

## Interactions

### Keyboard

- `/` — focus the search box (when not focused, typing jumps there).
- `j` / `k` — move one row up / down.
- `space` — toggle the highlighted row.
- `x` — same as space.
- `g g` / `G` — top / bottom of list.
- `1` `2` `3` `4` — switch filter (all / ready / in-progress / complete).
- `⏎` — open the highlighted milestone in a peek view (still inside the
  picker; not full-screen).
- `tab` — leave the picker, return to the main screen.
- `c` / `esc` / click outside — close the picker (changes preserved unless
  `Cancel` was clicked).
- `⏎ on Confirm` — apply selection, close picker, return focus to `[+ select]`.

### Mouse (Model B)

- **Click anywhere on the picker** = focus that row.
- **Click checkbox** = toggle, no row focus change.
- **Click filter chip** = switch filter.
- **Click search box** = focus search; typing filters.
- **Click outside the picker** = ambiguous — first click dims the picker
  further and adds a "click again to close" hint; second click closes.
  (Avoids accidental dismissal.)
- **Click Confirm / Cancel** = explicit apply / discard.

## Edge cases

| Case | Behavior |
|---|---|
| All filters return 0 milestones | Show a single dim row "No milestones match the filter"; Confirm is disabled. |
| Selection is empty on close (via Confirm) | Show a one-line warning underneath Confirm: "no milestones selected — Start will be disabled"; requires a second click to dismiss. |
| Milestone state changes while the picker is open (e.g. M234 became "in-progress" mid-search) | The row updates on the next refresh tick (1s). If the toggled-off item becomes unrunnable mid-session, the row dims and a small `↳ stale` indicator appears; it auto-untoggles on next Confirm. |
| `, select all` | Toggle every visible row. Toggle direction follows: if the majority of visible rows are unchecked, all become checked; otherwise all become unchecked. |
| Search has 0 matches but some milestones are selected | The selected items still appear in a "Selected (N from filters)" section above the "no matches" empty state. |

## What the picker owns

- The selection itself, in memory.
- The preview summary (count + est. time).
- The filter + search state.

## What the picker does NOT own

- The milestone JSON itself (read-only; mutations go through `mp milestone`
  CLI; the picker can `mp milestone bulk --dry-run --ids ...` to preview).
- Persistent state — on close, the selection is committed to
  `.mp/autopilot-state.json` (see `state-and-persistence.md`).
