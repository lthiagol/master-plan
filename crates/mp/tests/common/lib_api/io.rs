//! In-process CLI runner — drop-in for `TestEnv::run` on plan operations.
//!
//! **Background (M175):** tests can call the same argv through
//! `mp::app::run` in-process instead of spawning a subprocess; saves
//! ~50 ms per call and avoids the cargo rebuild / PATH races that
//! `TestEnv::run`'s `run_with_retry` has to absorb.
//!
//! **When to use this vs [`crate::common::TestEnv::run`]**
//! - Use `lib_api::run` / `lib_api::run_json` for plan operations
//!   (milestone, step, wp, review, validate, etc.).
//! - Use `env.run` for install/uninstall/doctor/watch/TUI/init/smoke,
//!   where the subprocess boundary matters for the test's claim.

use std::path::Path;
use std::sync::Mutex;

use super::capture::capture_stdio;
use crate::common::TestEnv;

/// Serializes cwd / fd redirection across parallel tests in the same process.
pub static RUN_LOCK: Mutex<()> = Mutex::new(());

/// In-process equivalent of [`crate::common::TestEnv::run_json`].
///
/// **Subprocess equivalent:** `env.run_json(args)`.
#[allow(clippy::needless_borrow)] // callers pass both `env` and `&env`
pub fn run_json(env: &TestEnv, args: &[&str]) -> serde_json::Value {
    let out = run(env, args);
    assert!(
        out.status.success(),
        "mp {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json")
}

/// In-process equivalent of [`crate::common::TestEnv::run_validate`].
///
/// **Subprocess equivalent:** `env.run_validate()`.
pub fn run_validate(env: &TestEnv) -> bool {
    run(env, &["validate", "--format", "json"]).status.success()
}

/// Harness skill-dir keys mirrored from [`crate::common::isolated_harness_env`].
const HARNESS_SKILL_ENV: &[(&str, &str)] = &[
    ("opencode", "harness/opencode/skills"),
    ("cursor", "harness/cursor/skills"),
    ("claude-code", "harness/claude-code/skills"),
    ("gemini", "harness/gemini/skills"),
    ("codex", "harness/codex/skills"),
    ("windsurf", "harness/windsurf/skills"),
    ("cline", "harness/cline/skills"),
    ("pi", "harness/pi/agent/skills"),
];

/// Restores cwd + a set of env vars on drop (shared by `run` / `run_at_repo`).
struct EnvRestore {
    cwd: std::path::PathBuf,
    vars: Vec<(String, Option<std::ffi::OsString>)>,
}
impl Drop for EnvRestore {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.cwd);
        for (key, prev) in self.vars.drain(..) {
            match prev {
                Some(v) => std::env::set_var(&key, v),
                None => std::env::remove_var(&key),
            }
        }
    }
}

fn harness_skill_env_key(id: &str) -> String {
    format!("MP_{}_SKILL_DIR", id.to_uppercase().replace('-', "_"))
}

/// Apply isolated harness skill dirs under `root` (F-09: parity with
/// subprocess `isolated_harness_env`). Returns prior values for restore.
fn apply_isolated_harness_env(root: &Path) -> Vec<(String, Option<std::ffi::OsString>)> {
    let mut prev = Vec::with_capacity(HARNESS_SKILL_ENV.len());
    for (id, sub) in HARNESS_SKILL_ENV {
        let dir = root.join(sub);
        let _ = std::fs::create_dir_all(&dir);
        let key = harness_skill_env_key(id);
        prev.push((key.clone(), std::env::var_os(&key)));
        std::env::set_var(&key, &dir);
    }
    prev
}

/// In-process equivalent of [`crate::common::TestEnv::run_at_repo`].
///
/// Cwd stays at the workspace root; plan lives under the test tmp dir.
/// Mirrors subprocess isolation: `MP_INSTALL_DIR` + per-harness
/// `MP_*_SKILL_DIR` under the test tmp tree (F-09 / M158 AC-10).
///
/// **Subprocess equivalent:** `env.run_at_repo(args)`.
pub fn run_at_repo(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let _guard = RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let root = crate::common::repo_root();
    let plan_dir = env.tmp.path().join("master-plan");
    let install_dir = env.tmp.path().join("install-target");
    let _ = std::fs::create_dir_all(&install_dir);

    let prior_path = std::env::var_os("PATH");
    let new_path = crate::common::path_with_install_bin(&install_dir);

    let prev_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let mut vars = vec![
        ("MP_HOME".to_string(), std::env::var_os("MP_HOME")),
        (
            "MP_INSTALL_DIR".to_string(),
            std::env::var_os("MP_INSTALL_DIR"),
        ),
        ("PATH".to_string(), prior_path),
        (
            "MP_VERIFY_TRUST_REPOSITORY".to_string(),
            std::env::var_os("MP_VERIFY_TRUST_REPOSITORY"),
        ),
        (
            "MP_VERIFY_ALLOW_SHELL".to_string(),
            std::env::var_os("MP_VERIFY_ALLOW_SHELL"),
        ),
    ];
    vars.extend(apply_isolated_harness_env(env.tmp.path()));
    let _env_restore = EnvRestore {
        cwd: prev_cwd,
        vars,
    };

    std::env::set_current_dir(&root).expect("set_current_dir to repo root");
    std::env::set_var("MP_HOME", &root);
    std::env::set_var("MP_INSTALL_DIR", &install_dir);
    std::env::set_var("PATH", &new_path);
    // Same trust/shell opt-in as `run`: bare-repo cwd is the real workspace
    // root, which is not auto-trusted for verification commands.
    std::env::set_var("MP_VERIFY_TRUST_REPOSITORY", "1");
    std::env::set_var("MP_VERIFY_ALLOW_SHELL", "1");

    let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(args.len() + 5);
    argv.push(std::ffi::OsString::from("mp"));
    argv.push(std::ffi::OsString::from("--project-root"));
    argv.push(root.as_os_str().to_os_string());
    argv.push(std::ffi::OsString::from("--plan-dir"));
    argv.push(plan_dir.as_os_str().to_os_string());
    for a in args {
        argv.push(std::ffi::OsString::from(*a));
    }

    let (stdout, stderr, code) = capture_stdio(|| dispatch_cli(argv));
    make_output(code, stdout, stderr)
}

/// In-process equivalent of [`crate::common::TestEnv::run`].
///
/// Parses `args` with the real clap `Cli`, runs `mp::app::run` against the
/// test env's temp project root, and returns a `std::process::Output`-shaped
/// value (status / stdout / stderr) so existing suite assertions keep working.
///
/// **Subprocess equivalent:** `env.run(args)`.
///
/// Prefer domain wrappers (`validate`, `milestone_create`, …) when a test only
/// needs one domain call. Use this when the test exercises multi-flag CLI
/// surface, bulk dispatch, or other paths not yet wrapped.
#[allow(clippy::needless_borrow)] // callers pass both `env` and `&env`
pub fn run(env: &TestEnv, args: &[&str]) -> std::process::Output {
    let _guard = RUN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let project_root = env.tmp.path().to_path_buf();
    let install_dir = project_root.join("install-target");
    let _ = std::fs::create_dir_all(&install_dir);

    let prev_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let _env_restore = EnvRestore {
        cwd: prev_cwd,
        vars: vec![
            ("MP_HOME".to_string(), std::env::var_os("MP_HOME")),
            (
                "MP_INSTALL_DIR".to_string(),
                std::env::var_os("MP_INSTALL_DIR"),
            ),
            (
                "MP_VERIFY_TRUST_REPOSITORY".to_string(),
                std::env::var_os("MP_VERIFY_TRUST_REPOSITORY"),
            ),
            (
                "MP_VERIFY_ALLOW_SHELL".to_string(),
                std::env::var_os("MP_VERIFY_ALLOW_SHELL"),
            ),
        ],
    };

    std::env::set_current_dir(&project_root).expect("set_current_dir to test tmp");
    std::env::set_var("MP_HOME", crate::common::repo_root());
    std::env::set_var("MP_INSTALL_DIR", &install_dir);
    std::env::set_var("MP_VERIFY_TRUST_REPOSITORY", "1");
    std::env::set_var("MP_VERIFY_ALLOW_SHELL", "1");

    let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(args.len() + 3);
    argv.push(std::ffi::OsString::from("mp"));
    argv.push(std::ffi::OsString::from("--project-root"));
    argv.push(project_root.as_os_str().to_os_string());
    for a in args {
        argv.push(std::ffi::OsString::from(*a));
    }

    let (stdout, stderr, code) = capture_stdio(|| dispatch_cli(argv));
    // EnvRestore Drop restores cwd / MP_HOME / MP_INSTALL_DIR.
    make_output(code, stdout, stderr)
}

fn dispatch_cli(argv: Vec<std::ffi::OsString>) -> i32 {
    use clap::Parser;
    let cli = match mp::cli::Cli::try_parse_from(&argv) {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let _ = e.print();
                    return 0;
                }
                _ => {
                    let _ = e.print();
                    return 2;
                }
            }
        }
    };
    match mp::app::run(cli) {
        Ok(()) => 0,
        Err(e) => {
            if let Some(code) = e.downcast_ref::<mp::ExitCode>() {
                return code.0;
            }
            // Mirror main.rs: real errors go to stderr as `Error: …`.
            eprintln!("Error: {e}");
            1
        }
    }
}

fn make_output(code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> std::process::Output {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // Wait-status encoding: exit code in high byte.
        let status = std::process::ExitStatus::from_raw(code << 8);
        std::process::Output {
            status,
            stdout,
            stderr,
        }
    }
    #[cfg(not(unix))]
    {
        // Fallback: spawn real binary (Windows not a primary target).
        let _ = (code, stdout, stderr);
        panic!("lib_api::run is unix-only in this tree");
    }
}
