//! M91 S2 removed sidebar_width from App. Narrow-width tab bar behavior
//! (compact labels) is covered by
//! `crates/raul/tests/tui_tab_bar.rs::narrow_terminal_uses_compact_labels`.
//! This file keeps a small regression that `Lane::compact_label` stays
//! non-empty for every lane (overflow/scroll coverage lives in tui_tab_bar).

use raul::tui::app::Lane;

#[test]
fn every_lane_has_nonempty_compact_label() {
    for lane in [Lane::Overview, Lane::Milestones, Lane::Backlog, Lane::Path] {
        assert!(
            !lane.compact_label().is_empty(),
            "{lane:?} compact_label must be non-empty for narrow terminals"
        );
    }
}
