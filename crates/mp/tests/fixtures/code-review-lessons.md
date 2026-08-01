# Code Review Lessons

Concrete lessons accumulated from real code reviews, organized so they can be
appended to over time. Each entry has a **name**, **what it looks like**, a
**real example**, and a **takeaway** for future reviews and skill design.

The intent is to seed a code review skill with patterns that actually caught
real bugs, not generic advice. Add new entries at the bottom of the relevant
section, or create a new section if the lesson doesn't fit an existing one.

---

## Methodology

### L1. Green tests do not imply correct behavior

**What it looks like.** A change ships with full test coverage, the suite is
green, smoke tests work, and the diff "looks reasonable". The reviewer
approves.

**Real example.** M94 (bulk milestone metadata ops) shipped on the first
review with 8 passing integration tests covering the happy paths. The review
verdict was "ok". An adversarial review two passes later surfaced **5
correctness bugs** — gate parity, dry-run honesty, op-arg validation, dead
code, and an N-times graph-load perf cliff — none of which any test had
caught.

**Takeaway.** A review that only checks "do tests pass" is a smoke test, not a
review. The review must independently try to break the change. Treat green
tests as a necessary precondition, not as evidence.

### L2. Reproducers catch what test suites miss

**What it looks like.** Reviewers ask "what happens if I pass X?" and try it
empirically — even when tests already exist for the surrounding area.

**Real example.** Three passes of M94 review found bugs by reproducing them
from the command line:

- Pass 2 reproduced `bulk set-spec-status review` on a no-AC milestone —
  found the gate-bypass bug (single-id checks G3, bulk didn't).
- Pass 3 reproduced `bulk set-priority --where ''` — found a vacuous-truth
  match-all bug inherited from `list milestones`.
- Pass 3 reproduced `bulk depends-on remove --depends-on 99` — found an
  over-strict existence guard that rejected a no-op.

None of those paths had tests. None would have been found by reading code.

**Takeaway.** For every new code path, ask: "what's the boundary case I would
never write a test for?" and reproduce it from the shell. Write down the
reproducer as a finding even if you don't intend to keep the test — the
evidence is what matters for the review.

### L3. Read the committed code, not the diff

**What it looks like.** Reviewers re-read the final committed version of the
change rather than relying on the diff they reviewed when it was uncommitted.

**Real example.** Pass 3 of the M94 review was run *after* the fix commit
landed. Five more issues surfaced (F-06..F-10), all of which were in code that
had been visible in the original diff but hadn't been probed. The fresh-eyes
moment was reading the committed shape, not the in-flight one.

**Takeaway.** Schedule a "post-commit review" after every substantial change.
The diff is what you want to *land*; the committed code is what you'll *live
with*.

### L4. Multiple review passes have uncorrelated findings

**What it looks like.** Each pass of the same review finds different bugs.

**Real example.** M94 had three passes — 0, 5, and 5 findings respectively.
The Pass 2 bugs were *gate-parity, dry-run honesty, op-arg validation, dead
code, perf* — mostly about the surface of the implementation. The Pass 3 bugs
were *no-op semantics, vacuous filter match, dead branch, docstring gaps,
concurrency notes* — mostly about the edges of the implementation. Almost no
overlap.

**Takeaway.** Plan for at least two adversarial reviews on substantial changes
using different mental frames:
- **Spec compliance** (does this match the spec? parity with sibling commands?)
- **Edge cases** (empty args, blank args, idempotent ops, no-op paths)
- **Empirical** (run it; try to break it; compare output to docs)

The findings will be largely orthogonal.

### L5. The author should not be the only reviewer

**What it looks like.** The same session that wrote the change runs the
review.

**Real example.** All three passes of M94 review were run by the same agent
in the same session that wrote the code. Pass 1 found nothing. The defects
that Pass 2 and Pass 3 caught were obvious in retrospect but invisible to the
implementer's frame.

**Takeaway.** For high-stakes changes, route review to a different session,
different model, or different agent role. The "executor / reviewer" split
exists in the Master Plan playbook for a reason — it works. Self-review in
the same session is a useful first pass, not the final one.

---

## Patterns to look for

### L6. New bulk paths that bypass single-path validation

**What it looks like.** A new "bulk" or "fan-out" command is added next to an
existing single-id command. The bulk path re-implements the operation but
forgets to call the same gate/validation function the single-id path uses.

**Real example.** M94's `bulk set-spec-status` called `apply_spec_status`
directly, which only writes the field. The single-id `set-spec-status` calls
`gate_errors_for_spec_status` + `check_g14_approval_requests` first. Bulk
silently promoted under-spec'd milestones to `review`/`ready` until F-01.

**How to find it.** Diff the new bulk handler against the single-id handler.
List every validation, gate, hook, or callback the single-id calls and check
the bulk calls it too. Better: extract a shared `with_gates()` function that
both call.

**Pattern:**

- **Pattern.** Every bulk handler MUST route through the same `with_gates` /
  preflight helper that the single-id handler calls before writing. A bulk
  path that writes the field directly (without calling the gate) is the
  bug.
- **Positive fixture.** `crates/mp/tests/ac_update_bulk.rs::bulk_update_unknown_ac_does_not_partial_apply`
  exercises the gate-parity contract for the `bulk_update` handler: a
  malformed AC id is rejected by `preflight_warning` BEFORE any AC is
  written, so a partial-apply bug (where some ACs land before the
  invalid one fails) is caught. Test:
  `cargo nextest run -p mp --test ac_update_bulk -E
  'test(/bulk_update_unknown_ac_does_not_partial_apply/)' --no-fail-fast`
  exits 0.
- **Negative fixture.** A bulk handler that does
  `apply_op_to_id(id, &op)?;` in a loop without ever calling the
  preflight. Detect with:
  `rg 'fn bulk_update_three_acs' -A 30 crates/mp/src/commands/milestone.rs |
  rg 'preflight_warning'` — must print at least one hit (the preflight
  runs before the per-id apply loop).

### L7. Dry-run paths that don't actually preview what would happen

**What it looks like.** A `--dry-run` flag is added. It skips the write but
also skips the validation that would have produced the failure. Dry-run
reports "ok" for rows that the live run would reject.

**Real example.** M94's dry-run on `depends-on add` skipped the cycle check
(commit=false just returned the would-be modified milestone). Live runs
correctly detected cycles. Dry-run was a liar — F-02.

**How to find it.** Trace each validation in the live path. For each one,
ask: "does dry-run also exercise this validation?" If not, dry-run is
incomplete and the row should report the would-be failure with `ok: false`
even in dry-run mode. Use a `commit: bool` parameter threaded through the
mutation helpers so dry-run runs the check without writing.

### L8. Operation-level args that re-validate per target

**What it looks like.** A bulk command takes an arg like `--priority`,
`--status`, or `--depends_on`. The arg is validated once per target inside
the per-target apply, so a bad value produces the same error N times for a
batch of N.

**Real example.** M94 originally called `set_priority` per id without a
batch-level validation. `--priority critical` reported "invalid priority"
twice for a batch of two (F-03).

**How to find it.** Look for enum / set / existence checks in the per-target
apply function. If the same arg is constant across all targets in the batch,
validate it once at dispatch (before the loop) and bail with one error.
Apply the same rule to: enum validation, regex validation, existence of
external references.

**Pattern:**

- **Pattern.** Validate any operation-level argument ONCE at dispatch, before
  the per-target loop. The per-target apply function assumes the arg is
  already valid.
- **Positive fixture.** `crates/mp/src/commands/milestone_bulk.rs` validates
  `--priority` once via `preflight_priority(&priority)?` before iterating
  over ids. Test:
  `cargo nextest run -p mp --test suite_milestone -E
  'test(/bulk_validates_operation_level_args_up_front/)' --no-fail-fast`
  exits 0; that test pins the single-error contract (operation-level args
  rejected once at dispatch, not N times per target).
- **Negative fixture.** A bulk handler that defers `--priority` validation
  to `apply_priority(id, &priority)?` inside the loop. The error appears
  per-id. Detect with:
  `rg 'apply_priority' crates/mp/src/commands/milestone_bulk.rs` — if
  the call site is NOT preceded (in the same function) by an early-exit
  `preflight_priority` call, the gate is missing.

### L9. New code inheriting pre-existing bugs

**What it looks like.** A new feature reuses a helper from elsewhere in the
codebase. That helper has a latent bug the new feature inherits, so the bug
ships with the new feature.

**Real example.** M94 used `parse_where_filters` from `list milestones`. That
parser silently skipped blank entries, so `list --where ''` matched every
milestone (vacuous truth). M94 inherited and propagated the bug — F-08.

**How to find it.** When the new feature reuses an existing helper, audit
the helper's behavior at the edges (empty input, blank input, malformed
input). Run the same edge-case reproducers against the helper in isolation.
If the helper has a bug, fix the helper, not the caller.

### L10. Idempotent operations that reject non-existent references

**What it looks like.** A remove / drop / unset / clear operation validates
that the thing being removed actually exists. The validation should be a
no-op (removing nothing is fine), but the code rejects the call.

**Real example.** M94's `bulk depends-on remove --depends-on 99` rejected
with "target doesn't exist" because the existence guard from `add` was
copied to `remove`. Removing a non-edge is a no-op and should succeed —
F-07.

**How to find it.** For every bulk operation, classify each input as
*additive* (creates a new edge/record) or *subtractive* (removes an edge or
clears a value). Subtractive operations should not fail on missing
references; additive ones should. The existence guard belongs on additive
ops only.

### L11. Cycle or graph checks using stale snapshots

**What it looks like.** A bulk operation loads the full graph once, then
checks each mutation against that snapshot. If another process (or another
concurrent batch) modifies the graph between load and write, the check can
false-positive (says cycle but live would succeed) or false-negative (says
no cycle but live would create one).

**Real example.** M94's `build_depends_on_graph` loads once at dispatch;
the cycle check uses that snapshot. F-10 documents the TOCTOU caveat in
the docstring; multi-process safety needs a plan-level lockfile.

**How to find it.** Search for "snapshot", "cache", "build_*_graph" in the
new code. If the check uses a snapshot, document the staleness assumption in
a docstring and verify the single-process case is correct (don't just
trust it). For multi-process safety, plan for a lockfile or transaction
boundary.

### L47. Atomicity in batch processing: write the result BEFORE deleting the inputs

**What it looks like.** A batch function processes N inputs in a
loop, doing source.delete() as it goes. If any later step (parse,
validate, write) fails, the inputs that were already processed are
gone, and the merged result was never persisted. A user retrying
the batch is missing the items the first run already consumed.

**Real example.** M102's `migrate_kinds` (subagent review H-1) was
deleting each source file (`track-bugfix.json`,
`track-tweak.json`, `ideas.json`) as it was processed. If a parse
error in the second source file or any later `?` propagation
broke, the items in the first file were already gone. The fix:
rename each source to `.bak` FIRST, build the merged backlog
in memory, write the merged result (via temp + atomic rename),
THEN delete the source files (and the .bak). The .bak files are
recoverable from git if the write fails. A new test
(`migrate_kinds_clears_bak_after_successful_migration`) pins the
cleanup contract.

**How to find it.** For every batch function:
- Trace what happens to the inputs at each step. If they're
  deleted, renamed, or overwritten, can a failure between two
  steps leave the system in a state that cannot be undone by
  re-running the batch?
- The standard fix is the **atomic-replace** pattern applied to
  batch: rename-input → build-merged-state → write-target →
  delete-renames. A failure in any step before "write-target"
  leaves the rename intact; a failure after leaves the merged
  state in place. Only the post-write delete needs to succeed
  for the operation to be complete (a no-op if re-run).
- Add a regression test that simulates a failure in the merge step
  (e.g., inject an invalid input between two good inputs) and
  asserts the .bak files are still present.

**Takeaway.** A batch operation that deletes inputs as it goes is
a "torn write" — partial state on failure. Apply the
atomic-replace pattern (rename-input → build-state → write →
delete-rename) to the whole batch. Each input gets a
recoverable fallback; the merged result lands as one atomic
write.
### L12. "before" / "after" fields that misrepresent failures

**What it looks like.** A bulk operation reports `before` / `after`
snapshots in each row. When a target fails to load (file missing,
permission denied, parse error), the code falls back to a sentinel value
like `""` or `[]` or `null`, which makes the row look like the field was
legitimately that value.

**Real example.** M94's bulk set-priority on a bogus `--ids M9999`
reported `"before": null` — indistinguishable from a milestone whose
priority was literally unset. F-06: omit the field when the snapshot can't
be taken.

**How to find it.** Trace the load helper. If the load returns
`Result<T>` or `Option<T>`, the snapshot layer should propagate the
absence — `None` in the response struct, omitted key in the JSON output —
not substitute a default. A `null` in the row should mean "intentionally
absent", not "couldn't read".

### L13. Dead code sneaking in via refactors

**What it looks like.** A refactor renames or restructures a type but
leaves the old helper marked `#[allow(dead_code)]` because nothing calls
it anymore. The helper stays in the tree and confuses future readers.

**Real example.** M94 had `result_row_for_tests` as an unused wrapper
around `result_row`. The original implementation had multiple callers; a
refactor moved the logic inline and left the helper with `#[allow(dead_code)]`
— F-04.

**How to find it.** After every refactor, run `cargo build` with
`-W dead_code` (or the default rustc lint) and fix every warning. Don't
suppress warnings to land code; suppress them only when the suppression is
the actual design (e.g. test-only helpers called by macros).

**Pattern:**

- **Pattern.** A `#[allow(dead_code)]` annotation is a "look here next"
  flag. Every such annotation must be audited: either the function has a
  real caller elsewhere (delete the allow) or the function is dead
  (delete the function). Suppressing warnings is debt, not design.
- **Positive fixture.** `crates/mp/src/migrate.rs` has zero
  `#[allow(dead_code)]` annotations and zero `_keep_*_referenced()` shims.
  Test: `rg '#\[allow\(dead_code\)\]' crates/mp/src/migrate.rs` returns no
  matches AND `rg '_keep_.*_referenced' crates/mp/src/migrate.rs` returns
  no matches.
- **Negative fixture.** A refactor that left a shim like
  `#[allow(dead_code)] fn _keep_helpers_referenced() { legacy_x(); }` —
  the allow masks a real bug (the helper has callers elsewhere; the shim
  is pointless). Detect with:
  `rg -B1 '_keep_.*_referenced' crates/mp/src/ | rg '#\[allow\(dead_code\)\]'`
  — any match is a finding.

**L13 addendum — the dead-call shim that creates fake call sites.**
A sibling pattern: the `#[allow(dead_code)] fn _keep_X_referenced()`
shim that *creates* a fake call site to suppress the warning. M100's
`crates/mp/src/migrate.rs` carried a
`_keep_legacy_helpers_referenced()` that called
`legacy_spec_status_to_lifecycle` and
`legacy_execution_status_to_lifecycle` purely to keep the warning
quiet. Both functions were already `pub` in `mp-model` with real
callers (`effective_lifecycle`, `milestone.rs:490`). The shim was
pointless — and the `#[allow(dead_code)]` masked that fact. Fix:
delete the shim, delete the now-unused import. The broader rule:
any `#[allow(dead_code)]` annotation is a "look here next" flag.
Audit the function it's attached to: is there a real call site
elsewhere? If yes, the allow is unnecessary. If no, the function
is dead and should be removed. The annotation is a temporary
patch that should never outlive the work that added it.

### L14. Awkward control flow left over from earlier iterations

**What it looks like.** Code has an empty `if` branch (`if cond { /* empty */ } else { ... }`)
or a conditional with no body, or a `match` arm that's identical to the
fallthrough. These usually signal "this used to do something; the change
made it do nothing; I forgot to clean it up".

**Real example.** M94's dry-run path had `if outcome.ok { /* comment-only */ } else { failed += 1; }`
— F-09.

**How to find it.** Look for empty blocks (`{}`), comments-only blocks, or
branches with identical bodies. Each one is a smell. Ask: "what did this
used to do? Does it still need to exist?"

**Pattern:**

- **Pattern.** Every `if`/`match` arm must contain real code or be
  removed. Empty blocks (`if cond {}`) and comment-only blocks
  (`if cond { /* TODO was here */ }`) are refactor residue — they used
  to do something; they don't anymore.
- **Positive fixture.** `crates/mp/src/commands/milestone_bulk.rs` has
  zero empty `if` branches. Structural audit:
  `rg -n 'if .* \{\s*(\}|/\*[^}]*\})' crates/mp/src/commands/milestone_bulk.rs`
  returns no matches. Behavioral smoke:
  `cargo nextest run -p mp --test suite_milestone -E
  'test(/milestone_bulk/)' --no-fail-fast` exits 0 (the milestone_bulk
  module compiles and its gates pass with no empty-branch residue).
- **Negative fixture.** `if outcome.ok { /* empty */ } else { failed += 1; }`
  in any production file — the empty branch is the smell. Detect with:
  `rg -n 'if [^{]+\{[[:space:]]*(\}|/[[:space:]]*\*[^}]*\})' crates/mp/src/`
  — every match is a finding.

### L26. Docstring lists the fields a function handles — diff against it

**What it looks like.** A function's docstring or module comment enumerates
the fields/items it processes ("diffs outcome, problem, scope, ACs, open
questions, and design decisions"). The implementation handles a strict
subset. Nothing flags the gap because each field is handled by a separate
helper and no single test exercises all of them.

**Real example.** M80's `spec_review.rs` module docstring (lines 6-7)
promised the spec diff covers "outcome, problem, scope, ACs, open
questions, and design decisions". The `spec_review` projection included
`design_decisions`. But `diff_spec_fields` (line 288) never compared
`design_decisions` between baseline and current — it handled outcome,
problem, scope, open_questions, and ACs, then returned. A reviewer who
edited only a design decision saw "No spec changes since baseline" — a
silent false-negative. F-01. The only end-to-end diff test mutated
`intent.outcome` alone, so the omission was structurally uncatchable.

**How to find it.** When a docstring, comment, or spec enumerates a list
of handled items ("X, Y, and Z"; "covers A, B, C"), treat it as a contract
and audit the implementation against it item by item. For each item in the
prose list, find the code that handles it; if you can't, that's a finding.
Cross-check against the sibling surface (here, the review projection
included design_decisions but the diff didn't — the two surfaces
disagreed). Add a per-field test that mutates *only* that field and
asserts a change record appears.

---

### L48. CLI surface changes are a contract — emit legacy fields as deprecation aliases during a transition window

**What it looks like.** A command's output is restructured: old
keys are dropped, new keys replace them. The command's
_consumers_ (other tools, other commands, scripts, TUI views) all
read the old keys. A consumer doesn't notice its reads return
`null`/`0`/`""` — it silently renders a zeroed plan. The bug
shows up at the user hands, not in the producer's test suite.

**Real example.** M102's `cmd_status` rewrite (subagent review C-2)
dropped `milestones.{total, by_execution_status, by_spec_status,
by_lifecycle, track_pending, annotations_open}` from the output.
`crates/raul/src/commands/{status,explain,onboard,watch}.rs` and
`tui/dashboard.rs` all read those old keys, so they silently
zero-filled. The fix: emit BOTH the new shape and the legacy
shape from the same `build_lanes` LaneReport, so the wire shapes
don't drift. The new `lanes` block is the canonical source; the
legacy fields are deprecation aliases derived from the same data.

**How to find it.** For every command that drops a key from its
output, grep the rest of the codebase (and any external consumers
the docs name) for that key. If anyone reads it, emit it as an
alias during a deprecation window, with the new canonical key
beside it. The window: announce the deprecation in the docs, the
release notes, and the schema (when applicable). When the window
closes, remove the alias; consumers are expected to be on the new
key by then.

**Takeaway.** A wire-format change is a multi-party contract. The
producer's tests pass when the producer's own output is internally
consistent; the consumer's tests pass when the consumer's input is
what the producer emits. A change that breaks the producer→consumer
contract breaks both at once — silently, with `null` and `0` as
the only signal. Treat legacy fields as a transition aid: emit
them as aliases, document the deprecation, plan the removal.
### L39. A `max()` derivation over multiple inputs masks single-input bugs when both inputs happen to agree

**What it looks like.** A field is derived from two legacy inputs via
`max()` (or `min()`, fold, comparator-merge). Each input maps to a
target value via its own function. The derivation is correct *as
written*, but one input's mapping function is wrong. Tests pass
because (a) fixtures that exercise the bug also exercise the other
input with a value that doesn't bias the result, and
(b) the test for the wrong input pins the wrong value.

**Real example.** M100's `effective_lifecycle` derives a unified
lifecycle from `legacy_spec_status` and `legacy_execution_status`
via a numeric order comparison (`max`). The dogfood plan had 95
milestones all at `spec_status=verified, execution_status=done`.
The spec-side mapper correctly returned `complete` (verified is
terminal). The exec-side mapper returned `complete` too — but
that was wrong. M100's lifecycle defines `done → self-reviewed →
reviewed → complete`, so legacy `exec=done` meant "execution
finished, awaiting review" — *not* terminal. With the wrong
exec-side mapping, a milestone at `spec=implemented, exec=done`
would be reported as terminal `complete`, skipping the entire
review/verify phase.

The bug was masked against the dogfood because the spec-side
mapping (correct) and the exec-side mapping (wrong) both produced
`complete` for the only fixture that mattered. The unit test
`legacy_execution_status_mapping` pinned the wrong value
(`assert_eq!(...("done"), "complete")`) — the test was wrong,
the code matched the test, the suite was green, the bug shipped.
External review caught it by reading the M100 lifecycle definition
line-by-line.

**How to find it.** When deriving a value via `max`/`min`/fold:

- For each input, write a fixture where the *other* inputs are
  unset (or set to a value that doesn't bias the result). Test the
  wrong input alone.
- Treat the "spec says X, the implementer read it as Y" failure
  mode separately. The spec is the source of truth; the test
  should encode the spec, not the implementation. When a test
  asserts `legacy_X_to_Y("done") == "complete"` and that assertion
  is what *makes the implementation match the test*, the test is
  encoding the implementer's reading, not the spec. The check is:
  does this test reference the spec, or only the implementation?
- Audit each input's mapping function against the spec, not
  against the implementation. Cross-check with the lifecycle/state
  diagram.

**Takeaway.** A multi-input `max()` is a *risk concentrator*: a
single wrong input becomes invisible because the other input
*accidentally* produces the right answer. The review heuristic:
when you see "derives from N inputs", ask "is each input's
mapping independently correct, or do they happen to agree on the
fixtures?" When two inputs disagree on the right answer, the bug
is loud; when they agree by accident, the bug is silent until a
fixture exercises only the wrong input. Write that fixture.

### L40. Mixed-read during a migration window: every sibling read path must move together

**What it looks like.** You migrate a field from `A` to `B`. The
canonical reader is updated to use `B`. Sibling readers in the same
module keep reading `A` because they're "still correct" (the field
is still present in fixtures). The result: a mixed-read code path
that works during the migration window but becomes a *dead branch*
post-migration, silently stopping whatever the sibling read enforced
(G2 gate, W3 warning, drift detector).

**Real example.** M100 introduced `lifecycle` to replace `spec_status
+ execution_status`. The new `validate/plan.rs` reads `lifecycle`
in some gates (via `effective_lifecycle`) but reads the legacy
fields directly in others:

- Line 82: `if m.milestone.spec_status == "ready"` (G2 open-question check)
- Line 106: `if m.milestone.spec_status == "verified"` (G6 AC-passed check)
- Line 123: `if m.milestone.execution_status == "done" && m.milestone.spec_status != "verified"` (G7 done→verified check)
- Lines 130-132: G5 implementation-plan-before-ready check
- Lines 79, 94: G1/G3 reads

The new `gates.rs::check_gate_g1` was updated to use
`effective_lifecycle`, but `validate/plan.rs` was not. Today this is
consistent (legacy fields still populated). Post-bulk-migration,
when the legacy fields are cleared, the legacy-read gates become
dead branches — they fire "0/0" because the condition can never be
true. The bug surfaces as "G6 not firing on a milestone that should
trip G6" — silent loss of gate enforcement.

**How to find it.** When migrating a field, audit *every* read site
in the same module (or repo) and move them all together. The
migration-window rule:

- Either move every read site to the new field on the same commit
  (preferred — no mixed-read window), or
- Document the window explicitly: name the readers that stay on
  legacy, the readers that move to new, the date/commit by which
  the remaining readers must convert, and the test that will
  detect when the window has closed (a fixture with cleared legacy
  fields that asserts the gate still fires).

**Takeaway.** A migration is not a single commit; it's a plan
with a deadline. A reader that "still works" today is a reader
that will silently fail tomorrow. The L24 ("refactors surface
latent bugs in adjacent code") pattern applies here too: the
mixed-read state is a latent bug that surfaces only post-
migration. List the readers that need to convert and convert them
together, or accept the mixed-read state explicitly with a
deadline and a test that closes the window.

### L41. Render and hit-test must share the same layout

**What it looks like.** The render function computes a layout
based on terminal width (e.g. "compact mode if width < 60") and
emits widgets at specific columns. The hit-test function
(mouse-click → which widget?) computes *which widget was clicked*
based on a different value (e.g. the click column itself, or a
hard-coded threshold). The two diverge for any terminal width
where the heuristic guess differs from the actual layout.

**Real example.** M91's `render_tab_bar` keyed compact mode off
`area.width < 60` (the actual terminal width). `handle_mouse`
keyed it off `(x as u32) < 60` — the *click column*. At a 100-col
terminal, a click at column 50 was hit-tested as compact while
the bar rendered full labels → wrong lane or miss. R-3 fixed it
by threading `terminal.size()?.width` from `run_tui_inner` into
`handle_mouse`. R-9 was a sibling of the same class: at narrow
widths (`< 60` cols with 7 lanes), `render_tab_bar` switched to
an overflow strategy (subset of tabs + edge indicators + ellipses)
but `tab_hit_test` still assumed a contiguous left-to-right layout
of *all* lanes. A click in the overflow-rendered bar hit-tested
against the wrong (or hidden) lane.

**How to find it.** For every render function that consumes
terminal width (or any input that affects layout), find every
hit-test or input-routing function that operates on the same
layout. The rule:

- Render and hit-test must agree on the layout. The cleanest way
  to enforce this is to compute the layout in a shared helper
  (`compute_tab_layout(area_width) -> Vec<TabSlot>`) and have both
  render and hit-test consume it. The render function iterates and
  emits; the hit-test function searches the same vector.
- Heuristic shortcuts (e.g. "if x < 60, treat as compact") are
  smell — they encode a value the render function already
  computed elsewhere. Centralize the computation or pass the
  rendered value through.

**Takeaway.** A render/hit-test pair is a tiny invariant: "if the
user clicked where they see X, X is selected." Violating that
invariant is silent in code review (both functions look
self-consistent) and loud in user hands (clicks select the wrong
lane). The test pattern: any test that exercises the hit-test
should *also* exercise the render function with the same input,
and assert the layout the render produced is what the hit-test
expected. The structural fix is to share a layout vector between
the two.

---

## Anti-patterns in tests

### L15. Tests written against the implementation, not the spec

**What it looks like.** A test for a new feature exercises the paths the
implementer thought of. The spec has other paths and edge cases the test
suite never visits.

**Real example.** M94's 8 original integration tests covered ids/where/dry-run/
partial-failure/cycle/empty-targets — all the paths the implementer built.
None of them tested gate parity with single-id (because the implementer
forgot the gates existed). The spec listed the gates implicitly via
"matches single-id set-spec-status behavior" — which I missed because I
was reading my own tests, not the spec.

**Takeaway.** For each new feature, write a checklist from the *spec* (or
the user-facing contract), not from the implementation. Walk the checklist
and write a test for each item. Anything in the spec that has no test is a
review finding waiting to happen.

**Pattern:**

- **Pattern.** Each spec field (gates, behavior, edge case) MUST map to at
  least one integration test. The spec is the test contract; the test
  suite is the audit. Coverage-by-spec, not coverage-by-implementation.
- **Positive fixture.** `crates/mp/tests/suites/milestone_bulk.rs` (the
  `milestone_bulk` module of the `suite_milestone` target) carries one
  test per spec contract clause:
  `bulk_set_spec_status_blocks_on_gates` (gate parity),
  `bulk_validates_operation_level_args_up_front` (args validated once),
  `bulk_depends_on_dry_run_previews_cycle` (dry-run previews the cycle),
  `bulk_depends_on_remove_allows_nonexistent_target` (remove is a no-op
  when the edge is missing). Each test name names the spec clause it
  covers. Test:
  `cargo nextest run -p mp --test suite_milestone -E
  'test(/milestone_bulk/)' --no-fail-fast` exits 0 with all spec-clause
  tests green.
- **Negative fixture.** A spec clause like "matches single-id
  set-spec-status behavior" with no test pinning it. The review catches
  it the next time someone touches `set-spec-status` and forgets to
  update the bulk path. Detect with:
  `rg -A2 'AC-' master-plan/milestones/*.json | rg -v 'manual:' | rg
  'verification'` and cross-reference each `verification` line against
  the `tests` block of the same AC — any `verification` that doesn't
  match a runnable command is a coverage gap.

### L16. Smoke tests without negative cases

**What it looks like.** The integration suite covers happy paths and a few
"id not found" cases. It doesn't cover: empty input, blank input, idempotent
re-application, no-op paths, edge values of the result enum.

**Real example.** M94's original 8 tests had positive cases and one
"id-not-found" negative case. They didn't cover: empty `--where` (vacuous
match), self-dependency (cycle), idempotent re-set (no-op), gate-blocked
transitions (failed per-id with structured error).

**Takeaway.** For each new feature, list the negative cases:
- What if the input is empty?
- What if the input is the wrong type?
- What if the operation is applied to a value that's already at the target?
- What if a prerequisite isn't met?
- What if the same op is run twice?

Each negative case is a test that catches a real bug.

### L17. Tests that don't reproduce the documented bug

**What it looks like.** A bug is filed with a clear description. The fix
lands with a test that *checks the fix in isolation* but doesn't reproduce
the bug *as the user saw it*.

**Real example.** (Anti-pattern, not a M94 case.) A common failure mode
after a fix: the new test asserts the fixed behavior using a unit test on
the helper, but the integration path that originally exposed the bug
isn't tested end-to-end. The bug regresses silently because no test
catches the user-visible failure.

**Takeaway.** When fixing a bug, write the failing test *first* using the
exact reproduction steps from the bug report (CLI args, fixture setup,
asserted output). Run it; watch it fail; fix the code; run it; watch it
pass. The integration test is the artifact.

### L28. Extracted helper tested in isolation, call site still unprotected

**What it looks like.** A bug fix extracts the buggy logic into a
well-named helper, then the test exercises the helper directly — not the
original call site. The helper is correct, but the wiring at the call site
can regress (someone re-inlines the old buggy pattern) and the test still
passes.

**Real example.** M87's S5 fix extracted `co_approval_approve` from the
`CoApprovalAction::Approve` arm of the TUI event loop (the bug was `let _
= runner.run_raw(approve)?;` where `?` propagated before the `let _`
binding). The test, `co_approval_approve_tolerates_approve_failure`
(approval_flow.rs:99), calls `raul::tui::runner::co_approval_approve`
directly with a synthetic runner. If someone edits the event-loop arm to
re-inline `let _ = runner.run_raw(approve)?;` — the exact regression —
the test stays green because it never drives the event loop. F-06.

**Takeaway.** When a fix extracts a helper from a call site, the test
must cover the *path the user actually triggers*, not just the helper.
If the call site is hard to drive in a test (e.g. a TUI event loop),
either: (a) keep the call site trivially thin ("the arm does nothing but
call the helper") and document that invariant in a comment at the arm, or
(b) extract the arm's body into a function keyed by a synthetic `Event`
the test can construct. A green test on the helper alone is not evidence
the call site is wired correctly.

---

### L42. Unit-test the unit; CLI-test the entry

**What it looks like.** A library function or constructor is
unit-tested in isolation. The CLI surface that calls it is
not. If the CLI dispatcher ever takes a different code path
(passing different flags to the constructor, choosing a
different initial state based on a CLI flag), the unit tests
pass but the CLI breaks. The unit tests give a false sense of
correctness — they cover the *constructor* but not the *entry*.

**Real example.** `App::new()` initializes `active_lane =
Lane::Overview`. Multiple unit tests in `tui_tab_bar.rs` and
`tui_state.rs` verified `App::new()` returns `Lane::Overview`.
None of them verified the *CLI dispatch path*:
`raul -i` flag parsing → `run_tui` → `App::new()`. If a future
change routes `raul -i` through a different construction path
(e.g. `--board` flag → `Lane::Board` start), the unit tests
still pass but the CLI silently changes default behavior. R-5
fixed it by adding tests that drive the actual `TuiOptions`
decision end-to-end: bare `raul` and `raul -i` both land on
Overview; `raul -i --board` lands on Board. The new tests would
catch any future routing change.

**How to find it.** When a constructor or library function has a
*default behavior* (initial state, default config, default
flags), audit the *entry points* that call it. For each entry
point:

- CLI dispatch (`parse_args → dispatch → constructor`)
- Test harness (`TestEnv::run(&["path", "arg"])`)
- Library API (a public function that wraps the constructor)
- Background/scheduled invocation (a periodic job)

Each entry point is a *separate code path* that may pass different
arguments to the same constructor. Test each path. A test that
calls `App::new()` directly tests *one* path; the CLI test covers
the other.

**Takeaway.** Unit tests pin the unit. Entry-point tests pin the
entry. A constructor with a default behavior needs both:
unit tests for the constructor's behavior in isolation, entry-
point tests for the wiring between CLI/library/harness and the
constructor. The split: unit tests answer "what does this

### L43. Test fixture writes to the same wrong path as production code — test and bug are mutually reinforcing

**What it looks like.** A new code path has a path-bug
(production reads from the wrong directory or filename). The
test fixture uses a `write_*` helper that hard-codes the same wrong
path. The test passes because the test setup and the production
reader agree on the wrong location — the two errors cancel. The
bug stays invisible to the test suite and the reviewer.

**Real example.** M102's `migrate_kinds` (subagent review C-1) was
hard-coded to read from `plan_dir.join("track-bugfix.json")` while
the real layout is `plan_dir.join("tracks").join("{kind}.json")`.
Every other reader in the codebase uses
`store::load_track(ctx, kind)` which routes through
`PlanContext::track_path`. The new migration was the only site
reading the old flat layout. The test fixture
(`write_track_bugfix`) used the same wrong path. The test
exercised the production reader on a fixture it itself wrote; both
agreed on the wrong location. The bug was caught only by an
external smoke test against a fresh plan, not by any unit test.

**How to find it.** For every test that constructs a fixture:
- If the test uses the same path string as the production reader,
  the test exercises the production reader on a fixture it itself
  wrote — a closed loop. Break the loop by sourcing the fixture
  from a different path than the reader reads from.
- Use the production code's own *reader* (e.g.
  `store::load_track(ctx, "bugfix")`) to load the fixture in a
  follow-up assertion, proving the round-trip.
- When in doubt: have a separate smoke test that creates a fresh
  plan via the public API and exercises the new path against
  that. A unit test that shares infrastructure with the bug
  under test can mask it.

**Takeaway.** A test that constructs its own fixture and exercises
production code against it can hide the exact bug it should catch.
The test and the production reader form a self-contained
subsystem; a bug in both is invisible to that subsystem. Make the
fixture come from a *different* path than the reader, and have a
smoke test against a real plan as the backstop.



## Process

### L18. Findings belong in the plan, not in chat

**What it looks like.** A reviewer types "I noticed X is broken" in chat
and the implementer fixes it on the spot. The review artifact is lost.

**Real example.** M94's three passes produced 10 findings. All 10 were
filed as `mp reviews finding add` against the milestone, marked open,
remediated, then resolved. The plan carries the full audit trail.

**Takeaway.** Use `mp reviews finding add` (or whatever the project's
equivalent is) to file findings. Don't fix findings without filing them
first. The audit trail is the value of the review.

### L19. Reviewer verdict should reflect the evidence, not the goal

**What it looks like.** The reviewer wants to ship the milestone, so they
write `mp reviews pass --verdict ok` even though their notes mention "a few
small issues". The verdict becomes the answer; the notes are decoration.

**Real example.** M94's first review passed `verdict=ok` based on green
tests. The same review session had no findings — the issues were filed
later in a separate adversarial pass. Had the first reviewer noted the
gaps they suspected, the verdict should have been `changes-needed` or
`pending-findings`.

**Takeaway.** The verdict encodes the *reviewer's confidence in the
shipped state*, not their hope for the milestone. If you have doubts,
file them as findings and use `changes-needed`. If the milestone is clean,
file zero findings and use `ok`. Mixed signals are a process bug.

### L20. Commit remediation as `fix`, original as `feat`

**What it looks like.** A milestone lands as one commit that includes both
the original implementation and the post-review hardening. The history
shows the milestone "always" had the hardened behavior.

**Real example.** M94 first landed as one commit (`ef63d1b feat: bulk
ops + first-round hardening`). The third-pass review then landed as a
separate commit (`074d427 fix: third-pass polish F-06..F-10`). The two
commits tell two different stories:
- `ef63d1b` — the milestone shipped with these five known findings, fixed
  before merge.
- `074d427` — five more findings surfaced by post-commit review.

**Takeaway.** For substantial changes with multiple review passes:
- First commit: `feat(...)` with the shippable implementation, including any
  findings fixed before merge.
- Subsequent commits: `fix(...)` per review round, naming the finding IDs
  in the commit body.

The history is honest about what was caught when, and `git blame` makes
it easy to trace a fix back to the finding.

### L21. Spec lists "narrow filter" — is it a partition or a strict subset?

**What it looks like.** A spec describes a new artifact-type scope as a
"narrow filter" of an existing related scope. The implementer reads this
as a *partition* (the broad scope loses the narrowed field, the narrow
scope keeps only it) instead of a *strict subset* (the narrow scope is a
subset of the broad scope; both can include the shared field).

**Real example.** M95 spec said "`--type title`: search milestone titles
only (narrow filter)". The implementer read this as a partition: removed
title from `--type milestone` so it only searched intent.outcome +
problem.description. The user-visible result: `mp search "Markdown"
--type milestone` on a milestone titled "Markdown rendering robustness"
returned *no* hits, because Markdown was only in the title. The M53
behavior — and the natural reading of "narrow filter" — is that
`--type title` is a *subset* of `--type milestone`, not a partition of
it. F-11 (medium) caught this; the fix was to put title (and the missing
scope lines per the spec's "milestone (intent, problem, scope lines)"
list) back into `--type milestone`.

**How to find it.** When the spec introduces a new scope labeled "narrow"
or "broader", ask explicitly: "is this a subset, a partition, or a
replacement?" Default to subset unless the spec is unambiguous. Walk the
field list before and after the change; the field count for the broader
scope should not decrease unless explicitly removed.

### L22. Spec lists field groups — wire every one

**What it looks like.** The spec describes a scope by listing field
groups that should be searched: `milestone (intent, problem, scope
lines)`. The implementation searches two of the three; the third is
overlooked because the spec doesn't itemize which fields are *inside*
each group.

**Real example.** M95 spec said `--type milestone` matches "intent,
problem, scope lines". The implementation searched `intent.outcome` and
`problem.description` but skipped `scope.in_scope` and
`scope.out_of_scope`. F-11 caught this. A `mp search "Print" --type
milestone` query (where "Print" was in `out_of_scope`) returned no hits,
even though `Print` was explicitly in the plan as out of scope.

**How to find it.** When the spec uses a parenthesized list like
"`(intent, problem, scope lines)`", treat each item as a search field.
For structured fields (lists, objects), expand them into searchable
strings before running the fuzzy scorer. Add an integration test for
each listed item: search a term that lives only in that item and assert
the hit comes back with the expected `matched_field`.

### L23. Symmetry between flags — validate every enum the same way

**What it looks like.** One CLI flag validates against a known set and
errors clearly on unknown values; a sibling flag silently returns empty
results on unknown values. Users can't tell the difference between "no
matches" and "typo".

**Real example.** M95's `--include` rejects unknown values with a clear
error ("invalid --include value: OBJECT (expected: snippet or object)").
But `--type` silently returned `{ results: [] }` for `--type foo` —
indistinguishable from a real "no matches" query. F-13 fixed this by
adding the same kind of validation to `--type` that `--include` already
had.

**How to find it.** List every CLI flag that takes a value from a known
set (`--type`, `--include`, `--group-by`, `--format`, etc.). For each
one, write a test that passes an invalid value and asserts an error
message naming the valid options. The behavior should be uniform: every
flag with a closed enum errors loudly on bad input.

### L24. Refactors surface latent bugs in adjacent code

**What it looks like.** A refactor narrows the scope of a query or
narrows what fields a function reads. The narrower scope makes a
pre-existing false-positive bug *visible* in tests or empirical runs
even though the bug was there before.

**Real example.** M95 narrowed `--type milestone` to only search
`intent.outcome` and `problem.description`. That narrowed scope made
`fuzzy_match`'s tier-2 false positive visible: a search for "Markdown"
on text containing "Markdown" *only* as a scope line would have
returned a tier-2 partial-match hit on `problem.description` (e.g. with
2 of 8 chars matched, clamped to score 0.4). The same bug existed in
M53 but was masked by the higher-scoring title match. The M95 refactor
exposed it.

**How to find it.** After a refactor that narrows the surface area of
something (a query, a filter, a scope), run a few empirical searches
against fixtures and check the scores. If a top-scoring hit suddenly
disappears or a low-scoring hit suddenly dominates, the narrowed scope
is colliding with a fuzzy-match quirk — tighten the scorer or expand the
search fields to compensate.

### L25. Silent empty results mask typos — always error on invalid enums

**What it looks like.** A CLI accepts an enum value (`--type`, `--kind`,
`--mode`, …). When the value is invalid, the command exits 0 with empty
results, indistinguishable from a real "no matches" query. Users can't
tell a typo from a legitimate miss.

**Real example.** M95's `--type foo` returned `{ results: [] }` with
exit 0. Same for `--type all` — but `all` was meant to be a synonym for
"no filter" and should have worked. F-12 and F-13 fixed both: `--type
all` normalizes to None; unknown types error clearly with a list of
valid options.

**How to find it.** For every flag with a closed enum value, add a
negative test that asserts the command *fails* with a non-zero exit and
a stderr message naming the valid options. The cost is one test per
flag; the win is that typos surface immediately instead of silently
returning "no results found" for queries that should have hit
something.

### L27. Narrow ⊆ broad test invariant

**What it looks like.** Two scopes (`--type narrow` and `--type broad`)
share a field. There's no test asserting that every result of the
narrow scope also appears under the broad scope. The narrow scope
silently drifts out of sync.

**Real example.** M95's `--type title` was supposed to be a subset of
`--type milestone`. After the M95 refactor, `--type title` returned
title hits but `--type milestone` did not — because the refactor
removed title from `--type milestone` instead of keeping it. F-11.
A test like "for every `--type title` hit, the same query with
`--type milestone` also returns a hit" would have failed immediately.

**How to find it.** When introducing a narrow scope next to a broad
scope, add a property test that asserts the subset relation: every
hit in the narrow scope must also appear under the broad scope (with
the same id and matched_field). This invariant breaks loudly when
someone later partitions the broad scope instead of subsetting it.

---

## CLI ergonomics

### L29. Sentinel-prefixed flags that collide with legitimate content

**What it looks like.** A flag accepts a string value. The implementation
gives the value a special meaning when it starts with a sentinel prefix
(`@`, `=`, `-`, `~`). The spec lists the sentinel form as the happy path
(`--body @file.txt`, `--output =stdout`) but doesn't consider values that
legitimately start with that prefix (`@username`, `=heading`, `-5`
negative number). There's no escape. The user gets a confusing error
("could not read body file: username ping") or silent misbehavior.

**Real example.** M97's `mp note add --body @<path>` (note.rs `resolve_body`)
treats any `--body` starting with `@` as a file/stdin path. A legitimate
body whose text starts with `@` — `@username ping`, `@channel fyi`, a
markdown `@decorator` — is reinterpreted as a file read and fails. There
is no way to make a body that starts with `@`. Reproducer: `mp note add
--title x --body "@username ping"` exits 1 with "could not read body
file: username ping". F-03. The spec (AC-07) said `--body @file.txt` and
the implementer followed it exactly — the collision wasn't in the spec.

**How to find it.** For every string-valued flag, list the prefixes the
implementation treats specially. For each prefix, ask: "can a legitimate
value start with this?" If yes, there must be an escape or a separate
flag. The conventional escape (jq `--rawfile`, git `--output=<file>`,
kubectl `-f`) is a **separate flag for the special form**
(`--body-file <path>`) and the original flag (`--body`) stays literal.
Keep the sentinel form for backward-compat if you must, but document the
collision and provide the unambiguous path. A test should assert that a
value starting with the sentinel round-trips as literal content.

### L30. Path arguments that skip shell conventions (tilde, env)

**What it looks like.** Code accepts a path argument and passes it to
`std::path::Path::new` or `std::fs::read` directly. The user writes a
natural path (`@~/notes.md`, `$HOME/notes.md`) and it fails, because Rust
std deliberately doesn't expand `~` or `$VAR`. The error message echoes
the unexpanded path, which looks valid to the user.

**Real example.** M97's `mp note add --body @~/notes.md` (quoted, so the
shell doesn't expand `~`) failed with "could not read body file:
~/notes.md" — confusing because the same string works unquoted on the
shell. F-04. Absolute paths and shell-expanded `~` both worked; only the
quoted-tilde case broke.

**How to find it.** For every path argument, try it with a leading `~/`
(passed quoted, so the shell doesn't expand it) and with a `$HOME`
reference. If it fails, the error message should either (a) hint that
`~`/env vars aren't expanded, or (b) expand them manually (`dirs::home_dir()`
for `~`, `std::env::var` for `$VAR`). Low blast radius for local-only
CLIs, but the error message must not echo the unexpanded path as if it
were the literal filename the user intended.

---

When running a review:

1. Read this file first to load the patterns into context.
2. For each new feature, walk lessons L1–L53 as a checklist. L43–L53 are the lessons learned from the M101/M102/M103/M110 review remediations.
3. When you find a bug, file it. Don't fix it in the same pass — fix it
   in a remediation pass so the audit trail is clean.
4. When you find a new pattern not in this file, add it as a new lesson
   entry. Use the structure: name, what it looks like, real example,
   takeaway.

The goal is that every entry in this file is *earned* — it represents a
real bug a real reviewer caught in a real review. If a lesson doesn't have
a real example, it doesn't belong here yet.

---

### L31. Review carry-overs belong in tracked files, not just in commit messages

**What it looks like.** A reviewer (or a self-reviewing agent) catches
non-blocking findings during review: missed edge cases, future cleanup,
deferred decisions. They document them in the commit message: *"F-2
noted but accepted; F-3 retag caveat explained in commit body."* Then
the next person picking up the work has to scrape `git log --grep=` to
reconstruct what was learned.

**Real example.** The joint code review of M91+M81+M96 produced 8
follow-up items (R-1 through R-8). They were initially only in the
commit message of `1b59235` and in REVIEW-NOTES-M91-M81-M96.md (which
was created only after the user asked explicitly "have you added a
note"). R-1 in particular — *"the v2.0.0 tag was created at the wrong
commit; reviewer should confirm the retag"* — was a yes/no decision
the next reviewer needed to see immediately, not buried in git log.

**Takeaway.** Review carry-overs belong in a durable file in the repo,
co-located with the milestone they touch. Convention used here: a
`<MILESTONE-id>-REVIEW-NOTES.md` file at the repo root for each
milestone that needs handoff context. It is the durable record;
commit messages are convenience. The external reviewer should read
`<MILESTONE-id>-REVIEW-NOTES.md` *before* `git log` on the milestone.

---

### L32. Schema fields the CLI doesn't expose persist as hidden state

**What it looks like.** A milestone JSON file has a `follow_ups` (or
equivalent) array in its schema, but the `mp show milestone --fields`
CLI rejects it as *"unknown path"*. The data round-trips through the
plan file correctly, but no future agent can read it back through `mp`.
A reviewer or follow-up worker has to drop to a `python3 -c "import
json; ..."` parse, or `cat master-plan/milestones/<id>.json` and
eyeball.

**Real example.** M96's milestone file's `follow_ups` array exists in
the JSON schema (the schema-validate side accepts it). Trying to
`mp show milestone M96 --fields 'follow_ups'` returned *"unknown path"*
during the joint code review. The agent fell back to direct JSON
parsing to surface follow-up items — bypassing the contract that
"`mp` reads/writes all plan data." The pattern has the same risk as
"L22 — Spec lists field groups; wire every one" but at the
post-ship CLI surface rather than at the implementation surface.

**How to find it.** For every JSON field in the milestone schema
(`accept_criteria`, `steps`, `work_packages`, `intent`, `problem`,
`scope`, `verification`, `design_decisions`, `follow_ups`, etc.),
verify that `mp show milestone <id> --fields '<field>'` returns the
data. Where the schema has it but the CLI rejects it, either (a) wire
the field through `--fields` as a default-rejected path so the agent
can opt in, or (b) add a sibling read command (`mp milestone
follow-ups <id>`) so the data is reachable. The agent should never
have to read the JSON file directly to learn about a milestone.

**Takeaway.** The CLI surface is the contract for what an agent can
*see*. If a field is in the schema but not on the CLI, treat it as
a CLI coverage gap and either close the CLI surface or drop the field
from the schema. Do not let schema fields exist as write-only slots.

---

### L33. Tag-pointer drift at release time

**What it looks like.** A project has a release-cut ritual — version
bump + tag + "release ship" registry update — and the ritual's
individual steps land in separate commits. The tag is created at the
version-bump commit; the registry-finalization commit lands later. A
cloning user who checks out the tag gets shipped-version code but
un-shipped registry state. The drift only surfaces after self-review
or after a user runs `git show <tag>` and sees the tag pointing at
the wrong commit.

**Real example.** During the M96 2.0 GA cut, the v2.0.0 annotated tag
was created at `b8adcb4` (which set `Cargo.toml` version = 2.0.0 and
edited CHANGELOG / brief / plan.json). The registry finalization
(`mp release ship 2.0.0`) was committed separately two commits later
in `a88ace9`. Self-review caught the drift; the agent retagged
locally at `a88ace9`. The tag was not yet pushed, so the retag was
safe — but for an already-pushed tag the recovery is force-push +
re-cut, which is a process risk.

**How to find it.** For every release-cut ritual, identify the
**final** commit that makes the release "complete" (registry marked
shipped, release notes generated, change log signed off, etc.) and
*delay the tag* until that commit exists. If the tag has to be cut
before the registry step (e.g., to trigger CI), document the
expected gap and re-tag or re-cut as a follow-up. A pre-release
checklist item ("tag points at the commit that completes every
release-cut step") is more reliable than a code-review catch.

**Takeaway.** Tag-pointer drift is a class of bug specific to
multi-step release rituals. Treat the tag as the *last* artifact
the release produces, not the first. Add a check to the release
process: `git show <tag> --no-patch --format=%s` should mention
"complete" / "shipped" / "registry" in the commit subject of
whatever the tag points at. If it doesn't, retag.

---

### L34. A collation fix at one site doesn't fix its siblings

**What it looks like.** A bug surfaces in how items are ordered —
lexicographic vs numeric, ASCII vs natural, ascending vs descending.
The fix patches the one site the reproducer exercised. Other sort
sites across the codebase that order the *same kind of id* keep the
old comparator and silently keep the bug.

**Real example.** The plan crossed 100 milestones, and `mp list
milestones` began sorting `100` between `10` and `11` (string sort:
`"100" < "95"` because `'1' < '9'`). The fix (commit `db63821`)
patched `commands/list.rs` (milestones list) and `sync.rs` to use
`paths::compare_milestone_ids`. But eight sibling sort sites still
ordered milestone ids with `.cmp()` on the string:
`commands/list.rs:203` (`mp list steps`), `reviews.rs:112/148/149`
(pending reviews + `suggested_next`), and `plan_diff.rs:170/176/345/
406/447` (handoff baseline + diff). Reproduced on master: `mp list
steps` emitted `100,101,102,103,95,96,97`. Filed as B-42. The root
cause was upstream of all of them — `store::list_milestone_paths`
and `list_archived_milestones` sort filenames with `paths.sort()`
(lexicographic); every consumer that doesn't re-sort numerically
inherits the bug.

**How to find it.** When a sort-order bug is fixed at one site,
grep for *every* sort/comparator over the same id type before
considering the bug closed. Specifically: search for `.sort()`,
`.sort_by`, `.sort_by_key`, `.cmp(` over the offending field name
across the whole codebase, not just the file the bug was filed
against. Classify each hit: does it order milestone/track/step ids
(or whatever the bug class is), or something else? Domain ids,
challenge ids (`F-XX`), annotation ids (`AN-XX`) often share a
comparator by accident but have different collation needs — verify
each. The durable fix is usually *not* at each consumer: push the
correct collation into the source (the function that builds the
list), so consumers can't forget to re-sort.

**Takeaway.** A collation bug is almost never local to the reported
site. Treat the reproducer as one instance of a class; audit every
comparator over the same id space. The lesson is the inverse of L9
(new code inheriting an old helper's bug) — here a *fix* inherits
the old helper's siblings. File the audit (buggy sites AND verified-
clean sites) so the next pass doesn't re-walk the same ground.

---

### L35. A serde-default sentinel collides with a real "empty" value

**What it looks like.** You add a new field with
`#[serde(default = "default_foo")]`. `default_foo()` returns the
empty/false value of the type — `"draft"`, `""`, `0`. After
deserialization, the field is *never empty* — serde fills it in.
Your "is the field set?" check (`if !field.is_empty()`) then never
fires for legacy data that lacks the field, because legacy data
also deserializes to the default value, which is the sentinel you
were using to distinguish "not set" from "set to empty".

**Real example.** M100 added a `lifecycle: String` field with
`#[serde(default = "default_lifecycle")]` returning `"draft"`. The
intended `effective_lifecycle` helper was:

```rust
if !meta.lifecycle.is_empty() {
    return meta.lifecycle.clone();   // never reached!
}
// fall through to derive-from-legacy
```

The check `!is_empty()` returned false for *every* legacy milestone
(because the default filled in `"draft"`) and *every* migrated
milestone (because lifecycle is now populated to a real value like
`"approved"`). The legacy-derive branch never executed, and the
agent caught it only by debugging a stale-can_handoff test:

> `DEBUG execution_check: M02 lc=draft steps=2 blocked=false deps=["01"]`

— every milestone was being reported as draft regardless of its
`spec_status`/`execution_status`. The fix: distinguish the sentinel
*as the sentinel value*, not as "empty":

```rust
if !meta.lifecycle.is_empty() && meta.lifecycle != "draft" {
    return meta.lifecycle.clone();   // real value, trust it
}
if /* legacy fields populated */ {
    return /* derived value */;
}
return meta.lifecycle;  // real "draft" milestone
```

The check changed from `is_empty()` to `!= sentinel`, and the
default value (`"draft"`) became load-bearing instead of just a
default. Five new unit tests pinned this behavior, including a
fixture-mirroring test (`debug_m02_lifecycle_matches_fixture`) that
catches the regression where the helper returns `"draft"` for a
milestone with `spec_status=ready` + `execution_status=in-progress`.

**Takeaway.** `#[serde(default)]` is not just a back-compat shim —
it changes the *post-deserialization invariant* of the field.
Whenever you add a new field with a sentinel-valued default and
intend to use the field as a presence-check, write at least one test
that exercises the legacy-shape case explicitly. The
`is_empty()` check is the wrong tool for a defaulted field;
`!= sentinel` is the right one. If the type's natural "absent"
value (`""`, `0`, `false`) is the same as the default, the default
*is* the sentinel — treat it that way.

### L36. Layering a derived view over a removed field breaks show-parity

**What it looks like.** You remove a field from the on-disk shape
(spec says "drop `spec_status`/`execution_status`") and add a
backward-compatibility layer that derives the legacy view from the
new field at JSON-emission time. The strict show-parity test
(`mp show default JSON == on-disk JSON, key set equal`) fails
because the emitted JSON now has *extra* keys the on-disk file
doesn't.

**Real example.** M100's `inject_legacy_status_view` in
`commands/show.rs` adds `spec_status` and `execution_status` to the
emitted milestone JSON (derived from `lifecycle`). The on-disk file
doesn't carry these fields anymore (serde skips them when empty).
The `show_parity.rs` test asserted `disk_keys == shown_keys` (top-
level keys), which still passed, but a second assertion
`shown.milestone == disk.milestone` failed because the milestone
sub-object had extra keys.

The test had to be relaxed to strip the injected keys before
compare. This is a load-bearing acknowledgment: **the parity
contract is suspended for the migration window**, and the
relaxation becomes a no-op once bulk migration clears the legacy
fields from disk.

**Takeaway.** Strict-shape tests are *load-bearing*: they catch
unintended drift between the persisted and the emitted form. When
you break that contract on purpose (derivation layers, migration
windows), the relaxation must (a) name the migration window
explicitly in a comment, (b) gate itself so it becomes a no-op once
the migration completes, and (c) be added to the "things to remove
after bulk migration" follow-up list. Otherwise the relaxation
becomes a permanent exemption and the test stops catching future
drift.

### L37. Tooling-bridged tests fail when the installed binary is stale

**What it looks like.** A test passes locally, fails in CI, fails
again on the developer's machine a week later. The error message
is a serde "missing field X at line 24 column 3" inside a
subprocess of the test harness — not a clear "stale binary"
hint.

**Real example.** M100 changed the on-disk milestone schema. The
`raul` test harness (`crates/raul/tests/explain_impact.rs`) shells
out to `mp` via `Command::new(mp_bin())`. `mp_bin()` resolves mp
from `MP_HOME/bin/mp` if set, falling back to PATH. The global
install (`~/.agents/master-plan/bin/mp`) was last `make install`-ed
before M100. When the test ran with `MP_HOME` set (the default in
the developer's shell), it invoked the *installed* mp, which
required `spec_status` and failed to deserialize the *new* on-disk
shape. The fix was running the test with `MP_HOME=` (empty) so
raul falls through to PATH and finds the dev `target/debug/mp`.

**How to find it.** When a test fails with a serde "missing field"
error that originated from a subprocess, check whether the subprocess
binary is from the same build as the test runner. Specifically:

- Does the test rely on a globally-installed binary (via `MP_HOME`,
  `PATH`, or a config file pointing to a system path)?
- Is the dev binary in `target/debug` newer than the installed one?
- Does `git diff --stat HEAD -- <installed-binary-source-dir>` show
  unbuilt changes?

If yes, the test is silently running against the wrong version.

**Takeaway.** Test harnesses that bridge to a binary need a way to
pin to the in-tree version. Options: (a) document `MP_HOME=` (or
equivalent) in `AGENTS.md` and CI, (b) extend `find_mp()` to prefer
`CARGO_BIN_EXE_<bin>` when set, (c) add a CI smoke test that asserts
the dev binary is built within N commits of the source tree. This
particular class of bug is silent in code review — it only surfaces
when a developer with a stale install runs the test suite locally.

**L37 addendum — PATH-installed binaries are stale even with
MP_HOME unset.** L37 covered the case where `MP_HOME/bin/mp` is
older than the dev tree. The M100-M103 external review (F-3)
added a second case: even with `MP_HOME=` unset, the developer's
shell often has `~/.agents/master-plan/bin` on PATH from
`make install`. If that install predates the current dev tree,
raul's `find_mp` falls through from `MP_HOME` (unset) to PATH
(installed, stale) and resolves the stale binary. The test
passes locally (dev mp matches) but raul shell-out uses the
stale mp. The fix has two halves: (a) extend `find_mp()` to
prefer a sibling `target/{release,debug}/mp` next to the
running raul binary before falling through to PATH — check
at the *binary location*, not the env var; (b) when documenting
`MP_HOME=` as the workaround, also say "and verify `which mp`
resolves the dev binary, not the global install." The structural
fix in (a) supersedes the workaround; (b) is a stopgap. The
audit: when a binary-lookup function falls through multiple
sources (env var → installed dir → PATH), each source is a
candidate for staleness. Document the precedence order and the
fallback policy. The cheapest test: after a rebuild, does the
binary actually run the new code? `raul --version` from each
candidate, or a hash compare against the source tree.

### L38. Four-milestone scope-down is honest, but flag it loudly

**What it looks like.** A user asks for four milestones of work in
one session. Each milestone is ~1-2 days of focused implementation
plus review. The agent ships a foundational slice of each
(model + schema + 1-2 helpers + tests) and tags every commit
"partial: ...". The work is internally consistent and the test suite
stays green. But the partial scope is a real risk: a reviewer who
expects full AC coverage will be surprised, and the follow-up work
will look like "oh, the agent didn't finish."

**Real example.** M100/M101/M102/M103 each had ~9 steps and ~25
acceptance criteria collectively. The agent shipped roughly 30-50%
of each milestone's AC list (foundational slice) and tagged every
commit `M### (partial): ...`. The review notes file
(`REVIEW-NOTES-M100-M101-M102-M103.md`) documented the deferred
work per-milestone with one-line entries per AC, plus a TL;DR
explicitly stating "the work is self-consistent ... but the full AC
coverage is partial".

**Takeaway.** Scope-down is a legitimate agent choice when (a) the
work product is internally coherent, (b) the test suite is green
throughout, (c) the deferred items are well-scoped (small, testable
additions), and (d) the review notes file calls out the scope-down
explicitly with a per-AC checklist. Without (d), the reviewer reads
the commit messages and the closed-out milestone list and concludes
"shipped" when in fact 60% of the work is outstanding. The
`REVIEW-NOTES-*` template is the durable artifact: file it with
the same discipline as the implementation. The notes are not a
post-mortem; they are the handoff.


---

### L49. Extract the formatter as a pure function so unit tests don't depend on dev-binary-vs-installed-mp coupling

**What it looks like.** A CLI function reads JSON via an `MpRunner`
(which shells out to `mp` on PATH/sibling), then formats the JSON
to stdout with `println!`. Tests against this function either (a)
shell out to `raul` and grep stdout (fragile, depends on dev mp
binary being reachable), or (b) refactor the function to take a
mock runner (heavy, requires a new abstraction).

**Real example.** M103's `commands::path::path_lanes(runner,
filter)` was the canonical example. It called `reads::path_lanes`
or `reads::path_lane` (which shell out to `mp`), normalized the
response, and called `println!` for the swimlane. Tests against
it could only drive it via the `tests/path.rs` pattern
(`Command::new("raul").args(["path", "--all"])`), which depends on
`mp` being reachable via the sibling/MP_HOME/PATH fallback chain.
That coupling is fragile: on a fresh checkout without `make
install`, the tests skip; on a system where the installed `mp`
predates the `--all` / `--lane` flags, the tests fail with
"unexpected argument"; on a build with no sibling `mp` next to
the test binary, `find_mp` falls through and the test depends on
PATH. None of these break in a developer loop; all of them break
in CI.

**The fix.** Extract the formatter as a pure function:

```rust
pub fn format_path_lanes(data: &serde_json::Value) -> String { ... }

pub fn path_lanes(runner: &MpRunner, filter: Option<&str>) -> Result<()> {
    let data = read_via_runner(runner, filter)?;
    print!("{}", format_path_lanes(&data));
    Ok(())
}
```

Now unit tests drive `format_path_lanes` against frozen JSON
fixtures (`tests/contract/*.json`) with no `MpRunner`, no `mp`
binary, no shell. The CLI entry is a thin wrapper that becomes
trivial to test as a "did we shell out and print?" smoke test
(one integration test, not four).

**How to find it.** Any function in `crates/raul/src/commands/`
that takes an `MpRunner` and writes to stdout is a candidate. The
fix is mechanical: split it into a pure `format_*(data) -> String`
and a thin wrapper that calls the runner and `print!`s the
result. The pure function is unit-testable with frozen fixtures;
the wrapper gets one integration test that confirms the wire
end-to-end (commands/path.rs → reads/path_lanes → mp binary →
JSON → commands/path.rs → stdout).

**Takeaway.** A CLI function is a *formatter* + a *side-effecting
shell*. The shell is small (read JSON, print string); the
formatter is the logic. Test the formatter as a unit; smoke-test
the shell as an integration. The shell's fragility (depends on
the dev mp binary being installed) is contained to one test; the
formatter's complexity (the lane rendering rules, the M-prefix
logic, the empty indicator) is unit-tested cheaply. The
cost-shaping is roughly: one refactor (extract pure function)
saves four integration tests' worth of flakiness.

### L50. Frozen-contract pin = hand-crafted JSON fixture + typed deserialize struct

**What it looks like.** A CLI consumes JSON from another
process (`mp` here) and indexes fields with `.as_str().unwrap_or("?")`
or `.as_u64().unwrap_or(0)`. If the producer renames a field or
nests it differently, the consumer doesn't panic — it silently
prints `?` and `0` across the entire output. The bug surfaces at
the user hands, not in the consumer's test suite.

**Real example.** M103 ER-4 surfaced this. `reads::path_lanes` /
`path_lane` shell out to `mp path --all` / `--lane <name>` and
hand the raw `serde_json::Value` to `commands::path::path_lanes`,
which indexes with `data["lanes"]`, `lane["name"]`,
`item["milestone"]["id"]`, etc. F-05 had been filed for a
frozen-contract pin (`tests/contract/path_lanes_schema.json`)
and closed `fixed` against a plan-only commit; the fixture
didn't exist. The fix:

1. Hand-craft a stable JSON fixture at
   `crates/raul/tests/contract/path_lanes_schema.json` covering
   the all-lanes envelope, `path_lane_single_schema.json` for
   the single-lane response, and `path_lane_empty_schema.json`
   for the empty case.
2. Add typed `#[derive(Deserialize)]` structs (`PathLanes`,
   `Lane`, `PathAction`, `PathActionMilestone`, `LaneItemType`
   enum) in `reads.rs` that mirror the JSON shape exactly.
3. Add a deserialize test that loads each fixture and
   `serde_json::from_value::<PathLanes>(...)` — if M102 renames
   a field, the deserialize fails and the build breaks before
   the user sees a misrender.

The fixture is *hand-crafted*, not a snapshot of live `mp path
--all` output. A snapshot fixture would drift every time M102
changes (even good changes); a hand-crafted fixture is a
*contract*: it's the shape the consumer expects, written in
collaboration with the producer. Renames the consumer didn't
expect → deserialize fails. Renames the consumer expected →
bump the fixture as part of the M102 change.

**How to find it.** For every JSON field a CLI consumes from a
shell-out (`mp` or otherwise), ask: "if the producer renames
this, where does the failure surface?" If the answer is "the
user sees `?` and `0`", add a typed deserialize + frozen
fixture. The cost is ~30 lines of struct + one JSON file per
shape variant; the win is a build-time failure on producer
drift.

**Takeaway.** A wire-format consumer has a contract with the
producer. The contract is implicit (the JSON shape) until you
make it explicit (a typed struct + a frozen fixture). The
explicit form is testable, version-controllable, and grep-able
(`.json` files in `tests/contract/` are searchable; raw `Value`
indexing is not). The producer-side schema-validate (if any) is
a *separate* contract — it pins the producer's output against
its own schema, not against the consumer's expectations. The
consumer-side pin is the only thing that catches a producer
change the consumer didn't expect.

### L51. AC integrity under deferred scope: re-scope ACs to match what shipped; never mark `passed` with copy-pasted evidence across multiple ACs

**What it looks like.** A milestone ships with the implementation
half-done (e.g. CLI but not TUI). The author writes 7 ACs covering
both halves, marks all 7 `passed` with the same evidence string
that describes only the CLI. Findings F-01..F-05 ("TUI not
implemented") are filed against the milestone but closed
`fixed` against a plan-only commit. The milestone record reads
`7/7 ACs passed, 5/5 findings fixed, lifecycle=done`. The next
agent picking up the work has no signal that the TUI half is
outstanding.

**Real example.** M103 shipped exactly this way. AC-01..AC-03
were TUI-worded ("raul TUI path tab renders 4 lanes as
swimlanes"); AC-04/06/07 were CLI-worded. The author marked all
7 `passed` with copy-pasted CLI evidence. The external reviewer
(`docs/code-review/M103-external-review.md` ER-1) caught it:
"the AC evidence strings should not be identical across ACs".
The fix has two parts:

1. **Re-scope the ACs to match the shipped surface.** AC-01/02/03/05
   became CLI-equivalent. The TUI ACs moved to a new milestone
   (M126). Each AC now has unique per-AC evidence (not
   copy-pasted).
2. **File the deferred half as a follow-up milestone.** M126
   carries 7 ACs covering `path_view.rs`, keybindings, badges,
   narrow-width audit, shared palette, and closure. The
   closed F-01/F-04 stay closed as the historical deferral
   record; the open F-12/F-13/F-14 are the active tracking,
   with `fixed_in: deferred:M126` as the resolution reference.

The fix's invariant: **the milestone record's AC verdicts and
finding closures must agree with the diff in `crates/`.** If a
finding describes unimplemented work, it cannot be `fixed`
against a commit that has no `crates/` changes. The reviewer
catches this by running `git show <commit> -- crates/` and
asking "where's the code that fixes this?" If the answer is
"there isn't any", the closure is dishonest.

**How to find it.** For every `status: fixed` finding on a
`lifecycle: done` milestone, run `git show <fixed_in> -- crates/`
and check the diff. If the diff is empty (or unrelated to the
finding's description), the closure is dishonest — the fix is
either (a) deferred to a follow-up (which is legitimate but
should be `deferred:M###`, not `fixed`), or (b) a closure-by-
relabeling without substance (which is the bug). Pair this with
the AC evidence uniqueness check: if two ACs have byte-identical
evidence strings, at least one of them is lying about its
verifier run.

**Takeaway.** ACs and findings are a *contract* between the
milestone's stated scope and the actual diff. When the scope
shifts mid-milestone (scope-down, deferral, partial ship), the
contract must be re-papered: re-scope the ACs, file the deferred
half as a follow-up, and update the evidence strings. The
alternative — copy-paste evidence, plan-only `fixed_in`,
`--force` completions — produces a milestone record that lies
about its own state. The external reviewer catches it (L1, L19);
the dogfood catches it (this entry); the next agent on the
follow-up milestone inherits a contradictory record. Re-scope
or defer honestly, never both.

### L52. When a probe tightens, every test fixture that drives the probe must be updated to match the new strictness

**What it looks like.** A probe (`is_file()`, `is_dir()`,
`matches!()` against a regex, an equality check against a
closed enum) is updated to a stricter version. The pre-existing
test fixture was honest against the old probe but is dishonest
against the new one. The test starts failing; the fix is
mechanical (`chmod +x` the fake file, add the missing field to
the fixture, normalize the whitespace) but the *pattern* is
non-obvious: a green test under the old probe is not a green
test under the new one.

**Real example.** M103 ER-6 fix changed `find_mp_from`'s probe
from `sibling.is_file()` to `is_executable_file(sibling)`
(which checks `mode & 0o111 != 0` on unix). The pre-existing
test `find_mp_prefers_sibling_over_path` (in
`crates/raul/src/mp_runner.rs:284`) created fake binaries via
`OpenOptions::create + write_all(b"#!/bin/sh\n")` but never
called `set_mode(0o755)`. Pre-fix, the test was green (the
fake `mp` satisfied `is_file()`). Post-fix, the test failed
with `unwrap on NotFound` from `canonicalize` on the resolved
path — because `find_mp_from` correctly skipped the
non-executable fake, fell through to PATH, and returned the
installed `mp`, which doesn't exist in the temp dir's
namespace.

The closed loop (L43) was: the test fixture wrote the same
loose probe the production reader used. Both errors cancelled.
When the production reader tightened, the test fixture had to
follow. Fix: add `Permissions::set_mode(0o755)` after creating
each fake binary; the test now exercises the new strictness.
A new test `find_mp_skips_non_executable_sibling` was added to
pin the new behavior.

**How to find it.** When tightening a probe (executable bit,
non-empty check, regex match, enum membership), grep for every
test that *creates the artifact* the probe inspects. If the
test's setup step doesn't pass the new strictness, the test
silently weakens (or starts failing). The fix is mechanical:
update the setup step to produce an artifact that passes the
new probe. The new test should also pin the *rejection* path
(here: a non-executable sibling is not returned).

**Takeaway.** A test's setup step is part of the test. If the
production probe tightens and the test setup doesn't, the test
is now testing something different from what it claims. The
test still goes green (the old behavior is now stricter, so
the artifact still passes), but the test no longer exercises
the new probe's behavior on the boundary case. Pin the
boundary: a probe that accepts X must have a test that asserts
acceptance; a probe that rejects not-X must have a test that
asserts rejection. When the probe changes, both halves of the
test must follow.


### L53. A milestone that ships a lint must lint its own spec — and re-call to `mp milestone complete` overwrites per-AC evidence

**What it looks like.** A milestone ships a new lint (e.g. a
broad-scope AC verification checker). The author's own AC
verifications are written without consulting the lint, so they
trip the lint they just shipped. The lint output surfaces the
violation; the milestone is marked `done` (or `approved`) with
the violation still on disk. The producer's test suite is green
(because the lint is `WARN`-only, exits 0); the next agent
running `mp plan verify-lint` discovers the collision.

A second, related issue: agents who update per-AC evidence
via `mp milestone criterion pass --evidence "..."` then
re-call `mp milestone complete --evidence "..."` to set the
closure summary — and the closure call **overwrites all
per-AC evidence fields** with the closure string. The
audit trail collapses: 4 unique per-AC evidence strings
become 1 byte-identical copy-paste. L51 ("never mark AC
`passed` with copy-pasted evidence across multiple ACs")
applies, and the agent didn't notice.

**Real example.** M110 (hygiene sweep v2) shipped
`crates/mp/src/commands/plan_verify_lint.rs` (a Rust port of
the broad-scope checker). The milestone's own AC-04 had the
verification `cargo test -p mp -p raul && mp validate
--summary` — but M110's affected-crate set is only `{"mp"}`
(the gate caching, the verify-lint port, and the portability
patterns all live in `crates/mp/`, not `raul/`). The lint
correctly flagged AC-04 with `pattern=multi-crate -p
(affected={"mp"}, mentioned={"mp", "raul"})`. The same lint
flagged `steps[S5].tests` for the same string. The M110
milestone shipped with 2 hits against its own spec — visible
to anyone running `mp plan verify-lint` but not blocking the
work because the lint is soft-WARN.

The second issue manifested during the remediation: I called
`mp milestone criterion pass 110 AC-01..AC-04 --evidence
"<unique per-AC text>"`, then `mp milestone complete 110
--evidence "<closure summary>"`. The closure call
**overwrote** all four ACs' evidence with the closure
string. Re-checking the milestone showed all 4 ACs with
byte-identical evidence — the L51 pattern, freshly
re-created by the fix.

**How to find it.** For every milestone that ships a lint /
gate / contract-pin: run the lint against the milestone's
own spec BEFORE marking it `done`. If the lint flags the
spec, the spec violates its own contract. The fix is
mechanical: narrow the verification strings to match the
lint's notion of scope, or widen the lint's notion of scope.
Don't ship a milestone whose spec violates its own rule.

For the evidence-overwrite issue: after `criterion pass
--evidence`, never re-call `mp milestone complete
--evidence`. The closure call collapses all per-AC evidence
to the closure string. The correct flow is: (a) update
per-AC evidence via `criterion pass`, (b) call `mp milestone
complete` once at the very end with a SHORT closure summary
that's actually a pointer ("see per-AC evidence for details;
4/4 ACs verified; mp validate exits 0"), (c) re-mark any AC
whose evidence was overwritten. The closure string is meant
to be the milestone-level "this is what happened" pointer;
the per-AC evidence is the audit trail. Don't conflate them.

**Takeaway.** A milestone is a *producer* (it ships the
lint) AND a *consumer* (its ACs must pass the lint). The
producer side is green if the tests pass; the consumer side
is green if the lint accepts the milestone's own spec.
Ship-side green ≠ consume-side green. Run the lint against
the spec; if the lint flags the spec, the milestone is not
done. The audit-overwrite side: treat per-AC evidence as
write-once — once it's been set via `criterion pass`, don't
trigger another bulk-rewrite via `mp milestone complete`.
The `mp milestone complete --evidence` flag is a footgun:
every call to it with a non-empty string overwrites the
audit trail. If the per-AC evidence was already set, the
closure call should pass `--evidence ""` (or just not pass
the flag) and let the system compute its own closure
summary from the existing per-AC evidence.

### L54. A high warning count is data, not noise — verify the lint's contract before declaring findings "false positives"

**What it looks like.** A reviewer runs the lint a milestone
ships against the repo's own plan and sees a large number of
warnings (e.g. 57). The dominant category (27 of 57) points
at a single crate (`-p mp-model not in affected set {"mp"}`).
The reviewer's instinct is to call these false positives:
"the lint doesn't know about `crates/mp-model/`, that's a bug
in the lint, ship a fix that teaches it the crate." A fix is
written that generalizes the crate-derivation. Re-running
the lint shows the warning count *unchanged*. The reviewer
now has two problems: a fix that didn't move the needle, and
a misdiagnosis that wasted a build cycle.

**Real example.** M110 external review (2026-07-06). The
reviewer saw `mp plan verify-lint` emit 57 warnings, 27 of
which were `crate -p mp-model not in affected set {"mp"}`
across M100/M101. Initial diagnosis: "`affected_crates()`
hard-codes only `crates/mp/` and `crates/raul/`; `mp-model`
is a real crate; the lint is broken; 27 of 57 warnings are
spurious." Proposed remediation: generalize the derivation
to map any `crates/<name>/` to `<name>`.

The fix compiled and its unit test passed, but the 27
warnings persisted. Why: M100/M101 *do not list*
`crates/mp-model/` in their `steps[].files[]` — their files
arrays contain only `crates/mp/...` paths. The lint derives
the affected set faithfully from `steps[].files[]` per its
documented contract; the milestones genuinely run
`cargo test -p mp-model` (9 ACs in M101 alone) without
declaring `crates/mp-model/` files anywhere. The lint was
correctly surfacing a real spec-hygiene gap; the "false
positive" call was wrong.

**How to find it.** Before declaring a lint finding spurious:
1. Read the lint's documented contract (what is it *supposed*
   to flag, and from what input does it derive its truth?).
2. Open one of the flagged milestones and check whether the
   input the lint reads (`steps[].files[]` here) actually
   contains the data that would silence the warning.
3. Only if the input *does* contain the data and the lint
   still flags it, is the finding a lint bug.

In this case the contract is "derive affected crates from
`steps[].files[]`"; the flagged milestones' `files[]` did
not contain `crates/mp-model/`; therefore the finding was
real. The generalization fix is still a worthwhile
forward-compat improvement (future crates resolve without
code changes), but it does not, and should not, silence
warnings for milestones that under-declare their files.

**Takeaway.** A high warning count from a lint you just
shipped is the lint doing its job on a corpus the author
never ran it against. Treat the count as a triage list, not
a defect report against the lint. The question is never "is
the lint wrong?" until you've confirmed "did the flagged
subject actually satisfy the lint's input contract?" Most
"false positive" calls on lints that derive from structured
fields collapse, on inspection, into real gaps in the
structured fields themselves — the spec is under-declared,
not the lint over-reaching.

### L55. `--evidence ""` is not "skip the flag" — it overwrites per-AC evidence with empty strings

**What it looks like.** An agent follows the canonical L53 close-out
recipe: (a) `mp milestone criterion pass <ID> AC-XX --evidence
"<unique per-AC text>"` for each AC, (b) `mp milestone complete <ID>
--evidence ""` "to keep the closure summary computed from per-AC
detail." The `mp show milestone <ID>` afterward shows all per-AC
`evidence` fields are empty strings. The L51 collision pattern
freshly re-created, with the per-AC strings collapsed to `""`.

**Real example.** M111 close-out (2026-07-07). The agent followed
the L53 recipe literally: criterion pass on AC-01..AC-07 with
unique per-AC text; `mp milestone complete 111 --evidence ""`
"so the closure summary is computed from per-AC detail" (per L53).
The post-complete read showed all 7 ACs with `evidence: ""`. The
source code in `crates/mp/src/milestone.rs::complete_milestone`
(around line 1652–1668) does:

```rust
let evidence_text = evidence.clone().unwrap_or_else(|| "milestone complete".to_string());
for ac in &mut m.acceptance_criteria {
    if ac.status != "passed" {
        ac.status = "passed".to_string();
        if ac.evidence.is_empty() {
            ac.evidence = evidence_text.clone();
        }
    } else if evidence.is_some() {  // <-- Some("") is still Some
        ac.evidence = evidence_text.clone();  // <-- "" overwrites
    }
}
```

`Some("")` is `Some`, so the `else if evidence.is_some()` branch
fires — every passed AC's evidence gets `""`. The "skip the flag
entirely" half of the L53 advice (omit `--evidence` entirely) is
the only safe path: `evidence` is `None`, the `else if` doesn't
fire, and `verification.evidence` falls back to the default
`"milestone complete"`. The `--evidence ""` half of L53 is the
trap; the corrected guidance is **"omit the flag, do not pass an
empty string."**

The intended overwrite behavior IS tested by
`crates/mp/tests/suites/milestone_verify.rs::complete_refreshes_evidence_on_recomplete`
(non-empty `--evidence` on re-complete refreshes per-AC evidence
to the new closure string). That's the "remediation" use case: a
re-completion with new evidence SHOULD refresh. But the
remediation use case is a different intent from "close-out with
existing per-AC detail preserved" — the source code cannot tell
them apart, so the same code path serves both. The fix: split the
two intents at the CLI layer (e.g. `--closure-summary ""` for
"keep existing per-AC", vs. `--refresh-evidence "<text>"` for
"remediation refresh") or make the source check
`!evidence_text.is_empty()` instead of `evidence.is_some()`.

**How to find it.** After any `mp milestone complete`, regardless
of whether `--evidence` was supplied, run the L51 audit
(byte-equality check on per-AC `evidence` strings). Expected:
N unique, N ACs. Any collision OR any empty string = the
overwrite happened. Run the audit BEFORE calling complete (to
establish the pre-state) AND after (to confirm preservation).
The pre-state audit confirms the criterion pass set unique
strings; the post-state audit confirms complete didn't collapse
them.

**Takeaway.** "Pass an empty string to mean 'don't supply this'"
is a recurring footgun — it works in flag-only contexts where
the parser treats `""` as `None`, but in this code path the
parser treats `Some("")` as a deliberate value. When in doubt
about whether the flag carries semantic meaning for `""`, omit
the flag entirely. For close-out recipes specifically: the safe
pattern is **criterion pass → complete without --evidence**; do
not follow the L53 `--evidence ""` alternative; that alternative
clobbers per-AC evidence with empty strings and silently
re-creates the L51 collision it was meant to avoid.

### L56. Auto-increment ids must derive from `max(suffix)+1`, never `len()+1` — a removal breaks the len formula

**What it looks like.** A fragment-add command (`mp milestone ac
add`, `mp milestone question add`, `mp milestone step add`)
computes the next id as `PREFIX-{len()+1}`. On a pristine
milestone this works: three ACs → `len()=3` → next is `AC-04`.
The bug appears only after a **removal** that leaves a gap.
Removing AC-02 from `[AC-01, AC-02, AC-03]` leaves the array as
`[AC-01, AC-03]` (no renumbering), so `len()=2`. The next add
computes `AC-03` — **colliding with the surviving AC-03**. The
milestone now carries two AC-03 entries, breaking the unique-id
invariant that every `.find(|ac| ac.id == …)` lookup relies on.

The add succeeds (no error), the duplicate is persisted, and the
first downstream `criterion_update` / `criterion_pass` /
`criterion_remove` silently mutates the **wrong** AC (whichever
`.find()` matched first). The corruption is invisible until
someone notices the wrong AC's evidence changed.

**Real example.** M111 (fragment-CLI ergonomics) S2 fixed the
append regression (two sequential adds both returning `AC-01`)
by switching to `format!("AC-{:02}", m.acceptance_criteria.len() + 1)`.
That fixed the append case but inherited the len-based formula,
which collides after a removal. The S2 regression test
(`ac_add_appends_two_acs_on_fresh_milestone`) only covered the
sequential-append path — it never removed an AC between adds.
The parity reference, `step::next_step_id`, already used the
correct `max(suffix)+1` formula; the AC path was written
differently and diverged.

External review (2026-07-07) reproduced the collision
empirically: `ac remove 03 AC-02` then `ac add` produced
`[AC-01, AC-03, AC-03]`. Fix: a shared `next_fragment_id` helper
that takes `max(parse_suffix(id)) + 1` over the existing items,
parameterized by prefix (`AC`/`Q`). Applied to both
`criterion_add` and `question_add` (same bug class). Pinned by
`ac_add_after_remove_does_not_collide_with_surviving_ac`.

**How to find it.** For every auto-increment id generator:
1. Does it use `len() + 1`? **Bug** — it will collide after any
   removal that doesn't renumber.
2. Does it use `max(existing suffix) + 1`? **Correct** — gaps
   don't matter because the max monotonicly increases.
3. Is there a test that removes a middle element then adds? If
   not, the len-based formula passes every append-only test
   while still being wrong.

The smell generalizes beyond ids: any time a derived value is
computed from the **count** of a collection that supports
removal, rather than from the **max** of a monotonic field in
that collection, the removal case will silently corrupt state.
Auto-increment after delete is the canonical instance.

**Takeaway.** `len() + 1` is the wrong formula for any id space
that supports removal. The right formula is `max(suffix) + 1`,
which is robust to gaps because max only ever goes up. When you
see `len() + 1` on an id generator, ask: "what happens after a
removal?" — and write the removal-then-add test. Append-only
tests will never catch this; the bug only manifests in the
removal gap.

### L57. The AC-step status contract is a 2-tuple, not a singleton — a `passed` AC with a `pending` covering step is the same integrity bug as L51's copy-pasted evidence

**What it looks like.** A milestone ships with N steps and M ACs
where `covers_ac` wires each AC to one or more steps. The author
marks all M ACs `passed` with copy-pasted closure evidence (the
L51 pattern), but only flips the steps that are easy to forget to
`done` — the rest stay `pending`. The on-disk milestone record
reads "5/5 ACs passed, 4/5 steps done, 1 pending" and the next
agent picking up the work has no signal that the pending step
covers a passed AC. The contract between AC verdicts and step
statuses is broken: the AC says "verified" but the step that
actually shipped the work says "not done."

**Real example.** M112 (Read & inspection surface) shipped with
5 ACs and 5 steps. AC-01's `covers_ac` is `["S1"]`; S1 is the
`backlog list` step (`crates/mp/src/commands/backlog.rs`,
`tests/backlog_list.rs`). The on-disk state at the start of the
close-out pass was:

```text
AC-01: status=passed,  verification=cargo test -p mp --test backlog_list
S1:    status=pending, tests=cargo test -p mp --test backlog_list
```

The work shipped: 6/6 tests in `crates/mp/tests/backlog_list.rs`
were green, the `mp backlog list` subcommand existed and worked.
But S1 was never flipped to `done`. The milestone record's own
`mp show milestone 112 --summary` reported `5/5 ACs passed, 4/5
steps done, 1 pending` — a 2-tuple in an inconsistent state.

This is the same integrity bug as L51: the AC evidence is
copy-pasted closure prose, the step evidence is empty, and the
step status doesn't match the AC verdict. L51 catches the
*evidence* side ("never mark `passed` with copy-pasted evidence
across multiple ACs"); L57 catches the *coverage* side ("when an
AC is `passed`, every step in its `covers_ac` list must be
`done`"). The two are co-morbid: the same kind of lazy close-out
that produces L51 also produces L57.

**How to find it.** For every `status: passed` AC on a
`lifecycle: approved` or `lifecycle: complete` milestone, walk its
`covers_ac` list and check the status of every referenced step.
Expected: every referenced step is `done`. Any
`pending`/`failed`/missing step is an L57 violation. The audit
script:

```bash
# L57 audit: passed AC with non-done covering step
python3 << 'PY'
import json, subprocess, os
env = os.environ.copy()
env['PATH'] = '/Users/thiago/.agents/master-plan/bin:' + env.get('PATH', '')
env['MP_HOME'] = '/Users/thiago/.agents/master-plan'
for mid in ['111', '112', '113']:  # extend as needed
    d = json.loads(subprocess.check_output(
        ['mp','show','milestone', mid,'--format','raw'], env=env))
    step_by_id = {s['id']: s for s in d['steps']}
    for ac in d['acceptance_criteria']:
        if ac['status'] != 'passed':
            continue
        for s_id in ac.get('covers_ac', []):
            s = step_by_id.get(s_id)
            if s is None:
                print(f'{mid} {ac["id"]}: covers missing step {s_id}')
            elif s['status'] != 'done':
                print(f'{mid} {ac["id"]}={ac["status"]}: covers {s_id}={s["status"]}')
PY
```

For M112, the audit found exactly one violation: `AC-01=passed
covers S1=pending`. Fix: `mp milestone step done 112 S1` (with
per-step evidence). After the fix, the audit returns 0 violations.

**Takeaway.** The AC-step status contract is a 2-tuple: an AC
verdict is only honest if every step it covers is `done`. The
agent's close-out checklist for any milestone with a non-empty
`covers_ac` field should include: (a) every `passed` AC has at
least one covering step, (b) every covering step is `done`, (c)
every step's `evidence` is non-empty, (d) per-AC evidence is
unique (L51), (e) the verification strings are scope-narrow
(L53). The L51 fix is incomplete without the L57 fix; fixing
only the evidence side and leaving a `pending` step with a
`passed` AC is a half-fix that leaves the milestone record lying
about its own state. Pin the audit script in CI: a `passed` AC
that covers a `pending` step should be a hard G6 / W44-class
warning, not a soft lint.

### L58. Spec verifications that point at one-off shell scripts in /tmp are unverifiable after the artifact is deleted; verifications must be reproducible from a deterministic source in the repo

**What it looks like.** A milestone's AC declares its `verification`
as a one-off shell script in `/tmp` — e.g.,
`verification: /tmp/verify_b39_45.sh`. The author wrote the script
to run a multi-step check (filter the backlog, query a sub-command,
assert the slice) and copy-pasted the path into the spec so the
verifier would re-run it. The script is **not checked into the
repo**; it lives only on the author's machine (or in a CI scratch
volume). When the close-out pass fires weeks later, the script is
gone — `bash -c "$v"` returns exit 127 (file not found), and the
verifier reports a "real" failure even though the underlying work
shipped correctly. The milestone record now carries an AC whose
verification is unrunnable, and the next agent picking up the
close-out has no way to distinguish "the work regressed" from "the
verifier broke".

**Real example.** M114 (test & code hygiene sweep) shipped with
`AC-01: verification: /tmp/verify_b39_45.sh`. The script ran an
inline jq filter on `master-plan/backlog.json` to assert
`B-39..B-45` are all `status=resolved`. The script was a one-off —
it was never committed. When the close-out pass fired
(2026-07-07), `bash -c "/tmp/verify_b39_45.sh"` returned exit 127
(`No such file or directory`). The underlying work was fine: a
direct `jq -e '[.items[] | select(.id|test("^B-(39|40|41|42|43|44|
45)$")) | .status] | unique == ["resolved"]' master-plan/
backlog.json` returned exit 0, and the milestone record on disk
showed all 7 items resolved. The bug was in the verification
string itself, not in the work it claims to verify. Fix: re-scoped
AC-01's verification to the inline jq check (which S1's `tests`
field had been declaring all along as the runnable command). The
inline form is reproducible from the repo — anyone with the
milestone JSON can run it. The `/tmp/script.sh` form requires the
author's local disk state.

**How to find it.** For every `verification` and `tests` field in
a `lifecycle: approved` or `lifecycle: complete` milestone, check:

```bash
# L58 audit: verifications that point at /tmp, /var/tmp, $HOME,
# or any path that isn't either (a) a repo-relative path,
# (b) a `cargo test ...` invocation, or (c) a `mp ...` subcommand.
python3 << 'PY'
import json, subprocess, os
env = os.environ.copy()
env['PATH'] = '/Users/thiago/.agents/master-plan/bin:' + env.get('PATH', '')
env['MP_HOME'] = '/Users/thiago/.agents/master-plan'
listing = json.loads(subprocess.check_output(
    ['mp','list','milestones','--fields','milestones[].id'], env=env))
bad = []
for m in listing['milestones']:
    mid = m['id']
    try:
        d = json.loads(subprocess.check_output(
            ['mp','show','milestone', mid,'--format','raw'], env=env))
    except Exception:
        continue
    targets = []
    for ac in d.get('acceptance_criteria', []):
        v = ac.get('verification', '')
        if v.startswith('/tmp/') or v.startswith('/var/') or v.startswith('~/') or v.startswith('$'):
            targets.append(f'{ac["id"]}: {v[:80]}')
    for s in d.get('steps', []):
        t = s.get('tests', '')
        if t.startswith('/tmp/') or t.startswith('/var/') or t.startswith('~/') or t.startswith('$'):
            targets.append(f'{s["id"]}.tests: {t[:80]}')
    if targets:
        bad.append((mid, targets))
for mid, ts in bad:
    print(f'M{mid}: {len(ts)} non-deterministic verification(s)')
    for t in ts:
        print(f'  {t}')
PY
```

For M114, the audit found exactly one hit:
`AC-01: /tmp/verify_b39_45.sh`. After the fix (re-scope to inline
jq), the audit returns 0 hits.

**Takeaway.** A verification string is a **contract**: it must be
runnable by the next agent, the next CI runner, and the next code
reviewer — none of whom have the author's `/tmp` directory.
Acceptable shapes:

- A `cargo test -p <crate> --test <binary> [filter]` invocation
  (the integration test binary is the contract; reproducible from
  the repo).
- A `mp <subcommand> ...` invocation (the CLI is the contract;
  reproducible from the installed binary + plan).
- A `make <target>` invocation (the makefile is the contract;
  reproducible from the repo).
- A `jq` / `python3 -c` / shell pipeline that operates on
  **repo-relative paths** (e.g., `master-plan/backlog.json`, not
  `/tmp/foo.json`).
- A `manual: <reason>` marker when no automation is possible
  (and the manual audit is recorded in `evidence` per the
  on-disk state, not in a transient script).

Unacceptable shapes:

- `/tmp/<random>.sh` — a one-off script that disappears on
  reboot, container eviction, or workspace cleanup.
- `~/...` — depends on the author's home directory layout.
- A bare script name without a path (`./verify.sh` is fine if
  the script is in the repo; `verify.sh` is not).
- A command that depends on `mp` being on PATH in a way that
  hasn't been documented (the harness shell env is the
  exception, not the rule; per §0 of `master-plan/AGENTS.md`,
  the global `mp` install is on PATH for shells that sourced
  `~/.zshrc`, but CI runners may not have that source step).

The smell generalizes beyond verifications: any time a contract
points at external, non-reproducible state (a file in `/tmp`, a
local branch name, a database row, a network endpoint), the
contract becomes unenforceable after the external state is gone.
The fix is to anchor the contract in repo-relative paths or
test-binary invocations — anything that can be derived
deterministically from the repo's own state. Pair this lesson
with L43 (test fixture writes to the same wrong path as
production code): L43 is about a path collision between the
fixture and the production code; L58 is about a verification
path that has no anchor in the repo at all.

### L59. Value-parser ACs need an exercise-the-parser verification, not just a help-text grep

**What it looks like.** A milestone ships a value parser (clap
`value_parser = ...` on a `#[arg(long)]` flag) as part of fixing
a "user typed something that got persisted literally" bug. The
AC verification is `<cmd> --help | grep -A2 -- '--<flag>'` —
inspect the help text, declare the AC passed. A regression that
removes the `value_parser = ...` attribute, weakens the parser
logic, or shifts the rejection boundary by one character never
fails the verification, because the help text is unchanged. The
bug silently returns: the literal-string corruption is back, and
only a downstream observer (the dogfood log, an unrelated test)
catches it.

**Real example.** M116 (Meta-tooling & docs) shipped the
`files_value_parser` for `mp milestone step add/update --files`
to fix entry 17 sub-1 (`--files '["a.rs"]'` stored the literal
string in `step.files`). The parser only rejected
`starts_with('[') && ends_with(']')` — narrower than the bug
class. The M116 AC-02 verification was
`mp milestone step update --help | grep -A2 -- '--files'`, which
read the help text and confirmed it mentions "rejected by the
value parser." That test never hit the parser. The 2026-07-07
external code review reproduced the silent-acceptance bug by
running `mp milestone step update 116 S1 --files '{"a.rs"}'` —
the call returned `{ok:true}` and wrote `{"a.rs"}` to
`step.files`. The parser's `ends_with(']')` clause left three
inputs uncaught (`[a.rs`, `a.rs]`, `{"a.rs"}`), and the AC
verification string was the wrong shape to flag it.

**How to find it.** For any AC whose remediation is a value
parser, a CLI guard, or any in-code gate that's exercised by
runtime input:
1. **Read the AC verification string.** Does it grep docs/help
   text, or does it run the CLI and assert an input class is
   rejected? If it greps docs, the AC is shallow.
2. **Read the parser/guard's truth table.** What inputs are
   accepted, and what inputs are rejected? Are the rejection
   boundaries wider than the bug class? If they're the same
   width as the bug class, every adjacent shape slips through.
3. **Look for unit tests.** A value parser without a `#[test]`
   in the same file is unverified. A `grep "<symbol>"` in the
   test tree is the next-best signal — if no test references the
   parser, the AC is the only place the parser is exercised, and
   the AC is shallow.

**Fix.** Reword the AC verification to exercise the parser:
- Unit-test the parser in the source file's `#[cfg(test)] mod`
  with one test per input class (accepted: bare path, comma-
  separated, whitespace; rejected: empty, JSON array, malformed
  JSON array, JSON object, malformed JSON object).
- Add an integration test in `crates/<pkg>/tests/suites/` that
  runs the CLI end-to-end with each input class and asserts
  either `status.success()` and the expected on-disk payload, or
  `!status.success()` and that the on-disk payload was NOT
  corrupted by the rejected call.
- The AC verification can stay as a sanity grep, but only after
  the suite tests exist.

**Pair with L51** (AC integrity — evidence must be unique per
AC and derived from running the verification, not copy-pasted
across closures) and **L58** (verifications must be
reproducible from repo-anchored inputs). L51 and L58 both ask
"is the verification real?"; L59 asks "does the verification
cover the bug class?" — the third leg of the integrity stool.

### L60. Process-prompting docs live in two places — both must point at the current truth, not the one the author happened to look at

**What it looks like.** A milestone adds a process rule — "use
mp scratch, not /tmp" — and ships it to one of the AGENTS.md
files (root or master-plan/). The cross-reference in the second
file points at the first, the cross-reference in the first file
points at a third resource (the docs reference), and neither
cross-reference is checked against the actual surface. Months
later the docs reference has moved on (or never existed), and
agents following the link land on a 404. Authors discover this
when an agent can't find the subcommand they were told to run.

**Real example.** M116 (Meta-tooling & docs) shipped the
"Temporary workspace" rule:
- Root AGENTS.md got the dedicated subsection with the
  `SCRATCH=$(mp scratch new m-update)` example.
- master-plan/AGENTS.md §0 got a one-liner cross-reference.
- Root AGENTS.md's subsection cross-referenced
  `MP-COMMANDS.md#mp-scratch` — **a section that doesn't exist**
  in that file.
- Same subsection said the subcommand surface was
  `list`, `path`, `drop` — but the CLI only ships `path`, `new`.

The 2026-07-07 external code review fixed both: rewrote the
cross-reference to point at `mp scratch --help` (the source of
truth) and removed the false claims about `list`/`drop`.

**How to find it.** For every cross-reference added to
AGENTS.md (or any guide the agent reads on session start):
1. **Verify the link target exists.** `rg -- '<symbol>' <file>`
   should return at least one match. If empty, the link is
   broken.
2. **Verify the claimed surface matches reality.** If the doc
   lists `mp foo list` as a subcommand, `mp foo --help` should
   list `list` as a subcommand. Any discrepancy is a recipe for
   "the agent followed the guide and got rejected by the CLI."
3. **Check both directions.** A cross-reference from root
   AGENTS.md to master-plan/AGENTS.md should be balanced with a
   reverse in master-plan/AGENTS.md pointing back. If one side
   exists without the other, the link will rot independently.

**Fix.** When shipping a process-rule AGENTS.md entry, also
verify: (a) every cross-reference target exists in the cited
file, (b) every subcommand/enum/constant claimed in the entry
matches the CLI's actual surface, (c) both ends of any
root↔master-plan cross-reference are present and consistent.

### L61. When injecting a helper between two doc-bearing functions, manually verify the adjacent doc blocks are still attached to the correct function

**What it looks like.** A milestone adds a new helper function
to a module that already has long, prose-style doc comments.
The author uses a batch `edit` to insert the new function's
definition `+ doc block` *between* two existing functions,
reasoning that the heredoc is self-contained — "I've added the
helper, now compile and move on." The doc block being inserted
is delimited enough that the batch-tool's N-block counter
matches, but the *adjacent* doc block above the insertion
point got split: half attaches to the function above
(intended), but the last `/// ...` line of that block is now
floating between the new helper and the function below,
becoming an empty 1-line doc comment for the lower function.
Clippy later flags the misplaced fragment as "empty line after
doc comment."

**Real example.** M117 (Verifier timeout hardening) inserted
`fn killpg_child` into `crates/mp/src/ac_verify.rs` between
`bounded_join`'s long rationale doc (ended "future
consideration.") and `bounded_join`'s definition. The batch
edit succeeded (2/2 blocks replaced), but the doc block was
visibly split: an orphan `/// future consideration.` line
ended up as a 1-line doc comment for `bounded_join`. Clippy
flagged it at ac_verify.rs:623 with "empty line after doc
comment." The fix was to delete the orphan line and reorder
the helpers so the doc attaches to the correct function.

**How to find it.** After any batch edit that injects a new
function into an existing module:
1. **Read the surrounding code, not just the changed region.**
   Open the file with the diff applied and re-read the 20
   lines above and below the insertion point. If a
   `/// ...` line is followed by a *function* whose name
   doesn't match the doc text, the comment was orphaned.
2. **`grep -n "^///" <file>`** after the edit. The first `///`
   lines of a doc block should attach to the next non-doc
   declaration; orphan lines are visually obvious because
   they sit alone above a different function.
3. **Run `cargo clippy`** if `clippy::lint-groups` includes
   `clippy::doc_markdown` or similar. Some orphans get
   flagged by the orphan-doc-comment lint; some don't,
   depending on Rust version.

**Fix.** Delete the orphan line and reorder the helper so
the doc attaches to the function that owns the content.
If the inserted helper *must* sit between two existing
doc-bearing functions, the doc block above the helper's
insertion point needs to be split intentionally (each
function gets its own `///` block), not silently truncated.

**Pair with M116 CR's "edit-tool batch correctness" note**
in `AGENTS.md`: the same surface — a counter that reports
matches-attempted rather than matches-replaced — applies
here. A "Successfully replaced 2 blocks" report on a split
doc block is meaningless; the second block was a 1-line
fragment, not a doc block. Always re-read the surrounding
context after a batch edit, not just the changed region.

### L62. When a doc-comment promises "same X as the sibling path," the assertion surface must pin BOTH paths

**What it looks like.** A milestone ships two parallel paths
through the same logical operation — e.g., `mp milestone ac update`
(single-AC) and `mp milestone ac bulk` (multi-AC). The
multi-AC path's doc-comment says "same shell-parse preflight as
the single-AC path." The single-AC path's test surface pins the
preflight; the multi-AC path's test surface pins the JSON-shape
errors and the bulk-dispatch flow. The preflight claim is true in
the source comments but never exercised in tests, so it silently
drifts: the multi-AC path's CLI plumbing forgot to call
`shell_parse_preflight` while the single-AC path's CLI plumbing
remembers to. An agent using the bulk path with a malformed
verification gets NO warning, while the same agent using the
single-AC path WOULD have gotten a warning. The doc comment
stays true in the lib-level function but is false at the CLI
entry point.

**Real example.** M118 (Dogfood backlog close-out) shipped
`criterion_bulk_update` with the doc-comment promise "same
shell-parse preflight (`sh -n`), same evidence preflight, same
fragment-only stdout contract." The lib-level helper that's
documented to do the preflight is fine; the CLI plumbing in
`crates/mp/src/commands/milestone.rs` `Bulk` arm, however, never
calls `shell_parse_preflight`. Verified by reproduce: bulk-updating
`{"id":"AC-01","verification":"if then echo broken"}` through the
bulk CLI returned `{ok:true}` with no `preflight_warning`; the
same input through the single-AC `Update` CLI emitted the
`preflight_warning` correctly. The bulk path's existing test
suite pinned the shape-validation errors and the bulk-dispatch
flow but skipped the preflight surface area, so the gap was
uncovered.

**How to find it.** After any milestone that adds a bulk / batch /
"same as X but for many" surface:
1. **Read the doc-comments of the new lib-level helper.** If it
   promises "same Y as the sibling," grep the sibling's CLI arm
   for the Y call and verify the new CLI arm has it.
2. **Cross-arm diff:** `git diff <new arm> <sibling arm>` for the
   helper invocations and post-write hooks. A missing preflight
   in the new arm shows up as a missing `shell_parse_preflight`
   call.
3. **Test-pin both:** every diagnostic the single-AC path emits,
   the bulk path must either emit the equivalent or document an
   explicit divergence. If the bulk path's test suite covers the
   shape errors but not the preflight, that's the gap.

**Fix.** Add the missing invocation (in M118 CR F-1, the bulk path
now calls `shell_parse_preflight` per-element and collects warnings
into a `preflight_warnings` array on the response). For diagnostic
parity audits, also pin the diagnostic surface with a test that
_exercises_ the diagnostic and asserts the response shape.

**Pair with F-1 / F-2 of M118 CR:** this lesson is the
observation-side counterpart. F-2 (pre-check the invariant) is
the prevention-side lesson. L62 is the assertion-side lesson:
even when you've prevented the silent drift, the test must pin
the contract or the next round of refactors will reintroduce it.
Together they form the "atomic bulk updates + diagnostic
parity" pair: F-2 is the production-correctness half, L62 is the
test-coverage half.

### L63. Agent-enforced policy needs one vocabulary across config, parser, and skill instructions

**What it looks like.** A project-level policy is intentionally enforced by an
agent skill instead of by the CLI. The config and parser use one closed
vocabulary (`low|medium|high`), while the procedural skill tells agents to
produce a different vocabulary (`blocker|major|minor|nit`). Both surfaces look
reasonable in isolation, and their own tests pass. At the integration boundary,
however, every value emitted by the skill hits the parser's unknown-value
fallback. The feature silently degrades to its safest no-op behavior, so no
error tells the operator that the declared policy is ineffective.

**Real example.** M147 added `[agent.automation].auto_remediate`. Its
`SeverityRank::from_config_value` recognized `low`, `medium`, and `high`
(`all` is a config-side alias for `low`) and deliberately mapped unknown labels
to `None` / record-only. At the same time,
`templates/skills/mp-coordinator/reviewing.md` instructed external reviewers to
file `blocker`, `major`, `minor`, or `nit`, and `mp-flow/SKILL.md` ordered the
handoff with "blocker/major first." An agent following the shipped review skill
would therefore create only unknown severities. Even with
auto-remediation enabled, every finding stayed record-only. Unit tests for the
threshold truth table were green because they supplied the parser's vocabulary
directly; they never checked the values the agent instructions actually emit.
M147 external review filed F-03 and fixed the skills to use
`low|medium|high`, with contract tests that require those labels and forbid the
stale four-level vocabulary.

**How to find it.** For every agent-enforced enum or threshold:

1. List the values accepted by the config validator and runtime parser.
2. List the values the skill, prompt, examples, and handoff docs tell the agent
   to emit.
3. Compare the sets exactly, including spelling, case, and aliases.
4. Trace the unknown-value branch. A fail-safe fallback such as `None`, `false`,
   or record-only is operationally safe but diagnostically silent, so a
   vocabulary mismatch can disable the feature without failing a test.
5. Add a cross-surface contract test that checks both positive vocabulary
   presence and forbidden stale labels. Parser-only truth-table tests are not
   enough.

**Takeaway.** When enforcement lives in a skill, the skill text is executable
control-plane behavior. Config schema, parser, and instructions must share one
canonical vocabulary. If legacy labels need support, map them explicitly and
test the mapping; otherwise forbid them in the instructions and pin that
absence. A safe unknown-value fallback prevents accidental action, but it can
also hide a completely disabled automation feature.


