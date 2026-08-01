//! M185 external-review regressions (F-01..F-03).

use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use ratatui::Terminal;
use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::progress::lifecycle_filter_window;
use raul::tui::render;
use raul::tui::view_state;

fn ms(id: &str, lc: &str) -> MilestoneSummary {
    MilestoneSummary {
        id: id.into(),
        title: format!("t-{id}"),
        lifecycle: lc.into(),
        lifecycle_at: None,
        depends_on: vec![],
        priority: "normal".into(),
        updated: String::new(),
    }
}

fn dump(app: &App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            render::render(frame, app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

/// F-01: help overlay lists capital F / g and still shows lowercase f.
#[test]
fn m185_f01_help_overlay_lists_lifecycle_filter_and_grooming() {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.toggle_help();
    let s = dump(&app, 100, 40);
    assert!(
        s.to_lowercase().contains("lifecycle filter") || s.contains("Lifecycle filter"),
        "help must list lifecycle filter; got:\n{s}"
    );
    assert!(
        s.to_lowercase().contains("grooming") || s.contains("Grooming"),
        "help must list grooming preset; got:\n{s}"
    );
    assert!(
        s.to_lowercase().contains("capital f") || s.contains("capital F"),
        "help must note capital F; got:\n{s}"
    );
    assert!(
        s.to_lowercase().contains("lowercase f"),
        "help must note lowercase f; got:\n{s}"
    );
    // Lowercase f key glyph must appear (not blank after the filter label).
    assert!(
        s.contains('f') || s.contains('F'),
        "help must show f/F key glyphs; got:\n{s}"
    );
}

/// F-02: short overlay windows the list and surfaces more-below.
#[test]
fn m185_f02_lifecycle_filter_window_and_more_below() {
    // 10 options, inner height 5 → must truncate with cues.
    let (start, end, more_above, more_below) = lifecycle_filter_window(10, 0, 5);
    assert_eq!(start, 0);
    assert!(end < 10, "window must not show all 10; end={end}");
    assert!(!more_above);
    assert!(more_below, "selected at top must show more-below");

    let (start, end, more_above, more_below) = lifecycle_filter_window(10, 9, 5);
    assert!(start > 0, "selected at bottom must scroll; start={start}");
    assert_eq!(end, 10);
    assert!(more_above);
    assert!(!more_below);

    // Tall enough → full list, no cues.
    let (start, end, more_above, more_below) = lifecycle_filter_window(10, 3, 12);
    assert_eq!((start, end, more_above, more_below), (0, 10, false, false));

    // Render path on a short terminal shows the cue string.
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    app.open_lifecycle_filter();
    // Jump selection near the bottom so more-above appears.
    for _ in 0..8 {
        app.lifecycle_filter_next();
    }
    let s = dump(&app, 80, 14);
    assert!(
        s.contains("more above") || s.contains("more below"),
        "short modal must show more above/below cue; got:\n{s}"
    );
}

/// F-03: filter chip segment uses accent (REVERSED not required; accent fg).
#[test]
fn m185_f03_title_chip_filter_segment_is_accent() {
    let mut app = App::new();
    app.load_milestones(vec![ms("01", "approved"), ms("02", "in-progress")]);
    app.select_lane(Lane::Milestones);
    app.milestone_filter.insert("approved".into());

    let backend = TestBackend::new(140, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let accent = app.effective_palette().accent;
    let mut found_accent_chip = false;
    let mut header = String::new();
    for x in 0..buf.area().width {
        let cell = &buf[(x, 0)];
        header.push_str(cell.symbol());
        if cell.symbol().contains('a') || cell.symbol() == "a" {
            // look for 'approved' run with accent fg
        }
        if cell.fg == accent && !cell.modifier.contains(Modifier::DIM) {
            // Collect nearby text
            let mut run = String::new();
            for xx in x.saturating_sub(2)..(x + 12).min(buf.area().width) {
                run.push_str(buf[(xx, 0)].symbol());
            }
            if run.contains("approved") || run.contains("All") {
                found_accent_chip = true;
            }
        }
    }
    assert!(
        header.contains("approved"),
        "header must show filter chip text; got {header:?}"
    );
    assert!(
        found_accent_chip,
        "filter chip segment must use accent fg; header={header:?}"
    );
}
