# Reviewing sub-mode — stages 8 + 10

The reviewing sub-mode covers the coordinator's review domain in the 12-stage
timeline: External review (stage 8) and Re-review (stage 10). The
lesson-pattern pre-screen lives in this file (see "Lesson-pattern
pre-screen" below) — there is no separate external lessons catalog on
the consumer surface.

## Two-round review discipline

Round 1 is the runner's self-review at stage 6 (file findings with `--phase self`).
Round 2 is the coordinator's external review at stage 8.

The two rounds are separated by a session boundary. This is load-bearing: the
author should not be the only reviewer — same-session self-review is
unreliable. The session that produced the code (runner, stages 5-7) is
not the session that does the stage-8 external review (coordinator).
The coordinator's stage 8 review session is not the same as the planning
session (stages 1-4).

### Review loop (stages 8 → 9 → 10)

```
Stage 8 (External review) → coordinator files findings
    ↓
Stage 9 (Remediate)       → runner patches code (see mp-runner)
    ↓
Stage 10 (Re-review)      → coordinator re-verifies
    ├── clean → advance to Document (stage 11)
    └── new findings → loop back to stage 9
```

## Stage 8: External review

Goal: independently review the runner's work and file findings.

### Checklist

- [ ] Load `mp-flow` + `mp-coordinator` in a fresh session (not
  the planner session from stages 1-4, not the runner's execution session).
- [ ] **Automation consult:** `mp config get agent.automation.auto_remediate`.
  The threshold decides which findings the coordinator flags in the
  (c) hand-off as auto-remediate vs record-only (see SKILL.md →
  "Automation handoffs" for the full mapping). Default `"none"` — all
  findings are record-only and the runner decides at stage 9.
- [ ] Run the lesson-pattern pre-screen below: walk each pattern
  relevant to the change.
- [ ] For each defect found: `mp reviews finding add <id> --phase external --description "..."`.
  **Every finding is recorded unconditionally** — the audit trail is
  complete even when the threshold says "don't act on this".
- [ ] When the threshold is above `"none"`, classify each finding as
  `auto_remediate` (at-or-above threshold) or `record_only` in the
  hand-off payload. The runner reads this list at stage 9 to know
  which to fix immediately vs file back to the coordinator for later.
- [ ] File findings at the right severity using the canonical vocabulary
      that `[agent.automation].auto_remediate` and `SeverityRank`
      recognize — `low`, `medium`, or `high`. These are the same labels
      `should_remediate` parses; the threshold then decides whether
      each finding auto-remediates at stage 9 or stays record-only.
      Stale four-level severity labels (the legacy codebase review
      vocabulary) are forbidden here — `SeverityRank::from_config_value`
      would treat any value outside the documented set as `None`,
      silently defeating AC-06.
- [ ] After filing: `mp reviews finding list <id> --summary` to confirm the batch.
- [ ] **Hunk-export step (config-gated):** if the project has
  `[review] hunk = true` set in `mp config show`, file the
  external findings with spatial anchoring (`--file <path> --line
  <N>` flags on `mp reviews finding add`) so the hunk export
  carries line-level annotations rather than file-level summary
  notes. After filing all anchored findings, run
  `mp reviews hunk <M> --apply` at the stage-8 review handoff
  (AC-05): with a live hunk session the batch is applied; with no
  session it prints the batch + a pipe hint (exit 0) so the human
  can `hunk session comment apply --stdin` once a session is open.
  Offline alternative: `mp reviews hunk <M> --file <path>` writes
  the agent-context sidecar for `hunk diff --agent-context <path>`.
  The export is always-on for opted-in projects; opt-out (default)
  preserves the milestone-anchored review with no behavior change.
  The skill change is gated on the config flag — when
  `[review].hunk=false`, skip this step (no CLI call, no anchored
  flags).
- [ ] **Automation consult:** `mp config get agent.automation.push_after_review`.
  If `true`, `git push -u origin <branch>` after the (c) hand-off; if
  `false` (default), skip the push. The push target is the runner's
  branch from stage 5 (`agent.automation.branch_strategy`).
- [ ] Hand off to the runner (stage 9) per the (c) hand-off point in
  `mp-flow`'s Hand-off protocol section: external findings + evidence.

### Lesson-pattern pre-screen

Walk the patterns relevant to the change before filing findings:

- Green tests do not imply correct behavior.
- Reproducers catch what test suites miss.
- Gate parity — new paths don't bypass existing gates.
- The author should not be the only reviewer (session-boundary discipline).
- New bulk paths that bypass single-path validation.
- Dry-run paths that don't actually preview what would happen.

## Stage 10: Re-review

Goal: verify the runner's remediation of the findings filed at stage 8.

### Checklist

- [ ] Load a fresh coordinator session (not stage 8, not the runner's stage 9).
- [ ] Walk each finding resolved at stage 9: verify the remediation by testing.
- [ ] Run `mp milestone verify <id>` to re-check all ACs.
- [ ] `mp reviews finding list <id> --summary` shows zero unresolved external findings.
- [ ] **Mandatory close:** `mp reviews pass <id> --verdict ok --reviewer <who>` —
  records the review and, when lifecycle is still `done` with the legacy triple,
  auto-promotes to `complete`. Do not end stage 10 after verify alone.
- [ ] Confirm terminal: `mp show milestone <id> --fields 'milestone.lifecycle'` → `complete`.
- [ ] If clean: advance to Document (stage 11).
- [ ] If new findings surface: file them and loop back to stage 9.

### Loop-back criteria

If `mp milestone verify <id>` fails any AC, or if inspection surfaces new defects:
1. File the new finding(s): `mp reviews finding add <id> --phase external --description "..."`
2. Route back to stage 9 (runner's Remediate).
3. The co-ordinator session ends; a new coordinator session will pick up stage 10
   after remediation.

## AC verification integrity pre-flight

Before the Approve gate (stage 4, planning sub-mode), the coordinator runs an
AC verification integrity check. This walks every AC's `verification` field and
resolves the command or test target to confirm it exists. A bogus verification
field (e.g., a test name that doesn't exist in the crate) must fail the check
so the milestone never reaches the runner with unresolvable verifications.

### Command

```
mp plan verify-ac <id>
```

### Resolver classes

| Class | How resolved | Failure mode |
|-------|-------------|--------------|
| Cargo test | Parse `cargo test -p <crate> --test <target>` or `cargo test -p <crate> <test_name>`; resolve against the crate's `tests/` directory or `cargo test --no-run -- --list` | Test target not found in crate |
| Makefile target | Parse `make <target>`; resolve against `make -pn` target list | Target not found in Makefile |
| Bash script | Parse `./scripts/<name>.sh` or `bash scripts/<name>.sh`; resolve via file existence at project root | Script file not found |
| Python script | Parse `python scripts/<name>.py` or `python3 scripts/<name>.py`; resolve via file existence at project root | Script file not found |
| Manual check | The string starts with `manual:` — always passes (human verification) | N/A |

### Output

A per-AC table. Each row resolves the verification command or surfaces an
unresolvable symbol. Example:

```
AC-01  cargo test -p mp --test plan_verify_ac  =>  resolved (tests/plan_verify_ac.rs)
AC-02  cargo test -p raul --lib nonexistent_test =>  UNRESOLVABLE (test not found in crate raul)
AC-03  manual: content review                     =>  manual (always ok)
```

### Gate integration

`mp milestone approve --dry-run <id>` runs the same integrity check. If any AC
verification is unresolvable, the Approve gate fails with the integrity report.
The coordinator fixes the spec before re-running approve.

The integrity report is part of the (a) hand-off payload (mp-flow's Hand-off
protocol section). The runner rejects the hand-off if any AC verification is
unresolvable (defense in depth).

## See also

- `mp-flow` — Hand-off protocol section documents points (b), (c), (d): the
  review-loop data contracts.
- `mp-runner` — runner's self-review (stage 6) and remediation (stage 9).
- [planning.md](planning.md) — the Approve gate (stage 4) runs the integrity pre-flight.