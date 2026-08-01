# Test taxonomy — subprocess vs in-process

This document classifies every category of integration test in the
`master-plan` repo by **what surface it actually exercises**. The
classification drives a single mechanical question:

> *Does this test need to spawn `mp` as a subprocess, or can it call
> `mp::…` directly from the test process?*

Background: milestone **M162**
(`master-plan/milestones/162-convert-pure-logic-integration-tests-to-in-process-library-calls.json`).
The repo's 1974 integration tests spent ~50 ms × 2140 spawns ≈ **107 s**
of pure subprocess overhead per `make test` run before M162. The
boundary drawn below is the contract for the in-process conversion:
tests on the **right** side may be converted to in-process calls (via
`crates/mp/tests/common/lib_api.rs`); tests on the **left** side MUST
stay subprocess.

---

## Quick decision tree

```
Does the test write to a real path on the developer's machine
(e.g. install/uninstall under ~/.agents, doctor probes real fs)?
  YES ──► MUST be subprocess (real-fs smoke)
  NO ──►

Does the test exercise process supervision, IPC, a TUI,
or a child process group (watch_*, herdr, ratatui)?
  YES ──► MUST be subprocess (process supervision)
  NO ──►

Does the test only assert on the JSON shape of a CLI command's stdout
(validate, fragment ops, projection, store reads, milestone ac/step/wp
list/show, plan_io load, plan_diff)?
  YES ──► CAN be in-process (call mp::… directly via lib_api)
  NO ──►

Is the test asserting on a real file-system side effect that survives
the test process (init writes master-plan/ tree, install writes
~/.agents/master-plan/, mutation tests that read back from disk)?
  YES ──► MUST be subprocess (write+read-back contract)
  NO ──►

Default ──► MUST be subprocess (conservative; revisit if perf matters)
```

---

## Category A — MUST stay subprocess

| Category | Why subprocess | Examples |
|----------|----------------|----------|
| **install / uninstall** | Real-fs writes under `~/.agents/master-plan/`; rollback semantics depend on the user's actual home dir. | `tests/suite_install.rs`, `install_manifest.rs`, `install_skills_v2.rs`, `watch_install_*` |
| **doctor** | Probes real fs / env vars / harness presence. | `tests/suite_doctor.rs`, `suite_doctor.rs` |
| **watch / herdr** | Process supervision, IPC with the herdr binary, child process groups. | `tests/watch_*.rs`, `watch_herdr_*` |
| **TUI (raul)** | The `raul` TUI runs in a separate process with crossterm raw mode. | `crates/raul/tests/tui_*` |
| **init** | Writes `master-plan/` directory tree from scratch; subsequent assertions read back from disk. | `tests/suite_init.rs`, `init_*` |
| **end-to-end smoke** | The whole point of these is to verify the shipped binary. | `make test-fixtures` |

**Rule for these:** keep using `env.run([...])` from `tests/common/mod.rs`.
Do not refactor to `lib_api`.

---

## Category B — CAN be in-process

| Category | `lib_api` wrapper | Subprocess equivalent | Wrapped today? |
|----------|-------------------|----------------------|----------------|
| **validate (read-only)** | `lib_api::validate(&ctx)` | `env.run(&["validate", ...])` | yes |
| **milestone ac read** | `lib_api::milestone_ac_show(&ctx, id, ac_id)` / `milestone_ac_list(&ctx, id)` | `env.run(&["milestone", "ac", "show"/"list", ...])` | yes |
| **milestone ac write** | `lib_api::milestone_ac_pass(&ctx, id, ac_id, evidence)` / `milestone_ac_update(&ctx, id, ac_id, json)` | `env.run(&["milestone", "ac", "pass"/"update", ...])` | yes |
| **milestone create / approve / complete** | `lib_api::milestone_create / milestone_approve / milestone_complete` | `env.run(&["milestone", "create"/"approve"/"complete", ...])` | yes |
| **step show / step list** | `lib_api::step_show(&ctx, mid, sid)` / `step_list(&ctx, mid)` | `env.run(&["step", "show"/"list", ...])` | yes |
| **show milestone (whole file)** | `lib_api::show_milestone(&ctx, id)` | `env.run(&["show", "milestone", ...])` | yes |
| **projection / plan_io load / wp wrappers / step load*** | (none — defer to follow-up) | `env.run(...)` only | **no** (out of M162 scope) |
| **JSON shape assertions** | `serde_json::from_slice::<Value>(output)` | every test that just parses stdout | yes |

**Rule for these:** use `crates/mp/tests/common/lib_api.rs` instead of
`env.run`. `lib_api` is a thin wrapper around `mp::…` that returns
`Result<serde_json::Value>` matching the CLI's JSON shape, plus a
helper for setting up a `PlanContext` from a `TempDir`.

**Out of M162 scope:** `mp::projection::project_*`, `mp::plan_io::load`,
`mp::step::load_*`, and `mp::milestone::wp::load_*` are listed in the
"projection / plan_io load / wp wrappers / step load*" row of the
table as NOT wrapped today — tests that need those stay on `env.run`.
Wrapping them is a natural follow-up to the M162 mechanical conversion
of the remaining 385 spawn sites.

---

## Conversion recipe

Before:

```rust
use crate::common::TestEnv;

#[test]
fn ac_show_returns_only_requested_fragment() {
    let env = TestEnv::blank();
    // ... copy fixture to env.tmp.path() ...
    let out = env.run(&["milestone", "ac", "show", "03", "AC-03",
                        "--format", "json"]);
    assert!(out.status.success(), "ac show failed");
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json");
    // ... assertions on value ...
}
```

After:

```rust
use crate::common::{lib_api, TestEnv};

#[test]
fn ac_show_returns_only_requested_fragment() {
    let env = TestEnv::blank();
    // ... copy fixture to env.tmp.path() ...
    let value = lib_api::milestone_ac_show(&env, "03", "AC-03")
        .expect("ac show in-process");
    // ... same assertions on value ...
}
```

The key change is the **call surface** (in-process fn vs subprocess
spawn). The assertions stay byte-identical — that's what the
`lib_api_parity` test binary verifies (M162 AC-03).

---

## When to add to which side

**Add a subprocess test only if:**
- The test is on the "MUST stay subprocess" list above, OR
- The test is a one-off smoke for a new command surface and you want
  the full CLI argument-parsing path exercised end-to-end (acceptable
  for the first test of a new command; convert to in-process after).

**Add an in-process test only if:**
- The test's assertion is reachable via a public `mp::…` function, OR
- You're adding the public `mp::…` function at the same time (see
  M162 scope: no new public API, reuse existing).

**If neither fits:** file as `idea` (`mp idea create --title …`) and
defer the design discussion.

---

## Performance budget

The in-process path saves ~50 ms per spawn. With 2140 spawns today
(60% of which are convertible per M162 spec), the expected drop is:

- 60% of 2140 ≈ **1284 spawns** × 50 ms = **~64 s** saved per
  `make test` run.
- M162 AC-04 target: `make test` ≥20% faster than M161 baseline
  (~92 s → ≤74 s).

The 20% target is conservative; the full conversion should beat it
comfortably.

---

## See also

- [CONTRIBUTING.md](../../../CONTRIBUTING.md) — repo-root contributor
  guide that links here.
- `crates/mp/tests/common/lib_api.rs` — the typed in-process
  wrapper surface.
- `crates/mp/tests/lib_api_parity.rs` — the byte-identical parity
  guard between in-process and subprocess paths.
- `scripts/verify-m162-no-spawns.sh` — CI guard that fails the build
  if a converted suite re-introduces a subprocess spawn.
- [TESTING.md](../../05%20-%20Technical/TESTING.md) — the older
  fixture-driven TDD strategy doc.