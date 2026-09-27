# Autopilot tab — 10 layout options

This folder holds ten different shapes for the redesigned Autopilot tab in `raul`.
Each `optionNN/` folder contains:

- `main.md` — the main screen layout (ASCII art + interaction notes)
- `subscreens.md` — sub-screens / modals / drawers the main screen opens
- `notes.md` — what this option is good at, when to pick it, tradeoffs

## Background

The current Autopilot tab (`crates/raul/src/tui/autopilot.rs`, ~2,400 lines) is a
status-first view: a lane picker, an override panel as a full-screen drill-down,
a live status graph, and a telemetry strip. Mouse support landed in M221 but
the tab has no native button widgets.

The redesign treats the screen as **control-first**: a setup panel at the top
with clickable choices for topology / harness / milestones / commit policy / a
big Start, and a progress region below. Ten options explore the geometry,
density, and click model around that idea.

## The 10 options at a glance

| # | One-line shape | When to pick |
|---|---|---|
| 01 | Setup-first vertical (canonical) | Default. Single screen, nothing hidden. |
| 02 | Three-tab single screen (`Setup \| Controls \| Progress`) | When the audience is mixed (status-seekers vs. tuners). |
| 03 | 40/60 left-right split (setup / progress) | When 90% of clicks are setup and progress needs the wide side. |
| 04 | Hub-and-spoke (5 quick-launch cards) | When you want clicks → modal flow, not always-visible controls. |
| 05 | Wizard stepper (1→2→3→4→5) | When the user runs autopilot rarely and wants zero mistakes. |
| 06 | Compact control bar + drawer (status always-on) | When the tab is open all day and idle-by-default matters most. |
| 07 | Topology-as-graph (circles + arrows) | When teaching / explaining roles matters, not just configuring. |
| 08 | Dual-mode (Idle / Active flip) | When active state has so much to say the controls get in the way. |
| 09 | Right sidebar (collapsible tool-window) | When people want live telemetry pinned in a side strip. |
| 10 | Preset card-stack | When most users pick a named preset and only advanced users edit. |

## Shared vocabulary across all 10

- **Topology**: 1-agent / 2-agent / 3-agent orchestrator↔runner↔verifier trio.
- **Harness**: opencode / cursor / pi (per role).
- **Commit policy**: per-step / per-finding / per-cycle / batched.
- **Picker**: the milestone selection widget (search + selected list).
- **Live status graph**: the lane→lane progress rows from M216.
- **Activity tail**: 5-8 lines of recent orchestrator / verifier events.
- **Telemetry**: throughput counters from M216.

## Recommended combos

If you want a single ship recommendation, the strongest pair is **01 (canonical
shape)** + **08 (idle/active flip)** — option 01 is the geometry, option 08 is
the state-driven styling on top of it. The picker-overlap is honest because
they compose: option 08's idle sub-screen can be the option-01 layout, and
option 08's active sub-screen can collapse to graph + telemetry + Stop.

Option 09 (right sidebar) is a strong second if you don't want the idle/active
flip but still want a sticky progress strip on the right.

## What we deliberately do NOT do in any option

- Build a brand-new button widget library. Ratatui's `Block::bordered()` plus a
  `highlighted_style()` plus a focus-state is enough.
- Reinvent the runner engine. M207-M229 work is correct; this is a front-of-house
  rebuild, not a back-of-house one.
- Move setup state out of the engine. Setup state should write back to
  `.mp/autopilot-state.json` so it survives a refresh.
