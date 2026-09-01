use std::collections::BTreeMap;
use std::io::{stdout, Write};
use std::panic::{self, AssertUnwindSafe};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{self, Event as CrosstermEvent, MouseEventKind};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use ratatui::Terminal;

use super::action::{self, Action};
use super::app::{App, ContentState, Lane};
use super::render;
use super::render::scrollbar::track_click_to_scroll;
use super::view_state::{self, ScrollableId, ScrollbarHitArea};
use crate::mp_runner::MpRunner;

// M136: the data-loading + side-effect helpers (load_*,
// resolve_annotation, …) live in `runner_helpers` so both
// this module and `crate::tui::action` can call them with one import.
// `pub use` keeps the pre-M136 `raul::tui::runner::co_approval_approve`
// (etc.) call sites in the integration tests working unchanged.
pub use super::runner_helpers::{
    check_approval_status, co_approval_approve, create_annotation, create_approval_annotation,
    execute_review_action, invalidate_after_lifecycle_write, load_annotations, load_backlog,
    load_backlog_detail, load_dashboard, load_data_for_lane, load_milestone_detail,
    load_milestones, load_path_data, mp_dir_for_runner, navigate_from_inbox_item,
    parse_mp_ok_response, reopen_annotation, resolve_annotation,
};

#[derive(Debug, Clone, Default)]
pub struct TuiOptions {}

/// Injectable terminal setup operations used to verify partial-init rollback.
pub trait TerminalOps {
    fn enable_raw(&mut self) -> Result<()>;
    fn enter_alternate(&mut self) -> Result<()>;
    fn enable_mouse(&mut self) -> Result<()>;
    fn disable_mouse(&mut self);
    fn leave_alternate(&mut self);
    fn disable_raw(&mut self);
}

pub struct CrosstermOps;

impl TerminalOps for CrosstermOps {
    fn enable_raw(&mut self) -> Result<()> {
        enable_raw_mode().context("failed to enable raw mode")
    }

    fn enter_alternate(&mut self) -> Result<()> {
        stdout()
            .execute(EnterAlternateScreen)
            .context("failed to enter alternate screen")?;
        Ok(())
    }

    fn enable_mouse(&mut self) -> Result<()> {
        stdout()
            .execute(EnableMouseCapture)
            .context("failed to enable mouse capture")?;
        Ok(())
    }

    fn disable_mouse(&mut self) {
        let _ = stdout().execute(DisableMouseCapture);
    }

    fn leave_alternate(&mut self) {
        let _ = stdout().execute(LeaveAlternateScreen);
    }

    fn disable_raw(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TerminalStage {
    Normal,
    Raw,
    Alternate,
    Mouse,
}

/// Restores exactly the terminal stages that completed setup.
pub struct TerminalGuard<O: TerminalOps> {
    ops: O,
    stage: TerminalStage,
}

impl<O: TerminalOps> TerminalGuard<O> {
    pub fn new(ops: O) -> Result<Self> {
        let mut guard = Self {
            ops,
            stage: TerminalStage::Normal,
        };
        guard.ops.enable_raw()?;
        guard.stage = TerminalStage::Raw;
        guard.ops.enter_alternate()?;
        guard.stage = TerminalStage::Alternate;
        guard.ops.enable_mouse()?;
        guard.stage = TerminalStage::Mouse;
        Ok(guard)
    }
}

impl<O: TerminalOps> Drop for TerminalGuard<O> {
    fn drop(&mut self) {
        if self.stage >= TerminalStage::Mouse {
            self.ops.disable_mouse();
        }
        if self.stage >= TerminalStage::Alternate {
            self.ops.leave_alternate();
        }
        if self.stage >= TerminalStage::Raw {
            self.ops.disable_raw();
        }
    }
}

pub fn run_tui(runner: &MpRunner, options: TuiOptions) -> Result<()> {
    let _guard = TerminalGuard::new(CrosstermOps)?;

    // `MpRunner` only holds `PathBuf`s (no interior mutability) and the
    // closure only reads it; the assertion stays sound as long as neither
    // changes. If `MpRunner` ever gains a `RefCell`/mutable interior, revisit.
    let result = panic::catch_unwind(AssertUnwindSafe(|| run_tui_inner(runner, options)));

    translate_catch_unwind(result, &mut std::io::stderr())
}

/// Translate the result of a `catch_unwind` around the TUI loop into a
/// `Result<()>`. On panic, prints the payload to `stderr` (so it survives
/// even when the guard's drop is mid-flight) and returns a non-`Ok` error
/// instead of silently exiting 0 (M87 AC-02).
///
/// Exposed `pub` so `crates/raul/tests/tui_panic.rs` can drive it with a
/// `Vec<u8>` writer. Not part of the stable raul API.
pub fn translate_catch_unwind(
    result: std::thread::Result<Result<()>>,
    stderr: &mut dyn Write,
) -> Result<()> {
    match result {
        Ok(inner_result) => inner_result,
        Err(payload) => {
            let msg = panic_message(&payload);
            let _ = writeln!(stderr, "raul: TUI panicked: {msg}");
            let _ = stderr.flush();
            anyhow::bail!("TUI panicked: {msg}")
        }
    }
}

/// Extract a printable message from a `catch_unwind` payload. The payload is
/// `Box<dyn Any + Send>`; for `panic!("...")` it carries `&'static str` or
/// `String`. Fall back to a generic tag if neither is present.
///
/// Exposed `pub` for `crates/raul/tests/tui_panic.rs`; not stable API.
pub fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "<non-string panic payload>".to_string()
}

/// Minimum spacing between successive renders. 16ms approximates 60fps.
pub const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(16);

/// Tracks draw cadence and the number of frames that passed the rate gate.
///
/// Production code reads `can_render()` before each draw and calls
/// `record_render()` after; tests read `redraws` for the AC-01/AC-03
/// assertions. The minimum interval is configurable via
/// [`FrameClock::with_interval`] — production sticks to the 16ms default,
/// tests can drop it to zero so the gate never blocks.
#[derive(Debug)]
pub struct FrameClock {
    last_render: Instant,
    min_interval: Duration,
    /// M134: monotonic count of `terminal.draw()` calls that actually fired.
    /// Tests inspect this after a run to verify AC-01 (rate cap) and AC-03
    /// (no-op skips a redraw).
    pub redraws: u64,
}

impl FrameClock {
    pub fn new() -> Self {
        Self::with_interval(MIN_RENDER_INTERVAL)
    }

    /// Build a clock with a custom minimum interval. Tests use this with
    /// `Duration::ZERO` to disable the rate gate entirely so they can pin
    /// the dirty-signal and drain behaviour without timing flakiness;
    /// production code should always go through [`Self::new`].
    pub fn with_interval(min_interval: Duration) -> Self {
        // Initialise `last_render` far enough in the past that the very
        // first draw of a fresh session is allowed through the gate —
        // otherwise the user would see a blank terminal for `min_interval`.
        let last_render = Instant::now() - min_interval;
        Self {
            last_render,
            min_interval,
            redraws: 0,
        }
    }

    /// True when at least the configured minimum interval has elapsed
    /// since the last recorded render. Cheap — just an `Instant::elapsed`
    /// subtraction, no syscall.
    pub fn can_render(&self) -> bool {
        self.last_render.elapsed() >= self.min_interval
    }

    /// Time remaining until [`Self::can_render`] becomes true. Zero when
    /// the gate is already open. Used by `run_loop` to sleep out a
    /// deferred frame instead of blocking on the next input event
    /// (external-review F-02).
    pub fn time_until_ready(&self) -> Duration {
        self.min_interval.saturating_sub(self.last_render.elapsed())
    }

    /// Mark a render as having just fired. Updates `last_render` to now and
    /// bumps the redraw counter; both are required for the rate gate to
    /// take effect on the next iteration.
    pub fn record_render(&mut self) {
        self.last_render = Instant::now();
        self.redraws = self.redraws.wrapping_add(1);
    }
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new()
    }
}

/// M134 AC-04: RAII guard wrapping the synchronized-output sequences
/// (CSI ?2026 h / CSI ?2026 l) around a `terminal.draw()` call. The guard
/// is generic over the writer so tests can drive it against a `Vec<u8>` and
/// assert on the byte stream — production code passes `&mut stdout()`.
///
/// Why an RAII guard instead of two manual calls? Because the `end`
/// sequence must fire on every code path, including panics inside the draw
/// closure; `Drop` guarantees that, where hand-rolled begin/end pairs
/// routinely leak the begin sequence on early returns.
///
/// `W: ?Sized` so the guard can wrap `dyn Write` — production passes
/// `&mut stdout()` through a `&mut dyn Write` parameter to keep `run_loop`
/// free of any concrete-stdout dependency.
pub struct SyncOutputGuard<'a, W: Write + ?Sized> {
    writer: &'a mut W,
    finished: bool,
}

impl<'a, W: Write + ?Sized> SyncOutputGuard<'a, W> {
    /// Begin a synchronized-output region. Writes `CSI ?2026 h` to the
    /// supplied writer; the matching `CSI ?2026 l` lands in `Drop`.
    ///
    /// Write failures are logged in debug builds (external-review F-03);
    /// production still proceeds so a flaky tty does not abort the frame.
    pub fn begin(writer: &'a mut W) -> Self {
        if let Err(e) = writer.write_all(b"\x1b[?2026h") {
            eprintln!("raul: SyncOutputGuard begin write failed: {e}");
            debug_assert!(false, "SyncOutputGuard begin write failed: {e}");
        }
        if let Err(e) = writer.flush() {
            eprintln!("raul: SyncOutputGuard begin flush failed: {e}");
            debug_assert!(false, "SyncOutputGuard begin flush failed: {e}");
        }
        Self {
            writer,
            finished: false,
        }
    }

    /// End the region eagerly. After calling this, `Drop` becomes a no-op,
    /// which is useful for tests that want to inspect the stream between
    /// begin and end without un-binding the guard.
    pub fn finish(mut self) {
        if let Err(e) = self.writer.write_all(b"\x1b[?2026l") {
            eprintln!("raul: SyncOutputGuard finish write failed: {e}");
        }
        if let Err(e) = self.writer.flush() {
            eprintln!("raul: SyncOutputGuard finish flush failed: {e}");
        }
        self.finished = true;
    }
}

impl<'a, W: Write + ?Sized> Drop for SyncOutputGuard<'a, W> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.writer.write_all(b"\x1b[?2026l");
            let _ = self.writer.flush();
        }
    }
}

/// M134 AC-02: abstracts the event source so the loop can drain via
/// `poll(Duration::ZERO)` in production and a `VecDeque` in tests without
/// touching crossterm directly. Returning `Ok(None)` from either method
/// signals end-of-stream — the loop exits cleanly, which is how tests
/// tear down without keeping a real terminal open.
pub trait EventSource {
    /// Block until an event is available, or signal end-of-stream.
    fn wait_next(&mut self) -> Result<Option<CrosstermEvent>>;

    /// Wait up to `timeout`. `Idle` means the deadline elapsed without
    /// input; it is not end-of-stream.
    fn wait_next_timeout(&mut self, _timeout: Duration) -> Result<WaitResult> {
        Ok(match self.wait_next()? {
            Some(event) => WaitResult::Event(event),
            None => WaitResult::EndOfStream,
        })
    }

    /// Non-blocking probe for pending events. Returns `Ok(None)` when
    /// nothing is queued — this is what `crossterm::event::poll(Duration::ZERO)`
    /// produces and what the drain loop relies on.
    fn poll_next(&mut self) -> Result<Option<CrosstermEvent>>;
}

#[derive(Debug)]
pub enum WaitResult {
    Event(CrosstermEvent),
    Idle,
    EndOfStream,
}

/// M134: production event source backed by crossterm. `wait_next` blocks on
/// `event::read()`; `poll_next` is `poll(Duration::ZERO)` followed by a
/// non-blocking read so the drain loop can coalesce a burst of queued
/// events into a single render.
pub struct CrosstermSource;

impl EventSource for CrosstermSource {
    fn wait_next(&mut self) -> Result<Option<CrosstermEvent>> {
        Ok(Some(event::read().context("failed to read input event")?))
    }

    fn wait_next_timeout(&mut self, timeout: Duration) -> Result<WaitResult> {
        if event::poll(timeout).context("failed to poll input event")? {
            Ok(WaitResult::Event(
                event::read().context("failed to read input event")?,
            ))
        } else {
            Ok(WaitResult::Idle)
        }
    }

    fn poll_next(&mut self) -> Result<Option<CrosstermEvent>> {
        if event::poll(Duration::ZERO).context("failed to poll input event")? {
            Ok(Some(event::read().context("failed to read input event")?))
        } else {
            Ok(None)
        }
    }
}

fn run_tui_inner(runner: &MpRunner, _options: TuiOptions) -> Result<()> {
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    let mut app = App::new();
    let ui_config = crate::config::UiConfig::load(runner);
    crate::config::set_color_enabled(ui_config.color);
    crate::config::set_icons(ui_config.icons);
    app.hide_done = ui_config.hide_done;
    app.palette = ui_config.palette();
    // M182 S4: load per-lane sort keys from MP_HOME config so the
    // bound order survives a restart. Best-effort — mp missing or
    // corrupt config leaves the in-memory defaults in place.
    if let Err(e) = crate::tui::runner_helpers::load_persisted_sort_keys(runner, &mut app) {
        eprintln!("raul: failed to load persisted sort keys: {e}");
    }
    // M154: thread the [review].hunk flag onto App so the milestone
    // detail render can branch on it without re-reading mp config
    // every frame. The value is loaded once at startup; projects
    // that toggle the flag mid-session need a restart for the
    // indicator to flip (acceptable for the human surface).
    app.review_hunk_enabled = ui_config.review_hunk_enabled;
    // M198: same pattern as `review_hunk_enabled` — read
    // `ui.show_watch_tab` once at startup and pin it on App.
    // When `false` (the default), the Watch lane is filtered
    // out of the tab bar, the hit-test areas, and the
    // prev/next navigation. Mid-session flips need a restart,
    // which the spec accepts: the toggle is a setup-time
    // decision, not a hot key.
    app.show_watch_tab = ui_config.show_watch_tab;
    // M198 S4 / AC-04: if the operator toggled Watch off while
    // the active lane was Watch (stale state, e.g. an old
    // `state.json` or a mid-session reload), fall back to
    // Overview. The check uses the *visible* list so it is
    // the same set the tab bar / hit-test / prev-next
    // navigation all see — single filter point. F-05: this
    // is now a method on `App` so the same code path is
    // unit-testable from `app.rs::tests`.
    app.reconcile_active_lane_with_visible();
    app.keybinds = crate::tui::keybinds::Keybinds::load(runner);

    // M143: prime the LaneCache with the live plan-dir mtime so external
    // writes that bump mtime invalidate cached entries on the next read.
    // The cache starts at mtime=0 (from `App::new`) and would otherwise
    // never notice a real-world mtime change. Per-load polling in
    // `load_data_for_lane` keeps the cache fresh across the session;
    // this priming just makes the first load have a meaningful mtime
    // to compare against (instead of always comparing against 0).
    if let Some(mp_dir) = mp_dir_for_runner(runner) {
        app.lane_cache.check_and_update_mtime(&mp_dir);
        // The Watch poller owns log I/O and needs the discovered plan root.
        app.plan_dir = mp_dir;
    }

    load_dashboard(runner, &mut app)?;

    let mut frame_clock = FrameClock::new();
    let mut source = CrosstermSource;
    let mut sync_out = stdout();

    // The idle hook drives the rate-limited Watch poller.
    let on_idle = move |app: &mut App| -> Result<()> {
        let watch_lane_active = app.active_lane == crate::tui::app::Lane::Watch;
        if watch_lane_active {
            let mut poller = std::mem::take(&mut app.watch_poller);
            let result = crate::tui::watch::poll_watch_state(runner, app, &mut poller);
            app.watch_poller = poller;
            result?;
        }
        Ok(())
    };
    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_out,
        &mut source,
        |app, event, term_size| dispatch_event(app, runner, event, term_size),
        on_idle,
    )
}

#[allow(dead_code)]
pub(crate) fn fire_watch_tick(_app: &mut App, _runner: &MpRunner) -> Result<()> {
    Ok(())
}

/// M134: the heart of the smoothness work. Generic over the ratatui backend
/// (`B: Backend`) and an `EventSource` so tests can drive it with a
/// `VecDeque` of synthetic events against a `TestBackend`; production
/// passes `CrosstermBackend` + `CrosstermSource`.
///
/// Loop invariants:
///   * `needs_render` is set when an event dispatched a state mutation and
///     cleared immediately after a successful draw.
///   * `frame_clock.can_render()` gates every draw; a render that loses
///     the gate is *deferred* (needs_render stays true), not dropped.
///   * Every draw is wrapped in `SyncOutputGuard` so the bracket sequences
///     fire even when the draw closure panics.
///
/// The closure form keeps the loop independent of `MpRunner`; the idle hook
/// polls Watch state when its deadline expires.
pub fn run_loop<B, E, F, I>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    frame_clock: &mut FrameClock,
    sync_writer: &mut dyn Write,
    source: &mut E,
    mut dispatch: F,
    on_idle: I,
) -> Result<()>
where
    B: Backend,
    E: EventSource,
    F: FnMut(&mut App, CrosstermEvent, (u16, u16)) -> Result<()>,
    I: FnMut(&mut App) -> Result<()>,
{
    let mut needs_render = true;
    let mut on_idle = on_idle;
    loop {
        // ---- Render phase -------------------------------------------------
        // The dirty signal suppresses redraws when no state has changed
        // since the last frame (AC-03). The frame clock caps the rate even
        // when state *is* changing (AC-01). When the gate is closed the
        // flag stays set, so the next iteration will pick up the deferred
        // draw as soon as the interval has elapsed.
        if needs_render && frame_clock.can_render() {
            let _guard = SyncOutputGuard::begin(sync_writer);
            draw_frame(terminal, app)?;
            frame_clock.record_render();
            needs_render = false;
        }

        if app.quitting {
            break;
        }

        // Poll errors become visible application state instead of terminating
        // the event loop.
        let version_before = app.version();
        if let Err(e) = on_idle(&mut *app) {
            let msg = format!("{e:#}");
            let flash = format!("watch idle: {msg}");
            if app.flash_message.as_deref() != Some(flash.as_str())
                || app.last_action_error.as_deref() != Some(msg.as_str())
            {
                app.flash_message = Some(flash);
                app.last_action_error = Some(msg);
                app.touch();
            }
        }
        // F-08: poll_watch_state mutates `app.watch.status` /
        // `app.watch.output` via direct field writes. Those
        // writes now call `app.touch()` (see poll_watch_state),
        // so a version bump here means state changed and the
        // next render phase must fire — without this, the
        // screen would not refresh until the next keypress,
        // breaking AC-06's "update without a keypress" promise.
        if app.version() != version_before {
            needs_render = true;
        }

        // Snapshot the terminal size via a short reborrow. The dispatch
        // closure gets the value by Copy (`(u16, u16)`) so the closure does
        // not need its own borrow of `terminal` — which would conflict with
        // the `&mut Terminal<B>` already held by `run_loop` for `draw_frame`.
        // M135 F-05: the size carries BOTH width and height so `handle_mouse`
        // builds a `ViewState` against the true frame, not a hardcoded height.
        let term_size: (u16, u16) = terminal
            .size()
            .map(|s| (s.width, s.height))
            .unwrap_or((0, 0));

        // ---- Deferred frame: wait for the gate, not the next key -------
        // External-review F-02: when a mutation left `needs_render` set but
        // the frame clock is still closed, do NOT block forever on
        // `wait_next` — that would leave the last burst frame unpainted
        // until the user presses another key. Drain any already-queued
        // events, sleep the remaining interval, then `continue` so the
        // render phase above fires without requiring further input.
        if needs_render && !frame_clock.can_render() {
            while let Some(event) = source.poll_next()? {
                let version_before = app.version();
                dispatch(app, event, term_size)?;
                if app.version() != version_before {
                    needs_render = true;
                }
            }
            // M134 code-review: a `q` drained here sets `app.quitting`; bail
            // out now instead of sleeping out the interval and rendering one
            // extra frame before the next iteration's quit check fires.
            if app.quitting {
                break;
            }
            let remaining = frame_clock.time_until_ready();
            if remaining > Duration::ZERO {
                std::thread::sleep(remaining);
            }
            continue;
        }

        // ---- Wait for the next event -------------------------------------
        // Watch uses its polling deadline; other lanes wait effectively
        // indefinitely for input.
        let timeout = if app.active_lane == Lane::Watch {
            app.watch_poller.time_until_due()
        } else {
            Duration::from_secs(24 * 60 * 60)
        };
        match source.wait_next_timeout(timeout)? {
            WaitResult::Event(event) => {
                let version_before = app.version();
                dispatch(app, event, term_size)?;
                if app.version() != version_before {
                    needs_render = true;
                }
            }
            WaitResult::Idle => continue,
            WaitResult::EndOfStream => break,
        }

        // ---- Drain queued events -----------------------------------------
        // Coalesce every event crossterm has already buffered into a single
        // dispatch batch; we render once at the top of the next iteration
        // rather than once per event (AC-02).
        while let Some(event) = source.poll_next()? {
            let version_before = app.version();
            let term_size: (u16, u16) = terminal
                .size()
                .map(|s| (s.width, s.height))
                .unwrap_or((0, 0));
            dispatch(app, event, term_size)?;
            if app.version() != version_before {
                needs_render = true;
            }
        }
    }

    Ok(())
}

/// M134: thin wrapper around `terminal.draw(...)` that lifts
/// `<B as Backend>::Error` into `anyhow::Error` without requiring
/// `B::Error: std::error::Error + Send + Sync + 'static` — that bound is
/// not part of the `Backend` trait and isn't satisfied by every backend
/// `ratatui` ships (e.g. `TestBackend`). Using `Debug` keeps the
/// conversion total and avoids a wall of `where` clauses on `run_loop`.
///
/// M135: computes the `ViewState` once per frame and threads it into
/// `render::render` so the renderer is a pure read of the view. The
/// hit areas in the view are what the mouse handler later uses for
/// dispatch (see `handle_mouse`); computing them here keeps the
/// render pass and the dispatch on the same source of truth.
fn draw_frame<B: Backend>(terminal: &mut Terminal<B>, app: &App) -> Result<()> {
    // M135: compute the `ViewState` once per frame so the renderer
    // is a pure read. `terminal.size()` returns the Backend's
    // associated `Error`, which is not `Send + Sync` in general;
    // match it explicitly to lift into `anyhow::Error` without
    // adding a `where` bound (the original `draw_frame` did the
    // same for `terminal.draw`).
    let size = terminal
        .size()
        .map_err(|e| anyhow::anyhow!("failed to read terminal size: {:?}", e))?;
    let view = view_state::compute_view(app, size.into());
    terminal
        .draw(|frame| render::render(frame, app, &view))
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("failed to draw frame: {:?}", e))
}

/// M134: per-event dispatch, extracted from the old monolithic `match` in
/// `run_tui_inner` so the loop stays backend- and runner-agnostic. The
/// `term_size` argument (M135 F-05: was `term_width: u16`) is read live
/// from `terminal.size()` by the production closure (see `run_tui_inner`);
/// it threads through to `handle_mouse` so hit-testing agrees with what
/// `render()` drew — both width (compact/full threshold, M105 / B-39) and
/// height (list/board/dashboard vertical hit areas).
///
/// M136: key dispatch now uses the per-mode handler chain in `tui/modes/`.
/// `dispatch_key` returns a `Vec<Action>`; `apply_action` (in
/// `tui/action.rs`) is the single place that mutates `App` and shells out
/// to `mp`. The dispatch body is a `match` on `app.active_mode`; the
/// input overlay and the help / review-menu / annotation-thread modes
/// each have their own handler that ignores the global keys.
fn dispatch_event(
    app: &mut App,
    runner: &MpRunner,
    event: CrosstermEvent,
    term_size: (u16, u16),
) -> Result<()> {
    match event {
        CrosstermEvent::Mouse(mouse) => {
            // Pass the rendered tab-bar width so hit-testing uses the
            // same compact/full threshold as `render_tab_bar` (which
            // keys compact mode off `area.width < 60`). The mouse
            // event itself carries no terminal size, so without this
            // the heuristic would diverge from rendering at widths
            // where the click column disagrees with the real width.
            handle_mouse(app, runner, mouse, term_size)?;
        }
        CrosstermEvent::Key(key) => {
            for action in dispatch_key(app, key) {
                action::apply_action(app, runner, action)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// M136: the top-level key → `Vec<Action>` dispatcher.
///
/// Routes the event to one of the per-mode handlers in
/// [`crate::tui::modes`]; the handler returns `Action`s that the caller
/// (typically [`dispatch_event`]) feeds through `apply_action`. This is
/// the only place that calls into `modes::*` — adding a new mode means
/// adding exactly one match arm here and one module under `modes/`.
fn dispatch_key(app: &App, key: crossterm::event::KeyEvent) -> Vec<Action> {
    use crate::tui::mode::Mode as M;
    use crate::tui::modes;
    match app.active_mode {
        M::Normal => modes::normal::handle_key(key, app),
        M::Input(_) => modes::input::handle_key(key),
        M::Help => modes::help::handle_key(key),
        M::AnnotationThread => modes::annotation_thread::handle_key(key),
        M::ReviewMenu(_) => modes::review_menu::handle_key(key),
        M::LifecycleFilter(_) => modes::lifecycle_filter::handle_key(key),
        M::SearchInput(_) => modes::search_input::handle_key(key),
    }
}

/// M91 S5: pure helper that maps a horizontal pixel coordinate on the tab bar
/// row to a `Lane::ordered()` index, or `None` if the click missed every tab.
///
/// In wide mode (no overflow) this is equivalent to walking all lanes in
/// `Lane::ordered()` after a 1-col leading space. M105 S1 / B-39 replaces
/// the previous behavior under narrow overflow with `tab_hit_test_for_layout`
/// — `tab_hit_test` remains a thin wrapper around that helper so the
/// pre-existing tui_tab_bar.rs assertions (which all target non-overflow
/// widths) keep passing unchanged.
///
/// Tests that pin the wide-mode behavior are in
/// `crates/raul/tests/tui_tab_bar.rs`; the narrow-overflow contract is
/// pinned by `crates/raul/tests/tui_mouse.rs::overflow_hit_test_selects_visible_only`.
///
/// M135: this helper is no longer called from the mouse path —
/// `handle_mouse` reads from the pre-computed `ViewState` instead.
/// The function stays `pub` for the existing tests, but it now
/// imports from `view_state` (the layout machinery moved out of
/// `render`).
pub fn tab_hit_test(x: u16, compact: bool, lanes: &[Lane]) -> Option<usize> {
    // Force the wide-mode layout (no overflow): `u16::MAX` is wider than
    // any real terminal, so `compute_tab_bar_layout` skips the overflow
    // path and returns `visible = 0..total` regardless of which lane is
    // active. The active-lane argument is therefore irrelevant here; any
    // lane (we use `Lane::Overview` for concreteness) produces the same
    // answer. See `compute_tab_bar_layout` for the wide-mode contract.
    //
    // F-02: the helper now threads the filtered `&[Lane]` through to
    // both `compute_tab_bar_layout` and `tab_hit_test_for_layout` so
    // it goes through the M198 single-filter-point design. The
    // pre-existing tui_tab_bar tests pass `&Lane::ordered()` (the
    // historical 7-lane contract they pin); callers that want the
    // "Watch hidden" shape pass `&Lane::ordered_visible(false)`.
    let layout = view_state::compute_tab_bar_layout(u16::MAX, compact, &Lane::Overview, lanes);
    tab_hit_test_for_layout(x, &layout, lanes)
}

/// M105 S1 (B-39): wide-mode hit test helper. Kept `pub` for the
/// pre-existing `tui_tab_bar.rs` tests that pin the non-overflow
/// contract; the mouse path no longer calls it directly — it reads
/// from the pre-computed `ViewState` instead (M135).
///
/// F-02: the helper takes a `&[Lane]` argument so the hit-test
/// areas agree with the renderer's filtered list. Pass the same
/// `&[Lane]` slice that was used to build the `layout`; for the
/// pre-M198 7-lane tests that is `&Lane::ordered()`, for the
/// post-M198 "Watch hidden" tests it is
/// `&Lane::ordered_visible(false)`. Production mouse dispatch
/// reads the filtered list from the pre-computed `ViewState`
/// instead (M135).
pub fn tab_hit_test_for_layout(
    x: u16,
    layout: &view_state::TabBarLayout,
    lanes: &[Lane],
) -> Option<usize> {
    for &(lane_idx, start_x, end_x) in &view_state::visible_tab_x_ranges(layout, lanes) {
        if (x as u32) >= start_x as u32 && (x as u32) < end_x as u32 {
            return Some(lane_idx);
        }
    }
    None
}

/// M135: mouse dispatch. Reads every interactive element's rect from the
/// pre-computed `ViewState` so the hit-test agrees with what `render()`
/// drew by construction (L41). No layout-derivation function is called on
/// this path — `view.tab_hit_areas` / `view.list_item_rects` /
/// `view.board_card_rects` are the single sources of truth.
///
/// `pub` so integration tests in `crates/raul/tests/` can dispatch
/// synthetic mouse events (clicks per M135 F-04, wheel events per the
/// M169-rev scrollbar fix). Re-exported under
/// `crate::tui::runner::test_helpers` with `#[doc(hidden)]` so the
/// surface stays internal — not part of any external API.
pub fn handle_mouse(
    app: &mut App,
    runner: &MpRunner,
    mouse: crossterm::event::MouseEvent,
    term_size: (u16, u16),
) -> Result<()> {
    use crossterm::event::MouseButton;

    let x = mouse.column;
    let y = mouse.row;

    // M135 F-05: build the `ViewState` against the TRUE terminal size
    // (width AND height). Pre-fix this used a hardcoded `height: 24`, so
    // every vertical hit area (list rows, board cards, dashboard inbox)
    // was computed against the wrong frame on any non-24-row terminal —
    // latent while the list/board branches ignored the rects, but a live
    // L41 drift source once F-02 wired them in.
    let area = Rect {
        x: 0,
        y: 0,
        width: term_size.0,
        height: term_size.1,
    };
    let view = view_state::compute_view(app, area);

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // M135: walk the pre-computed `view.tab_hit_areas` for
            // the tab-bar click. No `compute_tab_bar_layout` call on
            // this path — the view is the single source of truth.
            if y == view.tab_bar_area.y {
                for hit in &view.tab_hit_areas {
                    let r = hit.rect;
                    if x >= r.x && x < r.x.saturating_add(r.width) {
                        app.select_lane(hit.id);
                        load_data_for_lane(runner, app)?;
                        return Ok(());
                    }
                }
            } else if let Some(hit) = view
                .scrollbar_rects
                .iter()
                .find(|h| point_in_rect(x, y, h.rect))
            {
                // External-review F-01 / AC-04: track click jumps scroll
                // before list/board row selection so a click on the
                // gutter never selects a row underneath.
                apply_scrollbar_track_click(app, hit, y);
                return Ok(());
            } else if app.content == ContentState::List {
                // M135 F-02: list-row clicks read from
                // `view.list_item_rects` instead of the hardcoded
                // `y.saturating_sub(3)` offset. The offset only
                // approximated the Milestones/Backlog bordered-table
                // layout and was wrong for the Overview inbox (whose
                // block sits inside the dashboard vertical split, and
                // whose items are 3 rows tall). Walking the pre-computed
                // rects agrees with render by construction (L41).
                if let Some(idx) = resolve_list_click(app, &view, x, y) {
                    app.selected_index = idx;
                    app.touch();
                    return Ok(());
                }
            } else if app.content == ContentState::AnnotationThread {
                if let Some(hit) = view
                    .list_item_rects
                    .iter()
                    .find(|hit| point_in_rect(x, y, hit.rect))
                {
                    if let Some(position) = app
                        .visible_annotations()
                        .iter()
                        .position(|annotation| annotation.id == hit.id)
                    {
                        app.selected_annotation_index = position;
                        app.touch();
                        return Ok(());
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
            // Reserved for S6 (tab-bar scroll) and future drag-to-select.
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            // M91 S7: wheel up/down scrolls the focused list. When the
            // cursor is over the tab bar (y == 1) the wheel does NOT
            // scroll per AC-09.
            //
            // M169-rev scrollbar fix: broaden the gate so the wheel
            // also scrolls MilestoneDetail / BacklogDetail /
            // AnnotationThread. Pre-fix the gate was
            // `app.content == ContentState::List`, which silently
            // dropped wheel events on detail screens — the user
            // reported "the mouse isn't working when scrolling into
            // the milestone" because `app.move_up` / `app.move_down`
            // already handle the detail scroll case (decrementing
            // `detail_scroll`); only the dispatch gate was wrong.
            let scrollable = matches!(
                app.content,
                ContentState::List
                    | ContentState::MilestoneDetail
                    | ContentState::BacklogDetail
                    | ContentState::AnnotationThread
            );
            if y >= 2 && scrollable {
                match mouse.kind {
                    MouseEventKind::ScrollUp => app.move_up(),
                    MouseEventKind::ScrollDown => app.move_down(),
                    _ => {}
                }
            }
            // Wheel on tab bar / header / non-scrollable content is a no-op.
        }
        _ => {}
    }

    Ok(())
}

/// M137 AC-04 / external-review F-01: map a scrollbar-track click to
/// the owning region's scroll state. Pure dispatch over
/// [`ScrollableId`] — the click math lives in
/// [`track_click_to_scroll`].
fn apply_scrollbar_track_click(app: &mut App, hit: &ScrollbarHitArea, y: u16) {
    // M137 code-review: when the content fits (no overflow), no thumb is
    // rendered — only the reserved gutter. A click on that rail should not
    // shift the selection, since there is nothing to scroll and no visual
    // affordance saying the click is meaningful.
    if hit.total <= hit.visible {
        return;
    }
    let y_in_track = y.saturating_sub(hit.rect.y);
    let new_scroll = track_click_to_scroll(hit.rect.height, y_in_track, hit.total);
    match &hit.id {
        ScrollableId::MilestonesList | ScrollableId::BacklogList | ScrollableId::OverviewInbox => {
            let max = hit.total.saturating_sub(1);
            app.selected_index = new_scroll.min(max);
            app.touch();
        }
        ScrollableId::MilestoneDetail
        | ScrollableId::AnnotationThread
        | ScrollableId::BacklogDetail => {
            let max = app.detail_max_scroll.get() as usize;
            app.detail_scroll = (new_scroll as u16).min(max as u16);
            app.touch();
        }
        ScrollableId::PathLane => {
            let max = app.path_max_scroll.get() as usize;
            app.path_scroll = (new_scroll as u16).min(max as u16);
            app.touch();
        }
    }
}

/// M135 F-02: resolve a list-row click `(x, y)` to the `selected_index`
/// of the item under it, per lane. Returns `None` when the click is not
/// inside any `view.list_item_rects` entry.
///
/// The `selected_index` semantics differ per lane, which is why the
/// id-to-position resolution lives here rather than in the view:
///   - **Milestones**: `selected_index` indexes into `visible_milestones()`
///     (a filtered view when `hide_done` is set), so the clicked item's id
///     is resolved back to its position in that filtered list.
///   - **Backlog** / **Overview**: `selected_index` indexes into
///     `app.backlog` / `app.dashboard.inbox_items` directly, so the id is
///     resolved to its position in the raw vector.
///
/// `pub(crate)` so `tui_view_state.rs` can assert the resolution directly
/// (F-04) without going through the full dispatch path.
pub(crate) fn resolve_list_click(
    app: &App,
    view: &view_state::ViewState,
    x: u16,
    y: u16,
) -> Option<usize> {
    let hit = view
        .list_item_rects
        .iter()
        .find(|h| point_in_rect(x, y, h.rect))?;
    match app.active_lane {
        Lane::Milestones => app.visible_milestones().iter().position(|m| m.id == hit.id),
        Lane::Backlog => app.backlog.iter().position(|b| b.id == hit.id),
        Lane::Overview => app
            .dashboard
            .inbox_items
            .iter()
            .position(|i| i.id == hit.id),
        // Path / Board / other lanes are not list-click surfaces here.
        _ => None,
    }
}

/// M135 F-02: inclusive-x, inclusive-y point-in-rect test. `Rect` is
/// exclusive on width/height in ratatui's layout model (a cell at
/// `x + width` belongs to the next widget), so the test is
/// `x in [r.x, r.x + width)` and `y in [r.y, r.y + height)`.
fn point_in_rect(x: u16, y: u16, r: Rect) -> bool {
    x >= r.x && x < r.x.saturating_add(r.width) && y >= r.y && y < r.y.saturating_add(r.height)
}

/// M91 S3: pure function that maps a single key press on the focused tab bar
/// into a lane navigation action, or `None` if the key isn't a tab-bar bind.
///
/// Routing table (modifiers NONE only):
///   Left | h       -> Previous
///   Right | l      -> Next
///   1..=N          -> Jump(idx)  where N == Lane::ordered().len()
///   Enter          -> FocusContent   (loads current lane data, defocuses tab bar)
///   _              -> None
///
/// M164: the upper bound `N` follows `Lane::ordered().len()` so adding a
/// lane extends the digit-jump range rather than leaving the upper bound
/// stuck at the pre-M164 hardcoded 7 (or 8 after a similar stale bump).
///
/// M91 S2 removed the legacy `[/]` resize keys (ResizeDec/ResizeInc). The
/// sidebar that they used to resize is gone — no resizable surface left.
#[derive(Debug, PartialEq, Eq)]
pub enum TabBarAction {
    Previous,
    Next,
    Jump(usize),
    FocusContent,
}

pub fn tab_bar_action(key: &crossterm::event::KeyEvent) -> Option<TabBarAction> {
    use crossterm::event::{KeyCode, KeyModifiers};
    if key.modifiers != KeyModifiers::NONE {
        return None;
    }
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => Some(TabBarAction::Previous),
        KeyCode::Right | KeyCode::Char('l') => Some(TabBarAction::Next),
        KeyCode::Enter => Some(TabBarAction::FocusContent),
        KeyCode::Char(c) => {
            let max = Lane::ordered().len();
            if max <= 9 {
                c.to_digit(10).and_then(|d| {
                    let idx = (d as usize).saturating_sub(1);
                    if idx < max {
                        Some(TabBarAction::Jump(idx))
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// **M169-rev scrollbar fix:** integration tests under
/// `crates/raul/tests/` drive mouse events through this module via the
/// `pub fn handle_mouse` above. The wheel-scroll gate change is pinned
/// by
/// `m169_rev_scrollbar.rs::rev_wheel_scrolls_milestone_detail_via_handle_mouse`
/// and friends. Not part of any external API surface.
#[doc(hidden)]
pub mod test_helpers {
    pub use super::handle_mouse;
}

#[cfg(test)]
mod tests {
    //! M134 review-remediation unit tests. The integration tests in
    //! `crates/raul/tests/tui_smoothness.rs` drive the loop end-to-end; the
    //! cases here exercise `handle_esc` directly so we can pin the
    //! dirty-signal contract for the List-path focus flip without needing
    //! a full `run_loop` integration.

    use super::*;

    /// M167: pressing Esc on a `ContentState::List` is a no-op (the
    /// pre-M167 focus-flip behavior is gone). Esc on a drilled-in detail
    /// still pops back via `go_back()`.
    #[test]
    fn esc_on_list_is_noop() {
        let mut app = App::new();
        app.load_milestones(vec![]);
        let before = app.version();
        let runner = MpRunner::new().expect("mp required for runner-using tests");
        super::action::apply_esc(&mut app, &runner).unwrap();
        assert_eq!(
            app.version(),
            before,
            "Esc on a top-level list must be a no-op (no focus state to flip)"
        );
    }

    /// M167 regression: pre-M167, Esc on an already-focused tab bar was
    /// idempotent for the version counter. The state still exists; the
    /// test is preserved but reframed — Esc on a List never changes
    /// anything today.
    #[test]
    fn esc_on_list_idempotent_for_version() {
        let mut app = App::new();
        app.load_milestones(vec![]);
        let before = app.version();
        let runner = MpRunner::new().expect("mp required for runner-using tests");
        super::action::apply_esc(&mut app, &runner).unwrap();
        // Run twice — both invocations must be no-ops.
        super::action::apply_esc(&mut app, &runner).unwrap();
        assert_eq!(
            app.version(),
            before,
            "Esc on a top-level list must be idempotent"
        );
        // _ = before; suppresses unused warning while keeping the
        // semantic anchor in the test name.
        let _ = before;
        // _original test body preserved below for trace.
        // assert!(
        //     app.version() == before,
        //     "Esc on an already-focused tab bar must not bump the version"
        // );
    }

    // =========================================================================
    // M135 F-04: click → selection behavior tests
    // =========================================================================
    //
    // The integration tests in `tui_view_state.rs` assert the ViewState's
    // SHAPE (rects have the right ids / heights / ordering). They
    // cannot prove that a click actually SELECTS the item under it —
    // which is why F-02 (board-card clicks did nothing, list clicks
    // used a wrong hardcoded offset) went undetected. These unit tests
    // live here, where `handle_mouse` and the `resolve_list_click` /
    // `resolve_board_click` helpers are visible, and exercise the
    // dispatch behaviorally: click a rect, assert selection changed to
    // the item under the click. (`handle_mouse` was widened from
    // `pub(crate)` to `pub` in M169-rev so the wheel-event regression
    // tests in `m169_rev_scrollbar.rs` could drive the dispatch
    // directly; the integration tests in `tui_view_state.rs` still
    // exercise the ViewState shape.)

    use crate::tui::app::{BacklogLine, DashboardSnapshot, InboxLine, MilestoneSummary};

    fn milestones_app_3() -> App {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        app.load_milestones(vec![
            MilestoneSummary {
                id: "01".to_string(),
                title: "Setup".to_string(),
                lifecycle: "complete".to_string(),
                lifecycle_at: None,
                depends_on: vec![],
                priority: "normal".to_string(),
                updated: String::new(),
                cancelled: false,
                cancelled_at: None,
                cancel_reason: None,
            flow_stages: BTreeMap::new(),
            },
            MilestoneSummary {
                id: "02".to_string(),
                title: "Engine".to_string(),
                lifecycle: "approved".to_string(),
                lifecycle_at: None,
                depends_on: vec![],
                priority: "normal".to_string(),
                updated: String::new(),
                cancelled: false,
                cancelled_at: None,
                cancel_reason: None,
            flow_stages: BTreeMap::new(),
            },
            MilestoneSummary {
                id: "03".to_string(),
                title: "Polish".to_string(),
                lifecycle: "draft".to_string(),
                lifecycle_at: None,
                depends_on: vec![],
                priority: "normal".to_string(),
                updated: String::new(),
                cancelled: false,
                cancelled_at: None,
                cancel_reason: None,
            flow_stages: BTreeMap::new(),
            },
        ]);
        app
    }

    fn backlog_app_3() -> App {
        let mut app = App::new();
        app.select_lane(Lane::Backlog);
        app.backlog = vec![
            BacklogLine {
                id: "BL-01".to_string(),
                title: "Refactor parser".to_string(),
                priority: "high".to_string(),
                status: "open".to_string(),
                resolution: "".to_string(),
            },
            BacklogLine {
                id: "BL-02".to_string(),
                title: "Add CSV export".to_string(),
                priority: "medium".to_string(),
                status: "open".to_string(),
                resolution: "".to_string(),
            },
            BacklogLine {
                id: "BL-03".to_string(),
                title: "Improve errors".to_string(),
                priority: "low".to_string(),
                status: "resolved".to_string(),
                resolution: "shipped".to_string(),
            },
        ];
        app
    }

    /// F-02/F-04: clicking each pre-computed list_item_rect selects the
    /// item under the click. Milestones lane.
    #[test]
    fn list_click_resolves_to_correct_milestone_index() {
        let app = milestones_app_3();
        let view = view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
        assert_eq!(view.list_item_rects.len(), 3, "3 milestones visible");

        // Click the center of each rect; resolve must return the item's
        // position in visible_milestones() (what selected_index indexes).
        for (expected_idx, hit) in view.list_item_rects.iter().enumerate() {
            let cx = hit.rect.x + hit.rect.width / 2;
            let cy = hit.rect.y;
            let resolved = resolve_list_click(&app, &view, cx, cy);
            assert_eq!(
                resolved,
                Some(expected_idx),
                "click on milestone {:?} (rect center {},{}) must resolve to index {}",
                hit.id,
                cx,
                cy,
                expected_idx
            );
        }
    }

    /// F-02/F-04: Backlog lane list click resolves correctly.
    #[test]
    fn list_click_resolves_to_correct_backlog_index() {
        let app = backlog_app_3();
        let view = view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
        assert_eq!(view.list_item_rects.len(), 3);

        for (expected_idx, hit) in view.list_item_rects.iter().enumerate() {
            let cx = hit.rect.x + hit.rect.width / 2;
            let cy = hit.rect.y;
            assert_eq!(
                resolve_list_click(&app, &view, cx, cy),
                Some(expected_idx),
                "click on backlog {:?} must resolve to index {}",
                hit.id,
                expected_idx
            );
        }
    }

    /// F-02/F-04: the pre-fix hardcoded `y.saturating_sub(3)` offset was
    /// wrong for Overview (inbox block sits inside the dashboard split).
    /// Verify resolve_list_click returns the right index for Overview
    /// inbox items via the pre-computed rects.
    #[test]
    fn list_click_resolves_overview_inbox_not_hardcoded_offset() {
        let mut app = App::new();
        app.select_lane(Lane::Overview);
        app.dashboard = DashboardSnapshot {
            inbox_items: vec![
                InboxLine {
                    id: "EXEC-1".to_string(),
                    kind: "spec-review".to_string(),
                    display: "M10 review".to_string(),
                    reason: "pending".to_string(),
                    action: "mp milestone approve 10".to_string(),
                },
                InboxLine {
                    id: "TW-3".to_string(),
                    kind: "track".to_string(),
                    display: "Fix output".to_string(),
                    reason: "tweak".to_string(),
                    action: "mp track show".to_string(),
                },
            ],
            ..Default::default()
        };
        let view = view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
        // Each inbox item must resolve to its position by id, NOT to
        // `y - 3` (which would pick the wrong item since the inbox block
        // starts well below row 3).
        for (expected_idx, hit) in view.list_item_rects.iter().enumerate() {
            let cx = hit.rect.x + hit.rect.width / 2;
            let cy = hit.rect.y + hit.rect.height / 2;
            assert_eq!(
                resolve_list_click(&app, &view, cx, cy),
                Some(expected_idx),
                "Overview inbox click on {:?} must resolve to index {} (pre-fix offset was wrong here)",
                hit.id,
                expected_idx
            );
        }
    }

    /// F-04: handle_mouse dispatch on a list row mutates selected_index.
    #[test]
    fn handle_mouse_on_list_row_sets_selected_index() {
        let mut app = milestones_app_3();
        let runner = match MpRunner::new() {
            Ok(r) => r,
            Err(_) => {
                eprintln!("skipping: mp binary not resolvable");
                return;
            }
        };
        let view = view_state::compute_view(&app, Rect::new(0, 0, 100, 30));
        // Click the second milestone (id "02", index 1).
        let m02 = &view.list_item_rects[1];
        let cx = m02.rect.x + m02.rect.width / 2;
        let cy = m02.rect.y;

        handle_mouse(
            &mut app,
            &runner,
            crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: cx,
                row: cy,
                modifiers: crossterm::event::KeyModifiers::empty(),
            },
            (100, 30),
        )
        .expect("handle_mouse on list row must not error");

        assert_eq!(app.selected_index, 1, "click on milestone 02 sets index 1");
    }

    /// M137 AC-04 / external-review F-01: clicking the scrollbar track
    /// jumps `selected_index` via `track_click_to_scroll` before any
    /// list-row hit test.
    #[test]
    fn handle_mouse_on_scrollbar_track_jumps_scroll() {
        let mut app = App::new();
        app.select_lane(Lane::Milestones);
        let ms: Vec<_> = (1..=40)
            .map(|i| MilestoneSummary {
                id: format!("{i:02}"),
                title: format!("M{i}"),
                lifecycle: "approved".to_string(),
                lifecycle_at: None,
                depends_on: vec![],
                priority: "normal".to_string(),
                updated: String::new(),
                cancelled: false,
                cancelled_at: None,
                cancel_reason: None,
            flow_stages: BTreeMap::new(),
            })
            .collect();
        app.load_milestones(ms);
        app.selected_index = 0;

        let runner = match MpRunner::new() {
            Ok(r) => r,
            Err(_) => {
                eprintln!("skipping: mp binary not resolvable");
                return;
            }
        };

        let term = (80u16, 16u16);
        let view = view_state::compute_view(&app, Rect::new(0, 0, term.0, term.1));
        let hit = view
            .scrollbar_rects
            .iter()
            .find(|h| matches!(h.id, ScrollableId::MilestonesList))
            .expect("milestones scrollbar present");
        assert!(
            hit.total >= 40,
            "expected overflowing list; total={}",
            hit.total
        );

        let click_y = hit.rect.y + hit.rect.height.saturating_sub(1);
        let before = app.version();
        handle_mouse(
            &mut app,
            &runner,
            crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: hit.rect.x,
                row: click_y,
                modifiers: crossterm::event::KeyModifiers::empty(),
            },
            term,
        )
        .expect("handle_mouse on scrollbar track must not error");

        assert!(
            app.selected_index > 20,
            "bottom-of-track click should jump selection well past the start; got {}",
            app.selected_index
        );
        assert!(
            app.version() > before,
            "scrollbar track click must bump the version (dirty signal)"
        );
    }

    /// M179: `fire_watch_tick` is now a no-op stub. The M164-era
    /// tests that drove the deadline re-arm path are gone — manual
    /// Overview refresh (r/R) is the only refresh path. M179 S7
    /// will reintroduce polling under a new entry point (the
    /// Watch-tab `on_idle` hook in `run_loop`).
    #[test]
    fn fire_watch_tick_is_a_noop() {
        let mut app = App::new();
        let runner = match MpRunner::new() {
            Ok(r) => r,
            Err(_) => {
                eprintln!("skipping: mp binary not resolvable");
                return;
            }
        };
        super::fire_watch_tick(&mut app, &runner)
            .expect("fire_watch_tick must remain a no-op post-M179");
    }
}
