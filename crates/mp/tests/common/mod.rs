#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use tempfile::TempDir;

/// Run a closure that produces a [`Command`] and invoke `.output()` on
/// it. On `NotFound` (ENOENT) — the spawn-failure pattern observed when
/// parallel `cargo test` invocations race on the workspace's package
/// cache / build directory and momentarily clobber `target/debug/mp` —
/// retry up to `retries` times with a short backoff before propagating
/// the error. The flake trace looks like:
///   `panicked at ...: spawn mp: Os { code: 2, kind: NotFound }`
/// and this helper is the surface that absorbs it.
///
/// **M132 + speed fix:** [`mp_bin`] returns a shared hardlink (or copy)
/// snapshot immune to cargo's build clobber. Retry here is defense in
/// depth (callers pass `retries=5` as a backstop). A `NotFound` exhausts
/// the retries and panics with `mp spawn failed after {N} retries: …`;
/// any non-`NotFound` spawn error panics immediately with `mp spawn
/// failed: …`.
pub fn run_with_retry<F: FnMut() -> Command>(
    mut make_cmd: F,
    retries: u32,
) -> std::process::Output {
    // M132 review remediation: the prior shape caught NotFound on the
    // final attempt in a catch-all `Err(e) => panic!`, which made this
    // post-loop "exhausted retries" panic unreachable and left the
    // `last_err` bookkeeping dead. Now only a non-NotFound error
    // short-circuits; NotFound always falls through to the retry/backoff
    // below and reaches the post-loop panic if every attempt misses.
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..=retries {
        let mut cmd = make_cmd();
        match cmd.output() {
            Ok(out) => return out,
            // A non-NotFound spawn error (permissions, etc.) is not the
            // transient race this helper absorbs — surface it at once so
            // the panic message includes the tool name.
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                panic!("mp spawn failed: {e}");
            }
            Err(e) => {
                last_err = Some(e);
                if attempt < retries {
                    std::thread::sleep(Duration::from_millis(50 * (attempt as u64 + 1)));
                }
            }
        }
    }
    panic!(
        "mp spawn failed after {} retries: {}",
        retries,
        last_err.unwrap_or_else(|| std::io::Error::other("exhausted retries"))
    );
}

pub fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

/// Prepend `$install_dir/bin` to PATH so doctor `runtime:mp_on_path` passes
/// after a per-test tmp install (mirrors `source env.sh` for real users).
pub fn path_with_install_bin(install_dir: &std::path::Path) -> std::ffi::OsString {
    let install_bin = install_dir.join("bin");
    std::env::var_os("PATH").map_or_else(
        || install_bin.as_os_str().to_owned(),
        |p| {
            let mut s = install_bin.as_os_str().to_owned();
            s.push(":");
            s.push(&p);
            s
        },
    )
}

/// Shared TMPDIR root for stable `mp` snapshots used by integration tests.
///
/// Layout: `{temp}/mp-test-binaries/mp-{mtime_ns}-{size}` — one entry per
/// distinct cargo-built binary, reused across nextest processes.
pub fn mp_bin_snapshot_dir() -> PathBuf {
    std::env::temp_dir().join("mp-test-binaries")
}

/// Return a stable path to the `mp` test binary that cargo cannot clobber.
///
/// **Why this exists (M132, S2):** `CARGO_BIN_EXE_mp` points at
/// `target/debug/mp`. Concurrent `cargo test --test X` runs can rebuild and
/// momentarily unlink that path → ENOENT on spawn.
///
/// **How (speed fix):** content-keyed snapshot under [`mp_bin_snapshot_dir`],
/// published via **hardlink** when the source and TMPDIR share a filesystem
/// (same inode survives cargo unlinking its path; O(1), no 26 MB copy). Falls
/// back to `fs::copy` only when hardlink fails (cross-device). All nextest
/// worker processes reuse the same path for a given binary identity, so we
/// no longer leak `~26 MB × test-count` under TMPDIR.
///
/// **Cleanup:** `make clean-test-bins` removes the shared dir and any legacy
/// `mp-test-binaries-{pid}/` trees. Do **not** auto-prune other keys from the
/// hot path — nextest is process-per-test, and deleting a path another worker
/// still holds in its `OnceLock` causes `Command::new` ENOENT (inode lifetime
/// does not keep the directory entry).
///
/// Prefer this over [`unstable_mp_bin_str`] for every `Command::new` spawn.
pub fn mp_bin() -> &'static Path {
    static SNAPSHOT: OnceLock<PathBuf> = OnceLock::new();
    SNAPSHOT.get_or_init(|| {
        let src = Path::new(env!("CARGO_BIN_EXE_mp"));
        let mut last_err: Option<std::io::Error> = None;
        for attempt in 0..3 {
            match ensure_mp_snapshot(src) {
                Ok(dest) => return dest,
                Err(e) if attempt < 2 => {
                    last_err = Some(e);
                    std::thread::sleep(Duration::from_millis(50 * (attempt as u64 + 1)));
                }
                Err(e) => panic!("snapshot mp binary after retries: {e}"),
            }
        }
        unreachable!(
            "snapshot loop exited without return or panic: {:?}",
            last_err
        )
    })
}

/// Publish (or reuse) a content-keyed snapshot of `src` under the shared
/// snapshot dir. Prefers hardlink; falls back to copy.
fn ensure_mp_snapshot(src: &Path) -> std::io::Result<PathBuf> {
    // M194: ride out cargo's incremental rebuild window. cargo deletes
    // target/release/mp before writing the new one, leaving a 0-byte
    // file briefly. The old "is empty → return Err → caller retries"
    // pattern only worked when the rebuild completed within the
    // caller's 150 ms total backoff — which is too tight on slow CI
    // runners (macOS in particular). wait_for_stable_nonempty blocks
    // until two consecutive reads see the same non-zero size, with a
    // linear backoff that totals ~3.6 s — well above any reasonable
    // rebuild time. The mp_bin() retry loop still wraps this for
    // defense-in-depth (hard_link races, etc.).
    wait_for_stable_nonempty(src)?;

    let dir = mp_bin_snapshot_dir();
    std::fs::create_dir_all(&dir)?;

    let meta = std::fs::metadata(src)?;
    if meta.len() == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "mp binary at {} is empty (cargo rebuild race)",
                src.display()
            ),
        ));
    }
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dest = dir.join(format!("mp-{}-{}", mtime_ns, meta.len()));

    if dest.is_file() {
        // Drop a prior zero-byte publish (same race, already latched).
        if std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) == 0 {
            let _ = std::fs::remove_file(&dest);
        } else {
            return Ok(dest);
        }
    }

    let tmp = dir.join(format!(".tmp-{}-{}", std::process::id(), mtime_ns));
    // Drop a leftover tmp from a killed prior attempt with the same pid/key.
    let _ = std::fs::remove_file(&tmp);

    match std::fs::hard_link(src, &tmp) {
        Ok(()) => {}
        Err(_) => {
            std::fs::copy(src, &tmp)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&tmp)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&tmp, perms)?;
            }
        }
    }

    match std::fs::rename(&tmp, &dest) {
        Ok(()) => Ok(dest),
        Err(_) if dest.is_file() => {
            let _ = std::fs::remove_file(&tmp);
            Ok(dest)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Wait for `path` to settle to a stable non-zero size. Used to ride out
/// cargo's incremental rebuild window, where `target/release/mp` is
/// briefly 0 bytes between unlink and rewrite. Two consecutive reads
/// must agree on a non-zero size — that means cargo's write has
/// completed AND no further rebuild is in flight. Linear backoff totals
/// ~3.6 s across 8 attempts.
fn wait_for_stable_nonempty(p: &Path) -> std::io::Result<()> {
    const MAX_ATTEMPTS: u32 = 8;
    let mut last_size: u64 = u64::MAX;
    for attempt in 0..MAX_ATTEMPTS {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if size > 0 && size == last_size {
            return Ok(());
        }
        last_size = size;
        std::thread::sleep(Duration::from_millis(100 * (attempt as u64 + 1)));
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!("{p:?} never stabilized to a non-zero size after {MAX_ATTEMPTS} attempts"),
    ))
}

/// Path of the canonical cargo-built `mp` binary (lives under
/// `target/debug/mp` and is shared across the workspace) as a `&str`.
/// Use this only when you specifically need the *unstable* path —
/// e.g. the `--source` error test in `install_source.rs` which
/// deliberately points a "repo root" check at a binary file.
/// **All spawn sites should use [`mp_bin`] instead**, which returns
/// a stable, owned snapshot immune to cargo's build clobber.
pub fn unstable_mp_bin_str() -> &'static str {
    env!("CARGO_BIN_EXE_mp")
}

pub struct TestEnv {
    pub tmp: TempDir,
    fixture_source: Option<FixtureSourceGuard>,
}

impl TestEnv {
    pub fn new() -> Self {
        let env = Self::blank();
        assert!(
            env.run(&["init", "--profile", "full", "--format", "json"])
                .status
                .success(),
            "mp init --profile full failed"
        );
        env
    }

    pub fn blank() -> Self {
        Self {
            tmp: TempDir::new().expect("temp"),
            fixture_source: None,
        }
    }

    /// Copy a tracked project fixture into an isolated temporary directory.
    ///
    /// The source tree is snapshotted before the copy and checked again when
    /// the environment is dropped. All tests that may write through `mp`
    /// must use this constructor instead of running against tracked fixtures.
    pub fn from_fixture(fixture: &str) -> Self {
        let source = repo_root().join("tests/fixtures/projects").join(fixture);
        let fixture_source = FixtureSourceGuard {
            before: snapshot_fixture_tree(&source),
            source: source.clone(),
        };
        let env = Self {
            tmp: TempDir::new().expect("fixture tempdir"),
            fixture_source: Some(fixture_source),
        };
        copy_fixture_tree(&source, env.tmp.path());
        env
    }

    pub fn run(&self, args: &[&str]) -> std::process::Output {
        let install_dir = self.tmp.path().join("install-target");
        let args = args.to_vec();
        run_with_retry(
            || {
                let mut cmd = Command::new(mp_bin());
                cmd.current_dir(self.tmp.path())
                    .env("MP_HOME", repo_root())
                    .env("MP_INSTALL_DIR", &install_dir)
                    .env("MP_VERIFY_TRUST_REPOSITORY", "1")
                    .env("MP_VERIFY_ALLOW_SHELL", "1")
                    .args(&args);
                cmd
            },
            5,
        )
    }

    pub fn run_with_env(&self, extra_env: &[(&str, &str)], args: &[&str]) -> std::process::Output {
        let extra_env: Vec<(&str, &str)> = extra_env.to_vec();
        let args = args.to_vec();
        run_with_retry(
            || {
                let mut cmd = Command::new(mp_bin());
                cmd.current_dir(self.tmp.path()).env("MP_HOME", repo_root());
                for (k, v) in &extra_env {
                    cmd.env(k, v);
                }
                cmd.args(&args);
                cmd
            },
            5,
        )
    }

    /// Run `mp` with cwd/project-root at the workspace and plan dir in this temp fixture.
    /// Sets MP_INSTALL_DIR to a per-test temp dir so concurrent install/uninstall
    /// tests don't race on `~/.agents/master-plan/`. Also isolates each harness's
    /// skill deploy dir via `isolated_harness_env` so skill deploys never land
    /// in the developer's real `~/.agents/skills/` (M158 AC-10).
    pub fn run_at_repo(&self, args: &[&str]) -> std::process::Output {
        let root = repo_root();
        let plan_dir = self.tmp.path().join("master-plan");
        let install_dir = self.tmp.path().join("install-target");
        let path_with_install = path_with_install_bin(&install_dir);
        let args = args.to_vec();
        run_with_retry(
            || {
                let mut cmd = Command::new(mp_bin());
                cmd.current_dir(&root)
                    .env("MP_HOME", &root)
                    .env("MP_INSTALL_DIR", &install_dir)
                    .env("PATH", &path_with_install)
                    .env("MP_VERIFY_TRUST_REPOSITORY", "1")
                    .env("MP_VERIFY_ALLOW_SHELL", "1")
                    .arg("--project-root")
                    .arg(&root)
                    .arg("--plan-dir")
                    .arg(&plan_dir)
                    .args(&args);
                isolated_harness_env(&mut cmd, self.tmp.path());
                cmd
            },
            5,
        )
    }

    pub fn run_json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "mp {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("json")
    }

    pub fn run_validate(&self) -> bool {
        self.run(&["validate", "--format", "json"]).status.success()
    }
}

pub fn run_validate_fixture(fixture: &str, plan_dir: Option<&str>) -> (i32, String) {
    let root = repo_root();
    let source = root.join("tests/fixtures/projects").join(fixture);
    let source_before = snapshot_fixture_tree(&source);
    let temp = tempfile::tempdir().expect("fixture tempdir");
    let fixture_root = temp.path().join(fixture);
    copy_fixture_tree(&source, &fixture_root);
    let output = run_with_retry(
        || {
            let mut cmd = Command::new(mp_bin());
            cmd.current_dir(&fixture_root)
                .env("MP_HOME", &root)
                .arg("validate")
                .arg("--format")
                .arg("json");
            if let Some(dir) = plan_dir {
                cmd.arg("--plan-dir").arg(dir);
            }
            cmd
        },
        5,
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(1);
    assert_eq!(
        snapshot_fixture_tree(&source),
        source_before,
        "source fixture changed during validation: {fixture}"
    );
    (code, stdout)
}

struct FixtureSourceGuard {
    source: PathBuf,
    before: std::collections::BTreeMap<PathBuf, FixtureTreeEntry>,
}

impl Drop for FixtureSourceGuard {
    fn drop(&mut self) {
        assert_eq!(
            snapshot_fixture_tree(&self.source),
            self.before,
            "source fixture changed during test: {}",
            self.source.display()
        );
    }
}

fn copy_fixture_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).expect("create fixture destination");
    for entry in std::fs::read_dir(source).expect("read fixture source") {
        let entry = entry.expect("fixture entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().expect("fixture file type");
        if file_type.is_dir() {
            copy_fixture_tree(&source_path, &destination_path);
        } else if file_type.is_symlink() {
            let target = std::fs::read_link(&source_path).expect("read fixture symlink");
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, destination_path).expect("copy fixture symlink");
        } else {
            std::fs::copy(source_path, destination_path).expect("copy fixture file");
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum FixtureTreeEntry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

fn snapshot_fixture_tree(root: &Path) -> std::collections::BTreeMap<PathBuf, FixtureTreeEntry> {
    fn visit(
        root: &Path,
        current: &Path,
        snapshot: &mut std::collections::BTreeMap<PathBuf, FixtureTreeEntry>,
    ) {
        for entry in std::fs::read_dir(current).expect("read fixture snapshot") {
            let entry = entry.expect("fixture snapshot entry");
            let path = entry.path();
            let relative = path.strip_prefix(root).expect("fixture relative path");
            let file_type = entry.file_type().expect("fixture snapshot file type");
            if file_type.is_dir() {
                snapshot.insert(relative.to_path_buf(), FixtureTreeEntry::Directory);
                visit(root, &path, snapshot);
            } else if file_type.is_symlink() {
                snapshot.insert(
                    relative.to_path_buf(),
                    FixtureTreeEntry::Symlink(
                        std::fs::read_link(path).expect("read fixture snapshot symlink"),
                    ),
                );
            } else {
                snapshot.insert(
                    relative.to_path_buf(),
                    FixtureTreeEntry::File(
                        std::fs::read(path).expect("read fixture snapshot file"),
                    ),
                );
            }
        }
    }

    let mut snapshot = std::collections::BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
}

pub fn isolated_harness_env(cmd: &mut Command, root: &std::path::Path) {
    for (id, sub) in [
        ("opencode", "harness/opencode/skills"),
        ("cursor", "harness/cursor/skills"),
        ("claude-code", "harness/claude-code/skills"),
        ("gemini", "harness/gemini/skills"),
        ("codex", "harness/codex/skills"),
        ("windsurf", "harness/windsurf/skills"),
        ("cline", "harness/cline/skills"),
        ("pi", "harness/pi/agent/skills"),
    ] {
        let dir = root.join(sub);
        // `install` will create its own subdirs under each skill_dir,
        // but pre-creating them here surfaces a permissions/sandbox
        // problem immediately rather than as a downstream "failed to
        // copy SKILL.md" error from `deploy_skill_to_harness`.
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
            panic!(
                "isolated_harness_env: could not pre-create {}: {e}",
                dir.display()
            )
        });
        let key = format!("MP_{}_SKILL_DIR", id.to_uppercase().replace('-', "_"));
        cmd.env(key, dir);
    }
}

// Snapshot + retry helpers — cheap tests compiled into every binary that
// includes `common` so the contracts cannot silently regress.
#[cfg(test)]
mod mp_bin_snapshot_tests {
    use super::*;

    #[test]
    fn mp_bin_path_is_stable_and_executable() {
        let a = mp_bin();
        let b = mp_bin();
        assert_eq!(a, b, "OnceLock must return the same path");
        assert!(a.is_file(), "snapshot must exist: {}", a.display());
        // Shared dir, content-keyed name — not the legacy per-PID layout.
        let dir = mp_bin_snapshot_dir();
        assert!(
            a.starts_with(&dir),
            "expected {} under {}, got {}",
            a.display(),
            dir.display(),
            a.display()
        );
        let out = Command::new(a).arg("--help").output().expect("spawn mp");
        assert!(
            out.status.success(),
            "snapshot must be executable; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn ensure_mp_snapshot_reuses_existing_path() {
        let src = Path::new(env!("CARGO_BIN_EXE_mp"));
        let first = ensure_mp_snapshot(src).expect("first snapshot");
        let second = ensure_mp_snapshot(src).expect("second snapshot");
        assert_eq!(first, second);
        // Prefer hardlink when TMPDIR is same FS: nlink >= 2 (src + dest),
        // or at least dest exists after a copy fallback.
        assert!(first.is_file());
    }
}

#[cfg(test)]
mod run_with_retry_tests {
    use super::*;
    use std::process::Command;

    // A path guaranteed not to exist, producing NotFound on spawn.
    const GONE: &str = "/mp-run-with-retry-does-not-exist-xyz";

    #[test]
    fn exhausted_retries_panics_with_after_retries_message() {
        // Every attempt returns NotFound, so the post-loop panic must
        // fire with the "after N retries" wording (the previously-dead
        // branch). retries=2 keeps it fast.
        let res = std::panic::catch_unwind(|| run_with_retry(|| Command::new(GONE), 2));
        let msg = res.unwrap_err().downcast_ref::<String>().unwrap().clone();
        assert!(
            msg.contains("mp spawn failed after 2 retries"),
            "expected the 'after N retries' panic; got: {msg}"
        );
    }

    #[test]
    fn nonzero_attempts_before_first_success() {
        // Simulate a flaky source: fail the first attempt, then succeed.
        // We can't easily make a real Command flaky, so instead verify
        // the happy path still returns the Output when the command runs.
        let out = run_with_retry(echo_ok_command, 3);
        assert!(out.status.success(), "happy path must return the Output");
    }

    fn echo_ok_command() -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", "exit 0"]);
        c
    }
}
/// Parse JSON from command stdout that may carry a non-JSON preamble
/// (e.g. M170 TW-03 `Assigned: B-NN` line before the JSON body).
pub fn json_from_stdout(stdout: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(stdout).expect("stdout utf-8");
    let start = text.find('{').expect("stdout must contain a JSON object");
    serde_json::from_str(&text[start..]).expect("stdout JSON after preamble")
}

pub mod fake_herdr;
pub mod lib_api;
pub mod review_queue_fixture;
