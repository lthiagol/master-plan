//! M151 — Harness command registry.
//!
//! `mp watch` invokes external agent harnesses via
//! `herdr agent start <pane> -- <harness_argv...>`. The v1 surface is
//! [opencode, pi, cursor]; the registry is the single source of truth
//! for the harness command and its model / thinking-level flag
//! translation. Both `mp watch` (the runtime consumer) and the new
//! `mp agent harness ...` CLI surface read from the same struct so
//! future harnesses light up in one place.
//!
//! Future expansions (herdr ships 14+ integrations today) are
//! additive: another `HarnessEntry` plus a v2-or-later registry
//! constant. Unknown harnesses surface a structured
//! [`HarnessError::Unsupported`] that suggests
//! `herdr integration install <name>` — the on-ramp until a registry
//! entry exists.
//!
//! Layering:
//! - Data: [`HarnessEntry`] (`Copy`-able static data, one per v1
//!   harness) + [`HarnessRegistry`] (owns the entries).
//! - Pure helpers: [`HarnessRegistry::v1`], `::iter`,
//!   [`HarnessRegistry::supported_names`], [`HarnessRegistry::get`],
//!   [`HarnessRegistry::resolve_argv`].
//! - Errors: [`HarnessError`] (`Unsupported`/`Other`). Display
//!   formats the supported-names list and the install hint so
//!   library callers and the CLI get the same human message.

use std::fmt;

use serde::Serialize;

/// One v1 harness entry. Static, `Copy`-able data; the registry owns
/// the canonical `Vec<HarnessEntry>`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct HarnessEntry {
    /// Canonical harness id used in CLI / config (e.g. `opencode`).
    pub id: &'static str,
    /// Human-friendly label for `mp agent harness list`.
    pub display_name: &'static str,
    /// Executable name that follows the `herdr agent start ... --`
    /// separator (e.g. `opencode`). The actual binary this resolves
    /// to is whatever is on `PATH`.
    pub command: &'static str,
    /// Flag that accepts the model name (e.g. `--model`). When
    /// `None`, the harness does not expose a model flag on its CLI;
    /// callers fall back to env vars or the harness's own defaults.
    pub model_flag: Option<&'static str>,
    /// Flag that accepts a thinking-level label (e.g. `--thinking`,
    /// `--reasoning-effort`). `None` means the v1 surface does not
    /// translate a thinking flag for this harness.
    pub thinking_flag: Option<&'static str>,
}

/// Single-source-of-truth for v1 harness entries. **Ids are declared as
/// string literals here**; `SUPPORTED_NAMES` derives element-wise
/// below. Adding a new harness is a one-line edit to `V1_ENTRIES`
/// followed by extending the `SUPPORTED_NAMES` re-projection in the
/// same commit — the integration tests in `tests/harness_registry.rs`
/// (`v1_registry_lists_opencode_pi_cursor`,
/// `registry_iter_yields_one_entry_per_v1_harness`,
/// `registry_is_the_single_source_for_supported_set`) pin the lockstep.
const V1_ENTRIES: &[HarnessEntry] = &[
    HarnessEntry {
        id: "opencode",
        display_name: "OpenCode",
        command: "opencode",
        // OpenCode's CLI accepts `--model <name>` (verified against
        // its `opencode --help` output). Thinking level is not a CLI
        // flag in v1 — the harness surfaces it via config.
        model_flag: Some("--model"),
        thinking_flag: None,
    },
    HarnessEntry {
        id: "pi",
        display_name: "Pi",
        command: "pi",
        // Pi accepts `--model <name>` for the model selector; no
        // thinking flag in the v1 surface.
        model_flag: Some("--model"),
        thinking_flag: None,
    },
    HarnessEntry {
        id: "cursor",
        display_name: "Cursor",
        command: "cursor",
        // `cursor-agent` (the harness CLI behind the v1 `cursor`
        // alias) accepts both `--model <name>` and
        // `--thinking <level>` flags. See M151 design decision:
        // per-harness flag translation is data-driven.
        model_flag: Some("--model"),
        thinking_flag: Some("--thinking"),
    },
];

/// Single-source-of-truth for the v1 supported-harness id list,
/// derived from `V1_ENTRIES` element-wise. The `&[&str]` slice is
/// `const`-evaluable because each entry's `id` is a `&'static str`
/// literal — we index into the static entry array at compile time.
/// Re-exported as `crate::config::WATCH_HARNESSES` so `mp config set`
/// validation, the `mp watch` precondition gate, and
/// `mp agent harness` all read from the same list. Compile-time
/// check: if a harness is added to `V1_ENTRIES` without extending
/// the index list below, the runtime `supported_names()` will still
/// reflect it (it iterates `V1_ENTRIES`), but this `const` will be
/// stale — the integration tests catch that drift.
pub const SUPPORTED_NAMES: &[&str] = &[V1_ENTRIES[0].id, V1_ENTRIES[1].id, V1_ENTRIES[2].id];

/// Owning handle over a slice of harness entries. Cheap to clone
/// (the entries are static `Copy` data; only the slice lives in
/// the struct).
#[derive(Debug, Clone, Copy)]
pub struct HarnessRegistry {
    entries: &'static [HarnessEntry],
}

impl HarnessRegistry {
    /// The v1 registry: opencode, pi, cursor. Single source for
    /// `mp watch`, `mp agent harness ...`, and the precondition
    /// check at startup.
    pub fn v1() -> Self {
        Self {
            entries: V1_ENTRIES,
        }
    }

    /// Borrowed iterator over the entries. Linear scan over the
    /// static slice — fine for the v1 trio (3 entries); a v2 with
    /// more harnesses can swap in a `HashMap<&'static str, …>` here
    /// without touching the call sites.
    pub fn iter(&self) -> std::slice::Iter<'_, HarnessEntry> {
        self.entries.iter()
    }

    /// Stable list of supported harness ids (e.g. `["opencode", "pi",
    /// "cursor"]`). Used for unknown-harness error formatting and
    /// by `mp config set`'s validation gate. Cheap (allocates one
    /// `Vec` only on error paths and config-set validation).
    pub fn supported_names(&self) -> Vec<&'static str> {
        self.entries.iter().map(|e| e.id).collect()
    }

    /// Look up an entry by id. Returns
    /// [`HarnessError::Unsupported`] when the harness is not in the
    /// registry — the message lists the supported names and a hint
    /// about `herdr integration install <name>`.
    pub fn get(&self, name: &str) -> Result<&HarnessEntry, HarnessError> {
        self.entries
            .iter()
            .find(|e| e.id == name)
            .ok_or_else(|| HarnessError::Unsupported {
                name: name.to_string(),
                supported: self.supported_names(),
            })
    }

    /// Translate `(harness_name, model?, thinking?)` to the argv
    /// that follows the `herdr agent start ... --` separator. The
    /// model / thinking flags are appended only when both the
    /// entry *and* the caller supply a value — a `None` from the
    /// caller side leaves the flag off the argv even if the entry
    /// supports it.
    pub fn resolve_argv(
        &self,
        name: &str,
        model: Option<&str>,
        thinking: Option<&str>,
    ) -> Result<Vec<String>, HarnessError> {
        let entry = self.get(name)?;
        let mut argv = Vec::with_capacity(5);
        argv.push(entry.command.to_string());
        if let (Some(flag), Some(m)) = (entry.model_flag, model) {
            argv.push(flag.to_string());
            argv.push(m.to_string());
        }
        if let (Some(flag), Some(t)) = (entry.thinking_flag, thinking) {
            argv.push(flag.to_string());
            argv.push(t.to_string());
        }
        Ok(argv)
    }
}

/// Errors surfaced by the registry. Today only
/// [`HarnessError::Unsupported`] is constructed — the registry has
/// exactly one failure mode (looking up an unknown id). New
/// variants can be added without breaking match sites (callers
/// should wildcard or pattern on `Unsupported` only). Implements
/// `std::error::Error` manually so the crate does not pull in
/// `thiserror` (anyhow is the project's standard error type — the
/// registry returns owned errors that both anyhow and the CLI
/// hand-off can format).
#[derive(Debug)]
pub enum HarnessError {
    Unsupported {
        name: String,
        supported: Vec<&'static str>,
    },
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HarnessError::Unsupported { name, supported } => write!(
                f,
                "unsupported harness '{name}'; supported: {supported_list}. \
                 To register a new harness, run: herdr integration install {name}",
                supported_list = supported.join("|"),
            ),
        }
    }
}

impl std::error::Error for HarnessError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> HarnessRegistry {
        HarnessRegistry::v1()
    }

    #[test]
    fn v1_registry_holds_three_entries() {
        let names = reg().supported_names();
        assert_eq!(names, vec!["opencode", "pi", "cursor"]);
    }

    #[test]
    fn get_returns_each_v1_entry_by_id() {
        let r = reg();
        for id in ["opencode", "pi", "cursor"] {
            let e = r.get(id).expect("v1 harness must resolve");
            assert_eq!(e.id, id, "id must round-trip");
            assert!(!e.command.is_empty(), "{id} must declare a command");
        }
    }

    #[test]
    fn get_unknown_harness_lists_supported_and_install_hint() {
        let err = reg().get("claude-code").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("claude-code"),
            "error must name the offending harness: {msg}"
        );
        assert!(
            msg.contains("opencode") && msg.contains("pi") && msg.contains("cursor"),
            "error must list all supported harnesses: {msg}"
        );
        assert!(
            msg.contains("herdr integration install claude-code"),
            "error must point at the on-ramp install command: {msg}"
        );
    }

    #[test]
    fn get_unknown_harness_never_panics_on_bizarre_input() {
        let r = reg();
        for name in ["", "OPENCODE", " opencode", "open code", "🎉"] {
            let result = r.get(name);
            assert!(result.is_err(), "{name:?} should not match a v1 entry");
        }
    }

    #[test]
    fn resolve_argv_returns_just_command_when_no_flags_supplied() {
        assert_eq!(
            reg().resolve_argv("opencode", None, None).unwrap(),
            vec!["opencode".to_string()]
        );
        assert_eq!(
            reg().resolve_argv("pi", None, None).unwrap(),
            vec!["pi".to_string()]
        );
        assert_eq!(
            reg().resolve_argv("cursor", None, None).unwrap(),
            vec!["cursor".to_string()]
        );
    }

    #[test]
    fn resolve_argv_appends_model_flag_when_entry_supports_it() {
        // opencode supports --model.
        assert_eq!(
            reg()
                .resolve_argv("opencode", Some("claude-opus-4"), None)
                .unwrap(),
            vec![
                "opencode".to_string(),
                "--model".to_string(),
                "claude-opus-4".to_string()
            ]
        );
        // cursor supports both.
        assert_eq!(
            reg()
                .resolve_argv("cursor", Some("claude-opus-4"), Some("high"))
                .unwrap(),
            vec![
                "cursor".to_string(),
                "--model".to_string(),
                "claude-opus-4".to_string(),
                "--thinking".to_string(),
                "high".to_string()
            ]
        );
    }

    #[test]
    fn resolve_argv_skips_flag_when_caller_omits_value() {
        // cursor supports thinking, but caller didn't set one.
        assert_eq!(
            reg()
                .resolve_argv("cursor", Some("claude-opus-4"), None)
                .unwrap(),
            vec![
                "cursor".to_string(),
                "--model".to_string(),
                "claude-opus-4".to_string()
            ]
        );
    }

    #[test]
    fn resolve_argv_unknown_harness_returns_structured_error() {
        let err = reg()
            .resolve_argv("aider", Some("claude-opus-4"), None)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("aider") && msg.contains("supported"),
            "unknown harness argv must report the id + 'supported': {msg}"
        );
    }

    #[test]
    fn iter_yields_each_v1_entry_exactly_once() {
        let ids: Vec<&str> = reg().iter().map(|e| e.id).collect();
        assert_eq!(ids, vec!["opencode", "pi", "cursor"]);
    }

    #[test]
    fn harness_entry_serde_uses_static_field_names() {
        // Pin the JSON shape `mp agent harness list` emits. M151
        // spec: JSON shape mirrors `id/display_name/command/
        // model_flag/thinking_flag`. Future changes here must be
        // reflected in agent_harness_cli.rs.
        let r = reg();
        let e = r.get("cursor").unwrap();
        let v = serde_json::to_value(e).unwrap();
        assert_eq!(v["id"], "cursor");
        assert_eq!(v["display_name"], "Cursor");
        assert_eq!(v["command"], "cursor");
        assert_eq!(v["model_flag"], "--model");
        assert_eq!(v["thinking_flag"], "--thinking");
    }
}
