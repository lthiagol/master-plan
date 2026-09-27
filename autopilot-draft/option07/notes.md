# Option 07 — notes

## Strong points

- **Teaching-friendly**: the topology-as-diagram makes it hard to *not* learn
  what orchestrator / runner / verifier means.
- 1-agent vs. 2-agent vs. 3-agent is *visually different* — the difference
  isn't just a chip it's a missing shape.
- The graph scales: future modes ("4-agent for a swarm", "sidecar verifier")
  become "add a circle and an arrow."

## Best when

- The audience includes engineers evaluating the autopilot architecture.
- Documentation screencasts need a visual anchor.
- New modes (1-agent, 4-agent) are coming; the diagram already handles them.

## Worst when

- Terminal doesn't render box-drawing characters (rare but real — old ssh
  bridges, certain Windows terminals).
- ASCII fallback degrades the experience significantly.
- The user wants to see the live progress bar; the canvas doesn't have one
  front-and-center.

## Mouse model

Model B (free click + focus) is required. Roles are click targets; arrows
are click targets; topology buttons are click targets. Each needs its own
hit-test rect with a focus style.

## Variants

- **Light variant (no canvas)**: replace circles with role-name chips
  arranged vertically; arrows become thin separators. Same interactions, no
  unicode dependency.
- **Visual preset**: a small "role color" toggle to give each role a distinct
  hue (orchestrator = blue, runner = green, verifier = orange).

## What you give up

- Click density (only 3-4 click targets vs. 20+ in option 01).
- Density-of-information per square inch — the canvas is sparse by design.

## Engineering cost (rough)

- Graph widget ~250 LOC.
- ASCII fallback ~50 LOC.
- Per-edge commit policy storage in `.mp/autopilot-state.json`.
- 3 topology variants as data, not as code duplication.
