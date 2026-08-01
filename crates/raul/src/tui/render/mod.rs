use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::app::{App, ContentState, Lane};
use super::mode::Mode;
use super::path_view;
use super::view_state::ViewState;

pub mod board;
mod chrome;
mod dashboard_view;
mod detail_sections;
pub mod lane_lists;
mod milestone_detail;

pub mod modal;
mod overlays;
pub mod scrollbar;
mod tab_bar;
pub mod watch;

use chrome::{
    list_lane_filter_chip, overlay_rect_or, render_footer, render_scrollbars, view_title,
};
use dashboard_view::render_dashboard;
use lane_lists::{render_backlog_detail, render_lane_list};
use milestone_detail::render_milestone_detail;
use overlays::{
    render_annotation_thread, render_co_approval, render_help_overlay, render_input_overlay,
    render_lifecycle_filter_overlay, render_review_menu_overlay, render_search_input_overlay,
    render_settings_lane, render_sort_rebind_overlay,
};
pub use scrollbar::{scrollbar_rect, track_click_to_scroll, SCROLLBAR_GUTTER};
use tab_bar::render_tab_bar;

// M135 (S4) - re-exports for the tab-bar layout machinery that
// moved to `view_state`. The cross-module leakage to `runner.rs` is
// gone (the mouse handler reads `view.tab_hit_areas` instead of
// recomputing the layout). The layout machinery is `pub(super)` in
// `view_state` (no longer `pub`); the integration tests that
// previously imported these symbols from `render` moved to
// `#[cfg(test)] mod tests` inside `view_state`.

// =============================================================================
// M135: tab-bar layout machinery moved to `view_state.rs` (re-exported
// above for backward compat with the integration tests). See the
// `view_state` module docs for the M135 design.
// =============================================================================

pub fn render(frame: &mut Frame, app: &App, view: &ViewState) {
    let area = frame.area();

    if matches!(app.active_mode, Mode::Help) {
        render_help_overlay(frame, app, overlay_rect_or(view, area));
        return;
    }

    // M135: every rect used below comes from the pre-computed
    // `view`. `compute_view` has already done the outer split
    // (header / main / footer) and the bar split (tab bar /
    // content), so this function is pure read + render_widget.
    //
    // M137-2 + M167: the title reads
    // `Review, Approve, Understand Layers - R.A.U.L. - <view title>`
    // (the project's R.A.U.L. acronym stands for the first three words +
    // 'Layers'; pre-M167 it was 'Lanes'). The mnemonic is preserved.
    // M185 F-03: header carries the Milestones filter chip — dim " · "
    // separator + accent on the active filter segment (S7).
    let header_line = {
        let accent = Style::default()
            .fg(app.effective_palette().accent)
            .add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(app.effective_palette().dim);
        let mut spans = vec![Span::styled(
            format!(
                " Review, Approve, Understand Layers - R.A.U.L. - {}",
                view_title(app)
            ),
            accent,
        )];
        if let Some(chip) = list_lane_filter_chip(app) {
            spans.push(Span::styled(" · ".to_string(), dim));
            spans.push(Span::styled(chip, accent));
        }
        ratatui::text::Line::from(spans)
    };
    frame.render_widget(Paragraph::new(header_line), view.header_area);

    render_tab_bar(frame, app, view.tab_bar_area, &view.tab_layout);

    let content_area = view.content_area;
    // External-review F-06: detail / annotation / backlog-detail
    // content is narrowed by the gutter so wrap glyphs never flash
    // under the rail (lists/board/path already reserve inside their
    // own layout).
    let detail_area = Rect {
        x: content_area.x,
        y: content_area.y,
        width: content_area.width.saturating_sub(SCROLLBAR_GUTTER),
        height: content_area.height,
    };
    let content = &app.content;
    let lane = &app.active_lane;

    match (lane, content) {
        (Lane::Settings, _) => {
            render_settings_lane(frame, app, content_area);
        }
        (Lane::Overview, ContentState::List) => render_dashboard(frame, app, view),
        (Lane::Path, ContentState::List) => {
            if let Some(ref data) = app.path_data {
                path_view::render(frame, app, content_area, data);
            } else {
                let msg = Paragraph::new("No path data — press r to load")
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(crate::lanes::LANE_PATH),
                    )
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(app.effective_palette().dim));
                frame.render_widget(msg, content_area);
            }
        }
        (_, ContentState::List) => {
            render_lane_list(frame, app, content_area, view);
        }
        (_, ContentState::MilestoneDetail) => {
            if matches!(app.active_mode, Mode::ReviewMenu(_)) {
                render_milestone_detail(frame, app, detail_area);
                render_review_menu_overlay(frame, app, overlay_rect_or(view, area));
            } else {
                render_milestone_detail(frame, app, detail_area);
            }
        }
        (_, ContentState::AnnotationThread) => render_annotation_thread(frame, app, detail_area),
        (_, ContentState::CoApproval) => render_co_approval(frame, app, view, area),
        (_, ContentState::BacklogDetail) => render_backlog_detail(frame, app, detail_area),
    }

    // M137: paint the scrollbar gutter on every scrollable region.
    // The region renderers above use the (area - gutter)-sized layout;
    // we walk `view.scrollbar_rects` and draw the track + thumb into
    // the buffer. This runs BEFORE the input overlay so the popup
    // always covers the scrollbar where the two overlap (scrollbars
    // are background chrome; popups sit on top).
    render_scrollbars(frame, view, app);

    if app.is_input_active() {
        render_input_overlay(frame, app, overlay_rect_or(view, area));
    }
    if matches!(app.active_mode, Mode::LifecycleFilter(_)) {
        render_lifecycle_filter_overlay(frame, app, overlay_rect_or(view, area));
    }
    if matches!(app.active_mode, Mode::SearchInput(_)) {
        render_search_input_overlay(frame, app, overlay_rect_or(view, area));
    }
    if app.sort_rebind_open() {
        render_sort_rebind_overlay(frame, app, overlay_rect_or(view, area));
    }

    // M183: two-line footer (globals + per-tab); flash/quit span both rows.
    render_footer(frame, app, view);
}
