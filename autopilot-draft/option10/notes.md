# Option 10 — notes

## Strong points

- Lowest-friction launch for the most common case.
- "Daily M-series" or "Long weekly" are the kinds of names that build
  loyalty — users develop *their* preset names.
- Custom preset editor is a power feature that costs a small UI surface.

## Best when

- The audience includes both "I just want to start a run" users and
  "I have an opinion about my presets" users.
- The product wants to telegraph editorial opinion (which presets exist)
  without locking in defaults.

## Worst when

- Users want to fine-tune on every run; the cards become overhead.
- The product wants every session to start from a fresh, empty state.

## Naming the 6 cards

This is editorial work. Suggested starting set:

- **Default** — 3-agent, the recommended starter set.
- **Lightweight** — 1-agent, single-role (when you don't need the verifier).
- **Heavy** — 3-agent, batched commits, all-ready (certification mode).
- **Resume** — last configuration (only enabled when a session exists).
- **Debug** — 2-agent, per-cycle, verbose log (5-min mental loop).
- **Custom** — start with an empty grid below.

The names matter. Avoid words like "Optimal", "Best", "Production-ready"
which can change meaning over time.

## Custom preset persistence

`~/.ra_cache/autopilot-presets.json` (per-user) — not per-plan, because
presets reflect *user preference*, not *project state*.

The plan's `.mp/autopilot-state.json` still holds the last-run config so
the user's "Resume" preset lands in the right project.

## Engineering cost (rough)

- 6 preset card components ~80 LOC each.
- Preset activation / loading logic ~120 LOC.
- Preset editor modal ~150 LOC.
- `.ra_cache/autopilot-presets.json` persistence ~50 LOC.
- The grid below reuses option 01's setup widget.

## What you give up

- A "configure from scratch" UX that's clean and explicit.
- The visual variety of seeing all choices at once (cf. option 01).
- Predictability: a renamed preset can change user behavior overnight.

## Variants worth considering

- **Per-project presets**: store presets in `.ra_cache/autopilot-presets.<plan-id>.json`
  so different repos have different defaults.
- **Git-tracked presets**: presets in `master-plan/autopilot-presets.toml`
  committed to the repo so the team shares defaults.
