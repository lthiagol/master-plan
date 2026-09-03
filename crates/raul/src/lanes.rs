//! Canonical lane-name string constants (M171 / TW-20).
//!
//! Historically the strings `"Overview"`, `"Path"`, `"Milestones"`,
//! `"Backlog"`, and `"Settings"` appeared as bare literals in
//! `Lane::label()`, the sidebar `title(...)` calls in the render
//! modules, and the `lane_icon(...)` keybind map in `config.rs`. This
//! module is the single source of truth — every consumer routes through
//! these constants so renaming or adding a lane is a one-line change.
//!
//! The [`Lane::label`] implementation in [`crate::tui::app`] returns
//! these constants directly (see the `match` arms below for parity);
//! tests pin the parity so a future drift breaks at compile time.

use crate::tui::app::Lane;

/// Sidebar / list-row header for the high-level Overview lane.
pub const LANE_OVERVIEW: &str = "Overview";
/// M214: Autopilot lane — the dedicated `mp autopilot` workflow surface
/// (milestone picker, preflight, start, lifecycle graph, queue,
/// log + agent output, attach/stop/detach controls). Renamed from
/// `LANE_WATCH = "Watch"` (M179) to align with the new CLI surface
/// (`mp autopilot`) and the role-name family (orchestrator / runner /
/// reviewer).
pub const LANE_AUTOPILOT: &str = "Autopilot";
/// Sidebar / list-row header for the milestone-tree lane.
pub const LANE_MILESTONES: &str = "Milestones";
/// Sidebar / list-row header for the Plan / Path lane.
pub const LANE_PATH: &str = "Path";
/// Sidebar / list-row header for the backlog lane.
pub const LANE_BACKLOG: &str = "Backlog";
/// Sidebar / list-row header for the settings overlay lane.
pub const LANE_SETTINGS: &str = "Settings";
/// Sidebar / list-row header for the bugfixes track lane (TW-20
/// sibling lane — listed because `config::lane_icon` keys against it).
/// M184: BF-* rows render under [`LANE_BACKLOG`]; the constant remains
/// for docs / icon maps / migration tests.
pub const LANE_BUGFIXES: &str = "Bugfixes";
/// Historical tweaks-track label. M184 folded Tweaks into Backlog
/// (`TW-*` prefix); the constant remains for docs / migration tests
/// (out of scope to drop — see M184).
pub const LANE_TWEAKS: &str = "Tweaks";
/// Sidebar / list-row header for the Ideas lane.
pub const LANE_IDEAS: &str = "Ideas";

/// Resolve a [`Lane`] enum variant to its canonical label string.
/// This is the single source of truth — every render site and config
/// map keys off `LANE_*` constants, but `Lane::label()` itself must
/// stay ergonomic (no `match` duplicated at every call site), so the
/// constants are the *only* literal source and `Lane::label` reads them.
pub fn lane_label(lane: &Lane) -> &'static str {
    match lane {
        Lane::Overview => LANE_OVERVIEW,
        Lane::Milestones => LANE_MILESTONES,
        Lane::Path => LANE_PATH,
        Lane::Backlog => LANE_BACKLOG,
        Lane::Ideas => LANE_IDEAS,
        Lane::Autopilot => LANE_AUTOPILOT,
        Lane::Settings => LANE_SETTINGS,
    }
}

#[cfg(test)]
mod lane_name_constants_are_the_only_source_of_truth {
    //! Pin: every `&'static str` lane-name literal in the codebase
    //! resolves to a `LANE_*` constant declared above. A future drift
    //! (e.g. someone typing `"Overview"` in a new render site without
    //! importing `LANE_OVERVIEW`) breaks the test suite immediately
    //! rather than at some downstream "string didn't match" failure.
    //!
    //! The check is symmetric: each `LANE_*` constant must equal its
    //! expected value (prevents typos in the canonical module) AND
    //! every known lane-name literal elsewhere must use the constant
    //! (prevents drift). We assert the first half with explicit
    //! `assert_eq!` checks; the second half is enforced by the
    //! M171 AC-05 verification grep:
    //!
    //! ```text
    //! grep -rn '"Overview"|"Path"|"Milestones"|"Backlog"|"Settings"' \
    //!      crates/raul/src/
    //! # → hits only inside crates/raul/src/lanes.rs
    //! ```
    use super::*;

    #[test]
    fn constants_match_their_expected_values() {
        assert_eq!(LANE_OVERVIEW, "Overview");
        assert_eq!(LANE_MILESTONES, "Milestones");
        assert_eq!(LANE_PATH, "Path");
        assert_eq!(LANE_BACKLOG, "Backlog");
        assert_eq!(LANE_AUTOPILOT, "Autopilot");
        assert_eq!(LANE_SETTINGS, "Settings");
        assert_eq!(LANE_BUGFIXES, "Bugfixes");
        assert_eq!(LANE_TWEAKS, "Tweaks");
        assert_eq!(LANE_IDEAS, "Ideas");
    }

    #[test]
    fn lane_label_routes_through_constants() {
        // Parity check: `lane_label(Lane::X)` must return the same
        // `&'static str` as the corresponding `LANE_*` constant. A
        // future change that adds a constant without wiring it into
        // `lane_label` will fail here.
        assert_eq!(lane_label(&Lane::Overview), LANE_OVERVIEW);
        assert_eq!(lane_label(&Lane::Milestones), LANE_MILESTONES);
        assert_eq!(lane_label(&Lane::Path), LANE_PATH);
        assert_eq!(lane_label(&Lane::Backlog), LANE_BACKLOG);
        assert_eq!(lane_label(&Lane::Ideas), LANE_IDEAS);
        assert_eq!(lane_label(&Lane::Autopilot), LANE_AUTOPILOT);
        assert_eq!(lane_label(&Lane::Settings), LANE_SETTINGS);
    }

    /// M171 external-review F-02: bare-literal grep misses format!()
    /// interpolations like `format!(" Backlog ({}) ", n)`. This test
    /// walks every render source file and asserts no string literal
    /// or `format!` macro call embeds a lane-name substring outside
    /// this module. A future drift that types `"Backlog"` inline (or
    /// in a format!) fails here rather than as a silent partial rename.
    #[test]
    fn no_lane_name_substring_in_render_sources_outside_canonical_module() {
        // Lane-name substrings the dashboard renders (TW-20 scope).
        let substrings: &[&str] = &[
            LANE_OVERVIEW,
            LANE_PATH,
            LANE_MILESTONES,
            LANE_BACKLOG,
            LANE_AUTOPILOT,
            LANE_SETTINGS,
        ];
        // Sources we audit. Any new render site added under
        // crates/raul/src/tui/render/ that introduces a lane-name
        // substring must be appended here — the test will surface
        // the miss on the next run.
        let sources: &[&str] = &[
            "crates/raul/src/tui/render/mod.rs",
            "crates/raul/src/tui/render/lane_lists.rs",
            "crates/raul/src/tui/render/overlays.rs",
            "crates/raul/src/tui/render/lane_lists.rs",
        ];
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        for src in sources {
            let path = workspace_root.join(src);
            let body = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!("could not read {src}: {e}");
            });
            for needle in substrings {
                // Skip doc comments and string-slice imports of the
                // constant — the test is about user-visible labels
                // that would diverge on a rename, not about constant
                // references. We grep for the needle inside any
                // string-context (literal "..." or format!("...{}",
                // needle) expansion). To keep this simple we only
                // flag bare literals and `format!("`/`write!(` calls
                // that embed the needle as a substring (which is how
                // the populated-state title bug surfaced).
                let needle_with_separator = format!("\"{needle}");
                if body.contains(&needle_with_separator) {
                    panic!(
                        "bare literal \"{needle}\" found in {src} — must use \
                         crate::lanes::LANE_* constant (M171 external-review F-02)"
                    );
                }
                if body.contains(&format!("format!(\"{needle}"))
                    || body.contains(&format!("format!(\" {needle}"))
                {
                    panic!(
                        "format!() literal embedding \"{needle}\" found in {src} — \
                         must use crate::lanes::LANE_* constant \
                         (M171 external-review F-02)"
                    );
                }
            }
        }
    }
}
