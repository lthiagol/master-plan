//! M173 S7: status parity helpers — every read of `lifecycle` /
//! `spec_status` / `execution_status` from a milestone JSON value
//! routes through these helpers so the TUI badge stays in lock-step
//! with `mp show milestone --fields milestone.lifecycle`.
//!
//! Direct field reads in `crates/raul/src/` outside this module are a
//! finding (see M173 AC-07). The mirror helpers in `mp-model`
//! (`effective_lifecycle`, `effective_execution_status`) operate on
//! typed structs; these helpers operate on `serde_json::Value` because
//! the TUI consumes the JSON output of `mp show milestone`.
//!
//! Contract:
//! - `effective_lifecycle(value)` returns the canonical lifecycle
//!   string. If the JSON carries a non-empty `lifecycle`, trust it
//!   (M100 migration populated it). Otherwise fall back to the
//!   legacy spec_status + execution_status derivation that mirrors
//!   `effective_lifecycle_from_legacy` in `mp-model`.
//! - `effective_execution_status(value)` returns the canonical
//!   execution_status string. Prefers the field; falls back to a
//!   lifecycle-derived value (planned / in-progress / done) plus the
//!   blocked/cancelled/deferred flag overrides.

use serde_json::Value;

/// Read the effective lifecycle from a milestone JSON object.
///
/// Honors the M100 contract: trust the `lifecycle` field when set;
/// only fall back to legacy derivation if the JSON is on the
/// pre-M100 shape (legacy spec/exec present, lifecycle empty or
/// the serde default `"draft"`).
pub fn effective_lifecycle(milestone: &Value) -> String {
    let lifecycle = milestone
        .get("lifecycle")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !lifecycle.is_empty() && lifecycle != "draft" {
        return lifecycle.to_string();
    }
    // Legacy fallback: lifecycle is empty or the serde default
    // "draft", and at least one of the legacy fields is populated.
    let spec = milestone
        .get("spec_status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let exec = milestone
        .get("execution_status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if spec.is_empty() && exec.is_empty() {
        // No data to derive from — return whatever lifecycle said
        // (empty string if absent).
        return lifecycle.to_string();
    }
    mp_model::effective_lifecycle_from_legacy(spec, exec)
}

/// Read the effective execution_status from a milestone JSON object.
///
/// Mirrors `mp_model::MilestoneFile::effective_execution_status`:
/// prefers the canonical field, falls back to lifecycle derivation
/// with the blocked/cancelled/deferred flag overrides.
pub fn effective_execution_status(milestone: &Value) -> String {
    let exec = milestone
        .get("execution_status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !exec.is_empty() {
        return exec.to_string();
    }
    let cancelled = milestone
        .get("cancelled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if cancelled {
        return "cancelled".to_string();
    }
    let blocked = milestone
        .get("blocked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if blocked {
        return "blocked".to_string();
    }
    let deferred = milestone
        .get("deferred")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if deferred {
        return "deferred".to_string();
    }
    let lifecycle = effective_lifecycle(milestone);
    if lifecycle.is_empty() {
        String::new()
    } else {
        mp_model::lifecycle_to_legacy_execution_status(&lifecycle).to_string()
    }
}

/// Read the effective spec_status from a milestone JSON object.
///
/// Mirrors `mp_model::validate::plan::effective_spec_status` byte-for-byte
/// (modulo the JSON Value vs typed struct input): prefers the canonical
/// field, falls back to deriving from lifecycle. The lifecycle→spec_status
/// map MUST stay in lock-step with `validate/plan.rs::effective_spec_status`
/// — divergence here breaks the parity-test contract from M173 AC-07.
pub fn effective_spec_status(milestone: &Value) -> String {
    let spec = milestone
        .get("spec_status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !spec.is_empty() {
        return spec.to_string();
    }
    let lifecycle = effective_lifecycle(milestone);
    if lifecycle.is_empty() {
        String::new()
    } else {
        mp_model::lifecycle_to_legacy_spec_status(&lifecycle).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn effective_lifecycle_prefers_canonical_field() {
        let m = json!({ "lifecycle": "in-progress", "spec_status": "ready", "execution_status": "in-progress" });
        assert_eq!(effective_lifecycle(&m), "in-progress");
    }

    #[test]
    fn effective_lifecycle_derives_when_only_legacy_present() {
        let m = json!({ "spec_status": "verified", "execution_status": "done" });
        // verified → complete via spec; done → complete via exec.
        assert_eq!(effective_lifecycle(&m), "complete");
    }

    #[test]
    fn effective_lifecycle_exec_in_progress_short_circuits() {
        // ER-7: exec-side in-progress wins regardless of spec-side.
        let m = json!({ "spec_status": "verified", "execution_status": "in-progress" });
        assert_eq!(effective_lifecycle(&m), "in-progress");
    }

    #[test]
    fn effective_lifecycle_returns_empty_when_nothing_present() {
        let m = json!({});
        assert_eq!(effective_lifecycle(&m), "");
    }

    #[test]
    fn effective_execution_status_prefers_canonical_field() {
        let m = json!({ "execution_status": "in-progress", "lifecycle": "in-progress" });
        assert_eq!(effective_execution_status(&m), "in-progress");
    }

    #[test]
    fn effective_execution_status_falls_back_to_lifecycle() {
        let m = json!({ "lifecycle": "approved" });
        assert_eq!(effective_execution_status(&m), "planned");
    }

    #[test]
    fn effective_execution_status_derives_blocked_from_flag() {
        let m = json!({ "lifecycle": "in-progress", "blocked": true });
        assert_eq!(effective_execution_status(&m), "blocked");
    }

    #[test]
    fn effective_execution_status_derives_cancelled_from_flag() {
        let m = json!({ "lifecycle": "in-progress", "cancelled": true });
        assert_eq!(effective_execution_status(&m), "cancelled");
    }

    #[test]
    fn effective_spec_status_prefers_canonical_field() {
        let m = json!({ "spec_status": "ready", "lifecycle": "in-progress" });
        assert_eq!(effective_spec_status(&m), "ready");
    }

    #[test]
    fn effective_spec_status_falls_back_to_lifecycle() {
        // M173 F-05 (sub-agent review): groomed → review, NOT
        // groomed. The TUI helper must mirror
        // `validate/plan.rs::effective_spec_status` byte-for-byte.
        let m = json!({ "lifecycle": "groomed" });
        assert_eq!(effective_spec_status(&m), "review");
    }

    #[test]
    fn effective_spec_status_returns_empty_when_nothing_present() {
        let m = json!({});
        assert_eq!(effective_spec_status(&m), "");
    }
}
