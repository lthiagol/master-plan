# WIP CI — Follow-up Handoff

> **Companion to:** [`WIP-CI-HANDOFF.md`](WIP-CI-HANDOFF.md)
> **Status:** ✅ Items below implemented on `wip` (2026-08-01). Remaining: merge
> path to `stable` (D1) and archive these handoff files after ship (D3).

| ID | Item | Status |
|----|------|--------|
| A1 | `stable-ci.yml` PATH parity | done |
| A2 | nextest `--profile ci` `fail-fast=false` when `NEXTTEST=1` | done |
| A3 | `make ci` runs `mp-flow-lint` + `test-scenarios` | done |
| A4 | mp preflight on `make test` + `make ci` | done |
| B1 | `path_with_install_bin` helper | done |
| B2 | `find_mp` cargo-test `deps/` layout | done |
| B3 | doctor comment on `mp_on_path` contract | done |
| B4 | herdr flake retries in nextest.toml | done |
| C1 | `mp idea` for install PATH hardening | done (this session) |
| C2 | activity.json noise documented in AGENTS.md | done |
| D1 | Merge path to stable | **open** (prefer squash) |
| D2 | CHANGELOG Unreleased section | done |
| D3 | Archive handoffs after merge | **open** → `docs-old/` |
| E1 | CI-parity recipe in AGENTS.md | done |
| E2 | Transient `handoff.md` removed | done |

## Still open when merging

1. **D1 — Merge path:** prefer squash onto `stable`; keep handoff history in git.
2. **D3 — After ship:** move `WIP-CI-*.md` to `docs-old/` (or delete).
