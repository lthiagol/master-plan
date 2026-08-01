# v1.0.0-rc1 — Master Plan Toolkit 1.0 RC1

First release candidate of the 1.0 line. Ships the M187–M193 code-review
remediation batch (groups A–G) on top of the 2.0.0 dogfood train, plus the
version-policy re-anchor from `2.0.0-rc.X` to `1.0.0-rcN`.

## Highlights

- **Versioning re-anchor.** Workspace version is now `1.0.0-rc1`. The
  `2.0.0-rc.X` entries in CHANGELOG and `master-plan/plan.json` are
  preserved as historical record. See the Versioning policy section in
  CHANGELOG.md for the rationale.
- **Persistence safety + transactional mutations (M187).** PlanWriteTxn
  token routed through every authoritative plan-resource RMW path; multi-
  file mutation engine with versioned recovery manifest, fsync, idempotent
  retries; activity append post-commit non-relocking.
- **Schema + JSON input hardening (M188).** mini_schema explicit allowlist
  (rejects unknown keywords/types/regex/ref at compile time); one bounded
  reader for every durable JSON load and stdin; canonical containment for
  `--file` / `@file` / symlinks / `..`; unified create-input pipeline.
- **Typed lifecycle transitions (M189).** `MilestonePhase / MilestoneEvent /
  TransitionEffects` in mp-model; one pure `transition(current, event, ctx)`
  function owns sources, destinations, gate requirements, overlay changes,
  timestamp sync, and legacy alias sync; remediation restores exact prior
  phase; legacy `self-reviewed` / `reviewed` demoted to read/migration
  aliases; nested step ID dedup.
- **Watch state-machine correctness (M190).** WatchRunStore::transition
  persists every externally observable state bump; sequencer solely owns
  aggregate terminality; `PartialFailure` for mixed-completed queues;
  restoration wired into the Watch lane with a required fresh preflight;
  preflight tied to exact ordered queue fingerprint; `EventSource` trait
  waits on input-or-deadline (no busy-spin).
- **Raul selection, co-approval, timestamps (M191).** One canonical
  `visible_annotations()` projection shared by renderer, cursor limits,
  keyboard, and mouse; co-approval marked confirmed only after full
  success (retryable on failure); one strict RFC3339 parser in mp-model
  with leap / calendar / offset / UTC-day-boundary semantics, shared by
  durable audit timestamps and Raul humanize.
- **Security trust + install/display hardening (M192).** `ac_verify`
  requires repository-scoped trust before any verification command runs;
  one display-boundary sanitizer strips C0/C1/ESC/DEL/bidi/zero-width
  controls from plan/subprocess strings; install: POSIX-single-quote
  `env.sh` paths + atomic write; uninstall: canonical harness root +
  `O_NOFOLLOW` opened-dir fd + containment verification before deletion.
- **Architecture cleanup + comment remediation (M193).** `crates/mp/src/app.rs`
  trimmed to a 14-line dispatch surface; staged terminal setup rollback;
  Watch log tailing moved into polling state with bounded cache; renderer
  paths perform no I/O; active-schema filenames centralized; oracle tests
  that spawn `mp` relocated to `crates/mp/tests`; fixture-isolation helper;
  milestone/finding tags stripped from production comments per
  code-review-fixes.txt §10.4.

## Install

```bash
make install   # toolkit + OpenCode + Cursor + Pi
mp doctor      # verify
```

## Compatibility

- No breaking CLI changes relative to `2.0.0-rc.26`. The version string is
  the only externally visible change. `mp install` does not bake the
  version into any install path (`~/.local/share/mp/` is stable across
  renames).
- Consumers pinning to `>=2.0.0-rc.1` should be aware that `1.0.0-rc1` is
  a downgrade in semver pre-release ordering. There are no known external
  consumers at this time; the Homebrew formula has not yet been cut.

See CHANGELOG.md for the full set of changes and the historical
`2.0.0-rc.1`–`2.1.0-rc.1` chain.