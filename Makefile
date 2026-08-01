# Master Plan — developer & install targets
# See: docs/concepts/02 - Getting Started/INSTALL.md

ROOT           := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
CARGO          ?= cargo
TARGET         := $(ROOT)target/release/mp
INSTALL_DIR    ?= $(HOME)/.agents/master-plan
INSTALL_HARNESSES ?= opencode,cursor,pi
MP_INSTALL_ENV = MP_HOME=$(ROOT) MP_INSTALL_DIR=$(INSTALL_DIR)
INSTALL_JSON   = $(MP_INSTALL_ENV) $(TARGET) install --dev --source $(ROOT) --format json
SUMMARY        = bash $(ROOT)scripts/install-summary.sh

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show this help
	@echo "Master Plan — common targets"
	@echo ""
	@grep -E '^[a-zA-Z0-9_.-]+:.*##' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'

.PHONY: build build-release
build: build-release ## Alias for build-release

build-release: ## Build all workspace binaries (mp + raul)
	$(CARGO) build --release --manifest-path $(ROOT)Cargo.toml

.PHONY: check check-plan-json
check: ## cargo check all workspace members (mp + raul + mp-model)
	$(CARGO) check --manifest-path $(ROOT)Cargo.toml
	@bash scripts/audit-stub-tests.sh
	@bash scripts/check-plan-json-only.sh

check-plan-json: ## M92 gate: no .toml under plan dirs
	@bash scripts/check-plan-json-only.sh

.PHONY: test test-serial test-fixtures test-scenarios test-nextest test-mp-lib dev-linker mp-flow-lint lint consumer-surface-lint ci regen-goldens
mp-flow-lint: ## M120: assert mp-flow SKILL.md matches the 12-stage manifest (stages.toml)
	@python3 $(ROOT)scripts/mp_flow_lint.py

# M195: prevent internal-provenance leaks onto the consumer surface
# (templates/skills/** + docs/** + adopter-facing READMEs). Flags M\d+ IDs,
# L\d+ codes, and the dead doc paths (docs/code-review-lessons.md,
# docs/dogfood/…); repository-internal skills are excluded; an inline
# allowlist absorbs known-good exceptions (e.g. synthetic IDs in CLI
# examples). Wired into `make lint` so a fresh leak fails the same gate
# clippy + fmt fail.
consumer-surface-lint: ## M195: ripgrep guard over the consumer surface
	@bash $(ROOT)scripts/check-consumer-surface.sh
# Shared preflight: raul integration tests shell out to `mp`.
define require_mp
	@if ! command -v mp >/dev/null 2>&1 && ! { [ -n "$$MP_HOME" ] && [ -x "$$MP_HOME/bin/mp" ]; }; then \
		echo "mp not findable (PATH or \$$MP_HOME/bin/mp)."; \
		echo "  CI puts target/release on PATH (wip-ci.yml / stable-ci.yml)."; \
		echo "  Locally: eval \"\$$(make dev-env)\"  or  make install"; \
		exit 2; \
	fi
endef

# When NEXTTEST=1 (CI), use nextest profile.ci (fail-fast=false).
NEXTEST_ARGS := $(if $(NEXTTEST),--profile ci,)

test: ## Run tests in parallel (nextest) + fmt check. Lint lives on `make lint` (M159). Needs: cargo install cargo-nextest
	@command -v cargo-nextest >/dev/null || { echo "cargo-nextest missing; install: cargo install cargo-nextest  (or: brew install cargo-nextest)"; exit 1; }
	$(require_mp)
	$(CARGO) nextest run --manifest-path $(ROOT)Cargo.toml $(NEXTEST_ARGS)
	$(CARGO) fmt --all -- --check

test-serial: ## Serial cargo test + fmt check (fallback if nextest unavailable). Lint lives on `make lint` (M159)
	$(CARGO) test --manifest-path $(ROOT)Cargo.toml
	$(CARGO) fmt --all -- --check

# M159: clippy was costing ~ one cold test-binary compile per `make test`
# invocation because `--all-targets` forces re-checking tests / examples /
# benches. Move it behind `make lint` so `make test` only pays the nextest
# compile cost. `make ci` is a local convenience for humans who want a
# one-shot lint+test; CI does not invoke this Makefile. CI runs from
# .github/workflows/plan.yml: cargo build --release (mp, raul), cargo fmt,
# cargo clippy --release --all-targets, make mp-flow-lint, ./target/release/mp
# validate, make test-fixtures, make test-scenarios, make dep-audit-raul,
# cargo test -p raul. Note: CI uses `cargo clippy --release` (release
# profile) while `make lint` uses the dev profile for faster local iteration;
# both pass `-D warnings` and both must remain clean — see the AGENTS.md
# "matching CI" note for the policy.
lint: ## Run clippy + fmt + consumer-surface check (not part of `make test`; pair with `make ci` locally). M159 + M195
	$(CARGO) clippy --manifest-path $(ROOT)Cargo.toml --all-targets -- -D warnings
	$(CARGO) fmt --all -- --check
	@bash $(ROOT)scripts/check-consumer-surface.sh

# CI workflows put target/release on PATH before invoking this target; locally
# use `eval "$(make dev-env)"` or `make install`. Also runs mp-flow-lint +
# golden CLI scenarios (not part of plain `make test`).
ci: ## Run lint + tests + mp-flow-lint + test-scenarios (requires mp)
	$(require_mp)
	$(MAKE) lint test mp-flow-lint test-scenarios

# Rewrite committed golden JSON under tests/fixtures/ from current mp/model
# output. Not part of CI — run after intentional schema changes, then review
# the diff and re-run the golden compare tests.
regen-goldens: build-release ## Rewrite JSON golden fixtures (json-shape + track)
	$(CARGO) run --release --example regen-goldens --manifest-path $(ROOT)crates/mp/Cargo.toml

test-nextest: test ## Alias for test (kept for muscle memory)

test-mp-lib: ## Fast mp loop: lib unit tests only (no integration link)
	$(CARGO) test -p mp --lib --manifest-path $(ROOT)Cargo.toml

dev-linker: ## Install mold on Linux for faster links (no-op on macOS — use split-debuginfo)
	@if [ "$$(uname -s)" = Linux ]; then command -v mold >/dev/null || (command -v apt-get >/dev/null && sudo apt-get install -y mold || brew install mold); else echo "macOS: split-debuginfo in .cargo/config.toml (mold unsupported on Mach-O)"; fi

test-scenarios: build-release ## Run golden CLI scenarios (tests/scenarios)
	# M194: use `cargo nextest` instead of `cargo test`. `cargo test`'s
	# incremental rebuild deletes `target/release/mp` (the binary the
	# snapshot tests hardlink) and rewrites it; the window is faster than
	# the snapshot test's 3-retry/backoff loop, so `mp_bin()` panics with
	# "is empty (cargo rebuild race)". Nextest avoids the rebuild because
	# it does its own dependency lifecycle management and doesn't re-link
	# dependent binaries while running tests.
	$(CARGO) nextest run --manifest-path $(ROOT)Cargo.toml --test scenarios_runner --release

test-fixtures: build-release ## Run mp validate on hand-crafted fixtures
	@echo "==> minimal-ready"
	@bash $(ROOT)scripts/validate-fixture-copy.sh minimal-ready
	@echo "==> walkthrough-oauth"
	@bash $(ROOT)scripts/validate-fixture-copy.sh walkthrough-oauth
	@echo "==> hybrid-work (.mp)"
	@bash $(ROOT)scripts/validate-fixture-copy.sh hybrid-work .mp
	@echo "==> linear-deps"
	@bash $(ROOT)scripts/validate-fixture-copy.sh linear-deps
	@echo "==> gate-g1-fail (expect validation errors)"
	@bash $(ROOT)scripts/validate-fixture-copy.sh gate-g1-fail "" failure

.PHONY: clean
clean: ## cargo clean
	$(CARGO) clean --manifest-path $(ROOT)Cargo.toml

.PHONY: clean-test-bins
clean-test-bins: ## remove mp test-binary snapshots under TMPDIR (shared dir + legacy per-PID trees)
	@tmpdir="$$(dirname $$(mktemp -u))"; \
	  rm -rf "$$tmpdir/mp-test-binaries" "$$tmpdir"/mp-test-binaries-* 2>/dev/null || true; \
	  echo "==> removed mp-test-binaries{,-*} snapshots from $$tmpdir"

.PHONY: verify-lint
verify-lint: build-release ## M110 (S2): soft lint via `mp plan verify-lint`; exits 0 (WARN-only; requires release build)
	@MP_HOME=$(ROOT) $(TARGET) plan verify-lint

.PHONY: adopt-check
adopt-check: build-release ## Validate v1 adoption paths (full + hybrid)
	@echo "==> toolkit repo (full profile, master-plan/)"
	@MP_HOME=$(ROOT) $(TARGET) doctor --format json
	@MP_HOME=$(ROOT) $(TARGET) validate
	@echo "==> hybrid-work fixture (.mp/)"
	@bash $(ROOT)scripts/validate-fixture-copy.sh hybrid-work .mp

.PHONY: dep-audit dep-audit-raul design-check
dep-audit-raul: ## raul dependency audit (single crossterm, no comfy-table, <=100 transitive)
	@bash $(ROOT)scripts/audit-raul-deps.sh

dep-audit: ## Dependency surface audit (transitive count gate + explicit features)
	@echo "==> direct runtime deps (crates/mp/Cargo.toml):"
	@awk '/^\[dependencies\]/{f=1;next}/^\[/{f=0}f && /^[a-z]/' $(ROOT)crates/mp/Cargo.toml | sed 's/^/  /'
	@echo "==> transitive unique crates:"
	@n=$$(cargo tree -p mp --prefix none 2>/dev/null | sort -u | wc -l); echo "  $$n"; \
		test "$$n" -le 150 || { echo "  FAIL: $$n > 150 (target <=150 pre-Phase-2)"; exit 1; }
	@echo "==> deps using defaults (no features=):"
	@awk '/^\[dependencies\]/{f=1;next}/^\[/{f=0}f && /^[a-z]/ && !/features/ && !/^mp-model/ && !/^walkdir/ && !/^include_dir/ {print "  - "$$1}' $(ROOT)crates/mp/Cargo.toml || true
	@echo "==> aws-lc-sys present (should be 0):"
	@cargo tree -p mp --prefix none 2>/dev/null | grep -c aws-lc-sys || true

design-check: ## Verify ARCHITECTURE.md module map matches src/ structure
	@echo "==> extracting root-level module list from ARCHITECTURE.md (section 1)..."
	@sed -n '/^## 1\. Module map/,/^## [0-9]/p' $(ROOT)ARCHITECTURE.md \
		| grep -E '^\| `[a-z]' | sed 's/| `//;s/`.*//;s/\.rs$$//' \
		| grep -v '^commands/' | sort > /tmp/arch-modules.txt
	@echo "==> listing actual src/*.rs modules..."
	@find $(ROOT)crates/mp/src -maxdepth 1 -name '*.rs' ! -name 'lib.rs' \
		| sed 's|.*/||;s|\.rs$$||' | sort > /tmp/src-root.txt
	@echo "==> comparing..."
	@mismatch=0; \
	diff /tmp/arch-modules.txt /tmp/src-root.txt > /tmp/arch-diff.txt 2>&1 || mismatch=1; \
	if [ "$$mismatch" -eq 1 ]; then \
		echo "MISMATCH (ARCHITECTURE.md vs src/):"; \
		cat /tmp/arch-diff.txt; \
		echo "Run 'make design-check' after adding/removing a module."; \
		exit 1; \
	fi; \
	echo "OK — module map matches src/."

.PHONY: doctor
doctor: build-release ## mp doctor (dev: MP_HOME = repo root)
	MP_HOME=$(ROOT) $(TARGET) doctor

.PHONY: dev-env
dev-env: ## Print shell exports for local development
	@echo 'export MP_HOME="$(ROOT)"'
	@echo 'export PATH="$(ROOT)target/release:$$PATH"'

.PHONY: install-global install-opencode install-cursor install-pi install uninstall

install-global: build-release ## Install toolkit only (~/.agents/master-plan, no harness skills)
	@$(INSTALL_JSON) --toolkit-only | $(SUMMARY)

install-opencode: build-release ## Install toolkit + OpenCode harness (master-planner + spec-grill)
	@$(INSTALL_JSON) --harness opencode | $(SUMMARY)

install-cursor: build-release ## Install toolkit + Cursor harness
	@$(INSTALL_JSON) --harness cursor | $(SUMMARY)

install-pi: build-release ## Install toolkit + Pi harness (pi.dev)
	@$(INSTALL_JSON) --harness pi | $(SUMMARY)

install: build-release ## Full install: toolkit + OpenCode + Cursor + Pi (v1 harness trio)
	@$(INSTALL_JSON) --harness $(INSTALL_HARNESSES) | $(SUMMARY)
	@codesign --force --deep --sign - "$(INSTALL_DIR)/bin/mp" 2>/dev/null || true
	@codesign --force --deep --sign - "$(INSTALL_DIR)/bin/raul" 2>/dev/null || true
	@echo ""
	@echo "Done. Verify: mp doctor"
	@echo "Docs: docs/concepts/02 - Getting Started/INSTALL.md"

uninstall: build-release ## Remove global install (toolkit + all harness artifacts)
	@$(MP_INSTALL_ENV) $(TARGET) uninstall --purge --format json
	@echo "Removed global install under $(INSTALL_DIR)"
