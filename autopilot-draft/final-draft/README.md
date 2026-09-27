# Autopilot tab — final draft

This folder is the **consolidated working draft** for the new Autopilot tab.
It mixes option 03 (left-right split, setup left / status right) with option
09 (right sidebar with tabbed sub-views), plus a new requirement: a **floating
milestone picker** that overlays the screen as a multi-select checkbox list.

The other ten options in `autopilot-draft/optionNN/` remain as alternative
explorations. This folder is the one we'll tighten into a milestone spec.

## What's in this folder

- `main.md` — the main screen layout (split geometry + tabbed sidebar).
- `floating-picker.md` — the floating milestone picker sub-screen (the new
  requirement), with selection rules, keyboard, mouse, and edge cases.
- `subscreens.md` — other modals (advanced override, detached confirm, etc.)
  that hang off the main screen.
- `interactions.md` — keyboard + mouse model, focus model, keymap.
- `state-and-persistence.md` — what lives in `.mp/autopilot-state.json` and
  what lives only in `config.toml`.
- `notes.md` — choices, tradeoffs, list of open questions for the milestone.
- `screen.jpg` — rendered mockup of the main screen.
- `picker-mockup.jpg` — rendered mockup of the floating picker overlay.

## What we took from each option

| Source | What we kept | Why |
|---|---|---|
| **option 03** | 40/60 left-right split geometry | You liked seeing setup and progress at the same time. |
| **option 09** | Tabbed right sidebar (progress / activity / state) | The progress-by-default tab + ability to flip to activity tail or state inspector. |
| option 01 | The Setup-row chips (Topology / Harness / Milestones / Commit / Start) | Cleanest shape for the setup region. |
| option 06 | The Pause/Stop/Resume control row lives under the Start button, not in the sidebar | Avoids losing Pause/Stop behind a sidebar tab. |
| (new) | Floating milestone picker | Multi-select with checkboxes, dismissable but overlayable — the new requirement. |

## What we explicitly do NOT keep

- Option 02's tab strip (Setup | Controls | Progress) — too many hops for the
  common Setup-first flow.
- Option 04's hub-and-spoke cards — too modal-launchy for daily use.
- Option 05's wizard — daily friction.
- Option 07's topology-as-graph — teaching is not the goal here.
- Option 08's idle/active mode flip — handling it as a sidebar tab instead.
- Option 10's preset cards — defer until users want them.

## Open questions

Listed at the bottom of `notes.md` — the milestone spec will resolve these.

## What ships in this folder

The screens + interactions + state model are detailed enough that when you
greenlight, we can convert this into one or two `mp` milestones (probably a
M241 + M242 if it splits naturally between geometry + picker).
