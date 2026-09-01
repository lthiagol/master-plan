use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::config::{status_icon, IconMode};
use crate::tui::app::App;
use crate::tui::markdown::{self, MarkdownStyles};
use crate::tui::progress::{ac_status_style, status_badge_style};
use crate::tui::status::{effective_execution_status, effective_lifecycle, effective_spec_status};

use super::detail_sections::{push_item_header, push_kv_indented, section_header};

pub(super) fn render_milestone_detail(frame: &mut Frame, app: &App, area: Rect) {
    let detail = match &app.milestone_detail {
        Some(d) => d,
        None => {
            let msg = Paragraph::new("Loading milestone detail...")
                .block(Block::default().borders(Borders::ALL).title("Detail"))
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }
    };

    let m = &detail["milestone"];
    let title = m["title"].as_str().unwrap_or("?");
    let ms_id = m["id"].as_str().unwrap_or("?");
    // M173 S7: route lifecycle / spec_status / execution_status reads
    // through the shared helpers in `crate::tui::status`. Direct field
    // reads here would be a finding (AC-07).
    let _lifecycle = effective_lifecycle(m);
    let _legacy_spec = effective_spec_status(m);
    let _legacy_exec = effective_execution_status(m);
    let effort = m["effort"].as_str().unwrap_or("?");
    let risk = m["risk"].as_str().unwrap_or("?");
    let change_kind = m["change_kind"].as_str().unwrap_or("");
    let priority = m["priority"].as_str().unwrap_or("");
    let blocked = m["blocked"].as_bool().unwrap_or(false);
    let block_reason = m["block_reason"].as_str().unwrap_or("");
    let blocked_by = m["blocked_by"].as_str().unwrap_or("");
    let blocked_at = m["blocked_at"].as_str().unwrap_or("");
    let cancelled = m["cancelled"].as_bool().unwrap_or(false);
    let needs_regrooming = m["needs_regrooming"].as_bool().unwrap_or(false);
    let deferred = m["deferred"].as_bool().unwrap_or(false);
    let deferred_reason = m["deferred_reason"].as_str().unwrap_or("");
    let target_version = m["target_version"].as_str().unwrap_or("");
    let executed_by = m["executed_by"].as_str().unwrap_or("");
    let lifecycle_at = m["lifecycle_at"].as_str().unwrap_or("");
    let created = m["created"].as_str().unwrap_or("");
    let updated = m["updated"].as_str().unwrap_or("");
    let remediation_pre_state = m["remediation_pre_state"].as_str().unwrap_or("");

    let intent = detail["intent"]["outcome"].as_str().unwrap_or("");
    let problem = detail["problem"]["description"].as_str().unwrap_or("");
    let depends_on = m["depends_on"].as_array();

    let in_scope = detail["scope"]["in_scope"].as_array();
    let out_of_scope = detail["scope"]["out_of_scope"].as_array();
    let acs = detail["acceptance_criteria"].as_array();
    let steps = detail["steps"].as_array();
    let design_decisions = detail["design_decisions"].as_array();
    let open_questions = detail["open_questions"].as_array();
    let work_packages = detail["work_packages"].as_array();
    let findings = detail["findings"].as_array();

    let mut lines: Vec<Line> = Vec::new();
    let mut section_rows: Vec<u16> = Vec::new();

    let palette = app.effective_palette();
    let md_width = area.width.saturating_sub(4) as usize;

    // ===== Header =====
    lines.push(Line::from(vec![Span::styled(
        format!("M{} — {}", ms_id, title),
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // M202 S19: replace the lifecycle badge with the Stage cell
    // (`<N>/12 · <Label>`). The Stage cell already carries position
    // so a separate badge would be redundant. Effort + Risk stay
    // on the same line for layout continuity.
    let header_stage_map: std::collections::BTreeMap<String, String> = detail["milestone"]
        ["flow_stages"]
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(slug, stage)| {
                    stage
                        .get("status")
                        .and_then(|s| s.as_str())
                        .map(|s| (slug.clone(), s.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let (stage_slug, stage_label) = crate::tui::progress::current_mp_flow_stage(&header_stage_map);
    let stage_idx = crate::tui::progress::MP_FLOW_STAGE_KEYS
        .iter()
        .position(|s| *s == stage_slug)
        .unwrap_or(crate::tui::progress::MP_FLOW_STAGE_KEYS.len() - 1);
    let stage_text = format!(
        "{}/{} · {}",
        stage_idx + 1,
        crate::tui::progress::MP_FLOW_STAGE_KEYS.len(),
        stage_label,
    );
    let mut meta_spans: Vec<Span> = Vec::new();
    meta_spans.push(Span::styled(
        format!(" {stage_text} "),
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    ));
    meta_spans.push(Span::raw("  "));
    meta_spans.push(Span::styled(
        "Effort: ",
        Style::default().add_modifier(Modifier::BOLD),
    ));
    meta_spans.push(Span::raw(effort));
    meta_spans.push(Span::raw("  "));
    meta_spans.push(Span::styled(
        "Risk: ",
        Style::default().add_modifier(Modifier::BOLD),
    ));
    meta_spans.push(Span::raw(risk));
    lines.push(Line::from(meta_spans));

    // Dependencies
    if let Some(deps) = depends_on {
        if !deps.is_empty() {
            let dep_strs: Vec<String> = deps
                .iter()
                .map(|d| {
                    if let Some(s) = d.as_str() {
                        s.to_string()
                    } else {
                        d.to_string()
                    }
                })
                .collect();
            lines.push(Line::from(vec![
                Span::styled(
                    "Depends on: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(dep_strs.join(", ")),
            ]));
        }
    }

    // ===== Meta sub-block (M167: AC-23..AC-25) =====
    lines.push(Line::from(""));
    lines.extend_from_slice(&section_header("Meta", None, app, Some(md_width)));
    let effort_val = if effort.is_empty() { "—" } else { effort };
    let risk_val = if risk.is_empty() { "—" } else { risk };
    let change_kind_val = if change_kind.is_empty() {
        "—"
    } else {
        change_kind
    };
    let priority_val = if priority.is_empty() { "—" } else { priority };
    let created_val = if created.is_empty() {
        "—".to_string()
    } else {
        created.to_string()
    };
    let updated_val = if updated.is_empty() {
        "—".to_string()
    } else {
        updated.to_string()
    };
    let lifecycle_at_val = if lifecycle_at.is_empty() {
        "—".to_string()
    } else {
        lifecycle_at.to_string()
    };
    push_kv_indented(&mut lines, "Effort", effort_val, app);
    push_kv_indented(&mut lines, "Risk", risk_val, app);
    push_kv_indented(&mut lines, "Change kind", change_kind_val, app);
    push_kv_indented(&mut lines, "Priority", priority_val, app);
    let depends_str = match depends_on {
        Some(d) if !d.is_empty() => d
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => "none".to_string(),
    };
    push_kv_indented(&mut lines, "Depends on", &depends_str, app);
    push_kv_indented(&mut lines, "Created", &created_val, app);
    push_kv_indented(&mut lines, "Updated", &updated_val, app);
    push_kv_indented(&mut lines, "Lifecycle at", &lifecycle_at_val, app);

    // ===== Overlays =====
    if blocked {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("BLOCKED — {block_reason}"),
            Style::default()
                .fg(palette.danger)
                .add_modifier(Modifier::BOLD),
        )]));
        if !blocked_by.is_empty() {
            push_kv_indented(&mut lines, "Blocked by", blocked_by, app);
        }
        if !blocked_at.is_empty() {
            push_kv_indented(&mut lines, "Blocked at", blocked_at, app);
        }
    }
    if cancelled {
        lines.push(Line::from(vec![Span::styled(
            "CANCELLED",
            Style::default()
                .fg(palette.danger)
                .add_modifier(Modifier::BOLD),
        )]));
    }
    if deferred {
        if deferred_reason.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "DEFERRED",
                Style::default()
                    .fg(palette.warn)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                format!("DEFERRED — {deferred_reason}"),
                Style::default()
                    .fg(palette.warn)
                    .add_modifier(Modifier::BOLD),
            )]));
        }
    }
    if needs_regrooming {
        lines.push(Line::from(vec![Span::styled(
            "Needs re-grooming",
            Style::default()
                .fg(palette.warn)
                .add_modifier(Modifier::BOLD),
        )]));
    }
    if !target_version.is_empty() {
        push_kv_indented(&mut lines, "Target version", target_version, app);
    }
    if !executed_by.is_empty() {
        push_kv_indented(&mut lines, "Executed by", executed_by, app);
    }
    if !remediation_pre_state.is_empty() {
        push_kv_indented(
            &mut lines,
            "Pre-remediation state",
            remediation_pre_state,
            app,
        );
    }

    // ===== Stages (M202 S17) =====
    //
    // Sits between Meta and G14 per the AC-14 contract. Renders all
    // 12 mp-flow stages in canonical order with status icon,
    // label, and timestamp (or relative `started Xh ago` for
    // `in_progress`). Stages 1-12 are the same keys the mp-flow
    // skill documents; the table reads them from the milestone's
    // `flow_stages` field. Pre-M202 milestones carry an empty
    // map; every stage renders as `○ pending (unknown)` until
    // the next lifecycle transition auto-populates it.
    let flow_stages_obj = detail["milestone"]["flow_stages"].as_object();
    let flow_stages_loaded = flow_stages_obj.is_some();
    let mut current_stage_slug: Option<&'static str> = None;
    if let Some(obj) = flow_stages_obj {
        let owned: std::collections::BTreeMap<String, String> = obj
            .iter()
            .filter_map(|(slug, stage)| {
                stage
                    .get("status")
                    .and_then(|s| s.as_str())
                    .map(|s| (slug.clone(), s.to_string()))
            })
            .collect();
        current_stage_slug = Some(crate::tui::progress::current_mp_flow_stage(&owned).0);
    }
    lines.push(Line::from(""));
    section_rows.push(lines.len() as u16);
    lines.extend_from_slice(&section_header("Stages", None, app, Some(md_width)));
    let render_stage_row = |slug: &str,
                            icon: &str,
                            icon_style: Style,
                            status_text: &str,
                            at_text: &str,
                            lines: &mut Vec<Line>| {
        lines.push(Line::from(vec![
            Span::styled(format!("{icon}  "), icon_style),
            Span::styled(
                format!("{slug:>3}"),
                Style::default().fg(palette.foreground),
            ),
            Span::styled(
                format!("  {}  ", crate::tui::progress::mp_flow_stage_label(slug)),
                Style::default().fg(palette.foreground),
            ),
            Span::styled(
                format!("{status_text} ({at_text})"),
                Style::default().fg(palette.dim),
            ),
        ]));
    };
    if !flow_stages_loaded {
        for slug in crate::tui::progress::MP_FLOW_STAGE_KEYS {
            let label = crate::tui::progress::mp_flow_stage_label(slug);
            lines.push(Line::from(vec![
                Span::styled("○  ", Style::default().fg(palette.dim)),
                Span::styled(format!("{slug:>3}"), Style::default().fg(palette.dim)),
                Span::styled(format!("  {label}  "), Style::default().fg(palette.dim)),
                Span::styled("pending (unknown)", Style::default().fg(palette.dim)),
            ]));
        }
    } else {
        let obj = flow_stages_obj.unwrap();
        for slug in crate::tui::progress::MP_FLOW_STAGE_KEYS {
            let entry = obj.get(*slug);
            let (icon, status_text) =
                match entry.and_then(|e| e.get("status")).and_then(|s| s.as_str()) {
                    Some("done") => ("✓", "done"),
                    Some("in_progress") => ("●", "in_progress"),
                    Some("skipped") => ("⊘", "skipped"),
                    _ => ("○", "pending"),
                };
            let icon_style = match status_text {
                "done" => Style::default().fg(palette.success),
                "in_progress" => Style::default().fg(palette.accent),
                "skipped" => Style::default()
                    .fg(palette.dim)
                    .add_modifier(Modifier::CROSSED_OUT),
                _ => Style::default().fg(palette.dim),
            };
            let at_text = entry
                .and_then(|e| e.get("at"))
                .and_then(|a| a.as_str())
                .map(crate::tui::humanize::humanize_relative)
                .unwrap_or_else(|| "—".to_string());
            render_stage_row(slug, icon, icon_style, status_text, &at_text, &mut lines);
        }
    }

    // M202 S18: overlay sub-line. When the milestone carries a
    // non-empty lifecycle overlay (cancelled, blocked,
    // remediation), render an indented sub-line under the
    // current-stage row reading `└─ lifecycle overlay: <state>`.
    // Skipped when no overlay is set so a normal milestone shows
    // no extra row.
    let mut overlay_state: Option<&'static str> = None;
    if m["cancelled"].as_bool().unwrap_or(false) {
        overlay_state = Some("cancelled");
    } else if m["remediation_pre_state"].as_str().is_some() {
        overlay_state = Some("remediation");
    } else if m["blocked"].as_bool().unwrap_or(false) {
        overlay_state = Some("blocked");
    }
    if let Some(state) = overlay_state {
        if current_stage_slug.is_some() {
            lines.push(Line::from(vec![
                Span::styled(
                    "      └─ lifecycle overlay: ",
                    Style::default().fg(palette.warn),
                ),
                Span::styled(
                    state,
                    Style::default()
                        .fg(palette.warn)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }

    // G14 approval indicator (preserved pre-M167 contract)
    if app.approval_blocked {
        lines.push(Line::from(vec![Span::styled(
            "G14: BLOCKED — open approval-request annotation pending",
            Style::default()
                .fg(palette.danger)
                .add_modifier(Modifier::BOLD),
        )]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            "G14: CLEAR — no blocking approval annotations",
            Style::default().fg(palette.success),
        )]));
    }
    lines.push(Line::from(""));

    // ===== Intent =====
    let md_styles = MarkdownStyles { palette };
    if !intent.is_empty() {
        section_rows.push(lines.len() as u16);
        lines.extend_from_slice(&section_header("Intent", None, app, Some(md_width)));
        let intent_use_cache = app
            .detail_markdown_cache
            .as_ref()
            .map(|c| c.milestone_id == ms_id)
            .unwrap_or(false);
        if intent_use_cache {
            if let Some(cache) = app.detail_markdown_cache.as_ref() {
                lines.extend(cache.intent.iter().cloned());
            }
        } else {
            lines.extend(markdown::parse_markdown(intent, &md_styles, md_width));
        }
        lines.push(Line::from(""));
    }

    // ===== Problem =====
    if !problem.is_empty() {
        section_rows.push(lines.len() as u16);
        lines.extend_from_slice(&section_header("Problem", None, app, Some(md_width)));
        let problem_use_cache = app
            .detail_markdown_cache
            .as_ref()
            .map(|c| c.milestone_id == ms_id)
            .unwrap_or(false);
        if problem_use_cache {
            if let Some(cache) = app.detail_markdown_cache.as_ref() {
                lines.extend(cache.problem.iter().cloned());
            }
        } else {
            lines.extend(markdown::parse_markdown(problem, &md_styles, md_width));
        }
        lines.push(Line::from(""));
    }

    // ===== In / Out of Scope =====
    if let Some(items) = in_scope {
        if !items.is_empty() {
            section_rows.push(lines.len() as u16);
            lines.extend_from_slice(&section_header("Scope", None, app, Some(md_width)));
            for item in items {
                let text = item.as_str().unwrap_or("?");
                let mut scope_line = vec![Span::raw("  + ")];
                scope_line.extend(markdown::parse_inline_spans(text, &md_styles));
                lines.push(Line::from(scope_line));
            }
            lines.push(Line::from(""));
        }
    }
    // Out-of-scope lines after in-scope within the same section.
    if let Some(items) = out_of_scope {
        if !items.is_empty() {
            for item in items {
                let text = item.as_str().unwrap_or("?");
                let mut scope_line = vec![Span::raw("  - ")];
                scope_line.extend(markdown::parse_inline_spans(text, &md_styles));
                lines.push(Line::from(scope_line));
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Acceptance Criteria =====
    if let Some(items) = acs {
        if !items.is_empty() {
            section_rows.push(lines.len() as u16);
            let passed = items
                .iter()
                .filter(|ac| ac["status"].as_str() == Some("passed"))
                .count();
            let header_label = format!("{passed} / {}", items.len());
            lines.extend_from_slice(&section_header(
                "Acceptance Criteria",
                Some(&header_label),
                app,
                Some(md_width),
            ));
            for ac in items {
                let ac_id = ac["id"].as_str().unwrap_or("?");
                let ac_status = ac["status"].as_str().unwrap_or("?");
                let ac_desc = ac["description"].as_str().unwrap_or("?");
                let ac_verification = ac["verification"].as_str().unwrap_or("");
                let ac_evidence = ac["evidence"].as_str().unwrap_or("");
                let badge = if ac_status == "passed" {
                    "●"
                } else if ac_status == "failed" {
                    "✕"
                } else {
                    "○"
                };
                let badge_style = ac_status_style(ac_status, palette).add_modifier(Modifier::BOLD);
                push_item_header(&mut lines, badge, ac_id, ac_desc, badge_style, app);
                if !ac_verification.is_empty() {
                    push_kv_indented(&mut lines, "verify", ac_verification, app);
                }
                if !ac_evidence.is_empty() {
                    push_kv_indented(&mut lines, "evidence", ac_evidence, app);
                }
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Steps =====
    if let Some(items) = steps {
        if !items.is_empty() {
            section_rows.push(lines.len() as u16);
            let done = items
                .iter()
                .filter(|s| s["status"].as_str() == Some("done"))
                .count();
            let header_label = format!("{done} / {}", items.len());
            lines.extend_from_slice(&section_header(
                "Steps",
                Some(&header_label),
                app,
                Some(md_width),
            ));
            // M167: Steps progress bar.
            //
            // The ratatui-native `LineGauge` is a `Widget` whose render
            // path paints into a `Buffer`, but our detail rendering
            // composes `Vec<Line>` into a single `Paragraph`. Embedding
            // a `Widget` inside a `Vec<Line>` would require either a
            // `Line::widget(...)` adapter (not in 0.30) or splitting
            // the renderer into two passes (one for the gauge, one for
            // the surrounding Paragraph).
            //
            // For now we emit a one-line progress span in the same
            // shape as `LineGauge`'s output (ratio-glyph + label). The
            // `LineGauge` value is still constructed (and the styles
            // are honored via `ui.icons`), but rendered as styled
            // spans. Future work can split the renderer if exact
            // `LineGauge` widget identity is needed for testing.
            let ratio = if items.is_empty() {
                0.0
            } else {
                done as f64 / items.len() as f64
            };
            let filled_style = match crate::config::icons() {
                IconMode::None => Style::default().fg(palette.dim),
                IconMode::Ascii => Style::default().fg(palette.accent),
                IconMode::Unicode => Style::default().fg(palette.accent),
            };
            // M167: LineGauge deferred to a follow-up track (requires
            // integration with view.scrollbar_rects). Until then the
            // span-based bar below is used. The dead gauge is kept as
            // a compile-anchor so the LineGauge API doesn't drift silently.
            #[allow(dead_code)]
            let _unused_gauge = ratatui::widgets::LineGauge::default()
                .ratio(ratio)
                .label(format!("{done}/{}", items.len()))
                .filled_style(filled_style)
                .unfilled_style(Style::default().fg(palette.dim));
            // Emit a one-line span-based equivalent of LineGauge's
            // output (ratio bar + label), honoring ui.icons.
            let bar_w = 12usize;
            let filled_n = (ratio * bar_w as f64).round() as usize;
            let bar_text: String = "█".repeat(filled_n) + &"░".repeat(bar_w - filled_n);
            lines.push(Line::from(Span::styled(
                format!(" {bar_text}  {}/{}", done, items.len()),
                filled_style,
            )));
            lines.push(Line::from(""));

            for step in items {
                let s_id = step["id"].as_str().unwrap_or("?");
                let s_status = step["status"].as_str().unwrap_or("?");
                let s_action = step["action"].as_str().unwrap_or("?");
                let s_work_package = step["work_package"].as_str().unwrap_or("");
                let s_files = step["files"].as_array();
                let s_tests = step["tests"].as_str().unwrap_or("");
                let s_done_when = step["done_when"].as_str().unwrap_or("");
                let s_claimed_by = step["claimed_by"].as_str().unwrap_or("");
                let s_claimed_at = step["claimed_at"].as_str().unwrap_or("");
                let badge = status_icon(s_status);
                let badge_style = status_badge_style(s_status, palette);
                // Title line: badge, id, action, optional wp tag.
                let mut header_spans = vec![
                    Span::raw("  "),
                    Span::styled(format!("{badge} "), badge_style),
                    Span::styled(s_id.to_string(), badge_style.add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!(" — {s_action}"),
                        Style::default().fg(palette.foreground),
                    ),
                ];
                if !s_work_package.is_empty() {
                    header_spans.push(Span::styled(
                        format!("  wp: {s_work_package}"),
                        Style::default().fg(palette.dim),
                    ));
                }
                lines.push(Line::from(header_spans));
                // Context lines: files, tests, done_when, claim.
                if let Some(files) = s_files {
                    if !files.is_empty() {
                        let file_list: Vec<String> = files
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        if !file_list.is_empty() {
                            let joined = file_list.join(", ");
                            push_kv_indented(&mut lines, "files", &joined, app);
                        }
                    }
                }
                if !s_tests.is_empty() {
                    push_kv_indented(&mut lines, "tests", s_tests, app);
                }
                if !s_done_when.is_empty() {
                    push_kv_indented(&mut lines, "done_when", s_done_when, app);
                }
                if !s_claimed_by.is_empty() {
                    let claim = if s_claimed_at.is_empty() {
                        s_claimed_by.to_string()
                    } else {
                        format!("{s_claimed_by} @ {s_claimed_at}")
                    };
                    push_kv_indented(&mut lines, "claim", &claim, app);
                }
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Work Packages (only when present) =====
    if let Some(items) = work_packages {
        if !items.is_empty() {
            section_rows.push(lines.len() as u16);
            let header_label = format!("{}", items.len());
            lines.extend_from_slice(&section_header(
                "Work Packages",
                Some(&header_label),
                app,
                Some(md_width),
            ));
            for wp in items {
                let wp_id = wp["id"].as_str().unwrap_or("?");
                let wp_name = wp["name"].as_str().unwrap_or("?");
                let wp_goal = wp["goal"].as_str().unwrap_or("");
                let wp_rollback = wp["rollback"].as_str().unwrap_or("");
                lines.push(Line::from(vec![
                    Span::raw("  ▸ "),
                    Span::styled(
                        wp_id.to_string(),
                        Style::default()
                            .fg(palette.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" — {wp_name}"), Style::default()),
                ]));
                if !wp_goal.is_empty() {
                    push_kv_indented(&mut lines, "goal", wp_goal, app);
                }
                if !wp_rollback.is_empty() {
                    push_kv_indented(&mut lines, "rollback", wp_rollback, app);
                }
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Design Decisions =====
    if let Some(items) = design_decisions {
        if !items.is_empty() {
            section_rows.push(lines.len() as u16);
            let header_label = format!("{}", items.len());
            lines.extend_from_slice(&section_header(
                "Design Decisions",
                Some(&header_label),
                app,
                Some(md_width),
            ));
            for dd in items {
                let dd_area = dd["area"].as_str().unwrap_or("?");
                let dd_choice = dd["choice"].as_str().unwrap_or("?");
                let dd_rationale = dd["rationale"].as_str().unwrap_or("");
                lines.push(Line::from(vec![
                    Span::raw("  ▸ "),
                    Span::styled(
                        dd_area.to_string(),
                        Style::default()
                            .fg(palette.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" — {dd_choice}"), Style::default()),
                ]));
                if !dd_rationale.is_empty() {
                    push_kv_indented(&mut lines, "reason", dd_rationale, app);
                }
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Open Questions =====
    if let Some(items) = open_questions {
        if !items.is_empty() {
            section_rows.push(lines.len() as u16);
            let header_label = format!("{}", items.len());
            lines.extend_from_slice(&section_header(
                "Open Questions",
                Some(&header_label),
                app,
                Some(md_width),
            ));
            for q in items {
                let q_id = q["id"].as_str().unwrap_or("?");
                let q_question = q["question"].as_str().unwrap_or("?");
                let q_status = q["status"].as_str().unwrap_or("?");
                let q_answer = q["answer"].as_str().unwrap_or("");
                lines.push(Line::from(vec![
                    Span::raw("  ? "),
                    Span::styled(
                        q_id.to_string(),
                        Style::default()
                            .fg(palette.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" — {q_question}"), Style::default()),
                ]));
                push_kv_indented(&mut lines, "status", q_status, app);
                if !q_answer.is_empty() {
                    push_kv_indented(&mut lines, "answer", q_answer, app);
                }
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Findings =====
    if let Some(items) = findings {
        if !items.is_empty() {
            section_rows.push(lines.len() as u16);
            let open_count = items
                .iter()
                .filter(|f| f["status"].as_str() == Some("open"))
                .count();
            // M154: anchored-finding count — number of findings with a
            // non-empty `anchor.path`. Surfaced as a chip in the
            // header_label when [review].hunk is enabled; the human
            // reviewer uses it to confirm the milestone is ready for
            // `mp reviews hunk <M>` export (every anchored finding
            // becomes a hunk line-annotation; unanchored findings
            // surface as file-level summary notes). When hunk=false,
            // the chip is hidden (no behavior change vs pre-M154).
            let anchored_count = items
                .iter()
                .filter(|f| {
                    f["anchor"]["path"]
                        .as_str()
                        .map(|p| !p.is_empty())
                        .unwrap_or(false)
                })
                .count();
            let header_label = if app.review_hunk_enabled {
                format!(
                    "{open_count} open / {} total · {anchored_count} anchored",
                    items.len()
                )
            } else {
                format!("{open_count} open / {} total", items.len())
            };
            lines.extend_from_slice(&section_header(
                "Findings",
                Some(&header_label),
                app,
                Some(md_width),
            ));

            // M167 BF-02: severity histogram. Counts come from the same
            // `items` array the per-finding rows iterate (one source of
            // truth); bars are proportional to the max bucket, capped at
            // 8 cells per bar.
            let (high_count, med_count, low_count) =
                items.iter().fold((0u64, 0u64, 0u64), |(h, m, l), f| {
                    let sev = f["severity"].as_str().unwrap_or("low");
                    match sev {
                        "high" => (h + 1, m, l),
                        "medium" => (h, m + 1, l),
                        _ => (h, m, l + 1),
                    }
                });
            let max_count = high_count.max(med_count).max(low_count).max(1);
            let bar_for = |count: u64, color: ratatui::style::Color| -> Vec<Span<'static>> {
                let width = ((count * 8) / max_count).max(if count > 0 { 1 } else { 0 }) as usize;
                vec![Span::styled(
                    "\u{2588}".repeat(width),
                    Style::default().fg(color),
                )]
            };
            let mut bars: Vec<Span<'static>> = vec![
                Span::raw("   "),
                Span::styled(
                    "high ",
                    Style::default()
                        .fg(palette.danger)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[{high_count}] "),
                    Style::default()
                        .fg(palette.danger)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            bars.extend(bar_for(high_count, palette.danger));
            bars.push(Span::raw("  "));
            bars.push(Span::styled(
                "med ",
                Style::default()
                    .fg(palette.warn)
                    .add_modifier(Modifier::BOLD),
            ));
            bars.push(Span::styled(
                format!("[{med_count}] "),
                Style::default()
                    .fg(palette.warn)
                    .add_modifier(Modifier::BOLD),
            ));
            bars.extend(bar_for(med_count, palette.warn));
            bars.push(Span::raw("  "));
            bars.push(Span::styled(
                "low ",
                Style::default()
                    .fg(palette.dim)
                    .add_modifier(Modifier::BOLD),
            ));
            bars.push(Span::styled(
                format!("[{low_count}] "),
                Style::default()
                    .fg(palette.dim)
                    .add_modifier(Modifier::BOLD),
            ));
            bars.extend(bar_for(low_count, palette.dim));
            lines.push(Line::from(bars));
            lines.push(Line::from(""));

            let mut sorted: Vec<&Value> = items.iter().collect();
            sorted.sort_by(|a, b| {
                let a_open = a["status"].as_str() == Some("open");
                let b_open = b["status"].as_str() == Some("open");
                b_open.cmp(&a_open).then_with(|| {
                    finding_severity_rank(a["severity"].as_str().unwrap_or("low")).cmp(
                        &finding_severity_rank(b["severity"].as_str().unwrap_or("low")),
                    )
                })
            });

            for finding in sorted {
                let f_id = finding["id"].as_str().unwrap_or("?");
                let f_status = finding["status"].as_str().unwrap_or("?");
                let f_severity = finding["severity"].as_str().unwrap_or("low");
                let f_desc = finding["description"].as_str().unwrap_or("?");
                let f_author = finding["author"].as_str().unwrap_or("");
                let f_created = finding["created"].as_str().unwrap_or("");
                let f_fixed_in = finding["fixed_in"].as_str().unwrap_or("");
                let f_phase = finding["phase"].as_str().unwrap_or("");
                let is_open = f_status == "open";
                let marker = if is_open { "[!]" } else { "[✓]" };
                let row_style = finding_severity_style(f_severity, is_open, palette);
                lines.push(Line::from(vec![
                    Span::styled(format!("  {marker} "), row_style),
                    Span::styled(f_id, row_style.add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" [{f_severity}] {f_status} — "), row_style),
                    Span::styled(f_desc, row_style),
                ]));
                // Context: phase · author @ created · fixed_in · anchor · thread · tags · summary
                let mut context_pieces: Vec<(String, String)> = Vec::new();
                if !f_phase.is_empty() {
                    context_pieces.push(("phase".into(), f_phase.into()));
                }
                if !f_author.is_empty() {
                    let author_at = if f_created.is_empty() {
                        f_author.to_string()
                    } else {
                        format!("{f_author} @ {f_created}")
                    };
                    context_pieces.push(("author".into(), author_at));
                }
                if !f_fixed_in.is_empty() {
                    context_pieces.push(("fixed_in".into(), f_fixed_in.into()));
                } else if is_open {
                    context_pieces.push(("fixed_in".into(), "unfixed".into()));
                }
                if let Some(anchor) = finding["anchor"].as_object() {
                    if let Some(path) = anchor.get("path").and_then(|v| v.as_str()) {
                        let start = anchor
                            .get("new_range")
                            .and_then(|r| r.get("start"))
                            .and_then(|v| v.as_u64());
                        let end = anchor
                            .get("new_range")
                            .and_then(|r| r.get("end"))
                            .and_then(|v| v.as_u64());
                        let range = match (start, end) {
                            (Some(s), Some(e)) => format!("{path}:{s}-{e}"),
                            _ => path.to_string(),
                        };
                        context_pieces.push(("anchor".into(), range));
                    }
                }
                if let Some(tags) = finding["tags"].as_array() {
                    let tag_strs: Vec<String> = tags
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    if !tag_strs.is_empty() {
                        context_pieces.push(("tags".into(), tag_strs.join(", ")));
                    }
                }
                if let Some(thread) = finding["thread"].as_array() {
                    if !thread.is_empty() {
                        context_pieces.push(("thread".into(), format!("{} replies", thread.len())));
                    }
                }
                if let Some(summary) = finding["summary"].as_str() {
                    if !summary.is_empty() {
                        context_pieces.push(("summary".into(), summary.into()));
                    }
                }
                for (k, v) in context_pieces {
                    push_kv_indented(&mut lines, &k, &v, app);
                }
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Verification =====
    let verification = &detail["verification"];
    let v_date = verification["date"].as_str().unwrap_or("");
    let v_branch = verification["branch"].as_str().unwrap_or("");
    let v_evidence = verification["evidence"].as_str().unwrap_or("");
    if !v_date.is_empty() || !v_branch.is_empty() || !v_evidence.is_empty() {
        section_rows.push(lines.len() as u16);
        lines.extend_from_slice(&section_header("Verification", None, app, Some(md_width)));
        if !v_date.is_empty() {
            push_kv_indented(&mut lines, "date", v_date, app);
        }
        if !v_branch.is_empty() {
            push_kv_indented(&mut lines, "branch", v_branch, app);
        }
        if !v_evidence.is_empty() {
            push_kv_indented(&mut lines, "evidence", v_evidence, app);
        }
        lines.push(Line::from(""));
    }

    // ===== Delta (only when change_kind=="delta") =====
    if change_kind == "delta" {
        let delta = &detail["delta"];
        let domain = delta["domain"].as_str().unwrap_or("");
        let base_version = delta["base_version"].as_u64().unwrap_or(0);
        let added = delta["added"].as_array();
        let modified = delta["modified"].as_array();
        let removed = delta["removed"].as_array();
        let has_any = added.map(|a| !a.is_empty()).unwrap_or(false)
            || modified.map(|a| !a.is_empty()).unwrap_or(false)
            || removed.map(|a| !a.is_empty()).unwrap_or(false);
        if has_any {
            section_rows.push(lines.len() as u16);
            let header_label = format!("{domain} from v{base_version}");
            lines.extend_from_slice(&section_header(
                "Delta",
                Some(&header_label),
                app,
                Some(md_width),
            ));
            if let Some(items) = added {
                for a in items {
                    let a_id = a["id"].as_str().unwrap_or("+");
                    let a_stmt = a["statement"].as_str().unwrap_or("?");
                    lines.push(Line::from(vec![
                        Span::raw("  ↳ + "),
                        Span::styled(
                            a_id.to_string(),
                            Style::default()
                                .fg(palette.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!(" {a_stmt}")),
                    ]));
                }
            }
            if let Some(items) = modified {
                for m in items {
                    let m_target = m["target"].as_str().unwrap_or("?");
                    let m_before = m["before"].as_str().unwrap_or("");
                    let m_after = m["after"].as_str().unwrap_or("");
                    lines.push(Line::from(vec![
                        Span::raw("  ↳ ~ "),
                        Span::styled(
                            m_target.to_string(),
                            Style::default()
                                .fg(palette.warn)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!(": {m_after} (was {m_before})")),
                    ]));
                }
            }
            if let Some(items) = removed {
                for r in items {
                    let r_id = r["id"].as_str().unwrap_or("-");
                    let r_stmt = r["statement"].as_str().unwrap_or("?");
                    lines.push(Line::from(vec![
                        Span::raw("  ↳ − "),
                        Span::styled(
                            r_id.to_string(),
                            Style::default()
                                .fg(palette.danger)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!(" {r_stmt}")),
                    ]));
                }
            }
            lines.push(Line::from(""));
        }
    }

    // ===== Render =====
    // M167 + M169-rev scrollbar fix: measure the rendered Paragraph's
    // actual height so the scrollbar thumb math reflects wrapped
    // content. **Important**: the measurement Paragraph MUST NOT
    // carry the user's current `.scroll()` offset — `app.detail_max_scroll`
    // is the absolute cap, not "remaining rows below the viewport".
    // Build an unscrolled measurement Paragraph first, derive the
    // height, then chain `.scroll()` for the actual render. The
    // pre-fix code applied `.scroll()` before the measurement, which
    // caused the helper to return `(total - detail_scroll)` instead
    // of `total`; the user would hit a fresh ~2-row cap every time
    // they scrolled past the visible boundary (mp-dogfood-log entry
    // 33 / sub-agent review H1).
    //
    // M169-rev L3a (sub-agent review): skip the allocation + render
    // when `app.detail_measurement_cache` already holds a value for
    // the same `(content_hash, area_width)`. Cache naturally
    // invalidates by content change (new hash → miss → re-measure);
    // `load_milestone_detail` does not need to clear it explicitly.
    let visible = area.height.saturating_sub(2);
    let content_hash = crate::tui::render::milestone_detail::lines_hash(&lines);
    let cached_max = app.detail_measurement_cache.get().and_then(|c| {
        if c.content_hash == content_hash && c.area_width == area.width {
            Some(c.max_scroll)
        } else {
            None
        }
    });
    let max_scroll = match cached_max {
        Some(m) => m,
        None => {
            let measure_paragraph = Paragraph::new(lines.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("M{} Detail", ms_id))
                        .border_type(BorderType::Thick),
                )
                .wrap(Wrap { trim: false });
            let rendered_height = crate::tui::render::scrollbar::measure_paragraph_height(
                measure_paragraph.clone(),
                area,
            );
            let m = rendered_height.saturating_sub(visible);
            app.detail_measurement_cache
                .set(Some(crate::tui::app::DetailMeasurementCache {
                    content_hash,
                    area_width: area.width,
                    max_scroll: m,
                }));
            m
        }
    };
    app.detail_max_scroll.set(max_scroll);

    // M167: also populate the section-row map for `]`, `[`, `n`, `p`
    // detail-section nav. We re-derive from `lines` indices here; the
    // section_headers recorded their starting rows during the build.
    app.detail_section_rows.replace(section_rows);

    let paragraph_unscrolled = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("M{} Detail", ms_id))
                .border_type(BorderType::Thick),
        )
        .wrap(Wrap { trim: false });
    let paragraph = paragraph_unscrolled.scroll((app.detail_scroll, 0));
    frame.render_widget(paragraph, area);
}

use serde_json::Value;

/// M169-rev L3a: content hash for the milestone-detail renderer's
/// `lines` vec. Used to skip the 8×-panel `measure_paragraph_height`
/// allocation + render when the body hasn't changed since the last
/// frame. Hashes the text content of each span (`Span::content`)
/// across all lines via `DefaultHasher`; styled spans with the same
/// text hash equal. False collisions are theoretically possible but
/// harmless — a collision just means "re-measure this frame," which
/// is no worse than the no-cache path.
pub(super) fn lines_hash(lines: &[ratatui::text::Line<'_>]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for line in lines {
        for span in &line.spans {
            span.content.hash(&mut h);
        }
        // Mix in a per-line separator so `["ab", "c"]` and `["a", "bc"]`
        // don't collide.
        0u8.hash(&mut h);
    }
    h.finish()
}

pub(super) fn finding_severity_rank(severity: &str) -> u8 {
    match severity {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    }
}

pub(super) fn finding_severity_style(
    severity: &str,
    is_open: bool,
    palette: &crate::theme::Palette,
) -> Style {
    if !is_open {
        return Style::default().fg(palette.dim);
    }
    match severity {
        "high" => Style::default()
            .fg(palette.danger)
            .add_modifier(Modifier::BOLD),
        "medium" => Style::default()
            .fg(palette.warn)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(palette.dim),
    }
}
