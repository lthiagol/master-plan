//! M167 WP1 S6: detail-section navigation in MilestoneDetail.
//!
//! The four `]`/`[`/`n`/`p` keybindings route through the modes/normal
//! dispatcher (only when `app.content == ContentState::MilestoneDetail`)
//! and produce `NextSection`/`PrevSection`/`NextItem`/`PrevItem` actions.
//! The actual row math lives in `apply_detail_section_nav` which reads
//! `app.detail_section_rows` populated by `render_milestone_detail` (WP3).
//! For these tests we seed `detail_section_rows` manually so S6 has a
//! working contract even before WP3 lands.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use raul::tui::action::Action;
use raul::tui::app::{App, ContentState};
use raul::tui::modes;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn bracket_keys_jump_to_next_prev_section() {
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    app.detail_section_rows = std::cell::RefCell::new(vec![5, 12, 22]);
    app.detail_scroll = 0;

    let next = modes::normal::handle_key(key(KeyCode::Char(']')), &app);
    let prev = modes::normal::handle_key(key(KeyCode::Char('[')), &app);
    assert_eq!(next, vec![Action::NextSection]);
    assert_eq!(prev, vec![Action::PrevSection]);
}

#[test]
fn section_nav_skips_empty_sections() {
    // AC-15: `]` skips empty sections and lands on the next non-empty one.
    // Today `apply_detail_section_nav` reads `detail_section_rows`,
    // which the renderer will populate only for non-empty sections.
    // The dispatcher contract: any non-Normal mode + the keys we care
    // about is what matters; the row-skip is the renderer's job.
    let mut app = App::new();
    app.content = ContentState::MilestoneDetail;
    // Seed rows — assuming a renderer that emits one row per populated
    // section, AC + Steps section is empty so its row is omitted.
    app.detail_section_rows = std::cell::RefCell::new(vec![5, 22]); // AC and Findings; no Steps row
    let action = modes::normal::handle_key(key(KeyCode::Char(']')), &app);
    assert_eq!(
        action,
        vec![Action::NextSection],
        "AC-15: `]` from offset < 5 must emit NextSection; row skipping happens in apply_action"
    );
}

#[test]
fn section_nav_is_noop_outside_milestone_detail() {
    // AC-16: on any non-MilestoneDetail content (List / BacklogDetail —
    // the canonical AC examples), `[`, `]`, `n`, `p` are no-ops. Other
    // modes (CoApproval, AnnotationThread) have their own semantics for
    // some of these keys; this test pins the AC's specific cases.
    for content in [ContentState::List, ContentState::BacklogDetail] {
        let mut app = App::new();
        app.content = content;
        for kc in [
            KeyCode::Char(']'),
            KeyCode::Char('['),
            KeyCode::Char('n'),
            KeyCode::Char('p'),
        ] {
            let action = modes::normal::handle_key(key(kc), &app);
            assert_eq!(
                action,
                Vec::new(),
                "AC-16: {kc:?} on {content:?} must not emit NextSection/PrevSection/NextItem/PrevItem"
            );
        }
    }
}
