# raul TUI Walkthrough

Minimal review loop: browse milestones → drill into detail → view annotation thread
→ create annotation → approve/resolve → G14 clears.

## Launch

```bash
raul              # home dashboard (default)
raul --tui        # same as no-args
raul -i           # explicit TUI entry
```

Human PM guide: [RAUL.md](02%20-%20Getting%20Started/RAUL.md) · Diagram index:
[00-Concepts § Process visuals](00-Concepts.md#process-visuals)

If mp is not on PATH, set `MP_HOME`:

```bash
export MP_HOME=/path/to/master-plan
```

## Key bindings

The TUI has two focus regions: the **tab bar** at the top (where you
pick a lane) and the **content area** below (where you navigate list
items, milestone detail, etc.). The currently focused region changes
which keys are live.

### Tab bar focused (default at launch)

| Key          | Action                              |
|--------------|-------------------------------------|
| ← / h        | Previous lane                       |
| → / l        | Next lane                           |
| 1 .. 7       | Jump directly to lane               |
| Enter / Tab  | Move focus into content area        |
| q            | Quit                                 |

### Content area focused

| Key          | Action                                |
|--------------|---------------------------------------|
| ↑ / k        | Move up                                |
| ↓ / j        | Move down                              |
| PageUp       | Move up by one viewport page          |
| PageDown     | Move down by one viewport page         |
| Enter        | Drill in (milestone detail, annotation thread) |
| Esc          | Go back / close help                   |
| h            | Toggle hide-done (Milestones)          |
| f            | Toggle open-only annotation filter     |
| b            | Jump to review board                   |
| m            | Open review menu (milestone detail)    |
| A            | Create annotation                      |
| r / R        | Resolve / reopen selected annotation   |
| p            | Request or resolve approval            |
| Tab          | Focus back to tab bar                  |
| ?            | Show / hide help overlay               |
| q / Q        | Quit                                    |

### Mouse

| Action              | Effect                                            |
|---------------------|---------------------------------------------------|
| Click tab label     | Select that lane and load its data                |
| Click list item     | Move cursor to that row                           |
| Wheel over list     | Scroll selection up / down by one row             |
| Wheel over tab bar  | No-op (tab bar ignores mouse wheel)              |
| Drag in tab bar     | Reserved for narrow-width horizontal scroll        |

## Views

### 1. Tab bar (M91)

A horizontal strip of seven tabs sits at the top, below the header
row: Overview · Milestones · Path · Bugfixes · Tweaks · Backlog ·
Board. The active tab is highlighted in the accent palette. Two focus
modes ship:

- Tab bar focused: ←→/hl cycle lanes; digits 1–7 jump directly;
  Enter or Tab moves into the content pane.
- Content focused: ↑↓jk and PageUp / PageDown navigate list items;
  the mouse wheel scrolls the list; Tab returns to the bar.

At terminal widths under ~60 columns the bar uses compact labels
(`Ov`, `Ml`, ...) and shows overflow indicators (`◂` / `▸` / `…`)
when even compact labels do not fit. Narrow widths never revert to a
left sidebar.

### 2. Milestone List

Displays all milestones in a table with ID, title, spec status, and
execution status. Use ↑/↓ to navigate (or PageUp / PageDown for a
viewport page), Enter to drill into detail.

### 3. Milestone Detail

Shows the full milestone: status meta, intent, problem, in-scope /
out-of-scope items, steps, and acceptance criteria. The G14 approval
gate status is displayed as BLOCKED (red) or CLEAR (green).

Press Enter to open the annotation thread for this milestone.

Press `p` to request or resolve an approval-request annotation.

### 4. Annotation Thread

Lists annotations on the current milestone. Use `f` to toggle
open-only filter. Shows annotation ID, status, kind, and body preview.

- `A` — create a new annotation (prompts for body text)
- `r` — resolve selected open annotation
- `R` — reopen selected resolved annotation

## Walkthrough: minimal review loop

1. **Dashboard**: Launch `raul` (no args) or `raul -i`. The home dashboard shows status,
   inbox, path preview, and next action.

2. **Milestones**: Press `m` (or Enter) to open the milestone list.

3. **Review detail**: Press Enter on a milestone. The detail view shows steps,
   acceptance criteria, and G14 approval status.

4. **Annotations**: Press `a` to open the annotation thread. Use ↑/↓ to browse
   annotations.

5. **Create annotation**: Press `A`. Type the annotation body, press Enter to create
   it. The annotation delegates to `mp annotation create` via the shared read layer.

6. **Approve**: Press Esc to return to the detail view. Press `p` to create an
   approval-request annotation. The G14 status changes to BLOCKED.

7. **Resolve**: Go back to the annotation thread, select the
   approval-request, press `r` to resolve it. Press Esc to return to
   the detail view. The G14 status now shows CLEAR.

8. **Quit**: Press `Q` or Esc from the dashboard (or milestone list after backing out).

## Architecture

- **App state** (`src/tui/app.rs`): Pure state — view enum, navigation
  stack, selection, filter. No ratatui imports. Fully unit-tested.

- **Read layer** (`src/reads.rs`): Typed read helpers over MpRunner
  shared by CLI commands and TUI.

- **Rendering** (`src/tui/render.rs`): Ratatui widgets rendering
  MilestoneList, MilestoneDetail, AnnotationThread, help overlay.
  Snapshot-tested via TestBackend.

- **Event loop** (`src/tui/runner.rs`): Crossterm input handling,
  event mapping, data loading via mp shell-out. TerminalGuard RAII
  ensures raw mode is restored on panic.

- **Input handling** (`src/tui/event.rs`): Maps crossterm key events
  to app events (Up, Down, Enter, Back, Quit, etc.).

All writes delegate to `mp annotation create/resolve/reopen` — raul
never writes plan files directly.
