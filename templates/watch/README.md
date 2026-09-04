# `templates/watch/<stage>.md`

The prompt templates the drive loop sends to agent panes — one file
per drivable stage (`execute`, `remediate`). Override resolution lets
users ship per-project replacements without recompiling:

| Override path | Wins over |
|---------------|-----------|
| `<override_dir>/<stage>.md` (caller-supplied) | everything below |
| `<plan_dir>/watch/<stage>.md` (project-local) | the compiled default |
| `templates/watch/<stage>.md` (compiled in via `include_str!`) | nothing — final rung |

`mp watch` logs a `prompt_source` event per stage whose value
distinguishes `override` from `default`
(see `crates/mp/src/autopilot/drive/prompts.rs::TemplateSource::label`).

## Placeholders

Four placeholders are substituted at render time by
`render_body` in `crates/mp/src/autopilot/drive/prompts.rs`:

| Placeholder | Substituted with |
|-------------|-------------------|
| `{header}` | `# mp watch — <stage> M<id>: <title>…`, lifecycle target, and the SAFETY preamble + trust-boundary tags (the XML wrappers `<title>`, `<milestone-id>`, `<ac-list>`, `<step-list>`) |
| `{id}` | The milestone id (e.g. `M149`) |
| `{ac_list}` | Inline `<ac-list>...</ac-list>` rendering of acceptance criteria (xml-escaped) |
| `{step_list}` | Inline `<step-list>...</step-list>` rendering of steps (xml-escaped) |

`{header}` MUST be present in any override file. If an override
file is missing `{header}`, the loader refuses it and falls back
to the compiled default — the SAFETY preamble and trust-boundary
tags are essential for prompt-injection defense (M149 review F-10)
and we will not strip them silently.

## Override file policy (F-12)

Project-local overrides are operator-controlled, but a malformed
path should not hang or exhaust the automation process. The loader
applies the following policy to every override rung (caller
`override_dir` and project-local `plan_dir`):

| Check | Threshold | Diagnostic kind |
|-------|-----------|-----------------|
| File is a regular file | `symlink_metadata().file_type().is_file()` | `not_regular` |
| File size | ≤ `MAX_OVERRIDE_BYTES` (1 MiB, see `crates/mp/src/autopilot/drive/prompts.rs`) | `too_large` |
| Non-empty | `text.trim().is_empty() == false` | `empty` |
| Has `{header}` placeholder | `text.contains("{header}")` | `header_missing` |
| Valid UTF-8 | `String::from_utf8` succeeds | `invalid_utf8` |

A symlink is rejected as `not_regular` without chasing the link —
a future scoped-policy hook can relax this. FIFO / socket /
directory inputs are also rejected as `not_regular`. Refusals are
structured: the live watch loop emits an `override_refused`
JSONL event per refusal and the dry-run preview surfaces the
diagnostic in an `override_diagnostics` array. `NotFound` (no
file at the rung) is the **only** silent case.

`MAX_OVERRIDE_BYTES` is exposed via `mp::autopilot::drive::MAX_OVERRIDE_BYTES`
for callers that want to thread a different cap (tests, hardening
in a future hot loop). The default is 1 MiB; the largest shipped
template is ≈ 1 KiB.

## Override rungs — caller-supplied vs CLI flag

The `<override_dir>` rung is a **library/caller** knob, not a
public `mp watch` CLI flag. Today there is no `--template-override-dir`
on `mp watch`; the rung exists so future harnesses, internal callers,
and tests can plug in their own template directory without touching
the project tree. Operators using `mp watch` interact only with the
project-local rung (`<plan_dir>/watch/<stage>.md`), and that rung
is what the dry-run preview renders against.

## Caveats

- The files in this directory correspond to the stages the drive
  loop can actually reach: `[Execute, Remediate]`. Every stage is
  backed by a file, so template resolution always terminates at a
  file rung — there is no hardcoded-Rust fallback. A file named
  after anything else (for example a pre-cutover
  `<plan_dir>/watch/re-review.md`) is never looked up and has no
  effect.
- Severity values in the templates are `low|medium|high`. The
  older `info|minor|major` (and `minor|major|blocker`) sets are
  no longer accepted by `mp reviews finding add --severity`; the
  CLI rejects them.
- `mp reviews finding add` requires `--category`; templates that
  reference it call this out explicitly. An empty `--phase` is
  treated as `--phase self`, so any template that files a finding
  must pass `--phase` explicitly.
- The AC-pass command is `mp milestone ac pass <id> <ac-id>` (or
  the long form `mp milestone criterion pass`). The legacy
  `mp milestone ac criterion pass` form does not exist.

## Adding a new stage

A new stage is only reachable once
`crates/mp/src/autopilot/drive/state_machine.rs::next_stage` maps a
lifecycle to it. Adding one means updating all of:
1. `next_stage` — otherwise the stage is unreachable dead code.
2. `PromptStage` + `PromptStage::label` / `role`.
3. The new `<stage>.md` file (placeholders documented above).
4. `crates/mp/src/autopilot/drive/prompts.rs::compiled_default` match arm.
5. `crates/mp/src/autopilot/drive/prompts.rs::all_stages`.
6. `crates/mp/tests/watch_template_files.rs::EXPECTED_FILES`.
7. The sentinels in `watch_template_files.rs::compiled_default_contains_unique_file_sentinels`.
8. The stages array in `watch_template_override.rs::overrides_resolve_for_every_externalized_stage`.
