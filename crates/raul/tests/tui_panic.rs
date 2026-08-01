//! M87 AC-02: a TUI panic must surface on stderr and propagate as a non-`Ok`
//! error instead of the old `Err(_) => Ok(())` silent swallow.

use std::any::Any;
use std::panic;
use std::sync::{Arc, Mutex};

use raul::tui::runner::{translate_catch_unwind, TerminalGuard, TerminalOps};

fn force_panic() -> anyhow::Result<()> {
    panic!("boom-from-test");
}

#[test]
fn panic_payload_lands_on_stderr_and_translates_to_error() {
    // Trigger a real panic and translate it the same way run_tui does.
    let result = panic::catch_unwind(force_panic);
    let mut buf: Vec<u8> = Vec::new();
    let translated = translate_catch_unwind(result, &mut buf);

    // 1. Returns a non-Ok error (the old code mapped panic -> Ok(())).
    let err = translated.expect_err("panic must translate to Err");
    let err_msg = format!("{err}");
    assert!(
        err_msg.contains("TUI panicked"),
        "error should mention the panic: {err_msg}"
    );
    assert!(
        err_msg.contains("boom-from-test"),
        "error should carry the panic payload: {err_msg}"
    );

    // 2. stderr buffer contains the human-readable payload.
    let stderr = String::from_utf8_lossy(&buf);
    assert!(
        stderr.contains("TUI panicked"),
        "stderr should announce the panic: {stderr}"
    );
    assert!(
        stderr.contains("boom-from-test"),
        "stderr should carry the panic payload: {stderr}"
    );
}

#[test]
fn ok_path_still_passes_through() {
    let result: anyhow::Result<()> = Ok(());
    let translated = translate_catch_unwind(Ok(result), &mut Vec::new());
    assert!(translated.is_ok(), "non-panic Ok must pass through");
}

#[test]
fn inner_error_passes_through() {
    // An inner Err (not a panic) must surface unchanged, not be masked.
    let result: anyhow::Result<()> = Err(anyhow::anyhow!("draw failed"));
    let translated = translate_catch_unwind(Ok(result), &mut Vec::new());
    let err = translated.expect_err("inner Err must surface");
    assert!(err.to_string().contains("draw failed"));
}

#[test]
fn panic_payload_downcast_string() {
    // Confirm `panic_message` extracts both `&'static str` and `String` payloads.
    let payload_str: Box<dyn Any + Send> = Box::new("str payload");
    let payload_string: Box<dyn Any + Send> = Box::new(String::from("string payload"));
    assert_eq!(
        raul::tui::runner::panic_message(&payload_str),
        "str payload"
    );
    assert_eq!(
        raul::tui::runner::panic_message(&payload_string),
        "string payload"
    );
}

struct FakeTerminal {
    fail_at: Option<&'static str>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl FakeTerminal {
    fn setup(&mut self, name: &'static str) -> anyhow::Result<()> {
        self.events.lock().unwrap().push(name);
        if self.fail_at == Some(name) {
            anyhow::bail!("injected {name} failure");
        }
        Ok(())
    }
}

impl TerminalOps for FakeTerminal {
    fn enable_raw(&mut self) -> anyhow::Result<()> {
        self.setup("raw")
    }
    fn enter_alternate(&mut self) -> anyhow::Result<()> {
        self.setup("alternate")
    }
    fn enable_mouse(&mut self) -> anyhow::Result<()> {
        self.setup("mouse")
    }
    fn disable_mouse(&mut self) {
        self.events.lock().unwrap().push("disable-mouse");
    }
    fn leave_alternate(&mut self) {
        self.events.lock().unwrap().push("leave-alternate");
    }
    fn disable_raw(&mut self) {
        self.events.lock().unwrap().push("disable-raw");
    }
}

#[test]
fn terminal_setup_unwinds_only_completed_stages() {
    let cases = [
        ("raw", vec!["raw"]),
        ("alternate", vec!["raw", "alternate", "disable-raw"]),
        (
            "mouse",
            vec![
                "raw",
                "alternate",
                "mouse",
                "leave-alternate",
                "disable-raw",
            ],
        ),
    ];
    for (fail_at, expected) in cases {
        let events = Arc::new(Mutex::new(Vec::new()));
        let result = TerminalGuard::new(FakeTerminal {
            fail_at: Some(fail_at),
            events: events.clone(),
        });
        assert!(result.is_err());
        assert_eq!(*events.lock().unwrap(), expected, "failure at {fail_at}");
    }
}

#[test]
fn terminal_success_unwinds_all_stages_in_reverse_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    {
        let _guard = TerminalGuard::new(FakeTerminal {
            fail_at: None,
            events: events.clone(),
        })
        .unwrap();
    }
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            "raw",
            "alternate",
            "mouse",
            "disable-mouse",
            "leave-alternate",
            "disable-raw"
        ]
    );
}
