# Interactions — keyboard + mouse

## Mouse model: Model B (free click + focus)

- Click anywhere → that click target gets focus and activates.
- Tab → move keyboard focus through interactive elements.
- Arrow keys → move within a chip group or step a list row.
- Space / Enter → activate the focused element.
- Sidebar resize: drag the right-edge divider of the setup region.
- Floating picker: when active, it owns the central area; outside clicks are
  dimmed and require a second click to close.

## Keymap (full)

### Always available

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | next / previous interactive element |
| `↑` / `↓` / `←` / `→` | move within chip group or list |
| `Space` / `Enter` | activate focused chip / button |
| `Esc` | close floating picker / popover (commit if dirty) |
| `q` | quit raul (with confirm if a run is live) |
| `?` / `F1` | show keymap overlay |

### Setup region

| Key | Action |
|---|---|
| `t` | jump to Topology chip group |
| `h` | jump to Harness row |
| `m` | open floating milestone picker |
| `c` | jump to Commit policy chip group |
| `r` | jump to Run mode |
| `s` | Start (run) — same as clicking the Start button |
| `d` | toggle Detached mode |

### Control row

| Key | Action |
|---|---|
| `p` | pause live run |
| `r` (when running) | resume paused run |
| `x` / `Esc` | stop current run (with confirm) |

### Floating picker (when active)

See `floating-picker.md` — full keymap section there.

### Sidebar tabs

| Key | Action |
|---|---|
| `]` | next sidebar tab |
| `[` | previous sidebar tab |
| `1` / `2` / `3` | jump to Progress / Activity / State tab |
| `:` | collapse / expand the sidebar |

### Sub-screens (popovers)

Each sub-screen has its own dismissable key (usually `Esc` or `q`).

## Focus order

```
1. Setup region group chips → Top → Bottom (Topology, Harness rows,
   Advanced override, Milestones chips, Commit, Run mode, Start
   button).
2. Control row (Pause, Stop, Resume, ⤴ back).
3. Sidebar tabs (Progress / Activity / State).
4. Inside the active sidebar tab, focusable rows / buttons.
5. (When the floating picker is open, focus order is: search → filters
   → list rows → Confirm/Cancel — even if those elements are in the
   dimmed underneath layer.)
```

## Keybinds registry changes vs. existing autopilot tab

- 6 new bindings to add: `m`, `c`, `d`, `]`, `[`, `:`.
- Existing `s`, `p`, `r`, `x` may be re-mapped if they conflict.
- No removals expected.

## Mouse hit-test table

The mouse hit-test registry is populated for:
- Each radio / chip / button in the setup region (~30 click targets).
- Each of the 4 buttons in the control row.
- Each sidebar tab (~3 click targets).
- Each row in the active sidebar (rows = milestones / activity lines / state
  fields = ~20 click targets when active).
- Each interactive element in the floating picker (~rows × chips × buttons).
- The split divider (drag handle).

## Animation and motion

- Status dot pulse on the sidebar Progress tab — gentle cyan glow at ~1Hz
  on rows that are currently running. Skippable on slow terminals.
- Sidebar tab transition — 150ms fade between tabs (skippable).
- Floating picker — appears with a 100ms slide-down from the top + a
  one-shot backdrop fade. Dismissing = reverse slide-up.
- No audio cues. The terminal might be headless; we never assume audio.

## Multi-input rules

A single user can mix keyboard and mouse freely. The `focus_target` field on
`view_state` follows whichever input most recently produced an event.

## What users will confuse

- `r` means *resume* (when running) but *Run mode toggle* (when idle).
  Mitigation: the Run mode toggle only accepts `r` when idle; the runtime
  prevents the conflict by switching the binding's context with state.
- `s` is **Start** but the Start button text varies (Start / Resume /
  Re-run / Detached). We bind `s` to "primary action" and let the button
  decide.
- Sidebar tabs `1` / `2` / `3` may collide with terminal app shortcuts
  (rare but real in tmux / iTerm). We document the conflict in the help
  overlay.
