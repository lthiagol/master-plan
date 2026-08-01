# WIP CI — Test-Coverage Handoff

> **Branch:** `wip` (tracking `origin/wip`)
> **Goal:** `make ci` green on `wip-ci.yml`.
> **Status:** ✅ **Resolved** (follow-ups in `WIP-CI-FOLLOWUP-HANDOFF.md` largely landed).
> **Date resolved:** 2026-08-01

---

## Resolution summary

1. **CI provides `mp`:** both `wip-ci.yml` and `stable-ci.yml` run `make build`
   then append `$GITHUB_WORKSPACE/target/release` to `$GITHUB_PATH` (variant B2).
2. **`make ci` / `make test` preflight** fails fast if `mp` is not on PATH or
   `$MP_HOME/bin/mp`.
3. **Install-doctor tests** keep the real doctor contract; they prepend the
   install bin via `path_with_install_bin` (mirrors `source env.sh`).
4. **Do not** `#[ignore]` raul mp-shell tests; do not relax doctor `mp_on_path`.

See also: `WIP-CI-FOLLOWUP-HANDOFF.md`, `AGENTS.md` (Dev commands + CI-parity).
