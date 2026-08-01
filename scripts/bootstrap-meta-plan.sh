#!/usr/bin/env bash
# Bootstrap the repo meta master-plan/ (dogfooding). Idempotent: run on fresh `mp init`.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MP="${MP:-$ROOT/target/debug/mp}"
export MP_HOME="$ROOT"
export MP_PROJECT="$ROOT"

id_from() {
  python3 -c "import json,sys; print(json.load(sys.stdin)['milestone']['id'])"
}

create_ms() {
  "$MP" milestone create --json "$1" --format json | id_from
}

complete_ms() {
  local id="$1"
  "$MP" milestone approve "$id" --format json >/dev/null
  "$MP" milestone complete "$id" --evidence "shipped; cargo test -p mp green" --format json >/dev/null
}

"$MP" plan set \
  --name "master-plan toolkit" \
  --description "Spec-driven planning CLI and agent skill (crates/mp)" \
  --stack "Rust,TOML" \
  --target-version "0.2" \
  --planning-status in-execution \
  --planning-phase execution \
  --format json >/dev/null

"$MP" plan goals add "Ship agent-ready mp CLI for the full planning pipeline" --format json >/dev/null
"$MP" plan goals add "Dogfood master-plan/ in this repo to track toolkit work" --format json >/dev/null
"$MP" plan nongoals add "Maintain bash legacy workflow" --format json >/dev/null

"$MP" metrics set --unit-tests 4 --integration-tests 52 --format json >/dev/null

M1=$(create_ms '{
  "title": "CLI foundation",
  "slug": "cli-foundation",
  "effort": "L",
  "intent": { "outcome": "Core mp commands: init, validate, milestones, tracks, archive." },
  "problem": { "description": "Rust P0–P2: query layer and lifecycle basics." },
  "scope": { "in_scope": ["init", "doctor", "validate", "tracks", "archive"], "out_of_scope": ["brief", "brownfield"] },
  "acceptance_criteria": [{ "description": "Foundation tests pass", "verification": "make test" }]
}')
complete_ms "$M1"

M2=$(create_ms '{
  "title": "Adoption and PM surface",
  "slug": "adoption-pm",
  "depends_on": ["'"$M1"'"],
  "effort": "L",
  "intent": { "outcome": "Profiles, brief, ideas, session, inbox, graph, charter." },
  "problem": { "description": "WS1–WS4 adoption + PM commands (P3)." },
  "scope": { "in_scope": ["profiles", "brief", "ideas", "session", "inbox"], "out_of_scope": ["delta specs", "export views"] },
  "acceptance_criteria": [{ "description": "WS integration tests", "verification": "cargo test -p mp" }]
}')
complete_ms "$M2"

M3=$(create_ms '{
  "title": "Brownfield and execution",
  "slug": "brownfield-execution",
  "depends_on": ["'"$M2"'"],
  "effort": "L",
  "intent": { "outcome": "Domain specs, path engine, execution handoff, challenge." },
  "problem": { "description": "P4 brownfield + P1.8–P1.9 execution path." },
  "scope": { "in_scope": ["specs", "brownfield", "path", "execution", "challenge"], "out_of_scope": ["export", "meta plan"] },
  "acceptance_criteria": [{ "description": "p4_brownfield tests", "verification": "cargo test -p mp --test p4_brownfield" }]
}')
complete_ms "$M3"

M4=$(create_ms '{
  "title": "Promote ladder and publish",
  "slug": "promote-publish",
  "depends_on": ["'"$M3"'"],
  "effort": "M",
  "intent": { "outcome": "Promote flows, export, git, sync, split, doc alignment." },
  "problem": { "description": "P5–P12: polish and promotion closure." },
  "scope": { "in_scope": ["promote", "export", "git", "sync", "split", "brief promote"], "out_of_scope": ["brief reopen", "auto push"] },
  "acceptance_criteria": [{ "description": "52 integration tests", "verification": "cargo test -p mp" }]
}')
complete_ms "$M4"

M5=$(create_ms '{
  "title": "Meta master-plan",
  "slug": "meta-plan",
  "depends_on": ["'"$M4"'"],
  "effort": "S",
  "intent": { "outcome": "Track toolkit work in repo master-plan/; update handoff." },
  "problem": { "description": "Work tracked only in handoff.md — dogfood the product." },
  "scope": { "in_scope": ["master-plan/", "milestones", "backlog", "decisions"], "out_of_scope": ["release CI", "npm publish"] },
  "acceptance_criteria": [{ "description": "mp status reflects plan", "verification": "mp status --format json" }]
}')
"$MP" milestone approve "$M5" --format json >/dev/null
"$MP" milestone set-status "$M5" in-progress --format json >/dev/null

M6=$(create_ms '{
  "title": "Remaining polish",
  "slug": "remaining-polish",
  "depends_on": ["'"$M5"'"],
  "effort": "S",
  "risk": "low",
  "intent": { "outcome": "brief reopen, git.auto_push, path suggest." },
  "problem": { "description": "Last documented-only CLI gaps." },
  "scope": { "in_scope": ["brief reopen", "git.auto_push", "path suggest"], "out_of_scope": ["JSON schema CI"] },
  "acceptance_criteria": [{ "description": "AGENT-READINESS accurate", "verification": "docs review" }]
}')

"$MP" backlog add --desc "mp brief reopen" --source planning --priority medium --format json >/dev/null
"$MP" backlog add --desc "Wire git.auto_push after plan commit" --source planning --priority low --format json >/dev/null
"$MP" backlog add --desc "mp path suggest command" --source planning --priority low --format json >/dev/null

"$MP" decision add \
  --summary "Dogfood master-plan/ for toolkit tracking" \
  --context "AGENTS.md at repo root is session bridge; mp owns plan artifacts" \
  --format json >/dev/null

"$MP" sync --format json >/dev/null
echo "Meta plan ready. Milestones: $M1-$M6 (M5 in-progress)"
