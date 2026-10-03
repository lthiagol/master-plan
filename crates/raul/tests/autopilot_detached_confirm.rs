//! The detached-mode confirmation popover.
//!
//! The contract this file exists to protect: **every** selection of the
//! detached chip opens the popover — mouse or keyboard, first time or
//! hundredth — and all three outcomes are reachable. A path that let a
//! detached run start without the prompt would defeat the point of the
//! prompt, so the tests check the *absence* of such a path as
//! deliberately as they check the presence of the three outcomes.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use raul::tui::action::{self, Action};
use raul::tui::app::{App, Lane};
use raul::tui::autopilot::setup::{
    self, DetachedChoice, DetachedConfirm, RunMode, DETACHED_CHOICES,
};
use raul::tui::mouse;
use raul::tui::view_state;

const W: u16 = 132;
const H: u16 = 36;

fn app() -> App {
    let mut app = App::new();
    app.select_lane(Lane::Autopilot);
    app.autopilot.setup.selected = vec!["240".to_string()];
    app
}

fn screen(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).expect("terminal");
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(app, frame.area());
            raul::tui::render::render(frame, app, &view);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..H)
        .map(|y| {
            (0..W)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn detached_chip_rect(app: &App) -> ratatui::layout::Rect {
    let regions = view_state::compute_view(app, ratatui::layout::Rect::new(0, 0, W, H))
        .autopilot
        .expect("hit areas");
    regions
        .chip_rect("run_mode:detached")
        .expect("the detached chip is published")
}

// ─── the prompt appears every time ───────────────────────────────

/// AC-08: clicking the detached chip opens the popover.
#[test]
fn clicking_the_detached_chip_opens_the_prompt() {
    let mut app = app();
    assert!(app.autopilot.detached_confirm.is_none());
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H));
    let chip = detached_chip_rect(&app);
    assert!(mouse::handle_dispatch(
        &mut app, &view, chip.x, chip.y, false
    ));
    assert!(
        app.autopilot.detached_confirm.is_some(),
        "clicking detached must open the prompt"
    );
    assert_eq!(app.autopilot.setup.run_mode, RunMode::Detached);
}

/// AC-08: "every selection" means every one. Three cycles in a row each
/// open the prompt — a prompt that only appeared the first time would
/// train the operator to stop reading it.
#[test]
fn every_selection_of_the_detached_chip_opens_the_prompt() {
    let mut app = app();
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H));
    let chip = detached_chip_rect(&app);
    for attempt in 1..=3 {
        // Answer the previous prompt.
        if app.autopilot.detached_confirm.is_some() {
            mouse::apply_detached_choice(&mut app, DetachedChoice::Confirm);
        }
        assert!(app.autopilot.detached_confirm.is_none());
        assert!(
            mouse::handle_dispatch(&mut app, &view, chip.x, chip.y, false),
            "attempt {attempt}: the chip must be clickable"
        );
        assert!(
            app.autopilot.detached_confirm.is_some(),
            "attempt {attempt}: the prompt must open every time"
        );
    }
}

/// AC-08: the popover offers exactly the three specified outcomes, in
/// order.
#[test]
fn the_prompt_offers_exactly_three_outcomes_in_order() {
    assert_eq!(DETACHED_CHOICES.len(), 3);
    assert_eq!(DETACHED_CHOICES[0], DetachedChoice::Confirm);
    assert_eq!(DETACHED_CHOICES[1], DetachedChoice::ConfigureExtras);
    assert_eq!(DETACHED_CHOICES[2], DetachedChoice::Back);
    assert_eq!(DetachedChoice::Confirm.label(), "Confirm");
    assert_eq!(DetachedChoice::ConfigureExtras.label(), "Configure extras");
    assert_eq!(DetachedChoice::Back.label(), "Back");
}

/// All three labels are on screen while the prompt is open.
#[test]
fn the_prompt_renders_all_three_outcomes() {
    let mut app = app();
    app.autopilot.open_detached_confirm();
    let screen = screen(&app);
    for label in ["Confirm", "Configure extras", "Back"] {
        assert!(screen.contains(label), "{label:?} missing from:\n{screen}");
    }
    // The popover is titled with the mode it is asking about.
    assert!(screen.contains("Run mode: detached"));
}

// ─── the three outcomes ──────────────────────────────────────────

/// AC-08: Confirm keeps the run detached and closes the prompt.
#[test]
fn confirm_keeps_the_run_detached() {
    let mut app = app();
    app.autopilot.setup.run_mode = RunMode::Detached;
    app.autopilot.open_detached_confirm();
    mouse::apply_detached_choice(&mut app, DetachedChoice::Confirm);
    assert!(app.autopilot.detached_confirm.is_none());
    assert_eq!(
        app.autopilot.setup.run_mode,
        RunMode::Detached,
        "Confirm must keep the run detached"
    );
}

/// AC-08: Configure extras opens the override panel.
#[test]
fn configure_extras_opens_the_override_panel() {
    let mut app = app();
    app.autopilot.setup.run_mode = RunMode::Detached;
    app.autopilot.open_detached_confirm();
    assert!(!app.autopilot.panel_open);
    mouse::apply_detached_choice(&mut app, DetachedChoice::ConfigureExtras);
    assert!(app.autopilot.detached_confirm.is_none());
    assert!(
        app.autopilot.panel_open,
        "Configure extras must open the panel"
    );
    // The run mode survives — configuring extras does not silently
    // un-detach the run the operator was asked about.
    assert_eq!(app.autopilot.setup.run_mode, RunMode::Detached);
    // And the panel is on screen.
    assert!(screen(&app).contains("Overrides"));
}

/// AC-08: Back reverts to normal mode and closes the prompt.
#[test]
fn back_reverts_to_normal() {
    let mut app = app();
    app.autopilot.setup.run_mode = RunMode::Detached;
    app.autopilot.open_detached_confirm();
    mouse::apply_detached_choice(&mut app, DetachedChoice::Back);
    assert!(app.autopilot.detached_confirm.is_none());
    assert_eq!(
        app.autopilot.setup.run_mode,
        RunMode::Normal,
        "Back must revert to normal"
    );
    assert!(!app.autopilot.panel_open, "Back must not open the panel");
}

/// Selecting the *normal* chip does not open the prompt — there is
/// nothing to confirm about running in the foreground.
#[test]
fn selecting_normal_does_not_open_the_prompt() {
    let mut app = app();
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H));
    let regions = view.autopilot.clone().expect("hit areas");
    let chip = regions.chip_rect("run_mode:normal").expect("normal chip");
    assert!(mouse::handle_dispatch(
        &mut app, &view, chip.x, chip.y, false
    ));
    assert_eq!(app.autopilot.setup.run_mode, RunMode::Normal);
    assert!(app.autopilot.detached_confirm.is_none());
}

// ─── the prompt is modal ─────────────────────────────────────────

/// AC-08: the prompt is modal. A click outside it is swallowed rather
/// than reaching the setup form behind it — otherwise the operator
/// could change the topology "through" a prompt they have not answered.
#[test]
fn a_click_outside_the_prompt_is_swallowed() {
    let mut app = app();
    app.autopilot.open_detached_confirm();
    let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H));
    let regions = view.autopilot.clone().expect("hit areas");
    let popover = regions.detached_popover.expect("popover rect");
    // A chip well outside the popover.
    let topology = regions.chip_rect("topology:one-agent").expect("chip");
    assert!(
        !point_inside(topology, popover),
        "the fixture must click outside the popover"
    );
    let before = app.autopilot.setup.topology.clone();
    // Consumed (so the generic dispatch does not run) but no mutation.
    assert!(mouse::handle_dispatch(
        &mut app, &view, topology.x, topology.y, false
    ));
    assert_eq!(app.autopilot.setup.topology, before);
    assert!(
        app.autopilot.detached_confirm.is_some(),
        "the prompt must still be open"
    );
}

fn point_inside(inner: ratatui::layout::Rect, outer: ratatui::layout::Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// AC-08: clicking each row of the popover selects that outcome. The
/// rows are the drawn rows — the same rect the renderer used.
#[test]
fn clicking_a_prompt_row_selects_that_outcome() {
    for (index, expected) in DETACHED_CHOICES.iter().enumerate() {
        let mut app = app();
        app.autopilot.setup.run_mode = RunMode::Detached;
        app.autopilot.open_detached_confirm();
        let view = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H));
        let popover = view
            .autopilot
            .as_ref()
            .and_then(|r| r.detached_popover)
            .expect("popover rect");
        // One row per outcome, starting inside the top border.
        let y = popover.y + 1 + index as u16;
        assert!(mouse::handle_dispatch(
            &mut app,
            &view,
            popover.x + 1,
            y,
            false
        ));
        assert!(
            app.autopilot.detached_confirm.is_none(),
            "row {index} answered"
        );
        match expected {
            DetachedChoice::Confirm => {
                assert_eq!(app.autopilot.setup.run_mode, RunMode::Detached)
            }
            DetachedChoice::ConfigureExtras => assert!(app.autopilot.panel_open),
            DetachedChoice::Back => assert_eq!(app.autopilot.setup.run_mode, RunMode::Normal),
        }
    }
}

// ─── keyboard ────────────────────────────────────────────────────

/// The prompt's highlight starts on Confirm and wraps through all
/// three.
#[test]
fn the_highlight_starts_on_confirm_and_wraps() {
    let mut pop = DetachedConfirm::new();
    assert_eq!(pop.current(), DetachedChoice::Confirm);
    pop.move_cursor(1);
    assert_eq!(pop.current(), DetachedChoice::ConfigureExtras);
    pop.move_cursor(1);
    assert_eq!(pop.current(), DetachedChoice::Back);
    pop.move_cursor(1);
    assert_eq!(pop.current(), DetachedChoice::Confirm, "must wrap forward");
    pop.move_cursor(-1);
    assert_eq!(pop.current(), DetachedChoice::Back, "must wrap backward");
}

/// Down / Up move the highlight and Enter accepts it, through the
/// action dispatcher.
#[test]
fn arrow_keys_move_and_enter_accepts() {
    let mut app = app();
    let runner = raul::mp_runner::MpRunner::new().expect("runner");
    // Mirror the real flow: selecting the chip sets the mode, then the
    // prompt opens over it.
    app.autopilot.setup.run_mode = RunMode::Detached;
    app.autopilot.open_detached_confirm();
    action::apply_action(
        &mut app,
        &runner,
        Action::AutopilotDetachedMove { delta: 1 },
    )
    .expect("move");
    assert_eq!(
        app.autopilot
            .detached_confirm
            .as_ref()
            .expect("open")
            .current(),
        DetachedChoice::ConfigureExtras
    );
    action::apply_action(
        &mut app,
        &runner,
        Action::AutopilotDetachedMove { delta: -1 },
    )
    .expect("move");
    assert_eq!(
        app.autopilot
            .detached_confirm
            .as_ref()
            .expect("open")
            .current(),
        DetachedChoice::Confirm
    );
    action::apply_action(&mut app, &runner, Action::AutopilotDetachedAccept).expect("accept");
    assert!(app.autopilot.detached_confirm.is_none());
    assert_eq!(
        app.autopilot.setup.run_mode,
        RunMode::Detached,
        "Confirm kept it"
    );
}

/// The popover's rect is derived, so it is centred and grows with the
/// number of options.
#[test]
fn the_prompt_is_centred_over_the_setup_region() {
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    let rect = setup::detached_popover_rect(area);
    // One row per option plus two borders.
    assert_eq!(rect.height, DETACHED_CHOICES.len() as u16 + 2);
    assert_eq!(rect.width, 40);
    // Centred: equal space on both sides.
    assert_eq!(rect.x, (area.width - rect.width) / 2);
    assert_eq!(rect.y, (area.height - rect.height) / 2);
    // It never exceeds the area it sits in.
    assert!(rect.x + rect.width <= area.width);
    assert!(rect.y + rect.height <= area.height);
}

/// The popover is not published when it is closed, so there is no
/// phantom hit target swallowing setup clicks.
#[test]
fn no_prompt_rect_is_published_when_closed() {
    let app = app();
    let regions = view_state::compute_view(&app, ratatui::layout::Rect::new(0, 0, W, H))
        .autopilot
        .expect("hit areas");
    assert!(regions.detached_popover.is_none());
}
