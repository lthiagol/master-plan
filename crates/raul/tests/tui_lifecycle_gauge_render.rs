//! M185 AC-03: lifecycle gauge segment mapping.

use raul::theme::Palette;
use raul::tui::progress::{
    lifecycle_color, lifecycle_gauge_index, lifecycle_gauge_plain, LIFECYCLE_GAUGE_ORDER,
};

#[test]
fn gauge_index_and_plain_for_each_canonical_lifecycle() {
    for (i, lc) in LIFECYCLE_GAUGE_ORDER.iter().enumerate() {
        assert_eq!(lifecycle_gauge_index(lc), Some(i), "{lc}");
        let plain = lifecycle_gauge_plain(lc);
        assert_eq!(plain.chars().count(), 8, "{lc}: {plain}");
        // Current and prior filled; later empty.
        let chars: Vec<char> = plain.chars().collect();
        for (j, ch) in chars.iter().enumerate() {
            if j <= i {
                assert_eq!(*ch, '▮', "{lc} seg {j}");
            } else {
                assert_eq!(*ch, '▯', "{lc} seg {j}");
            }
        }
    }
}

#[test]
fn off_path_markers() {
    assert_eq!(lifecycle_gauge_plain("cancelled"), "✗");
    assert_eq!(lifecycle_gauge_plain("remediation"), "↺");
    assert_eq!(lifecycle_gauge_index("cancelled"), None);
}

#[test]
fn lifecycle_color_mapping() {
    let p = Palette::default_palette();
    assert_eq!(lifecycle_color("complete", p), p.success);
    assert_eq!(lifecycle_color("in-progress", p), p.accent);
    assert_eq!(lifecycle_color("blocked", p), p.danger);
    assert_eq!(lifecycle_color("approved", p), p.warn);
    assert_eq!(lifecycle_color("ready", p), p.warn);
    assert_eq!(lifecycle_color("draft", p), p.dim);
}
