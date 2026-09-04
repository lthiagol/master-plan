# raul TUI usage

Mouse, keyboard, and terminal-emulator notes for `raul`. The
human-facing surface of the toolkit.

> raul is a keyboard-canonical TUI — every action is reachable
> without a mouse. The mouse layer is a parallel accelerator; it
> must never **replace** a keyboard binding. If the mouse doesn't
> work in your terminal, every key still does.

## Quick reference

| Action | Mouse | Keyboard |
|--------|-------|----------|
| Switch lane | Click tab label | `Tab` / `Shift+Tab` (or `1`–`7`) |
| Select row | Click row | `j` / `k` (or arrow keys) |
| Open detail | Double-click row | `Enter` |
| Scroll | Wheel | `j` / `k` / `PgUp` / `PgDn` |
| Sort rebind | — | `o` (per-lane sort menu) |
| Help | — | `?` |
| Quit | — | `q` |

## Mouse support

`raul` reads mouse events through `crossterm`'s
`EnableMouseCapture` mode. The runner loop dispatches every
`MouseEvent` through a single
[`handle_mouse`](../../crates/raul/src/tui/runner.rs) function
that routes the click / wheel to the active lane's renderer.

### What works

- **Single click** — selects the row under the cursor on every
  list-bearing lane (Milestones, Backlog, Ideas, Path, Overview,
  Settings, Autopilot). The click resolution is driven by the
  pre-computed `ViewState` rects, so a click on the visible row
  always resolves to the row id rendered at that pixel — no
  hardcoded offsets.
- **Double click** — opens the detail view on Milestones,
  Backlog, Ideas, Path, and Autopilot. Overview and Settings are
  **selection-only** by design: Overview inbox rows route through
  the canonical Enter handler, and Settings rows have no row-
  detail action. The double-click window is **500 ms** (tuned to
  match macOS / iTerm2 defaults).
- **Mouse wheel** — scrolls the focused list by one row per
  tick. Reaches the same viewport / index as the keyboard `j` /
  `k` / `PgUp` / `PgDn`. Detail screens (`MilestoneDetail`,
  `BacklogDetail`, `AnnotationThread`) scroll too — the wheel
  never silently drops on a detail screen.
- **Scrollbar track click** — click anywhere on the gutter to
  jump the scroll position proportionally (matches the keyboard
  Page-Up / Page-Down semantics).

### What does NOT work

- **Right-click** — the dispatcher ignores `MouseButton::Right`.
- **Drag reorder / drag-to-select** — left-button drag is a
  no-op; only `Down` is meaningful.
- **Hover tooltips** — there is no hover state in the TUI; the
  terminal doesn't deliver hover events to the application.
- **Click on tab bar → wheel** — the wheel on the tab bar row is
  a no-op (AC-09).

## Escape hatch: `RAUL_NO_MOUSE`

Some terminals forward spurious mouse events that confuse the
dispatcher. Set `RAUL_NO_MOUSE=1` to disable the mouse layer
entirely; the keyboard path stays pristine:

```bash
RAUL_NO_MOUSE=1 raul
```

Accepted truthy values: `1`, `true`, `TRUE`, `yes`. The runner
checks the env on every mouse event, so flipping the variable
mid-session takes effect without restarting `raul`.

## Terminal capability matrix

The following table records what each tested terminal actually
delivers. "Click" / "Wheel" mean the corresponding crossterm
`MouseEventKind` reaches `raul`. "Long-press" / "Swipe" entries
describe what the terminal itself synthesizes before our app
sees anything — `raul` does not generate these in software.

| Terminal                | Click | Wheel | Long-press | Swipe | Notes |
|-------------------------|-------|-------|------------|-------|-------|
| iTerm2 3.4+ (macOS)     | ✓     | ✓     | ✗          | ✗     | Default settings report `mousetrack=on`. Touchpad two-finger scroll = wheel. |
| Terminal.app (macOS)    | ✓¹    | ✗     | ✗          | ✗     | ¹ Enable "Report mouse click events" in View → "Allow Mouse Reporting". Wheel not supported. |
| GNOME Terminal 3.40+    | ✓     | ✓     | ✗          | ✗     | Both buttons report as `Down`. |
| Windows Terminal 1.18+  | ✓     | ✓     | ✗          | ✗     | Default profile reports both click and wheel. |
| WezTerm 20220905+       | ✓     | ✓     | ✗          | ✗     | `mouse_enabled = true` is the default. |
| Alacritty 0.13+         | ✓     | ✓     | ✗          | ✗     | No configuration required. |
| kitty 0.27+             | ✓     | ✓     | ✗          | ✗     | `mouse_enabled` defaults to true. |
| tmux 3.3+ (passthrough) | ✓²    | ✓²    | ✗          | ✗     | ² `set -g mouse on` AND `set -g terminal-features "*:RGB"`. Without both, click events are eaten by tmux's own copy mode. |
| mosh 1.4+ over SSH      | ✗     | ✗     | ✗          | ✗     | mosh deliberately suppresses mouse events to keep its predictive echo honest. Use keyboard only. |
| Termux (Android)        | ✓³    | ✓³    | ✗          | ✗⁴    | ³ Tap = click; two-finger swipe = wheel. ⁴ Best-effort swipe translation performed by Termux itself, not by `raul`. |
| iSH (iOS)               | ✓⁵    | ✗     | ✗          | ✗     | ⁵ Tap = click; no wheel events forwarded. |
| SSH + PuTTY 0.78        | ✗     | ✗     | ✗          | ✗     | PuTTY strips mouse events at the protocol level. Use `RAUL_NO_MOUSE=1`. |

> Application-level long-press / swipe is **not claimed** by
> `raul`. We translate the events the terminal emits — anything
> the terminal synthesizes (e.g. Termux swipe → scroll) is the
> terminal's contract, not ours.

## Why no long-press / hover

`raul`'s hot path is keyboard-driven and runs over a serial byte
stream. Long-press requires the application to track
`Down → wait → Up` timing on its own; synthesizing that
software-side adds a millisecond of latency to every other click
and is easy to get wrong (the user's "double-click" and "long-
press" thresholds overlap). We treat long-press as a terminal-
emulator feature and document which emulators ship it (none of
the tested ones, currently).

## CI note

`RAUL_NO_MOUSE` does **not** need to be set in CI — the headless
test harness drives synthetic mouse events through
`handle_mouse` directly, never via the real `crossterm` event
loop. The env var is a runtime escape hatch for end users, not a
test fixture. The CI smoke run (`.github/workflows/plan.yml`)
builds the binary, runs the suite, and asserts each integration
test in `crates/raul/tests/suite_mouse_*.rs` exits 0.
