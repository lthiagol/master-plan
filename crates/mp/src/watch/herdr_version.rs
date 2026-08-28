//! M197 WP3 / AC-04: herdr CLI version + shape gate.
//!
//! The mp watch spawn shape (`herdr agent start <NAME> --kind <KIND>
//! --pane <ID>` after `herdr pane split --cwd <PATH>`) requires a
//! herdr CLI whose `agent start` subcommand accepts both `--kind`
//! and `--pane` and whose `pane split` subcommand exists. herdr
//! 0.7.x shipped this shape; older herdrs (≤0.6.x) only know the
//! legacy `agent start <NAME> --cwd <root> -- <harness argv>` form.
//!
//! Layering:
//! - Pure: [`HerdCliShape`] describes the compatibility verdict.
//! - Pure: [`expected_flags`] is the contract the rest of `mp
//!   watch` relies on; updating it is a breaking change to the
//!   spawn shape.
//! - I/O: [`detect_herdr_cli`] shells out to `herdr --version` and
//!   `herdr agent start --help`, returning a [`HerdCliShape`] with
//!   the raw stdout and a verdict. Doctor and the watch
//!   precondition gate both call this.
//!
//! Why a gate (not a hard error): the gate surfaces the missing
//! capability in `mp doctor` and `mp watch` preconditions; it does
//! not crash the binary when herdr is older. Operators on a
//! herdr 0.6.x install see a clear "upgrade herdr" message instead
//! of a cryptic spawn failure that would happen downstream at the
//! `pane split` / `agent start` calls.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// The expected `herdr agent start` flags. The values here are
/// the single source of truth for the spawn-shape contract; if
/// herdr ever renames `--kind` to `--harness` (or moves
/// `--pane` to a workspace-level flag), this is the place to
/// update and bump the version floor.
pub const EXPECTED_START_FLAGS: &[&str] = &["--kind", "--pane"];

/// The herdr version that introduced the current shape. The
/// version gate in [`detect_herdr_cli`] reports the installed
/// version and flags a warning when it is below this floor; the
/// shape check (does `agent start --help` list the expected
/// flags?) is the actual capability test, because the version
/// string is only a proxy.
pub const REQUIRED_HERDR_VERSION_FLOOR: &str = "0.7.0";

/// The verdict from probing the herdr CLI. `compatible=true` is
/// required for `mp watch` to attempt a spawn; the rest of the
/// fields are surfaced in `mp doctor` and the watch precondition
/// report so the operator can see the install / version state at
/// a glance.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HerdrCliShape {
    /// `true` when the herdr binary is on PATH *and* reports a
    /// version string *and* the `agent start --help` output lists
    /// every flag in [`EXPECTED_START_FLAGS`].
    pub compatible: bool,
    /// `true` when the herdr binary was reachable on PATH.
    pub on_path: bool,
    /// Raw `herdr --version` stdout, trimmed. Empty when herdr is
    /// not on PATH or failed to print a version.
    pub version_output: String,
    /// Parsed herdr version (e.g. `"0.8.2"`). `None` when the
    /// version string is unparseable or herdr is unreachable.
    pub parsed_version: Option<String>,
    /// Parsed herdr version compared against
    /// [`REQUIRED_HERDR_VERSION_FLOOR`] — `AtOrAbove` when the
    /// install is new enough, `Below` when it is not,
    /// `Unknown` when the version string is missing or
    /// unparseable. The shape check is the authoritative
    /// capability test; this field is informational.
    pub version_floor: VersionFloor,
    /// Raw `herdr agent start --help` stdout, trimmed. Empty
    /// when herdr is not on PATH or the subcommand failed.
    pub start_help_output: String,
    /// Raw `herdr pane split --help` stdout, trimmed. Empty when
    /// herdr is not on PATH or the subcommand failed.
    pub pane_help_output: String,
    /// The flags in [`EXPECTED_START_FLAGS`] that are missing
    /// from `agent start --help`. Empty when the shape is
    /// compatible.
    pub missing_flags: Vec<String>,
    /// Human-readable summary suitable for `mp doctor` and the
    /// watch precondition report. Never empty when herdr is on
    /// PATH.
    pub message: String,
}

/// Version-floor verdict. `Unknown` covers both "herdr is not
/// installed" and "herdr's version string could not be parsed" —
/// callers render the same doctor line for both, so they collapse
/// to one variant.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VersionFloor {
    AtOrAbove,
    Below,
    Unknown,
}

/// Probe the herdr CLI and return a [`HerdCliShape`]. Pure over the
/// supplied binary path (so tests can inject a fake); the
/// [`crate::watch::herdr::which_herdr`] default lives at
/// [`detect_herdr_cli_default`].
pub fn detect_herdr_cli(herdr_bin: &Path) -> HerdrCliShape {
    let version_out = run_capture(herdr_bin, &["--version"]);
    let start_help_out = run_capture(herdr_bin, &["agent", "start", "--help"]);
    let pane_help_out = run_capture(herdr_bin, &["pane", "split", "--help"]);
    let on_path =
        !version_out.is_empty() || !start_help_out.is_empty() || !pane_help_out.is_empty();

    let parsed_version = parse_version_string(&version_out);
    let version_floor = match parsed_version.as_deref() {
        Some(v) => match compare_versions(v, REQUIRED_HERDR_VERSION_FLOOR) {
            Some(std::cmp::Ordering::Less) => VersionFloor::Below,
            _ => VersionFloor::AtOrAbove,
        },
        None => VersionFloor::Unknown,
    };

    let missing_flags: Vec<String> = if start_help_out.is_empty() {
        EXPECTED_START_FLAGS.iter().map(|s| s.to_string()).collect()
    } else {
        EXPECTED_START_FLAGS
            .iter()
            .filter(|f| !flag_present(&start_help_out, f))
            .map(|s| s.to_string())
            .collect()
    };

    let compatible = on_path
        && missing_flags.is_empty()
        // The pane split subcommand must exist (herdr 0.6.x and
        // earlier did not ship it). An empty help output is the
        // "subcommand not found" signal from clap.
        && !pane_help_out.trim().is_empty();

    let message = render_message(
        on_path,
        parsed_version.as_deref(),
        version_floor,
        &missing_flags,
        compatible,
    );

    HerdrCliShape {
        compatible,
        on_path,
        version_output: version_out.trim().to_string(),
        parsed_version,
        version_floor,
        start_help_output: start_help_out.trim().to_string(),
        pane_help_output: pane_help_out.trim().to_string(),
        missing_flags,
        message,
    }
}

/// Convenience wrapper: resolve the herdr binary via
/// [`crate::watch::herdr::which_herdr`] and probe it. When herdr
/// is not on PATH, returns a [`HerdCliShape`] with
/// `on_path=false, compatible=false, parsed_version=None` so
/// callers do not have to special-case the missing binary.
pub fn detect_herdr_cli_default() -> HerdrCliShape {
    match crate::watch::herdr::which_herdr() {
        Some(bin) => detect_herdr_cli(&bin),
        None => HerdrCliShape {
            compatible: false,
            on_path: false,
            version_output: String::new(),
            parsed_version: None,
            version_floor: VersionFloor::Unknown,
            start_help_output: String::new(),
            pane_help_output: String::new(),
            missing_flags: EXPECTED_START_FLAGS.iter().map(|s| s.to_string()).collect(),
            message: "herdr binary not on PATH — install from https://herdr.dev/docs/install"
                .to_string(),
        },
    }
}

fn run_capture(bin: &Path, args: &[&str]) -> String {
    let Ok(out) = Command::new(bin).args(args).output() else {
        return String::new();
    };
    if !out.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Parse `herdr 0.8.2` / `herdr version 0.8.2` / `v0.8.2` /
/// `0.8.2` into a `MAJOR.MINOR.PATCH` string. Returns `None`
/// when no digit-led token is found.
pub fn parse_version_string(raw: &str) -> Option<String> {
    for tok in raw.split_whitespace() {
        let cleaned: String = tok
            .trim_start_matches(['v', 'V'])
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if cleaned.split('.').count() >= 2
            && cleaned.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            return Some(cleaned);
        }
    }
    None
}

/// Compare two dotted `MAJOR.MINOR.PATCH` versions. Returns
/// `None` when either side is not a well-formed dotted tuple.
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let parse = |s: &str| -> Option<Vec<u64>> {
        s.split('.')
            .map(|p| p.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    let av = parse(a)?;
    let bv = parse(b)?;
    let n = av.len().max(bv.len());
    for i in 0..n {
        let x = *av.get(i).unwrap_or(&0);
        let y = *bv.get(i).unwrap_or(&0);
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => continue,
            ord => return Some(ord),
        }
    }
    Some(std::cmp::Ordering::Equal)
}

fn flag_present(help: &str, flag: &str) -> bool {
    // clap's help output lists the flag under "Options:" as
    // `  --kind <KIND>  ...`. Match the long form only — short
    // aliases (rare for herdr) would change the shape.
    help.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(flag)
            && (trimmed.len() == flag.len()
                || trimmed[flag.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_whitespace() || c == '<'))
    })
}

fn render_message(
    on_path: bool,
    version: Option<&str>,
    floor: VersionFloor,
    missing: &[String],
    compatible: bool,
) -> String {
    if !on_path {
        return "herdr binary not on PATH — install from https://herdr.dev/docs/install".into();
    }
    if compatible {
        return match (version, floor) {
            (Some(v), VersionFloor::AtOrAbove) => {
                format!(
                    "herdr {v} supports mp watch spawn shape (≥ {REQUIRED_HERDR_VERSION_FLOOR})"
                )
            }
            _ => "herdr supports mp watch spawn shape".into(),
        };
    }
    let mut parts = Vec::new();
    if let Some(v) = version {
        parts.push(format!("installed herdr {v}"));
    } else {
        parts.push("installed herdr (version unparseable)".to_string());
    }
    if !missing.is_empty() {
        parts.push(format!(
            "missing `agent start` flags: [{}]",
            missing.join(", ")
        ));
    }
    parts.push(format!(
        "mp watch requires herdr ≥ {REQUIRED_HERDR_VERSION_FLOOR} (the pane split + --kind/--pane shape)"
    ));
    parts.join("; ")
}

#[allow(dead_code)]
pub(crate) fn _herdr_version_default_floor() -> &'static str {
    REQUIRED_HERDR_VERSION_FLOOR
}

#[allow(dead_code)]
pub(crate) fn _expected_flags() -> Vec<String> {
    EXPECTED_START_FLAGS.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_string_handles_canonical_shapes() {
        assert_eq!(parse_version_string("herdr 0.8.2"), Some("0.8.2".into()));
        assert_eq!(
            parse_version_string("herdr version 0.7.1"),
            Some("0.7.1".into())
        );
        assert_eq!(parse_version_string("v0.6.0"), Some("0.6.0".into()));
        assert_eq!(parse_version_string("0.10.0"), Some("0.10.0".into()));
    }

    #[test]
    fn parse_version_string_returns_none_for_garbage() {
        assert_eq!(parse_version_string(""), None);
        assert_eq!(parse_version_string("herdr (unknown)"), None);
    }

    #[test]
    fn compare_versions_orders_minor_and_patch() {
        assert_eq!(
            compare_versions("0.6.0", "0.7.0"),
            Some(std::cmp::Ordering::Less)
        );
        assert_eq!(
            compare_versions("0.7.0", "0.7.0"),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(
            compare_versions("0.8.2", "0.7.0"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            compare_versions("0.7", "0.7.0"),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn flag_present_matches_only_long_form_at_word_boundary() {
        let help = "Options:\n  --kind <KIND>  Harness kind\n  --pane <ID>   Pane id\n";
        assert!(flag_present(help, "--kind"));
        assert!(flag_present(help, "--pane"));
        assert!(!flag_present(help, "--kinda"));
    }

    #[test]
    fn expected_flags_pin_the_two_spawn_options() {
        // The spawn-shape contract depends on this exact list;
        // changing the order or removing a flag is a breaking
        // change to the wp2 realignment.
        assert_eq!(EXPECTED_START_FLAGS, &["--kind", "--pane"]);
    }

    #[test]
    fn render_message_for_off_path_is_actionable() {
        let msg = render_message(false, None, VersionFloor::Unknown, &[], false);
        assert!(msg.contains("herdr"));
        assert!(msg.contains("install"));
    }

    #[test]
    fn render_message_for_compatible_includes_version() {
        let msg = render_message(true, Some("0.8.2"), VersionFloor::AtOrAbove, &[], true);
        assert!(msg.contains("0.8.2"));
        assert!(msg.contains("0.7.0"));
    }

    #[test]
    fn render_message_for_missing_flags_calls_out_specific_options() {
        let msg = render_message(
            true,
            Some("0.6.0"),
            VersionFloor::Below,
            &["--pane".to_string()],
            false,
        );
        assert!(msg.contains("--pane"));
        assert!(msg.contains("0.7.0"));
    }
}
