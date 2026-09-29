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
//! calls `render_autopilot_lane` whenever the active lane is
//! `Lane::Autopilot`.
//!
//! M230: the picker / lifecycle / queue / log readers were
//! migrated off the legacy `app.watch` mirror. The renderer
//! now reads only `app.autopilot.picker` /
//! `autopilot::render_lifecycle_graph` /
//! `autopilot::render_compact_queue` /
//! `app.autopilot.log_tail`.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;

use crate::tui::autopilot;

/// Render picker, lifecycle/queue, and cached log/output regions.
pub fn render_autopilot_lane(frame: &mut Frame, app: &App, area: Rect) {
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
            // M217 / AC-07: the block grows to 4 rows when a health
            // badge is present (graph line + badge line + border);
            // 3 rows otherwise, preserving the pre-M217 layout for
            // sessions that have not been polled yet.
            Constraint::Length(if app.autopilot.health().is_some() {
                4
            } else {
                3
            }),
            Constraint::Length(7), // compact queue (up to 5 rows)
            Constraint::Min(5),    // log + output split
        ])
        .split(cols[1]);

    render_lifecycle(frame, app, right_rows[0]);
    render_queue(frame, app, right_rows[1]);
    render_log_and_output(frame, app, right_rows[2]);
}

fn render_picker(frame: &mut Frame, app: &App, area: Rect) {
    // M230 S3: the picker reads only `app.autopilot.picker`. The
    // legacy `app.watch.candidates` mirror and the
    // `app.watch.selected` / `app.watch.picker_index` fallbacks
    // are gone — the typed `Picker` is the single source of
    // truth. S4 will rename this module to `autopilot_lane`.
    let candidates = &app.autopilot.picker.candidates;
    let selected = app.autopilot.picker.queue_ids();
    let cursor = app.autopilot.picker.cursor;
    let title = format!(
        " Autopilot picker (drivable only) — {} candidates, {} selected ",
        candidates.len(),
        selected.len()
    );
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
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(list, area);
}

fn render_lifecycle(frame: &mut Frame, app: &App, area: Rect) {
    // M230 S3: the lifecycle graph gets no active node (identical
    // to the pre-M230 runtime, where the legacy watch-status
    // restore always returned `None`). The migrated helper lives
    // on `tui::autopilot` next to the typed lane state.
    let mut graph = autopilot::render_lifecycle_graph(None);
    // M217 / AC-07: append the liveness + heartbeat badge reported
    // by `mp autopilot status`. Rendered verbatim — raul emits no
    // pulses of its own, and a `stale` marker is informational
    // (the badge says "escalation: mp" so the operator knows raul
    // is not the component that will act on it).
    if let Some(health) = app.autopilot.health() {
        graph.push('\n');
        graph.push_str(&health.badge());
    }
    let title = " Lifecycle (M178) ";
    let p = Paragraph::new(graph)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(p, area);
}

fn render_queue(frame: &mut Frame, app: &App, area: Rect) {
    let body = autopilot::render_compact_queue(app);
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

    // M230 S3: the log pane reads the in-memory snapshot cached on
    // `app.autopilot.log_tail`. The pre-M230 mirror (`app.watch.log_tail`)
    // is gone; the poller writes the typed field directly.
    let log_body = if app.autopilot.log_tail.is_empty() {
        "(no log lines yet)".to_string()
    } else {
        app.autopilot.log_tail.join("\n")
    };
    let log_p =
        Paragraph::new(log_body).block(Block::default().borders(Borders::ALL).title(" Log "));
    frame.render_widget(log_p, rows[0]);

    // Output: the latest active-role pane snapshot.
    // M230: the legacy `app.watch.output` mirror that fed this
    // pane was only ever populated by `mp watch-control output`
    // (a verb removed by M229). The pane shows the placeholder
    // until the autopilot control surface ships an equivalent
    // snapshot.
    let out_body = "(no output yet — Start a run)".to_string();
    let out_p = Paragraph::new(out_body).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Active pane output "),
    );
    frame.render_widget(out_p, rows[1]);
}
