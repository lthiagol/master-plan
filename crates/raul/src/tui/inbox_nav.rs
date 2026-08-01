use crossterm::event::KeyCode;

use super::app::{App, InboxLine};

/// Key handling result for the Overview lane list view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewKeyAction {
    Refresh,
    ToggleHelp,
    QuitFromHelp,
    PassToEventHandler,
    Ignore,
}

pub fn map_overview_key(key: KeyCode, show_help: bool) -> OverviewKeyAction {
    if show_help {
        return match key {
            KeyCode::Char('?') => OverviewKeyAction::ToggleHelp,
            KeyCode::Char('Q') => OverviewKeyAction::QuitFromHelp,
            _ => OverviewKeyAction::Ignore,
        };
    }
    match key {
        KeyCode::Char('r') | KeyCode::Char('R') => OverviewKeyAction::Refresh,
        // M91 S4 follow-up: Tab is now handled at the top of the dispatcher
        // (single global bind). It used to route here as ToggleSidebar.
        KeyCode::Char('?') => OverviewKeyAction::ToggleHelp,
        _ => OverviewKeyAction::PassToEventHandler,
    }
}

/// M136: convenience wrapper that derives `show_help` from `app.active_mode`,
/// preserving the `map_overview_key(key, app)` shape for callers that used
/// to pass `app.show_help` directly.
pub fn map_overview_key_for_app(key: KeyCode, app: &App) -> OverviewKeyAction {
    let show_help = matches!(app.active_mode, super::mode::Mode::Help);
    map_overview_key(key, show_help)
}

/// Follow-up IO the runner must perform after applying inbox navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxNavFollowUp {
    None,
    LoadMilestoneDetail(String),
}

/// Apply inbox row navigation after the runner has switched lane and loaded list data.
pub fn apply_inbox_navigation(app: &mut App, item: &InboxLine) -> InboxNavFollowUp {
    match item.kind.as_str() {
        "milestone" | "spec-review" | "execution-review" => {
            navigate_milestone_inbox_item(app, item)
        }
        "backlog" => {
            if let Some(pos) = app.backlog.iter().position(|b| b.id == item.id) {
                app.selected_index = pos;
            } else {
                app.set_flash_message(item.action.clone());
            }
            InboxNavFollowUp::None
        }
        "track" => {
            // Track items migrated to backlog; flash the action so the user
            // can run it manually if they want raw CLI access.
            app.set_flash_message(item.action.clone());
            InboxNavFollowUp::None
        }
        _ => {
            app.set_flash_message(item.action.clone());
            InboxNavFollowUp::None
        }
    }
}

fn navigate_milestone_inbox_item(app: &mut App, item: &InboxLine) -> InboxNavFollowUp {
    // Guard: the inbox item references a milestone that hasn't been loaded
    // into the Milestones lane yet. Surface that to the user rather than
    // entering an empty detail view.
    if !app.milestones.iter().any(|m| m.id == item.id) {
        app.set_flash_message(item.action.clone());
        return InboxNavFollowUp::None;
    }
    // Resolve by id, not full-list position (AC-01): the inbox can surface a
    // done milestone even when hide_done filters the Milestones lane.
    app.enter_milestone_detail_by_id(&item.id);
    InboxNavFollowUp::LoadMilestoneDetail(item.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::ContentState;
    use crate::tui::app::MilestoneSummary;

    #[test]
    fn overview_refresh_key_maps_to_refresh() {
        assert_eq!(
            map_overview_key(KeyCode::Char('r'), false),
            OverviewKeyAction::Refresh
        );
        assert_eq!(
            map_overview_key(KeyCode::Char('R'), false),
            OverviewKeyAction::Refresh
        );
    }

    #[test]
    fn navigate_milestone_missing_sets_flash() {
        let mut app = App::new();
        app.select_lane(crate::tui::app::Lane::Milestones);
        app.load_milestones(vec![]);
        let item = InboxLine {
            id: "99".into(),
            kind: "milestone".into(),
            display: "M99".into(),
            reason: "review".into(),
            action: "mp milestone approve 99".into(),
        };
        assert_eq!(
            apply_inbox_navigation(&mut app, &item),
            InboxNavFollowUp::None
        );
        assert_eq!(
            app.flash_message.as_deref(),
            Some("mp milestone approve 99")
        );
    }

    #[test]
    fn navigate_milestone_found_enters_detail() {
        let mut app = App::new();
        app.select_lane(crate::tui::app::Lane::Milestones);
        app.load_milestones(vec![MilestoneSummary {
            id: "86".into(),
            title: "Visual".into(),
            lifecycle: "complete".into(),
            lifecycle_at: Some("2026-07-04T00:00:00Z".into()),
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
        }]);
        let item = InboxLine {
            id: "86".into(),
            kind: "milestone".into(),
            display: "M86".into(),
            reason: "review".into(),
            action: "mp reviews pass 86".into(),
        };
        assert_eq!(
            apply_inbox_navigation(&mut app, &item),
            InboxNavFollowUp::LoadMilestoneDetail("86".into())
        );
        assert_eq!(app.content, ContentState::MilestoneDetail);
        assert_eq!(app.selected_milestone_id.as_deref(), Some("86"));
    }

    #[test]
    fn navigate_spec_review_enters_milestone_detail() {
        let mut app = App::new();
        app.select_lane(crate::tui::app::Lane::Milestones);
        app.load_milestones(vec![MilestoneSummary {
            id: "91".into(),
            title: "Awaiting approval".into(),
            lifecycle: "groomed".into(),
            lifecycle_at: Some("2026-07-08T00:00:00Z".into()),
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
        }]);
        let item = InboxLine {
            id: "91".into(),
            kind: "spec-review".into(),
            display: "M91".into(),
            reason: "spec_status review".into(),
            action: "mp milestone approve 91".into(),
        };
        assert_eq!(
            apply_inbox_navigation(&mut app, &item),
            InboxNavFollowUp::LoadMilestoneDetail("91".into())
        );
        assert_eq!(app.content, ContentState::MilestoneDetail);
    }

    #[test]
    fn navigate_track_flashes_action() {
        // Track items migrated to backlog; the inbox row now just surfaces
        // its action so the user can run it manually.
        let mut app = App::new();
        let item = InboxLine {
            id: "BF-7".into(),
            kind: "track".into(),
            display: "Fix".into(),
            reason: "pending".into(),
            action: "mp track show bugfix".into(),
        };
        assert_eq!(
            apply_inbox_navigation(&mut app, &item),
            InboxNavFollowUp::None
        );
        assert_eq!(app.flash_message.as_deref(), Some("mp track show bugfix"));
    }
}
