# Option 04 — sub-screens

## 1. Per-card configure modal (shown above)

## 2. Post-start full progress screen

```
┌────────────────── Autopilot · 3-agent live ─────────────────────┐
│ ● M236 ⟶ herdr agent prompt opencode     ▰▰▰▱ 4/7     cycle 11 │
│ ● M238 ⟶ waiting on orchestrator          ▰▱▱▱ 1/3    cycle  1 │
│ ○ M234 (queued)                                                  │
│ ─────────────────────────────────────────────────────────────────  │
│  activity tail                                                   │
│  11:14:25 orchestrator → runner: M238 step S1                   │
│  11:14:26 runner     → orchestrator: S1 done                    │
│  11:14:27 orchestrator → verifier: M236/AC-04                   │
│                                                                    │
│  [ pause ]  [ stop ]  [ view findings ]  [ return to setup ]     │
└────────────────────────────────────────────────────────────────────┘
```

## 3. Return-to-setup button

When the user clicks "return to setup" mid-run, the screen flips back to the
hub. They can launch new configurations after the current run finishes (no
concurrent runs are allowed).

## 4. Help / first-time overlay (only shown once)

```
┌── Welcome to the Autopilot launcher ─────────────────────────────┐
│                                                                    │
│  Each card is a preset run shape. Click one to configure           │
│  milestones / harness / commit policy in a popup, then start.      │
│                                                                    │
│  During a run, this screen becomes a live progress view.          │
│                                                                    │
│  You can keep raul open or close it (if you chose detached).      │
│                                                                    │
│              ┌──────────────────┐  ┌──────────┐                  │
│              │  Show me again   │  │  Got it   │                  │
│              └──────────────────┘  └──────────┘                  │
└────────────────────────────────────────────────────────────────────┘
```

"Dismiss" stores `?help_dismissed = true` in `.mp/autopilot-state.json`.

## 5. Resume introspection

When the user clicks the Resume card, the modal skips directly to the
post-start progress screen, but **only after** the runner confirms the
session.json is fresh enough to resume. If not, the modal shows a
diagnostic:

```
│  ⚠ Cannot resume — state diverged.                           │
│  Last seen commit: a1b2c3d                                     │
│  Plan state changed at:        f4e5d6a                       │
│  Detected at:                   11:14:23                     │
│                                                                │
│  [ reset to fresh ]   [ inspect diff ]   [ cancel ]           │
```
