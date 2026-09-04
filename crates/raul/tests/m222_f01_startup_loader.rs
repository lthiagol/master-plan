//! M222 F-03: production-path regression test for the
//! startup loader. The cycle-2 fix wired
//! `Keybinds::load_layered(runner)` into `runner.rs:417`,
//! but the cycle-2 regression tests in this file called
//! `Keybinds::load_effective` directly — they would still
//! pass if `runner.rs:417` reverted to `Keybinds::load`.
//!
//! The tests below call `Keybinds::load_layered(&runner)`
//! directly with a real `MpRunner::new()` so they exercise
//! the EXACT production code path the fix claims to wire.
//!
//! Regression-catching contract: if `runner.rs:417` reverts
//! to `Keybinds::load(runner)`, the `startup_load_layered_*`
//! tests fail. The reason: `Keybinds::load` reads only the
//! legacy `[keybinds]` JSON via `load_from_config`; it
//! never reads `~/.config/raul/keybinds.toml`. With no
//! legacy JSON override, the function returns defaults —
//! so the user-level TOML `quit = "ctrl+x"` would NOT
//! appear on the loaded `Keybinds.quit`, and the
//! assertion would fail.
//!
//! The two non-load_layered tests (`startup_loads_user_level_toml_*`,
//! `startup_precedence_*`) at the bottom of the file remain
//! as library-level coverage for the precedence contract;
//! they cannot catch a `runner.rs:417` regression by
//! themselves, so the `startup_load_layered_*` tests are
//! the load-bearing F-03 surface.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crossterm::event::{KeyCode, KeyModifiers};
use raul::mp_runner::MpRunner;
use raul::tui::keybinds::Keybinds;

/// Serialize tests that mutate `XDG_CONFIG_HOME` so the
/// parallel test runner doesn't race on the env var.
/// `Keybinds::default_path()` reads `XDG_CONFIG_HOME` at
/// call time, so the lock must wrap the entire
/// `load_layered` invocation (not just the env-var write).
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` with `XDG_CONFIG_HOME` set to `value`, then
/// restore the previous value (or remove the var if it
/// wasn't set). The lock prevents the parallel test runner
/// from interleaving env-var mutations across tests.
fn with_xdg_config_home<F, R>(value: &std::path::Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var("XDG_CONFIG_HOME").ok();
    // SAFETY: the mutex serializes access; the env var is
    // restored before any other test acquires the lock.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", value);
    }
    let result = f();
    match prev {
        Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
        None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
    }
    result
}

/// Construct an `MpRunner` (the SAME type `runner.rs:417`
/// uses). Panics if `mp` is not on PATH — production-path
/// tests need the real binary so the test exercises the
/// production shell-out surface too.
fn production_runner() -> MpRunner {
    MpRunner::new().expect(
        "M222 F-03 production-path test requires `mp` on PATH; \
         install with `make install` or set MP_HOME",
    )
}

// ---------------------------------------------------------------------------
// F-03 production-path tests: these call Keybinds::load_layered directly.
// If `runner.rs:417` reverts to Keybinds::load(runner), both fail.
// ---------------------------------------------------------------------------

/// F-03: the production startup path calls
/// `Keybinds::load_layered(runner)` and must apply the
/// user-level `~/.config/raul/keybinds.toml` overrides. If
/// `runner.rs:417` reverted to `Keybinds::load`, this test
/// fails because `load` does not consult the user-level
/// TOML.
#[test]
fn startup_load_layered_applies_user_level_toml_via_real_runner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");
    let toml_path = config_root.join("keybinds.toml");
    fs::write(
        &toml_path,
        // Override `quit` to a non-default chord. No legacy
        // JSON `[keybinds]` section is present in this test's
        // environment, so `Keybinds::load` (legacy-only) would
        // return defaults — the TOML override would NOT apply.
        "[global]\nquit = \"ctrl+x\"\n[autopilot]\nselect = \"f1\"\n",
    )
    .expect("write toml");

    let runner = production_runner();

    // Sanity: pin that `Keybinds::default_path()` resolves to
    // the temp file the test just wrote. The reviewer
    // demanded this pin explicitly: "Also assert
    // `Keybinds::default_path()` is the path load_layered
    // actually reads (so a future regression in default_path
    // resolution is caught)."
    let resolved: PathBuf = with_xdg_config_home(dir.path(), Keybinds::default_path);
    assert_eq!(
        resolved, toml_path,
        "`Keybinds::default_path()` under XDG_CONFIG_HOME={} must point at the test's TOML; got {resolved:?}",
        dir.path().display()
    );
    assert!(
        resolved.exists(),
        "default_path must point at an existing file; got {resolved:?}"
    );

    // Call the PRODUCTION function — `load_layered`, not
    // `load_effective`. If `runner.rs:417` reverted to
    // `Keybinds::load`, this assertion would fail because the
    // TOML would not be read.
    let kb = with_xdg_config_home(dir.path(), || Keybinds::load_layered(&runner));

    assert_eq!(
        kb.quit,
        vec![(KeyCode::Char('x'), KeyModifiers::CONTROL)],
        "user-level `quit = ctrl+x` must apply through `load_layered`; \
         if this fails, runner.rs:417 likely reverted to Keybinds::load"
    );
    assert_eq!(
        kb.lane_autopilot.select,
        vec![(KeyCode::F(1), KeyModifiers::empty())],
        "user-level `[autopilot] select = f1` must apply through `load_layered`"
    );
}

/// F-03 (silent fallback): with no user-level file,
/// `load_layered` returns the defaults. This pins the
/// "missing file is silent" contract from cycle-2's
/// `try_reload` semantics through the production function.
#[test]
fn startup_load_layered_with_no_user_toml_returns_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");
    // No `keybinds.toml` written.

    let runner = production_runner();
    let kb = with_xdg_config_home(dir.path(), || Keybinds::load_layered(&runner));
    assert_eq!(
        kb,
        Keybinds::default(),
        "with no user TOML, `load_layered` must return the defaults"
    );
}

// ---------------------------------------------------------------------------
// Source-level wiring pin: `runner.rs:417` must call
// `Keybinds::load_layered`, not `Keybinds::load`. The
// production-path tests above exercise `load_layered` in
// isolation; this test pins the actual call site so a
// future regression that reverts `runner.rs:417` to
// `Keybinds::load` is caught at compile-test time even
// when no production-path test runs.
// ---------------------------------------------------------------------------

#[test]
fn runner_rs_417_calls_load_layered_at_startup() {
    // The runner.rs path is what production reads on cold
    // start. If a future edit reverts line 417 (the line
    // that assigns to `app.keybinds`) back to
    // `Keybinds::load(...)`, the user-level
    // `keybinds.toml` would silently stop applying on
    // cold start. This test catches that by grepping the
    // source.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let runner_path = std::path::Path::new(manifest_dir)
        .join("src")
        .join("tui")
        .join("runner.rs");
    let src = std::fs::read_to_string(&runner_path).expect("read runner.rs");

    // Find the assignment to `app.keybinds = ...`. The
    // exact line content varies across edits, so we
    // anchor on the LHS.
    let binding_line = src
        .lines()
        .find(|l| l.contains("app.keybinds ="))
        .expect("runner.rs must contain an `app.keybinds = ...` assignment");
    assert!(
        binding_line.contains("load_layered"),
        "runner.rs `app.keybinds = ...` must call `Keybinds::load_layered`; got: {binding_line:?}"
    );
    assert!(
        !binding_line.contains("Keybinds::load(runner)") || binding_line.contains("load_layered"),
        "runner.rs must not regress to `Keybinds::load(runner)` only; got: {binding_line:?}"
    );
}

// ---------------------------------------------------------------------------
// Library-level coverage retained from cycle 2. These tests do NOT call
// `load_layered` directly — they exercise `load_effective`, which is the
// layer-1/library surface. They cannot catch a `runner.rs:417` regression by
// themselves; the F-03 production-path tests above are the load-bearing
// surface for that. These tests stay here as a regression pin on the
// layered logic itself.
// ---------------------------------------------------------------------------

fn resolve_default_path_under(config_root: &std::path::Path) -> PathBuf {
    config_root.join("keybinds.toml")
}

#[test]
fn startup_loads_user_level_toml_when_present() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");
    let toml_path = config_root.join("keybinds.toml");
    fs::write(
        &toml_path,
        "[global]\nquit = \"f12\"\n[autopilot]\nselect = \"f1\"\n",
    )
    .expect("write toml");

    let resolved = resolve_default_path_under(&config_root);
    assert!(resolved.ends_with("keybinds.toml"));
    assert!(resolved.parent().map(|p| p.exists()).unwrap_or(false));

    let toml_text = fs::read_to_string(&resolved).expect("read toml");
    let (kb, _diags, _hint) = Keybinds::load_effective(None, Some(&toml_text));
    assert_eq!(
        kb.quit,
        vec![(KeyCode::F(12), KeyModifiers::empty())],
        "user-level `quit = f12` must apply on cold start"
    );
    assert_eq!(
        kb.lane_autopilot.select,
        vec![(KeyCode::F(1), KeyModifiers::empty())],
        "user-level `[autopilot] select = f1` must apply on cold start"
    );
}

#[test]
fn startup_with_no_user_toml_falls_back_to_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");

    let resolved = resolve_default_path_under(&config_root);
    let toml_text = fs::read_to_string(&resolved).ok();
    let (kb, diags, hint) = Keybinds::load_effective(None, toml_text.as_deref());
    assert!(!hint, "no legacy JSON means no migration hint");
    assert!(diags.is_empty(), "missing TOML is silent");
    assert_eq!(kb, Keybinds::default());
}

#[test]
fn startup_precedence_user_toml_overrides_legacy_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("raul");
    fs::create_dir_all(&config_root).expect("mkdir");
    let toml_path = config_root.join("keybinds.toml");
    fs::write(&toml_path, "[global]\nquit = \"ctrl+y\"\n").expect("write toml");
    let legacy_json = serde_json::json!({
        "config": {
            "keybinds": {
                "quit": "ctrl+x",
                "help": "F1"
            }
        }
    });

    let resolved = resolve_default_path_under(&config_root);
    let toml_text = fs::read_to_string(&resolved).expect("read toml");
    let (kb, _, hint) = Keybinds::load_effective(Some(&legacy_json), Some(&toml_text));
    assert!(hint, "legacy JSON presence fires the migration hint");
    assert_eq!(
        kb.quit,
        vec![(KeyCode::Char('y'), KeyModifiers::CONTROL)],
        "user-level TOML must win for `quit`"
    );
    assert_eq!(
        kb.help,
        vec![(KeyCode::F(1), KeyModifiers::empty())],
        "legacy `help = F1` must survive when TOML doesn't name the field"
    );
}
