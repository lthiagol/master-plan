use raul::theme::Palette;
use raul::tui::progress;

#[test]
fn compute_progress_bar_table() {
    assert_eq!(progress::compute_progress_bar(0, 0, 10), "[          ] 0/0");
    assert!(progress::compute_progress_bar(0, 12, 10).contains("0/12"));
    assert!(progress::compute_progress_bar(12, 12, 10).contains("12/12"));
    assert!(progress::compute_progress_bar(7, 12, 10).contains("7/12"));
    let narrow = progress::compute_progress_bar(1, 20, 5);
    assert!(narrow.contains("1/20"));
}

#[test]
fn ac_status_palette_mapping() {
    let palette = Palette::default_palette();
    let passed = progress::ac_status_style("passed", palette);
    let failed = progress::ac_status_style("failed", palette);
    let pending = progress::ac_status_style("pending", palette);
    assert_eq!(passed.fg, Some(palette.success));
    assert_eq!(failed.fg, Some(palette.danger));
    assert_eq!(pending.fg, Some(palette.warn));
}
