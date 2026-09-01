//! M201: typed `mp config schema` parser for the Settings lane.
//!
//! The runner caches the JSON from `mp config schema` into a
//! `SettingsSchema` once at lane-open. The renderer reads from the
//! cache on every frame — `mp config schema` is NOT re-run on
//! redraw. The schema is the single source of truth for per-key
//! type, default, allowed, and description; `help.rs` was deleted
//! in M201 WP4.
//!
//! The wire shape (mp side):
//! ```json
//! {
//!   "$schema_version": "1.0",
//!   "keys": [
//!     { "key": "ui.color", "type": "bool", "default": "true", "description": "..." },
//!     { "key": "ui.theme", "type": "choice", "default": "mocha",
//!       "allowed": ["mocha", "macchiato", "frappe", "latte", "dracula"],
//!       "description": "..." },
//!     ...
//!   ]
//! }
//! ```

use std::collections::BTreeMap;

use serde::Deserialize;

/// M201: mirror of the JSON entry in `mp config schema`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SchemaEntry {
    pub key: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub default: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<String>>,
    pub description: String,
}

/// M201: top-level payload from `mp config schema`.
#[derive(Debug, Clone, Deserialize)]
pub struct SchemaPayload {
    #[serde(rename = "$schema_version")]
    pub schema_version: String,
    pub keys: Vec<SchemaEntry>,
}

/// M201: typed wrapper around the schema payload. Holds both the
/// ordered vec (so the renderer can paint in the canonical order)
/// and a per-key lookup map for O(1) access.
#[derive(Debug, Clone)]
pub struct SettingsSchema {
    pub version: String,
    pub entries: Vec<SchemaEntry>,
    by_key: BTreeMap<String, usize>,
}

impl SettingsSchema {
    /// Parse the raw JSON from `mp config schema` and build the
    /// lookup map. Returns `Err` on malformed JSON — the caller
    /// surfaces a clear error and aborts the lane-open path.
    pub fn from_json(raw: &[u8]) -> Result<Self, String> {
        let payload: SchemaPayload = serde_json::from_slice(raw)
            .map_err(|e| format!("invalid mp config schema JSON: {e}"))?;
        let mut by_key = BTreeMap::new();
        for (i, e) in payload.keys.iter().enumerate() {
            by_key.insert(e.key.clone(), i);
        }
        Ok(Self {
            version: payload.schema_version,
            entries: payload.keys,
            by_key,
        })
    }

    /// Look up an entry by key.
    pub fn get(&self, key: &str) -> Option<&SchemaEntry> {
        self.by_key.get(key).map(|&i| &self.entries[i])
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the schema has zero entries (e.g. an older mp that
    /// emitted a different shape). Used by the renderer to detect
    /// the "schema command missing" path.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over the canonical entry list.
    pub fn iter(&self) -> impl Iterator<Item = &SchemaEntry> {
        self.entries.iter()
    }
}

/// M201: fetch the schema from `mp config schema` and parse it.
/// `runner` is the standard `MpRunner`; the schema subcommand lives
/// on `mp` itself (no `ral`-side knowledge required).
pub fn fetch_schema(runner: &crate::mp_runner::MpRunner) -> Result<SettingsSchema, String> {
    let raw = runner
        .run_raw("config", &["schema"])
        .map_err(|e| format!("mp config schema unavailable: {e}"))?;
    SettingsSchema::from_json(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> &'static str {
        r#"{
            "$schema_version": "1.0",
            "keys": [
                {"key": "ui.color", "type": "bool", "default": "true",
                 "description": "ANSI color."},
                {"key": "ui.theme", "type": "choice", "default": "mocha",
                 "allowed": ["mocha", "latte"], "description": "Theme."},
                {"key": "keybinds.refresh", "type": "keybind",
                 "default": "Ctrl-R", "description": "Refresh."}
            ]
        }"#
    }

    #[test]
    fn from_json_parses_payload_and_builds_lookup() {
        let schema = SettingsSchema::from_json(sample_payload().as_bytes()).unwrap();
        assert_eq!(schema.version, "1.0");
        assert_eq!(schema.len(), 3);

        let refresh = schema.get("keybinds.refresh").unwrap();
        assert_eq!(refresh.ty, "keybind");
        assert_eq!(refresh.default, "Ctrl-R");

        let theme = schema.get("ui.theme").unwrap();
        assert_eq!(
            theme.allowed.as_deref(),
            Some(&["mocha".to_string(), "latte".to_string()][..])
        );

        let color = schema.get("ui.color").unwrap();
        assert!(color.allowed.is_none(), "bool row must not carry `allowed`");

        assert!(schema.get("nonexistent.key").is_none());
    }

    #[test]
    fn from_json_rejects_malformed_payload() {
        let bad = "{not json";
        let err = SettingsSchema::from_json(bad.as_bytes()).unwrap_err();
        assert!(err.contains("invalid"), "got: {err}");
    }

    #[test]
    fn from_json_rejects_missing_required_fields() {
        let bad = r#"{"$schema_version": "1.0", "keys": [{"key": "ui.color"}]}"#;
        let err = SettingsSchema::from_json(bad.as_bytes()).unwrap_err();
        assert!(err.contains("invalid"), "got: {err}");
    }
}
