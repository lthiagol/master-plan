//! M218: `mp autopilot start` hard gate — refuse to run when herdr is
//! missing or older than the required floor.
//!
//! herdr is a hard architectural requirement for `mp autopilot start`
//! (and the legacy `mp watch` alias): one pane-set, three roles, and
//! workspace naming all depend on it. Unlike the precondition gate in
//! [`crate::autopilot::drive::preconditions::check_preconditions`] — which only
//! *surfaces* a diagnostic when herdr is unavailable — this gate is a
//! hard refusal that exits 78 (EX_CONFIG) before any session directory
//! is created, any plan state is written, or any spawn operation is
//! invoked.
//!
//! ## Exit code
//!
//! - 78 (EX_CONFIG) when the gate fires. The agent contract on page
//!   `master-plan/AGENTS.md` reserves 78 for "configuration error, not
//!   a runtime error" — distinct from 2 (bulk partial failure / generic
//!   runtime) so shell scripts can branch on exit code alone.
//!
//! ## `--force` does not bypass
//!
//! [`AutopilotCmd::Start`] accepts `--force`, but the design decision
//! in the milestone spec is that `--force` keeps its existing role
//! (bypass the M178 double-spawn guard) and does NOT bypass the herdr
//! gate. herdr is required by design, not by convenience.
//!
//! ## Why a separate module (vs adding to `preconditions.rs`)
//!
//! The existing precondition gate runs AFTER the lazy auto-set
//! fallback has potentially written `config.json`; the autopilot
//! contract needs the refusal to fire BEFORE any write. Putting the
//! hard gate in its own module keeps the diagnostic language distinct
//! ("autopilot hard gate" vs "watch precondition") and lets the
//! autopilot surface emit its own JSON report shape without coupling
//! to the watch driver's `DriveReport`.
//!
//! ## Shared code path
//!
//! Both `mp autopilot start` and `mp watch` (legacy alias) call
//! [`check_autopilot_herdr_gate_default`] from the same code path —
//! the legacy `mp watch` no longer has its own herdr probe; the
//! autopilot's hard gate is the only one. AC-03 pins this contract.

use std::path::Path;

use serde::Serialize;

use crate::autopilot::drive::herdr_version::{
    HerdrCliShape, VersionFloor, REQUIRED_HERDR_VERSION_FLOOR,
};

/// Exit code reserved by the agent contract for configuration errors
/// (sysexits.h `EX_CONFIG`). Distinct from 2 (bulk partial failure /
/// generic runtime) so scripts can branch on exit code alone.
pub const EX_AUTOPILOT_GATE: i32 = 78;

/// Install hint surfaced when herdr is not on PATH. Hardcoded because
/// the upstream install URL is the only supported on-ramp — there is no
/// package-manager story for herdr yet.
pub const HERDR_INSTALL_HINT: &str =
    "install herdr from https://herdr.dev/docs/install (or `brew install herdr` when available)";

/// Typed reason for the gate refusal. The diagnostic + JSON report
/// branch on this string so the operator (or an upstream script) can
/// tell missing from incompatible from below-floor without parsing the
/// human-readable message.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GateReason {
    /// `which herdr` returned nothing.
    HerdrMissing,
    /// herdr is on PATH but its version string is below
    /// [`REQUIRED_HERDR_VERSION_FLOOR`].
    HerdrBelowFloor,
    /// herdr is on PATH and reports a compatible version, but its
    /// `agent start --help` does not list every flag in
    /// [`crate::autopilot::drive::herdr_version::EXPECTED_START_FLAGS`] or its
    /// `pane split --help` returned no output (i.e. the pane split
    /// subcommand is missing). This is the "shape mismatch" path —
    /// version says OK but the wire shape has drifted.
    HerdrIncompatibleShape,
}

/// Structured report surfaced by the hard gate. Serialized to JSON
/// before the binary exits 78, so downstream tooling sees both the
/// reason and the action in one payload.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AutopilotGateError {
    pub reason: GateReason,
    pub exit_code: i32,
    /// Detected version string (e.g. `"0.6.0"`). `None` when herdr is
    /// not on PATH or its version string is unparseable.
    pub detected_version: Option<String>,
    /// The required minimum (mirrors [`REQUIRED_HERDR_VERSION_FLOOR`]).
    pub required_version: &'static str,
    /// Flags the gate found missing from `agent start --help`. Empty
    /// when the failure is `herdr_missing` or `herdr_below_floor`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_flags: Vec<String>,
    pub install_hint: String,
    pub upgrade_hint: String,
    /// Human-readable single-line summary. Suitable for stderr when the
    /// caller prints a tight error, or the `message` field of a JSON
    /// `error` envelope.
    pub message: String,
}

/// Run the autopilot hard gate against the supplied herdr binary path.
/// Returns `Ok(())` when herdr is on PATH AND reports a version
/// string AND has the expected spawn shape. Returns
/// [`Box<AutopilotGateError>`] otherwise (boxed so the Result stays
/// small — clippy `result_large_err` triggers at >128 bytes; the
/// gate error carries multi-paragraph messages + a flag list).
///
/// Pure over `herdr_bin` — the helper used by tests that want to
/// inject a fake binary. The [`check_autopilot_herdr_gate_default`]
/// wrapper resolves `which herdr` for the production call sites.
pub fn check_autopilot_herdr_gate(herdr_bin: &Path) -> Result<(), Box<AutopilotGateError>> {
    let shape = crate::autopilot::drive::detect_herdr_cli(herdr_bin);
    if !shape.on_path {
        return Err(Box::new(missing_gate_error()));
    }
    if !shape.compatible {
        return Err(Box::new(incompatible_gate_error(&shape)));
    }
    Ok(())
}

/// Convenience wrapper: resolve the herdr binary via
/// [`crate::autopilot::drive::herdr::which_herdr`] and run the gate. When herdr
/// is not on PATH, returns the [`GateReason::HerdrMissing`] error
/// directly without probing a binary (saves one fork/exec pair on
/// the failure path; keeps the missing-binary diagnostic clean).
pub fn check_autopilot_herdr_gate_default() -> Result<(), Box<AutopilotGateError>> {
    match crate::autopilot::drive::herdr::which_herdr() {
        Some(bin) => check_autopilot_herdr_gate(&bin),
        None => Err(Box::new(missing_gate_error())),
    }
}

/// Build the [`GateReason::HerdrMissing`] diagnostic.
fn missing_gate_error() -> AutopilotGateError {
    let msg = format!(
        "mp autopilot refuses to start: herdr is not on PATH. {}",
        HERDR_INSTALL_HINT
    );
    AutopilotGateError {
        reason: GateReason::HerdrMissing,
        exit_code: EX_AUTOPILOT_GATE,
        detected_version: None,
        required_version: REQUIRED_HERDR_VERSION_FLOOR,
        missing_flags: Vec::new(),
        install_hint: HERDR_INSTALL_HINT.to_string(),
        upgrade_hint: String::new(),
        message: msg,
    }
}

/// Build the gate error from a shape that probed as `compatible=false`.
/// Branches on [`HerdrCliShape`] to pick the typed reason:
/// - version string present but below floor → `HerdrBelowFloor`
/// - everything else (missing flags, missing `pane split`,
///   unparseable version) → `HerdrIncompatibleShape`
fn incompatible_gate_error(shape: &HerdrCliShape) -> AutopilotGateError {
    let detected = shape.parsed_version.as_deref();
    let reason = match (detected, shape.version_floor) {
        (Some(v), VersionFloor::Below) => {
            // The version parse path is the strongest signal — herdr
            // self-reported as below 0.7.0, so the upgrade hint names
            // the detected version. The shape probe can ALSO fail
            // simultaneously (older 0.6.x also lacks `pane split`)
            // but the message keeps the version up top so the
            // operator sees the cause first.
            let _ = v;
            GateReason::HerdrBelowFloor
        }
        _ => GateReason::HerdrIncompatibleShape,
    };
    let upgrade_hint = build_upgrade_hint(detected, &shape.missing_flags);
    let message = build_incompatible_message(shape, &upgrade_hint);
    AutopilotGateError {
        reason,
        exit_code: EX_AUTOPILOT_GATE,
        detected_version: detected.map(str::to_string),
        required_version: REQUIRED_HERDR_VERSION_FLOOR,
        missing_flags: shape.missing_flags.clone(),
        install_hint: HERDR_INSTALL_HINT.to_string(),
        upgrade_hint,
        message,
    }
}

fn build_upgrade_hint(detected: Option<&str>, missing: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = detected {
        parts.push(format!(
            "detected herdr {v}; upgrade to ≥ {REQUIRED_HERDR_VERSION_FLOOR}"
        ));
    } else {
        parts.push(format!(
            "install a herdr that reports ≥ {REQUIRED_HERDR_VERSION_FLOOR}"
        ));
    }
    if !missing.is_empty() {
        parts.push(format!(
            "missing `agent start` flags: [{}]; rebuild herdr against the 0.7.x contract",
            missing.join(", ")
        ));
    }
    parts.join("; ")
}

fn build_incompatible_message(shape: &HerdrCliShape, upgrade_hint: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "mp autopilot refuses to start: herdr does not satisfy the hard gate (≥ {REQUIRED_HERDR_VERSION_FLOOR})"
    ));
    if let Some(v) = shape.parsed_version.as_deref() {
        parts.push(format!("detected herdr {v}"));
    }
    if !shape.missing_flags.is_empty() {
        parts.push(format!(
            "missing `agent start` flags: [{}]",
            shape.missing_flags.join(", ")
        ));
    }
    if shape.pane_help_output.trim().is_empty() {
        parts.push("`pane split` subcommand is not available (required by 0.7.x)".to_string());
    }
    parts.push(upgrade_hint.to_string());
    parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_gate_error_carries_install_hint() {
        let err = missing_gate_error();
        assert_eq!(err.reason, GateReason::HerdrMissing);
        assert_eq!(err.exit_code, EX_AUTOPILOT_GATE);
        assert!(err.install_hint.contains("install"));
        assert!(err.install_hint.contains("herdr"));
        assert!(err.message.contains("not on PATH"));
        assert!(err.message.contains(&err.install_hint));
        assert_eq!(err.required_version, REQUIRED_HERDR_VERSION_FLOOR);
        assert!(err.upgrade_hint.is_empty());
    }

    #[test]
    fn incompatible_message_names_detected_version_and_missing_flags() {
        let shape = HerdrCliShape {
            compatible: false,
            on_path: true,
            version_output: "herdr 0.6.0".into(),
            parsed_version: Some("0.6.0".into()),
            version_floor: VersionFloor::Below,
            start_help_output: String::new(),
            pane_help_output: String::new(),
            missing_flags: vec!["--kind".into(), "--pane".into()],
            message: "prior".into(),
        };
        let err = incompatible_gate_error(&shape);
        assert_eq!(err.reason, GateReason::HerdrBelowFloor);
        assert_eq!(err.detected_version.as_deref(), Some("0.6.0"));
        assert_eq!(
            err.missing_flags,
            vec!["--kind".to_string(), "--pane".to_string()]
        );
        assert!(err.message.contains("0.6.0"));
        assert!(err.message.contains("0.7.0"));
        assert!(err.upgrade_hint.contains("0.6.0"));
        assert!(err.upgrade_hint.contains("--kind"));
    }

    #[test]
    fn shape_mismatch_without_version_uses_incompatible_shape_reason() {
        let shape = HerdrCliShape {
            compatible: false,
            on_path: true,
            version_output: String::new(),
            parsed_version: None,
            version_floor: VersionFloor::Unknown,
            start_help_output: "Options:\n  --harness <HARNESS>\n".into(),
            pane_help_output: String::new(),
            missing_flags: vec!["--kind".into(), "--pane".into()],
            message: "prior".into(),
        };
        let err = incompatible_gate_error(&shape);
        assert_eq!(err.reason, GateReason::HerdrIncompatibleShape);
        assert_eq!(err.detected_version, None);
        assert!(err.upgrade_hint.contains("0.7.0"));
    }

    #[test]
    fn upgrade_hint_includes_missing_flags_when_shape_drifted() {
        let hint = build_upgrade_hint(Some("0.6.0"), &["--kind".into(), "--pane".into()]);
        assert!(hint.contains("0.6.0"));
        assert!(hint.contains("0.7.0"));
        assert!(hint.contains("--kind"));
        assert!(hint.contains("--pane"));
    }

    #[test]
    fn upgrade_hint_without_detected_version_still_pins_floor() {
        let hint = build_upgrade_hint(None, &[]);
        assert!(hint.contains("0.7.0"));
        assert!(hint.contains("install"));
    }

    #[test]
    fn exit_code_constant_matches_sysexits_ex_config() {
        // sysexits.h: EX_CONFIG = 78. If a future refactor changes
        // the constant, this test fails so the agent contract (78 =
        // configuration error) cannot silently drift.
        assert_eq!(EX_AUTOPILOT_GATE, 78);
    }

    #[test]
    fn required_version_floor_is_pinned() {
        // F-05 pattern from herdr_version.rs: pin the floor so the
        // agent contract and the version-check literal cannot
        // disagree. Update this if you intentionally bump the
        // floor; the agent contract requires updating in lockstep.
        assert_eq!(REQUIRED_HERDR_VERSION_FLOOR, "0.7.0");
    }
}
