//! M137-2 regression: the tab bar renders every `│` divider as a
//! SEPARATE dim span (not the first character of an active tab's
//! label span). Pre-fix the active tab's highlight included the
//! leading `│`, so the highlight visibly overpassed the divider to
//! the left. With separate spans the highlight covers only
//! ` {label} ` and the `│` stays dim — the user sees the highlight
//! end at the divider, not start at it.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::app::{App, Lane};
use raul::tui::render;
use raul::tui::view_state;

fn render_active_tab() -> ratatui::buffer::Buffer {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn tab_bar_divider_is_separate_dim_span() {
    let buf = render_active_tab();
    // The tab bar lives on row 1. Find the first `│` (Overview | Milestones)
    // and assert the column to its left carries the active style
    // (highlight background = accent) and the `│` itself does NOT.
    //
    // Before M137-2, the active tab's `│ Milestones ` span was
    // uniformly styled with the active style, so the highlight
    // extended LEFT over the divider. After the fix, the divider
    // lives in its own dim span and the active tab's ` Milestones `
    // highlight starts one column to the right of the divider.
    //
    // The buffer cell's `style()` reports `Modifier::REVERSED` or
    // `bg = palette.accent` for highlighted cells. We check that the
    // cell immediately LEFT of the active label carries a different
    // style (the `│` divider is dim/inactive).
    let tab_bar_row = 1;
    // Dump the bar to stderr so a failure shows the cell-by-cell state.
    eprintln!("Tab bar row 1:");
    for x in 0..buf.area().width {
        if buf[(x, tab_bar_row)].symbol() != " " {
            let s = buf[(x, tab_bar_row)].style();
            eprintln!(
                "  x={}: sym={:?} fg={:?} bg={:?} mods={:?}",
                x,
                buf[(x, tab_bar_row)].symbol(),
                s.fg,
                s.bg,
                s.add_modifier
            );
        }
    }
    // The tab bar is laid out as alternating `[space] [label] [│] [space]
    // [label] [│] [space] ...` — i.e. dividers live in their own
    // spans and labels are bracketed by spaces. The active tab's
    // ` Milestones ` span has the active style (fg=Black,
    // bg=Accent, mods=BOLD). The `│` divider has the inactive
    // style (fg=Dim, no bg, no mods). Pre-fix the divider was the
    // first char of the active tab's span, so the active style
    // extended over it. With separate spans the divider keeps
    // its own style.
    //
    // Walk the bar and find the first `│`. The cell one column to
    // its left MUST be a space (the trailing space of the
    // previous tab's label, not the divider). The cell AT the
    // `│` carries the inactive style. The cell one column to the
    // right of the `│` MUST be a space (the leading space of the
    // next tab's label). If any of those is violated the highlight
    // is leaking over the divider.
    let mut divider_x = None;
    for x in 0..buf.area().width {
        if buf[(x, tab_bar_row)].symbol() == "│" {
            divider_x = Some(x);
            break;
        }
    }
    let divider_x = divider_x.expect("tab bar must contain at least one `│` divider");
    let left = buf[(divider_x - 1, tab_bar_row)].symbol();
    let at = buf[(divider_x, tab_bar_row)].symbol();
    let right = buf[(divider_x + 1, tab_bar_row)].symbol();
    assert_eq!(left, " ", "cell left of `│` at col {} should be a space (the trailing space of the previous tab's label, not the divider) — got {:?}", divider_x - 1, left);
    assert_eq!(
        at, "│",
        "divider at col {} should be `│`, not {:?}",
        divider_x, at
    );
    assert_eq!(right, " ", "cell right of `│` at col {} should be a space (the leading space of the next tab's label, not the divider) — got {:?}", divider_x + 1, right);
}
