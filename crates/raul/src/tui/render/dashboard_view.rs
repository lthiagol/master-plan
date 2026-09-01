use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::app::{App, ContentState};
use crate::tui::dashboard;
use crate::tui::view_state::{ScrollableId, ViewState};

/// M181 S5: build the Health strip lines. Six fields exactly per
/// AC-02: validation state, validation error count, blocker count,
/// execution mode, planning state, watch state. No active watch
/// milestone / stage / queue / log / pane details.
///
/// `compact=true` collapses the strip to two lines (validation +
/// execution+planning on line 1; blockers + watch on line 2) so
/// narrow terminals (~30 rows) can still see the inbox + activity
/// split below. All six fields are present in both modes — only
/// the layout density changes.
fn render_health_lines(app: &App, compact: bool) -> Vec<Line<'static>> {
    let snap = &app.overview;
    let health = &snap.health;
    let palette = app.effective_palette();
    let state = health.validation_state.clone();
    let state_span = Span::styled(
        if state.is_empty() {
            "—".to_string()
        } else {
            state.clone()
        },
        if state == "ok" {
            Style::default().fg(palette.success)
        } else {
            Style::default().fg(palette.danger)
        },
    );
    let watch = health.watch_state.clone();
    let watch_span = Span::styled(
        if watch.is_empty() {
            "—".to_string()
        } else {
            watch.clone()
        },
        match watch.as_str() {
            "running" => Style::default().fg(palette.success),
            "complete" => Style::default().fg(palette.success),
            "failed" | "stopped" => Style::default().fg(palette.warn),
            _ => Style::default(),
        },
    );
    if compact {
        return vec![
            Line::from(vec![
                Span::styled("Validation: ", Style::default().fg(palette.dim)),
                state_span,
                Span::styled("  Blockers: ", Style::default().fg(palette.dim)),
                Span::raw(health.blocker_count.to_string()),
                Span::styled("  Watch: ", Style::default().fg(palette.dim)),
                watch_span,
            ]),
            Line::from(vec![
                Span::styled("Execution: ", Style::default().fg(palette.dim)),
                Span::raw(if health.execution_mode.is_empty() {
                    "—".to_string()
                } else {
                    health.execution_mode.clone()
                }),
                Span::styled("  Planning: ", Style::default().fg(palette.dim)),
                Span::raw(if health.planning_state.is_empty() {
                    "—".to_string()
                } else {
                    health.planning_state.clone()
                }),
            ]),
        ];
    }
    vec![
        Line::from(vec![
            Span::styled("Validation: ", Style::default().fg(palette.dim)),
            state_span,
            Span::styled(
                format!("  ({} error", health.validation_error_count),
                Style::default().fg(palette.dim),
            ),
            Span::styled(
                if health.validation_error_count == 1 {
                    ""
                } else {
                    "s"
                },
                Style::default().fg(palette.dim),
            ),
            Span::styled(")", Style::default().fg(palette.dim)),
        ]),
        Line::from(vec![
            Span::styled("Blockers: ", Style::default().fg(palette.dim)),
            Span::raw(health.blocker_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Execution: ", Style::default().fg(palette.dim)),
            Span::raw(if health.execution_mode.is_empty() {
                "—".to_string()
            } else {
                health.execution_mode.clone()
            }),
            Span::raw("  "),
            Span::styled("Planning: ", Style::default().fg(palette.dim)),
            Span::raw(if health.planning_state.is_empty() {
                "—".to_string()
            } else {
                health.planning_state.clone()
            }),
        ]),
        Line::from(vec![
            Span::styled("Watch: ", Style::default().fg(palette.dim)),
            watch_span,
        ]),
    ]
}

/// M181 S5: build the Statistics box lines. Total milestones +
/// every step status. All zeros stay visible per AC-04.
///
/// `compact=true` collapses to two lines (Milestones + a single
/// comma-separated Steps summary) so narrow terminals can fit the
/// box. AC-04's "all five step statuses" requirement is met in both
/// modes — only the rendering density differs.
fn render_statistics_lines(app: &App, compact: bool) -> Vec<Line<'static>> {
    let snap = &app.overview;
    let palette = app.effective_palette();
    let total = snap.totals.milestones;
    let accent = |count: u64| {
        if count > 0 {
            Style::default().fg(palette.accent)
        } else {
            Style::default()
        }
    };
    if compact {
        let summary = format!(
            "Steps: pending {}, in-progress {}, done {}, failed {}, skipped {}",
            snap.steps.pending,
            snap.steps.in_progress,
            snap.steps.done,
            snap.steps.failed,
            snap.steps.skipped,
        );
        return vec![
            Line::from(vec![
                Span::styled("Milestones ", Style::default().fg(palette.dim)),
                Span::styled(total.to_string(), Style::default().fg(palette.accent)),
            ]),
            Line::from(Span::styled(summary, Style::default().fg(palette.dim))),
        ];
    }
    let label = |status: &str, count: u64| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("{:<14}", status), Style::default().fg(palette.dim)),
            Span::styled(count.to_string(), accent(count)),
        ])
    };
    vec![
        Line::from(vec![
            Span::styled("Milestones ", Style::default().fg(palette.dim)),
            Span::styled(total.to_string(), Style::default().fg(palette.accent)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Steps", Style::default().fg(palette.dim))),
        label("pending", snap.steps.pending),
        label("in-progress", snap.steps.in_progress),
        label("done", snap.steps.done),
        label("failed", snap.steps.failed),
        label("skipped", snap.steps.skipped),
    ]
}

/// M181 S5: build the Work queues box lines. All seven queue
/// counts (AC-07) — including zero — every time.
///
/// `compact=true` collapses the seven rows into a single comma-
/// separated line so narrow terminals can fit the box while still
/// surfacing every queue count.
fn render_work_queues_lines(app: &App, compact: bool) -> Vec<Line<'static>> {
    let snap = &app.overview;
    let palette = app.effective_palette();
    let label = |name: &str, count: u64| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("{:<20}", name), Style::default().fg(palette.dim)),
            Span::styled(
                count.to_string(),
                if count > 0 {
                    Style::default().fg(palette.accent)
                } else {
                    Style::default()
                },
            ),
        ])
    };
    if compact {
        // Two-line wrap so the 7-queue summary fits inside the
        // ~114-character block (120 - 2 borders - 2 padding). Each
        // line shows ~3-4 queues; the line break follows the visual
        // "Queues | Lifecycle | Path" split the rich layout uses.
        let line1 = format!(
            "Inbox {}  Pending {}  Backlog {}  Ideas {}",
            snap.queues.inbox,
            snap.queues.pending_reviews,
            snap.queues.backlog,
            snap.queues.parked_ideas,
        );
        let line2 = format!(
            "Annot {}  Blocked {}  Remediation {}",
            snap.queues.open_annotations,
            snap.queues.blocked_milestones,
            snap.queues.remediation_milestones,
        );
        return vec![
            Line::from(Span::styled(line1, Style::default().fg(palette.dim))),
            Line::from(Span::styled(line2, Style::default().fg(palette.dim))),
        ];
    }
    vec![
        Line::from(Span::styled("Queues", Style::default().fg(palette.dim))),
        label("Inbox", snap.queues.inbox),
        label("Pending reviews", snap.queues.pending_reviews),
        label("Backlog", snap.queues.backlog),
        label("Parked ideas", snap.queues.parked_ideas),
        label("Open annotations", snap.queues.open_annotations),
        label("Blocked", snap.queues.blocked_milestones),
        label("Remediation", snap.queues.remediation_milestones),
    ]
}

/// M181 S6 + M202 S20: build the lifecycle grid lines. Twelve
/// mp-flow buckets (canonical `draft` → `hand-off`) including
/// zeros, in canonical order. Display-only per AC-04 (no focus,
/// no drill-down). The title is `Lifecycle (current mp-flow
/// stage)` so the meaning shift from the legacy 8-state lifecycle
/// grid is explicit on screen.
///
/// `compact=true` lays the grid out in two rows of 6 buckets with
/// shorter labels; `compact=false` uses three rows of 4 buckets
/// with full labels. All twelve buckets always render in both
/// modes (AC-16). The bucket count is sourced from the
/// `overview` snapshot's `mp_flow_stage_counts` field (added by
/// the mp side per M202); absent counts collapse to zero so a
/// pre-M202 snapshot still renders cleanly.
fn render_lifecycle_grid_lines(app: &App, compact: bool) -> Vec<Line<'static>> {
    let snap = &app.overview;
    let palette = app.effective_palette();
    let bucket_labels: [(&'static str, &'static str); 12] = [
        ("1/12 draft", "draft"),
        ("2/12 groom", "groom"),
        ("3/12 specify", "specify"),
        ("4/12 approve", "approve"),
        ("5/12 execute", "execute"),
        ("6/12 self-review", "self-review"),
        ("7/12 complete", "complete"),
        ("8/12 external-review", "external-review"),
        ("9/12 remediate", "remediate"),
        ("10/12 re-review", "re-review"),
        ("11/12 document", "document"),
        ("12/12 hand-off", "hand-off"),
    ];
    let counts = snap.mp_flow_stage_counts.as_ref();
    let count_for = |slug: &str| -> u64 {
        counts
            .and_then(|c| c.get(slug).copied())
            .unwrap_or(0)
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Lifecycle (current mp-flow stage)",
        Style::default().fg(palette.dim),
    )));
    let cols = if compact { 6 } else { 4 };
    for chunk in bucket_labels.chunks(cols) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (i, (label, slug)) in chunk.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            let pad = if compact { 15 } else { 17 };
            spans.push(Span::styled(
                format!("{:<width$}", label, width = pad),
                Style::default().fg(palette.dim),
            ));
            spans.push(Span::styled(
                count_for(slug).to_string(),
                if count_for(slug) > 0 {
                    Style::default().fg(palette.accent)
                } else {
                    Style::default()
                },
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// M181 S6: build the suggested-path lines. Up to five items in
/// order; empty array renders an explicit empty state.
///
/// `compact=true` caps the rendered list at three items so narrow
/// terminals can fit the box.
fn render_path_lines(app: &App, compact: bool) -> Vec<Line<'static>> {
    let snap = &app.overview;
    let palette = app.effective_palette();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let path = &snap.path;
    if path.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no path)",
            Style::default().fg(palette.dim),
        )));
        return lines;
    }
    lines.push(Line::from(Span::styled(
        "Next",
        Style::default().fg(palette.dim),
    )));
    let max = if compact { 3 } else { 5 };
    for p in path.iter().take(max) {
        let label = if p.display.is_empty() {
            p.id.clone()
        } else {
            p.display.clone()
        };
        lines.push(Line::from(vec![
            Span::styled("  → ", Style::default().fg(palette.dim)),
            Span::raw(label),
        ]));
    }
    lines
}

/// Build the activity panel lines (M181 S7). At most five events,
/// newest first. Display-only per AC-06 — never receives focus.
fn render_activity_lines(app: &App) -> Vec<Line<'static>> {
    let snap = &app.overview;
    let palette = app.effective_palette();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let events = &snap.activity;
    if events.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no recent activity)",
            Style::default().fg(palette.dim),
        )));
        return lines;
    }
    // M180 already emits newest-first; render at most five.
    for ev in events.iter().take(5) {
        let ts = if ev.timestamp.is_empty() {
            String::new()
        } else {
            ev.timestamp.clone()
        };
        // Trim RFC3339 timestamp to YYYY-MM-DD HH:MM for the row.
        let ts_short = if ts.len() >= 16 {
            ts[..16].replace('T', " ")
        } else {
            ts
        };
        let subject = if ev.subject.is_empty() {
            String::new()
        } else {
            format!("[{}] ", ev.subject)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", ts_short), Style::default().fg(palette.dim)),
            Span::styled(ev.event_type.clone(), Style::default().fg(palette.accent)),
            Span::raw(" "),
            Span::styled(format!("{}{}", subject, ev.summary), Style::default()),
        ]));
    }
    lines
}

pub(super) fn render_dashboard(frame: &mut Frame, app: &App, view: &ViewState) {
    let layout = view
        .dashboard_chunks
        .as_ref()
        .expect("render_dashboard requires compute_view to have populated dashboard_chunks");
    let palette = app.effective_palette();

    // M181 narrow-terminal support + M183 F-04: density must match
    // the chunk heights `compute_overview_list_rects` allocated.
    // Read `DashboardChunks.compact` (content_area height predicate)
    // — never re-derive from `frame.area().height`, which desynced
    // after the 2-line footer shrank content by one row.
    let compact = layout.compact;

    // === Health strip (display-only, top of dashboard) ============================
    frame.render_widget(
        Paragraph::new(render_health_lines(app, compact))
            .block(Block::default().borders(Borders::ALL).title(" Health ")),
        layout.health,
    );

    // === Statistics | Work queues (side-by-side inside the second slot) =========
    frame.render_widget(
        Paragraph::new(render_statistics_lines(app, compact))
            .block(Block::default().borders(Borders::ALL).title(" Statistics ")),
        layout.statistics,
    );
    frame.render_widget(
        Paragraph::new(render_work_queues_lines(app, compact)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Work queues "),
        ),
        layout.work_queues,
    );

    // === Lifecycle grid (display-only) =========================================
    frame.render_widget(
        Paragraph::new(render_lifecycle_grid_lines(app, compact))
            .block(Block::default().borders(Borders::ALL).title(" Lifecycle ")),
        layout.lifecycle,
    );

    // === Suggested path (display-only, M180 3..5 items) =========================
    frame.render_widget(
        Paragraph::new(render_path_lines(app, compact)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Suggested path "),
        ),
        layout.suggested_path,
    );

    // === Lower split: inbox (interactive) | activity (display-only) ==========
    let item_count = app.overview.inbox.len();
    let mut inbox_lines: Vec<Line> = vec![Line::from(format!(
        "{item_count} inbox item(s) — Enter to navigate",
        item_count = item_count
    ))];
    if app.overview.inbox.is_empty() {
        inbox_lines.push(Line::from(Span::styled(
            "  Inbox is empty",
            Style::default().fg(palette.dim),
        )));
    } else {
        let inbox_scroll = view
            .scrollbar_rects
            .iter()
            .find(|hit| matches!(hit.id, ScrollableId::OverviewInbox))
            .map(|hit| hit.scroll)
            .unwrap_or(0);
        let mut flat_idx = 0usize;
        let focus = app.content == ContentState::List;
        for kind in dashboard::inbox_kind_order() {
            let items: Vec<_> = app
                .overview
                .inbox
                .iter()
                .filter(|i| i.kind == *kind)
                .collect();
            if items.is_empty() {
                continue;
            }
            let group_last = flat_idx + items.len() - 1;
            if group_last < inbox_scroll {
                flat_idx += items.len();
                continue;
            }
            if flat_idx >= inbox_scroll {
                inbox_lines.push(Line::from(Span::styled(
                    format!("── {} ──", kind),
                    Style::default()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            for item in items {
                if flat_idx < inbox_scroll {
                    flat_idx += 1;
                    continue;
                }
                let selected = focus && flat_idx == app.selected_index;
                let row_style = if selected {
                    Style::default()
                        .fg(crate::tui::palette::on_accent_fg(palette))
                        .bg(palette.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(palette.dim)
                };
                let label = if item.display.is_empty() {
                    item.id.clone()
                } else {
                    item.display.clone()
                };
                inbox_lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", item.id), row_style),
                    Span::styled(label, row_style),
                ]));
                inbox_lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(item.reason.clone(), Style::default().fg(palette.dim)),
                ]));
                inbox_lines.push(Line::from(vec![
                    Span::raw("      → "),
                    Span::styled(item.action.clone(), Style::default().fg(palette.success)),
                ]));
                flat_idx += 1;
            }
        }
        // Catch-all kinds (defensive — typed snapshot's inbox items
        // carry the same `kind` strings the legacy snapshot did).
        for item in app
            .overview
            .inbox
            .iter()
            .filter(|i| !dashboard::inbox_kind_order().contains(&i.kind.as_str()))
        {
            if flat_idx < inbox_scroll {
                flat_idx += 1;
                continue;
            }
            let selected = focus && flat_idx == app.selected_index;
            let row_style = if selected {
                Style::default()
                    .fg(crate::tui::palette::on_accent_fg(palette))
                    .bg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            inbox_lines.push(Line::from(Span::styled(
                format!("  [{}] {}", item.kind, item.display),
                row_style,
            )));
            flat_idx += 1;
        }
    }
    frame.render_widget(
        Paragraph::new(inbox_lines).block(Block::default().borders(Borders::ALL).title(" Inbox ")),
        layout.lower_inbox,
    );

    // Activity panel — only rendered when there's room for it (wide
    // terminal side-by-side, or a tall narrow terminal that fits the
    // stacked height).
    if let Some(activity_rect) = layout.lower_activity {
        frame.render_widget(
            Paragraph::new(render_activity_lines(app)).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Recent activity "),
            ),
            activity_rect,
        );
    }
}
