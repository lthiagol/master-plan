//! M140: herdr-style modal layout helpers.
//!
//! `modal_stack_areas` splits a modal body into header / content / footer /
//! actions bands. `centered_popup_rect` centers a percentage-sized popup
//! (field-edit overlay). Callers must paint [`ratatui::widgets::Clear`] on the
//! modal area before drawing so the underlying UI is wiped.

use ratatui::layout::{Constraint, Layout, Rect};

/// Four stacked regions of a settings-style modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModalAreas {
    pub header: Rect,
    pub content: Rect,
    pub footer: Rect,
    pub actions: Rect,
}

/// Split `area` into header (1), content (flex), footer (1), actions (1).
pub fn modal_stack_areas(area: Rect) -> ModalAreas {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    ModalAreas {
        header: rows[0],
        content: rows[1],
        footer: rows[2],
        actions: rows[3],
    }
}

/// Center a popup occupying `percent_x` × `percent_y` of `area`.
pub fn centered_popup_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_areas_sum_to_height() {
        let area = Rect::new(0, 0, 40, 20);
        let m = modal_stack_areas(area);
        assert_eq!(m.header.height, 1);
        assert_eq!(m.footer.height, 1);
        assert_eq!(m.actions.height, 1);
        assert_eq!(
            m.header.height + m.content.height + m.footer.height + m.actions.height,
            area.height
        );
    }

    #[test]
    fn centered_popup_is_inside_area() {
        let area = Rect::new(0, 0, 100, 40);
        let p = centered_popup_rect(area, 50, 30);
        assert!(p.x >= area.x);
        assert!(p.y >= area.y);
        assert!(p.x + p.width <= area.x + area.width);
        assert!(p.y + p.height <= area.y + area.height);
    }
}
