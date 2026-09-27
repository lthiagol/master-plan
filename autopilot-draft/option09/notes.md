# Option 09 — notes

## Strong points

- Live telemetry never goes away — it's pinned in the sidebar.
- IDE-pattern feels right for engineers used to tool windows.
- Tabbed sidebar scales: future "queue depth over time" or "lane cost"
  charts slot in as new tabs.

## Best when

- Users keep the tab open all day.
- The audience is split between tuners (main pane) and operators
  (sidebar tabs).
- The user wants the live status graph visible always.

## Worst when

- Terminal < 100 cols — the 25% sidebar eats too much.
- The user wants a "settings only" view and telemetry should be opt-in.

## Mouse model

Model B (free click + focus) mandatory. Sidebar tabs are click targets;
main form is its own click area; sidebar resize is a drag handle.

## Sidebar visibility — auto-collapse on narrow terminals

When the terminal drops below 100 cols, the sidebar auto-collapses to give
the main form more space. A small `▸ open sidebar` arrow on the right edge
brings it back.

## Engineering cost (rough)

- Right-sidebar widget ~150 LOC.
- 4 sidebar tabs ~80 LOC each (~320 LOC total).
- Drag-resize handler for the sidebar boundary.
- State-inspector widget ~120 LOC (read-only field dump + copy button).
- `.mp/autopilot-state.json` for sidebar tab visibility and tab identity.

## What you give up

- Visual prominence for setup (the main pane is now narrower).
- The "all chips one place" density of options 01, 05.
- Terminal < 90 cols is essentially unusable.

## Variants worth considering

- **Bottom-docked** instead of right-docked — same idea, different orientation.
  Bottom dock can grow upward more freely.
- **Tear-off** — the sidebar floats and can be moved by dragging. Ratatui
  support is partial; usually not worth the complexity.
