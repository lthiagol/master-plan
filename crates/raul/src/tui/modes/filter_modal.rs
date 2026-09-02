//! M204: unified per-lane filter modal.
//!
//! The widget is data-driven: each lane passes a
//! `Vec<DimensionSpec>` describing the dimensions it offers
//! (Milestones: lifecycle + priority + age; Backlog: priority +
//! status + age + source prefix; Ideas: priority + status + age
//! + tags). The renderer flattens the spec into a single
//! navigable list of `(dimension, value)` rows; the dispatcher
//! routes Up/Down/Space/Enter/Esc into the same handlers
//! across all three lanes, so keybindings are consistent.
//!
//! ## Keybindings (load-bearing — the AC tests pin these names)
//!
//! | Key        | Action                                  |
//! |------------|-----------------------------------------|
//! | Up / k     | Move cursor up                          |
//! | Down / j   | Move cursor down                        |
//! | Space      | Toggle the highlighted value            |
//! | Enter      | Commit the draft filter and close       |
//! | Esc        | Restore the prior filter and close      |
//!
//! ## Visual style
//!
//! The modal renders identically across lanes (per AC-03's
//! `modal_visual_style_consistent_across_lanes`). Per-tab
//! dimensions are data, not UI; the title, footer hints, and
//! selection highlight are uniform.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::action::Action;

/// M204: per-lane filter modal key handler. The same handler
/// routes keys for Milestones, Backlog, and Ideas — the lane
/// shape is in `FilterModalState::dimensions`, not in the
/// handler. The dispatched `Action`s carry the per-mode
/// semantics (Filter* vs LifecycleFilter*); `apply_action`
/// looks up the active mode and dispatches to the right
/// mutator.
pub fn handle_key(key: KeyEvent) -> Vec<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL)
        || key.modifiers.contains(KeyModifiers::ALT)
        || key.modifiers.contains(KeyModifiers::SUPER)
    {
        return Vec::new();
    }
    match key.code {
        KeyCode::Esc => vec![Action::FilterCancel],
        KeyCode::Enter => vec![Action::FilterCommit],
        KeyCode::Up | KeyCode::Char('k') => vec![Action::FilterPrev],
        KeyCode::Down | KeyCode::Char('j') => vec![Action::FilterNext],
        KeyCode::Char(' ') => vec![Action::FilterToggle],
        _ => Vec::new(),
    }
}

/// M204: canonical per-lane dimension specs. Single source of
/// truth — the render path, the modal-open path, and the
/// filter-apply path all read from this. Reusing the
/// `progress::LIFECYCLE_FILTER_OPTIONS` constant for the
/// lifecycle dimension keeps the M185-era order stable
/// (the test fixtures pin the order, so reordering would
/// surface as a regression).
///
/// Age presets: `>7d` / `>30d` / `>90d` (Preset kind — single
/// select, AC-06). The labels are stored as the spec values
/// (e.g. `>7d`); the `apply_filter` path maps them to the
/// `created`-field delta.
pub mod spec {
    use crate::tui::mode::{DimensionKind, DimensionSpec};
    use crate::tui::progress::LIFECYCLE_FILTER_OPTIONS;

    /// Milestones-lane filter dimensions.
    pub fn milestones() -> Vec<DimensionSpec> {
        vec![
            DimensionSpec {
                name: "lifecycle".to_string(),
                label: "Lifecycle".to_string(),
                values: LIFECYCLE_FILTER_OPTIONS.iter().map(|s| (*s).to_string()).collect(),
                kind: DimensionKind::Toggle,
            },
            DimensionSpec {
                name: "priority".to_string(),
                label: "Priority".to_string(),
                values: vec![
                    "urgent".to_string(),
                    "high".to_string(),
                    "normal".to_string(),
                    "low".to_string(),
                ],
                kind: DimensionKind::Toggle,
            },
            DimensionSpec {
                name: "age".to_string(),
                label: "Age".to_string(),
                values: vec![">7d".to_string(), ">30d".to_string(), ">90d".to_string()],
                kind: DimensionKind::Preset,
            },
        ]
    }

    /// Backlog-lane filter dimensions.
    pub fn backlog() -> Vec<DimensionSpec> {
        vec![
            DimensionSpec {
                name: "priority".to_string(),
                label: "Priority".to_string(),
                values: vec![
                    "urgent".to_string(),
                    "high".to_string(),
                    "normal".to_string(),
                    "low".to_string(),
                ],
                kind: DimensionKind::Toggle,
            },
            DimensionSpec {
                name: "status".to_string(),
                label: "Status".to_string(),
                values: vec![
                    "open".to_string(),
                    "pending".to_string(),
                    "active".to_string(),
                    "in-progress".to_string(),
                    "resolved".to_string(),
                    "done".to_string(),
                    "archived".to_string(),
                    "dismissed".to_string(),
                    "closed".to_string(),
                    "cancelled".to_string(),
                ],
                kind: DimensionKind::Toggle,
            },
            DimensionSpec {
                name: "age".to_string(),
                label: "Age".to_string(),
                values: vec![">7d".to_string(), ">30d".to_string(), ">90d".to_string()],
                kind: DimensionKind::Preset,
            },
            DimensionSpec {
                name: "source".to_string(),
                label: "Source".to_string(),
                // Source-prefix filter — matches the row's id prefix.
                values: vec!["B-".to_string(), "BL-".to_string(), "TW-".to_string(), "BF-".to_string()],
                kind: DimensionKind::Toggle,
            },
        ]
    }

    /// Ideas-lane filter dimensions.
    pub fn ideas() -> Vec<DimensionSpec> {
        vec![
            DimensionSpec {
                name: "priority".to_string(),
                label: "Priority".to_string(),
                values: vec![
                    "urgent".to_string(),
                    "high".to_string(),
                    "normal".to_string(),
                    "low".to_string(),
                ],
                kind: DimensionKind::Toggle,
            },
            DimensionSpec {
                name: "status".to_string(),
                label: "Status".to_string(),
                values: vec![
                    "open".to_string(),
                    "pending".to_string(),
                    "active".to_string(),
                    "in-progress".to_string(),
                    "resolved".to_string(),
                    "done".to_string(),
                    "archived".to_string(),
                    "dismissed".to_string(),
                    "closed".to_string(),
                    "cancelled".to_string(),
                ],
                kind: DimensionKind::Toggle,
            },
            DimensionSpec {
                name: "age".to_string(),
                label: "Age".to_string(),
                values: vec![">7d".to_string(), ">30d".to_string(), ">90d".to_string()],
                kind: DimensionKind::Preset,
            },
            DimensionSpec {
                name: "tags".to_string(),
                label: "Tags".to_string(),
                // Tag-prefix filter — the user picks a prefix and
                // rows whose tags array contains a matching tag
                // pass. A typed-in tag free-form is deferred to a
                // follow-up; this milestone ships the four most
                // common prefixes (alpha-tagged / beta-tagged /
                // unblocked / spike).
                values: vec![
                    "alpha".to_string(),
                    "beta".to_string(),
                    "unblocked".to_string(),
                    "spike".to_string(),
                ],
                kind: DimensionKind::Toggle,
            },
        ]
    }

    /// Total number of items (dimension-value rows) the modal
    /// flattens into a single navigable list.
    pub fn total_items(dims: &[DimensionSpec]) -> usize {
        dims.iter().map(|d| d.values.len()).sum()
    }
}
