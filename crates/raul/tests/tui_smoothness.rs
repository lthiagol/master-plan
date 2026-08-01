//! M134: TUI smoothness — frame-rate gate, batch drain, dirty signal,
//! sync output.
//!
//! These tests pin the four behaviors called out in the milestone's
//! acceptance criteria. The first three drive `run_loop` end-to-end with
//! a `TestBackend` and synthetic events; the fourth exercises
//! `SyncOutputGuard` directly against a `Vec<u8>` to assert on the byte
//! stream. None of them shell out to `mp`, so the suite stays hermetic
//! even on machines without the `mp` binary installed.

use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use raul::tui::app::{App, Lane, MilestoneSummary};
use raul::tui::runner::{run_loop, EventSource, FrameClock, WaitResult};

// ---------------------------------------------------------------------------
// Event sources for tests
// ---------------------------------------------------------------------------

/// Wraps a `VecDeque<CrosstermEvent>` so the loop can be driven by a
/// deterministic, time-independent sequence of synthetic events. Both
/// `wait_next` and `poll_next` simply pop from the front; once empty,
/// both return `Ok(None)` and the loop exits cleanly. This is what lets
/// the integration tests assert on render counts without spinning up a
/// real terminal.
struct VecSource {
    events: VecDeque<CrosstermEvent>,
}

impl VecSource {
    fn new(events: Vec<CrosstermEvent>) -> Self {
        Self {
            events: events.into(),
        }
    }
}

impl EventSource for VecSource {
    fn wait_next(&mut self) -> Result<Option<CrosstermEvent>> {
        Ok(self.events.pop_front())
    }

    fn poll_next(&mut self) -> Result<Option<CrosstermEvent>> {
        Ok(self.events.pop_front())
    }
}

/// Real-time event source backed by a channel. The test spawns a thread
/// that pushes events at a configurable cadence; `wait_next` blocks on
/// `recv()`, `poll_next` is a non-blocking `try_recv()`. Used by AC-01 to
/// simulate sustained key-repeat input over ~100ms so the frame-rate
/// gate has real wall-clock pressure to throttle against.
struct ChannelSource {
    rx: mpsc::Receiver<CrosstermEvent>,
}

struct IdleSource {
    waits: Vec<Duration>,
}

impl EventSource for IdleSource {
    fn wait_next(&mut self) -> Result<Option<CrosstermEvent>> {
        Ok(None)
    }

    fn wait_next_timeout(&mut self, timeout: Duration) -> Result<WaitResult> {
        self.waits.push(timeout);
        Ok(WaitResult::Idle)
    }

    fn poll_next(&mut self) -> Result<Option<CrosstermEvent>> {
        Ok(None)
    }
}

impl ChannelSource {
    fn new(rx: mpsc::Receiver<CrosstermEvent>) -> Self {
        Self { rx }
    }
}

impl EventSource for ChannelSource {
    fn wait_next(&mut self) -> Result<Option<CrosstermEvent>> {
        match self.rx.recv() {
            Ok(ev) => Ok(Some(ev)),
            // Sender dropped → end of stream. The loop treats `None` as a
            // clean exit so the test tears down without hanging.
            Err(_) => Ok(None),
        }
    }

    fn poll_next(&mut self) -> Result<Option<CrosstermEvent>> {
        match self.rx.try_recv() {
            Ok(ev) => Ok(Some(ev)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_j_key() -> CrosstermEvent {
    // Use `KeyEvent::new` (stable API) instead of a struct literal so this
    // test stays portable across crossterm versions where `kind`/`state`
    // may become private fields. `KeyEvent::new` ships a sensible default
    // (`KeyEventKind::Press`, `KeyEventState::empty`/NONE) that matches
    // what every terminal produces for a normal key press.
    CrosstermEvent::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
}

fn make_q_key() -> CrosstermEvent {
    CrosstermEvent::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
}

fn app_with_n_milestones(n: usize) -> App {
    let mut app = App::new();
    app.select_lane(Lane::Milestones);
    let milestones: Vec<MilestoneSummary> = (1..=n)
        .map(|i| MilestoneSummary {
            id: format!("{:03}", i),
            title: format!("Milestone {i}"),
            lifecycle: "planned".to_string(),
            lifecycle_at: None,
            depends_on: vec![],
            priority: "normal".to_string(),
            updated: String::new(),
        })
        .collect();
    app.load_milestones(milestones);
    app
}

fn new_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(80, 24)).unwrap()
}

#[test]
fn watch_idle_deadline_ticks_without_input_or_end_of_stream() {
    let mut terminal = new_terminal();
    let mut app = App::new();
    app.select_lane(Lane::Watch);
    app.watch_poller.last_poll = Some(Instant::now());
    let mut clock = FrameClock::with_interval(Duration::ZERO);
    let mut sync = Vec::new();
    let mut source = IdleSource { waits: vec![] };
    let mut idle_count = 0usize;

    run_loop(
        &mut terminal,
        &mut app,
        &mut clock,
        &mut sync,
        &mut source,
        |_app, _event, _size| Ok(()),
        |app| {
            idle_count += 1;
            if idle_count == 2 {
                app.quitting = true;
            }
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(idle_count, 2);
    assert_eq!(source.waits.len(), 2);
    assert!(source
        .waits
        .iter()
        .all(|timeout| *timeout > Duration::from_secs(2)));
}

/// Production-mimicking dispatch closure for the test loop. Maps the
/// keys we use (`j`/`k`/`q`) onto `App` mutations; everything else is a
/// no-op. The closure deliberately avoids going through `MpRunner`
/// because the test does not need to shell out — the goal is to verify
/// the loop's frame gate / dirty signal / drain semantics, not the
/// full event-handling tree.
fn test_dispatch(app: &mut App, event: CrosstermEvent, _term_size: (u16, u16)) -> Result<()> {
    if let CrosstermEvent::Key(key) = event {
        match key.code {
            KeyCode::Char('j') => {
                app.move_down();
            }
            KeyCode::Char('q') => {
                app.quit();
            }
            _ => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AC-01 — rate gate caps renders under a synthetic key-repeat storm
// ---------------------------------------------------------------------------

#[test]
fn rate_gate_caps_redraws_under_storm() {
    // 100 'j' events at 1ms cadence == ~100ms of synthetic key-repeat.
    // Over 100ms with a 16ms gate, the floor is ~6 renders plus the
    // initial frame; the upper bound the spec pins is 7 (60fps × 0.1s).
    let (tx, rx) = mpsc::channel();
    let producer = thread::spawn(move || {
        for _ in 0..100 {
            tx.send(make_j_key()).expect("send");
            thread::sleep(Duration::from_millis(1));
        }
        // Dropping `tx` signals end-of-stream to the receiver.
    });

    let mut source = ChannelSource::new(rx);
    let mut app = app_with_n_milestones(200); // enough rows to scroll through
                                              // Production default: 16ms gate.
    let mut frame_clock = FrameClock::new();
    let mut terminal = new_terminal();
    let mut sync_buf = Vec::new();

    let start = Instant::now();
    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_buf,
        &mut source,
        test_dispatch,
        |_app| Ok(()),
    )
    .expect("loop");
    let elapsed = start.elapsed();

    producer.join().expect("producer");

    // Sanity: the loop ran for roughly the producer's pacing window.
    // Under load the 1ms sleeps stretch (often ~200ms); only require a
    // lower bound so we know the storm was not instantaneous.
    assert!(
        elapsed >= Duration::from_millis(80),
        "loop should run for ~100ms+; got {}ms",
        elapsed.as_millis()
    );

    // Core assertion: redraw count is bounded by the 16ms gate *relative
    // to wall time*. An absolute ceiling of 8 only held when elapsed was
    // ~100ms; under scheduling load the window stretches and more gated
    // frames are correct (e.g. 10 redraws in 200ms ≈ 50fps, still gated).
    // ceiling = floor(elapsed_ms / 16) + headroom for the initial frame
    // and timer jitter.
    let elapsed_ms = elapsed.as_millis();
    let ceiling = (elapsed_ms / 16).saturating_add(3);
    assert!(
        u128::from(frame_clock.redraws) <= ceiling,
        "rate gate should cap redraws; got {} redraws in {}ms (ceiling {ceiling})",
        frame_clock.redraws,
        elapsed_ms,
    );
    // Without a gate, 100 key events would each force a redraw. Require
    // heavy coalescing so a broken gate cannot hide behind a long elapsed.
    assert!(
        frame_clock.redraws < 40,
        "rate gate should coalesce heavily under 100-event storm; got {} redraws in {}ms",
        frame_clock.redraws,
        elapsed_ms,
    );
    // And the gate must actually have let some renders through — otherwise
    // the test would be vacuous (e.g. producer never started).
    assert!(
        frame_clock.redraws >= 2,
        "rate gate should allow several redraws; got {} redraws in {}ms",
        frame_clock.redraws,
        elapsed_ms,
    );
}

// ---------------------------------------------------------------------------
// AC-02 — batch drain coalesces N pending events into a single render
// ---------------------------------------------------------------------------

#[test]
fn batch_drain_coalesces_pending_events() {
    // 50 events queued up-front. `VecSource` returns them instantly, so
    // the loop processes all of them in one drain pass.
    let events: Vec<CrosstermEvent> = (0..50).map(|_| make_j_key()).collect();
    let mut source = VecSource::new(events);
    let mut app = app_with_n_milestones(200); // plenty of scroll headroom
                                              // Disable the rate gate so the post-batch render fires regardless of
                                              // how fast the loop runs — otherwise an instant batch could be gated
                                              // out by the 16ms timer and we'd observe 0 post-batch draws, which
                                              // would be indistinguishable from "the loop forgot to render".
    let mut frame_clock = FrameClock::with_interval(Duration::ZERO);
    let mut terminal = new_terminal();
    let mut sync_buf = Vec::new();

    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_buf,
        &mut source,
        test_dispatch,
        |_app| Ok(()),
    )
    .expect("loop");

    // With the pre-M134 loop, 50 events would produce 50 draws (one per
    // iteration). With M134, the batch coalesces into a single post-batch
    // render on top of the initial frame: total draws == 2 (1 initial + 1
    // post-batch). The gate is disabled above, so the post-batch render
    // is unconditional.
    assert_eq!(
        frame_clock.redraws, 2,
        "batched events should coalesce into one post-batch draw; got {} redraws for 50 events",
        frame_clock.redraws
    );

    // Confirm the dirty signal actually fired: every 'j' would have moved
    // selected_index, so the app's version counter must have advanced.
    assert!(
        app.version() > 0,
        "dispatched events should have bumped app.version(); got {}",
        app.version()
    );
}

// ---------------------------------------------------------------------------
// F-02 regression — a mutation left pending behind a closed gate still
// paints before the loop exits, instead of being stranded until the next
// keypress.
// ---------------------------------------------------------------------------

/// External-review F-02: when a mutation sets `needs_render` while the
/// frame gate is closed and the event stream then ends, the deferred-frame
/// path must still paint that final mutation. Without the fix the loop
/// would `wait_next` → `None` → `break`, leaving the last state unpainted
/// until the user pressed another key.
///
/// We use a wide gate interval (300ms) so the gate is reliably closed
/// between the initial frame and the single `j` mutation, then end the
/// stream. The assertion is that the `j` mutation's frame fired
/// (>= 2 redraws: the initial frame + the deferred one). A regression that
/// drops the deferred block leaves this at exactly 1.
#[test]
fn deferred_frame_paints_after_mutation_when_stream_ends() {
    let mut source = VecSource::new(vec![make_j_key()]);
    let mut app = app_with_n_milestones(200); // enough rows that `j` mutates
    let mut frame_clock = FrameClock::with_interval(Duration::from_millis(300));
    let mut terminal = new_terminal();
    let mut sync_buf = Vec::new();

    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_buf,
        &mut source,
        test_dispatch,
        |_app| Ok(()),
    )
    .expect("loop");

    // The initial frame is 1 redraw; the `j` mutation is then stranded
    // behind the closed gate and the stream ends. The deferred-frame fix
    // must paint it before exiting — a regression drops this back to 1.
    assert!(
        frame_clock.redraws >= 2,
        "deferred frame should paint the final mutation before exit; \
         got {} redraws",
        frame_clock.redraws,
    );
    // Sanity: the mutation actually happened, so `needs_render` was set
    // and the deferred path is what we exercised.
    assert_eq!(
        app.selected_index, 1,
        "the `j` event should have advanced selection; got {}",
        app.selected_index,
    );
}

// ---------------------------------------------------------------------------
// AC-03 — no-op event for the current focus does not redraw
// ---------------------------------------------------------------------------

#[test]
fn no_op_event_skips_redraw() {
    // Single-milestone list, cursor at index 0. Pressing 'j' on the
    // bottom row is a no-op (move_down clamps); the dirty signal must
    // stay false and `redraws` must not advance past the initial frame.
    let mut source = VecSource::new(vec![make_j_key(), make_j_key(), make_j_key()]);
    let mut app = app_with_n_milestones(1);
    // Disable the rate gate so the only thing standing between a
    // no-op event and a redraw is the dirty signal itself.
    let mut frame_clock = FrameClock::with_interval(Duration::ZERO);
    let mut terminal = new_terminal();
    let mut sync_buf = Vec::new();

    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_buf,
        &mut source,
        test_dispatch,
        |_app| Ok(()),
    )
    .expect("loop");

    assert_eq!(
        frame_clock.redraws, 1,
        "no-op events must not trigger additional redraws; got {}",
        frame_clock.redraws
    );
    // The dirty-signal contract: no mutation, no version bump beyond the
    // baseline `app_with_n_milestones(1)` already established (select_lane
    // + load_milestones = 2 bumps).
    assert_eq!(
        app.version(),
        2,
        "no-op events must not bump app.version() beyond setup; got {}",
        app.version()
    );
}

/// Companion to the no-op test: a *real* mutation in the same batch must
/// still trigger exactly one post-batch redraw. This pins the
/// dirty-signal's other half — "only skip renders when state did NOT
/// change" — and guards against an over-aggressive optimisation that
/// drops legitimate redraws.
#[test]
fn mutation_in_batch_triggers_one_post_batch_redraw() {
    // Three mutations (cursor 0 → 1 → 2 → 3). All three 'j' presses move
    // the cursor — there is no no-op in this batch — so the dirty signal
    // stays set and exactly one post-batch render must fire on top of
    // the initial frame.
    let events = vec![make_j_key(), make_j_key(), make_j_key()];
    let mut source = VecSource::new(events);
    let mut app = app_with_n_milestones(5);
    // Disable the rate gate so the only thing standing between a mutation
    // and a redraw is the dirty signal itself.
    let mut frame_clock = FrameClock::with_interval(Duration::ZERO);
    let mut terminal = new_terminal();
    let mut sync_buf = Vec::new();

    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_buf,
        &mut source,
        test_dispatch,
        |_app| Ok(()),
    )
    .expect("loop");

    assert_eq!(
        frame_clock.redraws, 2,
        "expected initial + one post-batch redraw; got {}",
        frame_clock.redraws
    );
    assert_eq!(
        app.selected_index, 3,
        "three 'j' presses should have moved cursor to 3; selected_index = {}",
        app.selected_index
    );
}

// ---------------------------------------------------------------------------
// AC-04 — sync-output guard emits CSI ?2026 h/l around a draw
// ---------------------------------------------------------------------------

#[test]
fn sync_output_guard_writes_begin_and_end() {
    use raul::tui::runner::SyncOutputGuard;

    // The guard holds `&mut buf` exclusively for its lifetime — anything
    // we want to write between the bracket sequences has to land in a
    // separate buffer that we concatenate afterwards. The structural
    // invariants we pin are: (a) the buffer starts with the begin
    // sequence, (b) ends with the end sequence, and (c) the two are
    // back-to-back when no draw bytes are interleaved.
    let mut buf: Vec<u8> = Vec::new();
    {
        let _guard = SyncOutputGuard::begin(&mut buf);
    } // guard drops here → CSI ?2026 l

    let bytes = String::from_utf8_lossy(&buf);
    assert!(
        bytes.starts_with("\x1b[?2026h"),
        "guard must emit begin sequence first; got {bytes:?}"
    );
    assert!(
        bytes.ends_with("\x1b[?2026l"),
        "guard must emit end sequence last; got {bytes:?}"
    );
    // With no draw bytes interleaved, the begin and end sequences sit
    // back-to-back (modulo the flushes the guard performs).
    assert!(
        bytes == "\x1b[?2026h\x1b[?2026l",
        "guard must emit only the bracket sequences; got {bytes:?}"
    );
}

/// End-to-end check that the loop's draw path writes the bracket
/// sequences into the `sync_writer` it was given. Drives `run_loop`
/// against a `Vec<u8>` as the sync writer and a `VecSource` containing
/// a single mutation so the post-batch render fires.
#[test]
fn run_loop_writes_sync_sequences_around_draw() {
    let mut source = VecSource::new(vec![make_j_key()]);
    let mut app = app_with_n_milestones(5);
    // Disable the rate gate so the post-batch render fires
    // deterministically and we can assert a known number of
    // bracket sequences.
    let mut frame_clock = FrameClock::with_interval(Duration::ZERO);
    let mut terminal = new_terminal();
    let mut sync_buf: Vec<u8> = Vec::new();

    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_buf,
        &mut source,
        test_dispatch,
        |_app| Ok(()),
    )
    .expect("loop");

    let s = String::from_utf8_lossy(&sync_buf);
    // The loop should have begun and ended the sync-output region once
    // per actual draw — once for the initial frame, once for the
    // post-batch frame.
    assert!(
        s.contains("\x1b[?2026h"),
        "loop must wrap draws with CSI ?2026 h; got {s:?}"
    );
    assert!(
        s.contains("\x1b[?2026l"),
        "loop must wrap draws with CSI ?2026 l; got {s:?}"
    );
    // And the count of begin sequences must match the count of redraws.
    let begin_count = s.matches("\x1b[?2026h").count();
    assert_eq!(
        begin_count, frame_clock.redraws as usize,
        "expected one begin per redraw; begins={begin_count}, redraws={}",
        frame_clock.redraws
    );

    // Structural invariant: every begin must be paired with an end.
    let mut opens = 0usize;
    let mut closes = 0usize;
    for chunk in s.split("\x1b[?2026h").skip(1) {
        opens += 1;
        assert!(
            chunk.contains("\x1b[?2026l"),
            "begin sequence at redraw #{opens} has no matching end sequence"
        );
        closes += chunk.matches("\x1b[?2026l").count();
    }
    assert_eq!(opens, closes, "every begin must be paired with an end");
}

// ---------------------------------------------------------------------------
// AC-05 — `q` still terminates the loop (regression guard)
// ---------------------------------------------------------------------------

#[test]
fn quit_event_terminates_loop_without_extra_redraw() {
    let mut source = VecSource::new(vec![make_j_key(), make_q_key()]);
    let mut app = app_with_n_milestones(5);
    let mut frame_clock = FrameClock::with_interval(Duration::ZERO);
    let mut terminal = new_terminal();
    let mut sync_buf = Vec::new();

    run_loop(
        &mut terminal,
        &mut app,
        &mut frame_clock,
        &mut sync_buf,
        &mut source,
        test_dispatch,
        |_app| Ok(()),
    )
    .expect("loop");

    assert!(app.quitting, "q must set the quitting flag");
    // Initial render (1) + post-j redraw (1) — the quit must NOT trigger
    // a third redraw after the dirty signal was cleared by the loop's
    // top-of-iteration check on `app.quitting`.
    assert_eq!(
        frame_clock.redraws, 2,
        "quit should not add a redundant redraw; got {}",
        frame_clock.redraws
    );
}
