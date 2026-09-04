//! Render the Autopilot lane as three regions:
//!
//! 1. **Picker** (left) — the drivable candidate list with the
//!    current selection highlighted. `>` marks the active
//!    picker row, `+` marks a selected candidate.
//! 2. **Lifecycle graph** (top-right) — the canonical
//!    milestone lifecycle with the current lifecycle
//!    highlighted. Includes the remediation loop
//!    indicator (↺) when active.
//! 3. **Queue + log + output** (bottom-right) — the ordered
//!    queue, the recent watch log entries, and the active-pane
//!    output snapshot. Rendered as separate blocks so the
//!    user can read them at a glance without overwhelming the
//!    terminal.
//!
//! The renderer is intentionally text-based — no tui
//! widgets beyond `Paragraph` + `Block`. The TUI main loop
//! calls `render_watch_lane` whenever the active lane is
//! `Lane::Autopilot`.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;

use super::super::watch;

/// Render picker, lifecycle/queue, and cached log/output regions.
pub fn render_watch_lane(frame: &mut Frame, app: &App, area: Rect) {
    // Two-column layout: picker on the left, everything else
    // on the right. The right column stacks the lifecycle
    // graph (top), the compact queue (middle), and the
    // log+output pair (bottom).
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_picker(frame, app, cols[0]);

    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // lifecycle graph (one line + border)
            Constraint::Length(7), // compact queue (up to 5 rows)
            Constraint::Min(5),    // log + output split
        ])
        .split(cols[1]);

    render_lifecycle(frame, app, right_rows[0]);
    render_queue(frame, app, right_rows[1]);
    render_log_and_output(frame, app, right_rows[2]);
}

fn render_picker(frame: &mut Frame, app: &App, area: Rect) {
    // M215 / F-01: prefer the new typed Picker state
    // (`app.autopilot.picker`). The legacy `app.watch.candidates`
    // is still rendered when the new picker is empty — this is the
    // backcompat path for the M179 backcompat surface and keeps
    // the M179 / M214 tests green.
    let (candidates, selected, cursor) = if app.autopilot.picker.candidates.is_empty() {
        let legacy: Vec<crate::tui::autopilot::PickerCandidate> = app
            .watch
            .candidates
            .iter()
            .map(|c| crate::tui::autopilot::PickerCandidate {
                id: c.id.clone(),
                title: c.title.clone(),
                lifecycle: c.lifecycle.clone(),
                priority: c.priority.clone(),
            })
            .collect();
        let ids = app.watch.selected.clone();
        let cursor = app.watch.picker_index;
        (legacy, ids, cursor)
    } else {
        let ids = app.autopilot.picker.queue_ids().to_vec();
        let cursor = app.autopilot.picker.cursor;
        (app.autopilot.picker.candidates.clone(), ids, cursor)
    };
    let title = format!(
        " Autopilot picker (drivable only) — {} candidates, {} selected ",
        candidates.len(),
        selected.len()
    );
    let dep_blocked_color = app.palette.warn;
    let items: Vec<ListItem> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let marker = if i == cursor { ">" } else { " " };
            let sel = if selected.contains(&c.id) { "+" } else { " " };
            let mut spans: Vec<Span> = vec![Span::raw(marker), Span::raw(sel), Span::raw(" ")];
            spans.push(Span::raw("  "));
            spans.push(Span::raw(c.id.clone()));
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                c.lifecycle.clone(),
                Style::default().fg(crate::tui::palette::dim_color(app.palette)),
            ));
            spans.push(Span::raw("  "));
            spans.push(Span::raw(c.title.clone()));
            let _ = dep_blocked_color; // legacy dep_color fallback kept for backcompat
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(list, area);
}

fn render_lifecycle(frame: &mut Frame, app: &App, area: Rect) {
    let current = app.watch.status.as_ref().and_then(|s| {
        s.raw
            .get("state")
            .and_then(|st| st.get("current_lifecycle"))
            .and_then(|c| c.as_str())
    });
    let graph = watch::render_lifecycle_graph(current);
    let title = " Lifecycle (M178) ";
    let p = Paragraph::new(graph)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(p, area);
}

fn render_queue(frame: &mut Frame, app: &App, area: Rect) {
    let body = watch::render_compact_queue(app);
    let title = " Queue (mp outcomes verbatim — AC-10) ";
    let p = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(p, area);
}

fn render_log_and_output(frame: &mut Frame, app: &App, area: Rect) {
    // Split the remaining vertical area in two: log (top)
    // and active-pane output (bottom). The split is 50/50
    // when both have content; on small terminals, the output
    // wins the lower half.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Log I/O belongs to the idle poller; rendering consumes memory only.
    let log_body = if app.watch.log_tail.is_empty() {
        "(no log lines yet)".to_string()
    } else {
        app.watch.log_tail.join("\n")
    };
    let log_p =
        Paragraph::new(log_body).block(Block::default().borders(Borders::ALL).title(" Log "));
    frame.render_widget(log_p, rows[0]);

    // Output: the latest active-role pane snapshot.
    let out_body = app
        .watch
        .output
        .as_ref()
        .map(|o| {
            if o.ok {
                o.output.clone()
            } else {
                format!("(output error: {})", o.reason)
            }
        })
        .unwrap_or_else(|| "(no output yet — Start a run)".to_string());
    let out_p = Paragraph::new(out_body).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Active pane output "),
    );
    frame.render_widget(out_p, rows[1]);
}
