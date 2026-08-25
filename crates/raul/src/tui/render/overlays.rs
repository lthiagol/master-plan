use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::{App, CoApprovalAction, CoApprovalState};
use crate::tui::key_combo::{format_key_combo, KeyCombo};
use crate::tui::mode::Mode;
use crate::tui::modes::settings::{
    help::help_for, keybind_default_label, value_for_key, SETTINGS_KEYS,
};
use crate::tui::render::modal::centered_popup_rect;
use crate::tui::view_state::ViewState;

/// First formatted combo in a binding, or empty when unbound. Mirrors the
/// private `Keybinds::primary` helper so overlays can render a one-key
/// legend directly from the same `Keybinds` struct the dispatcher resolves
/// against (M138 goal: the on-screen legend cannot drift from reality).
fn primary_key(combos: &[KeyCombo]) -> String {
    combos
        .first()
        .map(|c| format_key_combo(*c))
        .unwrap_or_default()
}

pub(super) fn render_annotation_thread(frame: &mut Frame, app: &App, area: Rect) {
    let visible = app.visible_annotations();

    if visible.is_empty() {
        let msg = if app.open_only {
            "No open annotations."
        } else {
            "No annotations for this milestone."
        };
        let paragraph = Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).title("Annotations"))
            .style(Style::default().fg(app.effective_palette().warn))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut items: Vec<ListItem> = Vec::new();
    for a in visible.iter() {
        let status_style = match a.status.as_str() {
            "open" => Style::default().fg(app.effective_palette().warn),
            "addressed" => Style::default().fg(app.effective_palette().accent),
            "resolved" => Style::default().fg(app.effective_palette().success),
            _ => Style::default(),
        };

        // Character-safe truncation avoids treating CJK byte length as width.
        let body_display = crate::text::truncate(&a.body, 60);

        // M167: selection styling is owned by `List::highlight_style`
        // (REVERSED). Per-item bg/fg styles for the selected row are
        // removed — the highlight style is the single source of truth.
        let line = Line::from(vec![
            Span::styled(
                format!(" {}", a.id),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" [{}]", a.status), status_style),
            Span::styled(format!(" {} | {}", a.kind, body_display), Style::default()),
        ]);
        items.push(ListItem::new(line));
    }

    let title = if app.open_only {
        format!("Annotations — open only ({})", visible.len())
    } else {
        format!("Annotations ({})", visible.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_type(BorderType::Thick),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(
        list,
        area,
        &mut ratatui::widgets::ListState::default()
            .with_selected(Some(app.selected_annotation_index)),
    );
}

pub(super) fn render_co_approval(frame: &mut Frame, app: &App, view: &ViewState, area: Rect) {
    let ann = match &app.co_approval_annotation {
        Some(a) => a,
        None => {
            let msg = Paragraph::new("No approval request selected.")
                .block(Block::default().borders(Borders::ALL).title("Co-Approval"))
                .alignment(Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }
    };

    // M135: read the pre-computed co-approval chunks from the view.
    let chunks = view
        .co_approval_chunks
        .expect("render_co_approval requires compute_view to have populated co_approval_chunks");

    let header_text = format!(
        " Co-Approval: {} on milestone {} ",
        ann.id,
        app.co_approval_milestone_id.as_deref().unwrap_or("?")
    );
    let header_block = Block::default()
        .borders(Borders::ALL)
        .title(header_text)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(app.effective_palette().warn));
    frame.render_widget(header_block, chunks[0]);

    let status_style = match ann.status.as_str() {
        "open" => Style::default().fg(app.effective_palette().warn),
        "addressed" => Style::default().fg(app.effective_palette().accent),
        "resolved" => Style::default().fg(app.effective_palette().success),
        _ => Style::default(),
    };

    let info = vec![
        Line::from(vec![
            Span::styled("Author: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&ann.author),
        ]),
        Line::from(vec![
            Span::styled("Kind: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&ann.kind),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(&ann.status, status_style),
        ]),
        Line::from(vec![
            Span::styled("Created: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&ann.created_at),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Request Body",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]),
        Line::from(""),
        Line::from(ann.body.as_str()),
    ];

    let detail_para = Paragraph::new(info)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Request Details"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(detail_para, chunks[1]);

    let action_label = match &app.co_approval_action {
        Some(CoApprovalAction::Approve) => "[ ▶ Approve ]",
        Some(CoApprovalAction::Reject) => "[ ▶ Reject  ]",
        _ => "[         ]",
    };

    // M138 code-review: drive the co-approval legend from `app.keybinds`
    // (the same struct the dispatcher resolves against). CoApproval maps
    // `Action::ToggleApproval` (bound to `p` by default) → Approve and
    // `Action::ReopenAnnotation` (bound to `R`) → Reject — so the old
    // hardcoded "[A]pprove" lied (pressing A did nothing).
    let approve_key = primary_key(&app.keybinds.approve);
    let reject_key = primary_key(&app.keybinds.reopen);
    let confirm_key = primary_key(&app.keybinds.enter);
    let back_keys = format!(
        "{}/{}",
        primary_key(&app.keybinds.escape),
        primary_key(&app.keybinds.quit),
    );

    let actions_text = format!(
        " [{}] Approve  [{}] Reject  {}  [{}] Confirm  [{}] Back",
        approve_key,
        reject_key,
        if app.co_approval_state == CoApprovalState::Confirmed {
            "✓ Done"
        } else if app.co_approval_state == CoApprovalState::Executing {
            "… Executing"
        } else {
            action_label
        },
        confirm_key,
        back_keys,
    );
    let actions_block = Block::default()
        .borders(Borders::ALL)
        .title("Actions")
        .style(if app.co_approval_state == CoApprovalState::Confirmed {
            Style::default().fg(app.effective_palette().success)
        } else {
            Style::default()
        });
    let actions_para = Paragraph::new(actions_text).block(actions_block);
    frame.render_widget(actions_para, chunks[2]);

    let status_text = match app.co_approval_state {
        CoApprovalState::Confirmed => " Confirmed — press Esc to return ".to_string(),
        CoApprovalState::Executing => " Executing co-approval… ".to_string(),
        CoApprovalState::Choosing => format!(
            " Select an action ({}/{}) then press {} to confirm ",
            approve_key, reject_key, confirm_key,
        ),
    };
    let status_block = Block::default()
        .borders(Borders::ALL)
        .title("Status")
        .style(if app.co_approval_state == CoApprovalState::Confirmed {
            Style::default().fg(app.effective_palette().success)
        } else {
            Style::default().fg(app.effective_palette().warn)
        });
    frame.render_widget(Paragraph::new(status_text).block(status_block), chunks[3]);
}

pub(super) fn render_help_overlay(frame: &mut Frame, app: &App, overlay_area: Rect) {
    // M199: the help overlay mirrors the footer's two-row split —
    // Per-lane (active lane's keys) shown first per Q-03
    // (contextual-first), then Global (the six universal bindings).
    // Both groups are generated from `app.keybinds` so the legend
    // can never drift from the actual dispatcher.
    let accent = Style::default()
        .fg(app.effective_palette().accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.effective_palette().dim);

    let (global, per_lane) = app
        .keybinds
        .help_entries_grouped(app.active_lane, app.content);

    // "Last action details" is a separate concern from the keyboard
    // legend — when the dispatcher set `last_action_error`, the
    // overlay surfaces the message text instead. (Preserved from
    // pre-M199 behavior.)
    if let Some(ref err) = app.last_action_error {
        let mut help_lines: Vec<Line> = vec![
            Line::from(vec![Span::styled(" Last action details ", accent)]),
            Line::from(""),
        ];
        for raw_line in err.lines() {
            help_lines.push(Line::from(format!("  {raw_line}")));
        }
        let paragraph = Paragraph::new(help_lines)
            .block(Block::default().borders(Borders::ALL).title(" Help "))
            .style(Style::default().fg(app.effective_palette().foreground))
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(paragraph, overlay_area);
        return;
    }

    let lane_label = app.active_lane.label();
    let mut help_lines: Vec<Line> = vec![
        Line::from(vec![Span::styled(" Keyboard Shortcuts ", accent)]),
        Line::from(""),
        // Per-lane group first per Q-03. When the active lane has
        // no per-tab keys (Path, Watch), show a single placeholder
        // line so the section is still visually present and the
        // user learns the empty state is normal.
        Line::from(vec![Span::styled(
            format!(" Per-lane ({lane_label})"),
            accent,
        )]),
    ];
    if per_lane.is_empty() {
        help_lines.push(Line::from(Span::styled(
            "  (no lane-specific keys — see Global)",
            dim,
        )));
    } else {
        // Render the per-lane group as one line per entry — the
        // overlay is small and one-line-per-entry keeps the keys
        // scannable. The label is human-readable; the keys come
        // from `Keybinds::footer_per_tab` so the overlay and the
        // footer can never drift.
        for (label, keys) in &per_lane {
            help_lines.push(Line::from(format!("  {} {}", keys.join(", "), label)));
        }
    }
    help_lines.push(Line::from(""));
    help_lines.push(Line::from(vec![Span::styled(" Global", accent)]));
    // Render the global group one entry per line, mirroring the
    // per-lane layout for visual consistency.
    for entry in &global {
        help_lines.push(Line::from(format!(
            "  {} {}",
            entry.keys_display(),
            entry.label
        )));
    }

    let paragraph = Paragraph::new(help_lines)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .style(Style::default().fg(app.effective_palette().foreground))
        .wrap(ratatui::widgets::Wrap { trim: false });

    frame.render_widget(paragraph, overlay_area);
}

pub(super) fn render_input_overlay(frame: &mut Frame, app: &App, overlay_area: Rect) {
    let state = match &app.active_mode {
        Mode::Input(s) => s,
        _ => return,
    };

    // M136: input buffer lives inside `Mode::Input(InputState)` so the
    // overlay can always trust the buffer is in sync with the mode.
    let instruction = format!(
        " Creating {} annotation on {} \n\n\
         Body (Enter to confirm, Esc to cancel):\n\
         {}",
        state.kind, state.target, state.buffer
    );

    let paragraph = Paragraph::new(instruction)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" New Annotation ")
                .border_type(BorderType::Double)
                .style(Style::default().bg(crate::tui::palette::overlay_backdrop(
                    app.effective_palette(),
                ))),
        )
        .style(Style::default().fg(app.effective_palette().foreground));

    frame.render_widget(paragraph, overlay_area);
}

/// M140: Settings lane — flat list with optional field-edit popup.
pub(super) fn render_settings_lane(frame: &mut Frame, app: &App, overlay_area: Rect) {
    use crate::tui::modes::settings::flat_key;

    let Some(state) = app.settings.as_ref() else {
        let msg = Paragraph::new("Loading settings…")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(crate::lanes::LANE_SETTINGS),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(app.effective_palette().dim));
        frame.render_widget(msg, overlay_area);
        return;
    };

    // M168 BF-03 layout:
    //   rows 0..2  (above the pane) — the lane title bar / footer
    //     come from `chrome.rs`; the renderer only paints inside
    //     `overlay_area` (the content pane).
    //   row 0..3   inside the pane — header strip
    //   row 3..    flat list (clamped by the pane's bottom)
    //
    // The previous four-band `modal_stack_areas` (header / content /
    // footer / actions) is gone. The field-edit popup (inner) keeps
    // its own Clear + Block — it's a small inner modal that benefits
    // from a clean underline.

    let palette = app.effective_palette();
    let dim = Style::default().fg(palette.dim);
    let accent = Style::default()
        .fg(palette.accent)
        .add_modifier(Modifier::BOLD);

    // ---- 3-row help box at the top of the pane (rows 0..2 inside
    //      `overlay_area`). The first row carries the key in accent;
    //      the next two rows carry the hard-coded prose from
    //      `modes::settings::help`.
    let help_rows = 3usize;
    let help_area = Rect {
        x: overlay_area.x,
        y: overlay_area.y,
        width: overlay_area.width,
        height: help_rows as u16,
    };
    if let Some((_section, key)) = flat_key(state.selected_idx) {
        let prose = help_for(key);
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(format!(" {key}"), accent)));
        for prose_line in prose {
            lines.push(Line::from(Span::styled(format!(" {prose_line}"), dim)));
        }
        // Pad with empty lines if the prose is shorter than help_rows-1.
        while lines.len() < help_rows {
            lines.push(Line::from(""));
        }
        frame.render_widget(Paragraph::new(lines), help_area);
    }

    // ---- Grouped list (BF-04): one section header row before each
    //      section's key rows. The cursor is over a Key row; Up/Down
    //      skip Section rows automatically (handled at the renderer
    //      level here — the dispatcher's selected_idx is the index
    //      into SETTINGS_KEYS, not into the rendered list).
    let list_y = overlay_area.y + help_rows as u16;
    let list_height = overlay_area.height.saturating_sub(help_rows as u16);
    if list_height == 0 {
        return;
    }
    let list_area = Rect {
        x: overlay_area.x,
        y: list_y,
        width: overlay_area.width,
        height: list_height,
    };

    // Build the full rendered row sequence (one entry per row,
    // counting Section headers as rows too). The cursor lands on the
    // Key row at SETTINGS_KEYS[state.selected_idx].
    let section_header = Style::default()
        .fg(palette.accent)
        .add_modifier(Modifier::BOLD);
    // Each entry: a Section row, or a Key row. We track (key_index,
    // is_section) per entry so cursor math is simple.
    enum RowKind {
        Section(&'static str),
        Key(usize, &'static str, &'static str), // (SETTINGS_KEYS index, section, key)
    }
    let mut rows: Vec<RowKind> = Vec::new();
    let mut last_section: Option<&'static str> = None;
    for (i, (section, key)) in SETTINGS_KEYS.iter().enumerate() {
        if Some(*section) != last_section {
            rows.push(RowKind::Section(section));
            last_section = Some(*section);
        }
        rows.push(RowKind::Key(i, section, key));
    }

    // Map `state.selected_idx` (an index into SETTINGS_KEYS) onto the
    // row index in the rendered list. We scan `rows` to find the row
    // whose Key index matches `state.selected_idx`; the rendered row
    // index is that scan offset.
    let selected = state
        .selected_idx
        .min(SETTINGS_KEYS.len().saturating_sub(1));
    let mut selected_row_idx: Option<usize> = None;
    for (i, r) in rows.iter().enumerate() {
        if let RowKind::Key(idx, _, _) = r {
            if *idx == selected {
                selected_row_idx = Some(i);
                break;
            }
        }
    }
    let selected_row_idx = selected_row_idx.unwrap_or(0);

    // Smooth-scroll the window so the cursor stays in view. Section
    // headers take a rendered row but they aren't Key rows; the
    // smooth-scroll offset is computed in `total_rows` (mixed Key +
    // Section) so the cursor never lands on a Section header even when
    // it's the first/last row.
    let visible_rows = list_height as usize;
    let total_rows = rows.len();
    let cursor_offset = selected_row_idx;
    let view_offset = total_rows.checked_sub(visible_rows).map_or(0, |_| {
        cursor_offset
            .saturating_sub(visible_rows.saturating_sub(2))
            .min(total_rows.saturating_sub(visible_rows))
    });
    let view_end = (view_offset + visible_rows).min(total_rows);

    // Render the visible window.
    let mut items: Vec<ListItem> = Vec::new();
    for r in &rows[view_offset..view_end] {
        match r {
            RowKind::Section(section) => {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("--- {section} ---"),
                    section_header,
                ))));
            }
            RowKind::Key(_, _section, key) => {
                let mut val = state
                    .staged_edits
                    .get(*key)
                    .cloned()
                    .unwrap_or_else(|| value_for_key(&state.config, key));
                if val.is_empty() {
                    if let Some(rest) = key.strip_prefix("keybinds.") {
                        if let Some(label) = keybind_default_label(rest) {
                            val = label;
                        }
                    }
                }
                // Key row: `  key = value` (no [section] prefix — the
                // section header above the group carries it).
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  {key} = {val}"),
                    Style::default(),
                ))));
            }
        }
    }

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(crate::tui::palette::on_accent_fg(palette))
            .bg(palette.accent)
            .add_modifier(Modifier::BOLD),
    );
    // Map the cursor's row index in the full `rows` vec to the
    // relative index within the visible window. The List widget's
    // `selected` is relative to the slice it was given.
    let list_selected = selected_row_idx
        .saturating_sub(view_offset)
        .min(view_end.saturating_sub(view_offset).saturating_sub(1));
    frame.render_stateful_widget(
        list,
        list_area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(list_selected)),
    );

    // Field-edit popup (inner) — unchanged from the pre-M168 shape.
    if let Some(edit) = &state.edit {
        let popup = centered_popup_rect(overlay_area, 50, 30);
        frame.render_widget(Clear, popup);

        let chars: Vec<char> = edit.buffer.chars().collect();
        let len = chars.len();
        let idx = edit.cursor.min(len);
        let (before, caret, after) = if idx == len {
            (
                chars[..idx].iter().collect::<String>(),
                " ".to_string(),
                String::new(),
            )
        } else {
            (
                chars[..idx].iter().collect::<String>(),
                chars[idx].to_string(),
                chars[idx + 1..].iter().collect::<String>(),
            )
        };
        let caret_style = {
            let (bg, fg) = crate::tui::palette::caret_block(palette);
            Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD)
        };
        let buf_line = Line::from(vec![
            Span::raw(before),
            Span::styled(caret, caret_style),
            Span::raw(after),
        ]);

        let key_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Edit {} ", edit.key));
        let inner = key_block.inner(popup);
        frame.render_widget(key_block, popup);

        if edit.errors.is_empty() {
            let body = Text::from(vec![
                buf_line,
                Line::from(""),
                Line::from(Span::styled(
                    "Enter: save  Esc: cancel",
                    Style::default().fg(palette.dim),
                )),
            ]);
            frame.render_widget(Paragraph::new(body), inner);
        } else {
            let body = Text::from(vec![
                buf_line,
                Line::from(""),
                Line::from(Span::styled(
                    edit.errors.join("; "),
                    Style::default().fg(palette.warn),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter: retry  Esc: cancel",
                    Style::default().fg(palette.dim),
                )),
            ]);
            frame.render_widget(Paragraph::new(body), inner);
        }
    }
}

pub(super) fn render_review_menu_overlay(frame: &mut Frame, app: &App, overlay_area: Rect) {
    // M136: review-menu state lives inside `Mode::ReviewMenu(_)`; pull it
    // out as a single match so adding a new mode-local field is localized
    // to `mode.rs`.
    let (items_vec, selected) = match &app.active_mode {
        Mode::ReviewMenu(menu) => (menu.items.clone(), menu.selected),
        _ => return,
    };

    // M163 AC-03: pre-flight gate — "Approve milestone" is greyed when the
    // milestone fails `mp plan verify-ac`.
    let gate_closed = app.preflight_gate.as_ref().is_none_or(|gate| !gate.open);
    let palette = app.effective_palette();

    let mut items: Vec<ListItem> = Vec::new();
    for item in items_vec.iter() {
        // M167: `List::highlight_style` is the sole selection painter;
        // per-item fg/bg styles removed.
        // M163 AC-03: grey out "Approve milestone" when gate is closed.
        let style = if item == "Approve milestone" && gate_closed {
            Style::default().fg(palette.dim)
        } else {
            Style::default()
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!(" {} ", item),
            style,
        ))));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Review Actions ")
                .border_type(BorderType::Plain),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(
        list,
        overlay_area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(selected)),
    );
}

/// M186: live substring search input overlay.
pub(super) fn render_search_input_overlay(frame: &mut Frame, app: &App, overlay_area: Rect) {
    let Mode::SearchInput(state) = &app.active_mode else {
        return;
    };
    let palette = app.effective_palette();
    let body = format!(
        " Search (id + title, case-insensitive)\n\n  /{}\n\n  Enter to commit · Esc to cancel ",
        state.buffer
    );
    let para = Paragraph::new(body)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search ")
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(palette.accent))
                // M186 F-05: backdrop so the underlying list does not
                // bleed through empty cells (mirrors the Input overlay).
                .style(Style::default().bg(crate::tui::palette::overlay_backdrop(palette))),
        )
        .style(Style::default().fg(palette.foreground));
    // Clear behind the modal first so prior chrome is fully erased.
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(para, overlay_area);
}

/// M185: multi-select lifecycle filter modal.
///
/// F-02: when the overlay is too short for all 10 options, window a
/// scroll window around `st.selected` and show "… more above/below …"
/// so the list never overflows without feedback (S5 / AC-04).
pub(super) fn render_lifecycle_filter_overlay(frame: &mut Frame, app: &App, overlay_area: Rect) {
    use crate::tui::progress::{lifecycle_filter_window, LIFECYCLE_FILTER_OPTIONS};
    let Mode::LifecycleFilter(st) = &app.active_mode else {
        return;
    };
    let palette = app.effective_palette();
    let dim = Style::default().fg(palette.dim);

    // Inner height: borders top+bottom (2). Reserve up to 2 rows for
    // more-above / more-below indicators when truncated.
    let inner_h = overlay_area.height.saturating_sub(2) as usize;
    let n = LIFECYCLE_FILTER_OPTIONS.len();
    let (start, end, more_above, more_below) = lifecycle_filter_window(n, st.selected, inner_h);

    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_in_window = 0usize;
    if more_above {
        items.push(ListItem::new(Line::from(Span::styled(
            " … more above … ",
            dim,
        ))));
    }
    for (i, lc) in LIFECYCLE_FILTER_OPTIONS
        .iter()
        .enumerate()
        .take(end)
        .skip(start)
    {
        let mark = if st.draft.contains(*lc) { "[x]" } else { "[ ]" };
        let style = if i == st.selected {
            selected_in_window = items.len();
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!(" {mark} {lc} "),
            style,
        ))));
    }
    if more_below {
        items.push(ListItem::new(Line::from(Span::styled(
            " … more below … ",
            dim,
        ))));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Lifecycle filter · Space toggle · ⏎ commit · Esc revert ")
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(palette.accent))
                // M186 F-05: backdrop so the underlying list does not
                // bleed through empty cells.
                .style(Style::default().bg(crate::tui::palette::overlay_backdrop(palette))),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_widget(Clear, overlay_area);
    frame.render_stateful_widget(
        list,
        overlay_area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(selected_in_window)),
    );
}

/// Sort-rebind inline menu (S keybind). Mirrors the lifecycle filter
/// overlay shape: small modal listing the per-lane sort keys, highlight
/// on `sort_rebind_index`, footer hint in the block title. The state
/// lives on `App::sort_rebind_menu` / `sort_rebind_index`; this renderer
/// only reads.
pub(super) fn render_sort_rebind_overlay(frame: &mut Frame, app: &App, overlay_area: Rect) {
    use crate::tui::app::SortKey;
    let Some(keys) = app.sort_rebind_menu.as_ref() else {
        return;
    };
    if keys.is_empty() {
        return;
    }
    let palette = app.effective_palette();
    let current_for_lane = app.lane_sort_key(app.active_lane);
    let mut items: Vec<ListItem> = Vec::new();
    for (i, k) in keys.iter().enumerate() {
        let is_active_sort = *k == current_for_lane;
        let marker = if is_active_sort { "● " } else { "  " };
        let label = match k {
            SortKey::Id => "Id",
            SortKey::Lifecycle => "Lifecycle",
            SortKey::Priority => "Priority",
            SortKey::Updated => "Updated",
            SortKey::Status => "Status",
            SortKey::Title => "Title",
        };
        let extra = if is_active_sort { "  (current)" } else { "" };
        let style = if i == app.sort_rebind_index {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.foreground)
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!("{marker}{label}{extra}"),
            style,
        ))));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Sort · ↑↓ cycle · ⏎ bind · Esc cancel ")
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(palette.accent))
                .style(Style::default().bg(crate::tui::palette::overlay_backdrop(palette))),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    // Clear behind the modal so the underlying list does not bleed
    // through (M186 F-05 pattern).
    frame.render_widget(Clear, overlay_area);
    frame.render_stateful_widget(
        list,
        overlay_area,
        &mut ratatui::widgets::ListState::default().with_selected(Some(app.sort_rebind_index)),
    );
}
