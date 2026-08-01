# Performance acceptance criteria (PERF-ACS)

**Status:** Implemented (M176).  
**Audience:** Agents and humans authoring milestones with quantitative
wall-clock or throughput claims.

---

## The rule

Every acceptance criterion whose description or verification states a
**numeric performance threshold** (`≥` / `>=` with a unit such as `s`,
`%`, `min`, or `sec`) must either:

1. **Measure it** with the `mp_measure!` helper (or the free function
   `mp::perf_measure::measure`), and pin the threshold with a
   `#[track_caller]` assertion on the returned record, **or**
2. Carry an explicit **`manual:`** verification prefix with a documented
   re-measure schedule (and usually a dogfood-log entry).

The AC verifier (`crates/mp/src/ac_verify.rs`) is **exit-code-only**. It
cannot parse "≥30 s faster". Without `mp_measure!` (or honest `manual:`),
a green `make test` will mark a failed perf claim as `passed`.

### Plan JSON is not executable

Stamping `mp_measure!(…)` into a milestone's `verification` field does
**not** run the macro at `mp milestone complete` time. Plan-zone strings
are **lint targets** for `perf_ac_threshold_reality_check` and human
readers. An **executable** numeric gate only exists when a Rust test (or
future CLI) calls `mp_measure!` + `assert_*`. Historical ACs should stay
`manual: mp_measure!(…) …; measured … (entry N)` with the dogfood log as
the authoritative measurement.

A regression test enforces the convention:

```bash
cargo nextest run -p mp -E 'test(/perf_ac_threshold_reality_check/)'
```

---

## Measure first, then author

Before writing a quantitative threshold:

1. Run the workload **locally** (same class of machine you care about).
2. Capture **≥3 runs** cold and/or warm as appropriate.
3. Record mean, range, and hardware notes in `mp-dogfood-log.md`.
4. Set the AC threshold to a **achievable** bound (e.g. mean − noise),
   not an aspirational marketing number.

---

## `mp_measure!` surface

Defined in `crates/mp/src/perf_measure.rs`:

```rust
use mp::mp_measure;

// Default: 3 runs via /usr/bin/time -p (falls back to Instant).
let rec = mp_measure!("cold_nextest", "cargo nextest run --manifest-path Cargo.toml");
assert!(rec.mean_s > 0.0);
rec.assert_mean_at_most(120.0);

// Explicit run count:
let rec = mp_measure!("echo_ok", "echo ok", runs = 5);
rec.assert_mean_at_most(1.0);

// Percent drop vs a known baseline mean:
rec.assert_drop_at_least_pct(baseline_mean_s, 15.0);
```

Record shape: `{ label, mean_s, stddev_s, runs: Vec<f64> }`.

If the threshold is not met, the assertion **panics** (test fails /
AC fails) — it does **not** silently classify as passed.

---

## Plan-zone verification strings

For historical milestones (already measured in the dogfood log), stamp
the verification field so the reality-check test passes and readers see
the contract:

```text
manual: mp_measure!(m159_cold_nextest, "cargo nextest run --manifest-path Cargo.toml", runs = 3) assert mean drop ≥15 s vs baseline 108.39 s; measured 17.4 s mean (entry 24)
```

Prefer `manual:` + `mp_measure!` when re-running the cold suite on every
`mp milestone complete` would be prohibitively expensive; the log entry
remains the authoritative measurement.

---

## Worked examples (M159 / M160 / M161)

| Milestone | Original claim | Measured | Re-spec (M176) |
|-----------|----------------|----------|----------------|
| **M159** AC-01 | ≥30 s cold nextest drop | 17.4 s mean (entry 24) | ≥15 s cold drop; `mp_measure!` in verification |
| **M160** AC-01 | ≥50 % CI warm drop | ~19.2 % local warm (entry 26) | ≥15 % local warm-cache 3rd run; CI deferred to sibling AC |
| **M161** AC-04 | ≥1 min cold `make test` | +4.21 s noise (entry 27) | Incremental re-link ≥5 s; cold claim retired |

### Why the originals failed

- Thresholds were authored **without** local measurement.
- Verifier only checked exit code → force-bypassed markers on ship.
- The real win often lived on a **different axis** (warm cache, incremental
  re-link) than the cold wall-clock claim.

---

## Authoring checklist

- [ ] Local N≥3 measurement recorded (dogfood log or evidence).
- [ ] Threshold ≤ measured mean with noise margin.
- [ ] Verification is `mp_measure!(…)` and/or `manual: …`.
- [ ] No `[force-bypassed` marker left on ship without a follow-up.
- [ ] `perf_ac_threshold_reality_check` is green.

---

## See also

- [GROOMING.md](./GROOMING.md) — grooming-depth rubric (criterion 5: real AC examples).
- `crates/mp/src/perf_measure.rs` — implementation.
- `crates/mp/tests/perf_ac_threshold_reality_check.rs` — lint test.
- `mp-dogfood-log.md` entries 24, 26, 27 — original measurements.
