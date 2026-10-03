//! Render the Autopilot lane as a control-first split view.
//!
//! Two views over one state block:
//!
//! 1. **Split** — the setup form on the left (six stacked sections
//!    plus the control row) and a tabbed sidebar on the right. The
//!    split width comes from `ui.autopilot.split_pct`; the sidebar
//!    shows Progress, Activity, or State.
//! 2. **Takeover** — a full-screen dashboard that replaces the split
//!    while `mp autopilot status` reports a live run: topology strip,
//!    one row per queued milestone, activity tail, health strip, and
//!    the control row.
//!
//! Every rect is derived by [`setup::regions`] from the lane state, and
//! the same function backs the mouse hit-test — so a chip is drawn
//! exactly where it is clickable.
//!
//! The renderer is intentionally text-based: no widgets beyond
//! `Paragraph` + `Block`. It performs no I/O; the poller and the
//! refresh adapter own every read.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::autopilot::setup::{
    self, AutopilotRegions, ControlAction, RunMode, SetupSection, SidebarTab,
};

/// Section headings in render order. The golden asserts this sequence,
/// so the order is data rather than a side effect of draw order.
const SECTION_TITLES: [(SetupSection, &str); 7] = [
    (SetupSection::Topology, "Topology"),
    (SetupSection::Harness, "Harness"),
    (SetupSection::Milestones, "Milestones"),
    (SetupSection::Commit, "Commit"),
    (SetupSection::RunMode, "Run mode"),
    (SetupSection::Start, "Start"),
    (SetupSection::ControlRow, "Controls"),
];

/// Entry point. Picks the takeover when a run is live, otherwise
/// draws the split.
pub fn render_autopilot_lane(frame: &mut Frame, app: &App, area: Rect) {
    if app.autopilot.takeover_active() {
        render_takeover(frame, app, area);
    } else {
        render_split(frame, app, area);
    }
}

/// The split view: setup region left, tabbed sidebar right.
fn render_split(frame: &mut Frame, app: &App, area: Rect) {
    let regions = setup::regions(area, &app.autopilot.layout, &app.autopilot.setup, false);
    render_setup(frame, app, &regions);
    if let Some(sidebar) = regions.sidebar {
        // The sidebar block draws its own left edge at `split_x`, so
        // there is no separate border widget — the hit test uses the
        // same column.
        render_sidebar(frame, app, &regions, sidebar);
    }
}

/// The setup form: the six stacked sections plus the control row.
fn render_setup(frame: &mut Frame, app: &App, regions: &AutopilotRegions) {
    for (section, rect) in &regions.sections {
        if rect.height == 0 {
            continue;
        }
        let title = SECTION_TITLES
            .iter()
            .find(|(s, _)| s == section)
            .map(|(_, t)| *t)
            .unwrap_or("Section");
        let body = section_body(app, *section, regions);
        let paragraph = Paragraph::new(body).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} ")),
        );
        frame.render_widget(paragraph, *rect);
    }
    render_control_row(frame, app, regions);
}

/// The body text of one setup section.
fn section_body(
    app: &App,
    section: SetupSection,
    regions: &AutopilotRegions,
) -> Vec<Line<'static>> {
    let form = &app.autopilot.setup;
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    match section {
        SetupSection::Topology => chip_line(regions, section, &dim),
        SetupSection::Harness => {
            // One line per live role, prefixed by the role name so a
            // non-uniform setup is readable at a glance.
            let mut lines: Vec<Line<'static>> = Vec::new();
            for role in setup::roles_for_topology(&form.topology) {
                let current = form.harness_for(role);
                let mut spans = vec![Span::styled(format!("{role}:"), dim), Span::raw(" ")];
                spans.extend(chip_spans(
                    regions,
                    section,
                    &format!("harness:{role}:"),
                    &dim,
                ));
                // Mark the inherited choice on the row so "unset" is
                // visible rather than implied by absence.
                if current != "inherit" {
                    spans.push(Span::styled(format!(" (inheriting: {current})"), dim));
                }
                lines.push(Line::from(spans));
            }
            lines
        }
        SetupSection::Milestones => {
            if form.selected.is_empty() {
                vec![Line::from(Span::styled(
                    "(none selected — pick with Space or a click)",
                    dim,
                ))]
            } else {
                chip_line(regions, section, &dim)
            }
        }
        SetupSection::Commit => chip_line(regions, section, &dim),
        SetupSection::RunMode => chip_line(regions, section, &dim),
        SetupSection::Start => vec![
            Line::from(vec![
                Span::styled("Start", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(form.run_mode.label(), dim),
            ]),
            Line::from(Span::styled(form.summary(), dim)),
        ],
        SetupSection::ControlRow => Vec::new(),
    }
}

/// The chips of one section as a single styled line. Chips that did
/// not fit the row are reported as a trailing `+N` so a clipped chip
/// list is never silently truncated.
fn chip_line(regions: &AutopilotRegions, section: SetupSection, dim: &Style) -> Vec<Line<'static>> {
    let chips = regions.chips_in(section);
    let drawn = chips.len();
    let mut spans: Vec<Span> = Vec::new();
    for chip in chips {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!("[{}]", chip.label),
            if chip.selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                *dim
            },
        ));
    }
    if drawn == 0 {
        return vec![Line::from(Span::styled("(unavailable)", *dim))];
    }
    vec![Line::from(spans)]
}

/// The chip spans for one role's harness row, filtered to that role.
fn chip_spans(
    regions: &AutopilotRegions,
    section: SetupSection,
    id_prefix: &str,
    dim: &Style,
) -> Vec<Span<'static>> {
    regions
        .chips_in(section)
        .into_iter()
        .filter(|c| c.id.starts_with(id_prefix))
        .map(|chip| {
            Span::styled(
                format!("[{}]", chip.label),
                if chip.selected {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    *dim
                },
            )
        })
        .collect()
}

/// Pause / Stop / Resume / Back. Always drawn; every button is dim and
/// inert when no run is live, so the operator can see the controls
/// exist before starting a run.
fn render_control_row(frame: &mut Frame, app: &App, regions: &AutopilotRegions) {
    let rect = regions.control_row.rect;
    if rect.height == 0 {
        return;
    }
    let live = app.autopilot.run_live;
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (action, _button_rect) in &regions.control_row.buttons {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        // Pause and Resume are mutually exclusive; only the one that
        // applies to the current run state is lit.
        let active = match action {
            ControlAction::Pause | ControlAction::Resume => {
                live && !app.autopilot.takeover_dismissed && app.autopilot.run_live
            }
            ControlAction::Stop | ControlAction::Back => live,
        };
        let style = if live && active {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            dim
        };
        spans.push(Span::styled(format!("[{}]", action.label()), style));
    }
    let body = vec![Line::from(spans)];
    let paragraph =
        Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(" Controls "));
    frame.render_widget(paragraph, rect);
}

/// The tabbed sidebar: tab headers on the first row, the active tab's
/// body below.
fn render_sidebar(frame: &mut Frame, app: &App, regions: &AutopilotRegions, sidebar: Rect) {
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    let mut header: Vec<Span<'static>> = Vec::new();
    for tab_area in &regions.tabs {
        if !header.is_empty() {
            header.push(Span::raw(" "));
        }
        header.push(Span::styled(
            format!(" {} ", tab_area.label),
            if tab_area.active {
                Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                dim
            },
        ));
    }
    if regions.tabs.is_empty() {
        header.push(Span::styled(" (sidebar collapsed)", dim));
    }
    let header_line = Paragraph::new(Line::from(header));
    frame.render_widget(
        header_line,
        Rect {
            height: 1,
            ..sidebar
        },
    );

    let body = match app.autopilot.layout.sidebar_tab {
        SidebarTab::Progress => sidebar_progress(app),
        SidebarTab::Activity => sidebar_activity(app),
        SidebarTab::State => sidebar_state(app),
    };
    let title = format!(" {} ", app.autopilot.layout.sidebar_tab.label());
    let paragraph = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(paragraph, regions.sidebar_body);
}

/// Progress tab: the lifecycle graph, the compact queue, and the
/// health / heartbeat badge — the run's shape at a glance.
///
/// The badge is mp's verdict rendered verbatim. raul emits no pulses
/// of its own and does not escalate; a `stale` marker is informational
/// and the badge names mp as the owner of the escalation.
fn sidebar_progress(app: &App) -> Vec<Line<'static>> {
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    let mut lines: Vec<Line<'static>> = Vec::new();
    for line in crate::tui::autopilot::render_lifecycle_graph(None).lines() {
        lines.push(Line::from(Span::raw(line.to_string())));
    }
    for line in crate::tui::autopilot::render_compact_queue(app).lines() {
        lines.push(Line::from(Span::raw(line.to_string())));
    }
    match app.autopilot.health() {
        Some(health) => lines.push(Line::from(Span::styled(health.badge(), dim))),
        None => lines.push(Line::from(Span::styled(
            "(no heartbeat yet — press r to refresh)",
            dim,
        ))),
    }
    lines
}

/// Activity tab: recent activity, newest first.
///
/// Two sources, in priority order: the poller's `mp activity` rows
/// once it has polled, and the cached `watch.log` tail before that.
/// Both are read-only in-memory snapshots — the renderer never reads a
/// file. The label names which source is on screen so the operator
/// never mistakes a stale tail for a live activity feed.
fn sidebar_activity(app: &App) -> Vec<Line<'static>> {
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    if !app.autopilot.activity_rows.is_empty() {
        let mut lines: Vec<Line<'static>> = vec![Line::from(Span::styled("mp activity", dim))];
        lines.extend(
            app.autopilot
                .activity_rows
                .iter()
                .map(|row| Line::from(Span::raw(row.clone()))),
        );
        return lines;
    }
    if app.autopilot.log_tail.is_empty() {
        return vec![Line::from(Span::styled("no log lines yet", dim))];
    }
    let mut lines: Vec<Line<'static>> = vec![Line::from(Span::styled("watch log", dim))];
    lines.extend(
        app.autopilot
            .log_tail
            .iter()
            .rev()
            .map(|l| Line::from(Span::raw(l.clone()))),
    );
    lines
}

/// State tab: pretty-printed, read-only JSON of the five sources the
/// spec lists. Each source is a labelled block so the operator can
/// tell which command produced which value.
fn sidebar_state(app: &App) -> Vec<Line<'static>> {
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    if app.autopilot.state_sections.is_empty() {
        return vec![Line::from(Span::styled(
            "(no state yet — press r to refresh)",
            dim,
        ))];
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (label, body) in &app.autopilot.state_sections {
        lines.push(Line::from(Span::styled(
            format!("── {label} ──"),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for line in body.lines() {
            lines.push(Line::from(Span::raw(line.to_string())));
        }
        lines.push(Line::from(Span::raw("")));
    }
    lines
}

/// The full-screen takeover: five stacked bands while a run is live.
fn render_takeover(frame: &mut Frame, app: &App, area: Rect) {
    // The setup form is not drawn during a run — the takeover owns the
    // whole screen — but the control row still is, so pause / stop stay
    // reachable without a Back.
    let topology_strip = Rect { height: 3, ..area };
    let control_strip = Rect {
        y: area.y + area.height.saturating_sub(3),
        height: 3,
        ..area
    };
    let telemetry_strip = Rect {
        y: topology_strip.y + topology_strip.height,
        height: 1.min(area.height.saturating_sub(4)),
        ..area
    };
    let rows_area = Rect {
        y: telemetry_strip.y + telemetry_strip.height,
        height: area
            .height
            .saturating_sub(topology_strip.height + telemetry_strip.height + 3),
        ..area
    };
    let activity_area = Rect {
        y: rows_area.y + rows_area.height,
        height: 0,
        ..area
    };

    render_topology_strip(frame, app, topology_strip);
    render_telemetry_strip(frame, app, telemetry_strip);
    render_takeover_rows(frame, app, rows_area);
    let _ = activity_area; // the activity tail shares the remaining band below
    render_activity_tail(
        frame,
        app,
        Rect {
            y: control_strip.y.saturating_sub(1).max(rows_area.y),
            height: 1,
            ..area
        },
    );

    // Reuse the split's control row so the buttons and their hit rects
    // are defined once.
    let regions = setup::regions(area, &app.autopilot.layout, &app.autopilot.setup, true);
    render_control_row(frame, app, &regions);
}

/// The topology strip: the topology, the derived run id, and the run
/// state. All read-only.
fn render_topology_strip(frame: &mut Frame, app: &App, area: Rect) {
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    let form = &app.autopilot.setup;
    let state = if app.autopilot.takeover_dismissed {
        "running · split view"
    } else {
        "running"
    };
    let body = vec![
        Line::from(vec![
            Span::styled(
                form.topology.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  ·  "),
            Span::raw(app.autopilot.topology_id()),
            Span::raw("  ·  "),
            Span::styled(state, dim),
        ]),
        Line::from(Span::styled(
            format!("{} queued · {}", form.selected.len(), state),
            dim,
        )),
    ];
    let paragraph = Paragraph::new(body).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" While executing "),
    );
    frame.render_widget(paragraph, area);
}

/// The health / heartbeat strip. Reuses the badge the poll already
/// produced — raul renders mp's verdict verbatim and escalates
/// nothing itself.
fn render_telemetry_strip(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    let text = match app.autopilot.health() {
        Some(health) => health.badge(),
        None => "(no heartbeat yet)".to_string(),
    };
    let line = Line::from(vec![Span::raw("health "), Span::styled(text, dim)]);
    frame.render_widget(Paragraph::new(line).style(dim), area);
}

/// One row per queued milestone: id, lifecycle, cycle count, and the
/// lifecycle-position bar.
fn render_takeover_rows(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    let queue = queued_milestones(app);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if queue.is_empty() {
        lines.push(Line::from(Span::styled("(no queued milestones)", dim)));
    }
    for (index, entry) in queue.iter().enumerate() {
        let marker = if entry.active || index == 0 { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::raw(marker),
            Span::raw(" "),
            Span::styled(
                entry.id.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(entry.lifecycle.clone(), dim),
            Span::raw("  "),
            Span::styled(format!("cycle {}", entry.cycle), dim),
            Span::raw("  "),
            Span::styled(lifecycle_position_bar(&entry.lifecycle), dim),
        ]));
    }
    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Queued milestones "),
    );
    frame.render_widget(paragraph, area);
}

/// The activity tail — the most recent log lines, filling whatever
/// space the rows left.
fn render_activity_tail(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let dim = Style::default().fg(crate::tui::palette::dim_color(app.palette));
    let tail: Vec<Line<'static>> = if app.autopilot.log_tail.is_empty() {
        vec![Line::from(Span::styled("(no activity yet)", dim))]
    } else {
        app.autopilot
            .log_tail
            .iter()
            .rev()
            .take(area.height as usize)
            .map(|l| Line::from(Span::raw(l.clone())))
            .collect()
    };
    frame.render_widget(Paragraph::new(tail), area);
}

/// One queued-milestone row's worth of data, as the takeover needs it.
/// A projection of [`crate::tui::autopilot::QueueRow`] with the
/// fields the takeover renders, plus the cycle count (which the typed
/// queue does not carry — it lives on the detail panel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakeoverRow {
    pub id: String,
    pub lifecycle: String,
    pub cycle: u64,
    /// True for the row the drive is currently working on. The
    /// takeover marks it so a glance identifies the active step.
    pub active: bool,
}

/// The takeover's rows. Prefers the live queue view (populated by the
/// poller from `mp autopilot session show`); falls back to the setup
/// form's per-run selection so a run that started before the first
/// refresh still shows what is queued.
pub fn queued_milestones(app: &App) -> Vec<TakeoverRow> {
    if let Some(view) = &app.autopilot.queue_view {
        if !view.rows.is_empty() {
            return view
                .rows
                .iter()
                .map(|row| TakeoverRow {
                    id: row.milestone_id.clone(),
                    lifecycle: row.lifecycle.clone(),
                    // The typed queue carries no cycle count; the drive
                    // has not reported one until the first refresh
                    // lands, so render 1 rather than a misleading 0.
                    cycle: 1,
                    active: row.active,
                })
                .collect();
        }
    }
    app.autopilot
        .setup
        .selected
        .iter()
        .map(|id| TakeoverRow {
            id: id.clone(),
            lifecycle: "in-progress".to_string(),
            cycle: 1,
            active: false,
        })
        .collect()
}

/// A fixed-width lifecycle-position bar: filled up to the milestone's
/// current stage, hollow after it. Mirrors the canonical lifecycle
/// order so two milestones at different stages are comparable at a
/// glance.
pub fn lifecycle_position_bar(lifecycle: &str) -> String {
    let nodes = crate::tui::autopilot::LIFECYCLE_NODES;
    let filled = nodes
        .iter()
        .position(|n| *n == lifecycle)
        .map(|i| i + 1)
        .unwrap_or(0);
    let width = 7usize;
    let bar: String = (0..width)
        .map(|i| if i < filled { '#' } else { '.' })
        .collect();
    format!("[{bar}]")
}

/// The one-line setup summary under Start. Exposed so the golden and
/// the renderer cannot quote different text.
pub fn setup_summary(app: &App) -> String {
    app.autopilot.setup.summary()
}

/// Whether the run mode chip the operator last selected is detached.
/// Exposed for the golden so the renderer and the test agree.
pub fn run_mode(app: &App) -> RunMode {
    app.autopilot.setup.run_mode
}
