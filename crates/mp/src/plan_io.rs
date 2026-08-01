//! M113 S1: advisory file lock around the plan_io read-modify-write critical
//! section. Two concurrent `mp` invocations on the same plan no longer
//! clobber each other; the second caller either blocks (default) or fails
//! with a bounded-timeout error after MP_LOCK_TIMEOUT_SECS.
//!
//! Implementation note: `flock(2)` via `libc` on Unix. Windows is a
//! best-effort no-op (the dogfood log / M110 portability scope is macOS
//! and Linux; Windows is not in the supported matrix today). It is
//! process-scoped, not thread-scoped: a single `mp` process can still
//! load+save serially through multiple threads without self-deadlock
//! because each lock-unlock cycle releases the previous one. Concurrent
//! CLI processes are the failure mode this addresses.
//!
//! Reference: dogfood log entry 2026-07-04
//! `Parallel mp milestone wp|step invocations race and drop writes`.

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::Duration;

use anyhow::{bail, Result};

/// Default bounded-wait for a contended lock. The dogfood log shows
/// contention windows are typically < 100ms (a single mp CLI round trip).
/// 15s gives 150× headroom for slow CI / sandboxed environments while
/// keeping an interactive session from hanging too long.
pub const DEFAULT_LOCK_TIMEOUT_SECS: u64 = 15;

/// Command-owned token for authoritative plan mutations.
///
/// Acquiring the token serializes load-modify-write work and performs recovery
/// for any multi-file transaction interrupted by process termination. Helpers
/// below this layer must not acquire the plan lock again.
pub struct PlanWriteTxn {
    plan_dir: PathBuf,
    #[cfg(unix)]
    _guard: PlanWriteLock,
}

impl PlanWriteTxn {
    pub fn acquire(plan_dir: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            let guard = acquire_bounded(&plan_dir.join(".mp-write.lock"))?;
            crate::mutation_txn::recover_pending(plan_dir)?;
            Ok(Self {
                plan_dir: plan_dir.to_path_buf(),
                _guard: guard,
            })
        }
        #[cfg(not(unix))]
        {
            warn_no_lock_once();
            crate::mutation_txn::recover_pending(plan_dir)?;
            Ok(Self {
                plan_dir: plan_dir.to_path_buf(),
            })
        }
    }

    /// Acquire the stable project-root lock used while a plan is initialized or
    /// relocated. This lock does not move with the plan directory.
    pub fn acquire_project_root(project_root: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            let guard = acquire_bounded(&project_root.join(".mp-project-write.lock"))?;
            Ok(Self {
                plan_dir: project_root.to_path_buf(),
                _guard: guard,
            })
        }
        #[cfg(not(unix))]
        {
            warn_no_lock_once();
            Ok(Self {
                plan_dir: project_root.to_path_buf(),
            })
        }
    }

    pub fn run<R>(&self, op: impl FnOnce(&Self) -> Result<R>) -> Result<R> {
        op(self)
    }

    /// Execute a multi-file operation with a durable before-image manifest.
    ///
    /// A normal error restores immediately. If the process terminates, the next
    /// `PlanWriteTxn::acquire` restores before loading mutable state — unless a
    /// durable `COMMITTED` marker was sealed, in which case the after-image is kept.
    pub fn run_recoverable<R>(&self, op: impl FnOnce(&Self) -> Result<R>) -> Result<R> {
        let recovery = crate::mutation_txn::RecoveryTxn::begin(&self.plan_dir)?;
        match op(self) {
            Ok(value) => {
                recovery.commit()?;
                Ok(value)
            }
            Err(error) => {
                // Crash failpoint armed mid-mutation: leave the txn pending for
                // recover_pending (do not Drop-restore), then abort.
                if crate::store::mutation_crash_armed() {
                    std::mem::forget(recovery);
                    std::process::abort();
                }
                // ExitCode is an intentional process exit after the command has
                // already emitted its result (e.g. bulk partial failure exit 2).
                // Those writes must stay committed; only unexpected errors roll back.
                if error.downcast_ref::<crate::ExitCode>().is_some() {
                    recovery.commit()?;
                } else {
                    recovery.rollback()?;
                }
                Err(error)
            }
        }
    }

    pub fn plan_dir(&self) -> &Path {
        &self.plan_dir
    }

    /// Append activity without re-acquiring the plan lock.
    ///
    /// Call only after the authoritative resource write succeeds. Journal
    /// failure is warning-only and never converts a durable plan commit into a
    /// reported rollback.
    pub fn append_activity_best_effort(
        &self,
        ctx: &crate::paths::PlanContext,
        event: crate::activity::ActivityEvent,
    ) -> Result<Option<()>> {
        if self.plan_dir != ctx.plan_dir {
            bail!(
                "activity context {} does not match transaction plan {}",
                ctx.plan_dir.display(),
                self.plan_dir.display()
            );
        }
        crate::activity::append_event_best_effort_unlocked(ctx, event)
    }
}

#[cfg(unix)]
fn acquire_bounded(lock_path: &Path) -> Result<PlanWriteLock> {
    let timeout_secs = std::env::var("MP_LOCK_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_LOCK_TIMEOUT_SECS);
    PlanWriteLock::acquire(lock_path, Duration::from_secs(timeout_secs)).map_err(|e| {
        anyhow::anyhow!(
            "plan file is locked by another mp process: {} \
             (set MP_LOCK_TIMEOUT_SECS to raise; another mp invocation \
             may still be writing — retry after it completes)",
            e
        )
    })
}

/// Hold the plan-writer lock for the duration of the supplied closure.
/// Returns whatever the closure returns. On Unix this opens
/// `<plan_dir>/.mp-write.lock` and `flock`s it for the lifetime of the
/// returned guard. On Windows (unsupported in this build matrix) the
/// guard is a no-op — the CLI will run, just without serialization.
///
/// Bounded wait: if another process holds the lock past
/// `MP_LOCK_TIMEOUT_SECS`, returns an error pointing at the next step.
pub fn with_plan_write_lock<F, R>(plan_dir: &Path, op: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    let txn = PlanWriteTxn::acquire(plan_dir)?;
    txn.run(|_| op())
}

/// Tiny RAII wrapper around an exclusive `flock(2)`. Drop = unlock.
/// Unix-only; the cfg gate suppresses the type on non-Unix builds.
#[cfg(unix)]
pub struct PlanWriteLock {
    file: File,
    path: PathBuf,
}

#[cfg(unix)]
impl PlanWriteLock {
    fn acquire(path: &Path, timeout: Duration) -> Result<Self> {
        // Create the lock file if missing. `OpenOptions` lets us
        // idempotently create + read + write without a TOCTOU window
        // versus mkdir-based locks.
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        // Bounded wait with `LOCK_EX | LOCK_NB` polls. `flock` itself
        // would block indefinitely; the NB + poll variant is what
        // gives us a controllable timeout.
        let start = std::time::Instant::now();
        loop {
            // SAFETY: `libc::flock` is a thin wrapper over the syscall.
            // We pass the file descriptor via `as_raw_fd` (auto-deref via
            // `BorrowedFd`); on success returns 0, on contention returns
            // -1 with errno=EWOULDBLOCK.
            let rc = unsafe { libc::flock(file_as_raw_fd(&file), libc::LOCK_EX | libc::LOCK_NB) };
            if rc == 0 {
                return Ok(Self {
                    file,
                    path: path.to_path_buf(),
                });
            }
            let err = std::io::Error::last_os_error();
            // F-6: contention surfaces as EWOULDBLOCK (BSD) or EAGAIN
            // (Linux). On BSD-derived platforms these are the same
            // numeric value, so check each explicitly (a `|` pattern
            // would be flagged unreachable there). Any other errno is a
            // real failure (bad fd, permission, ...) and must surface.
            let errno = err.raw_os_error();
            let contention = errno == Some(libc::EWOULDBLOCK) || errno == Some(libc::EAGAIN);
            if !contention {
                bail!("flock({}) failed: {}", path.display(), err);
            }
            if start.elapsed() >= timeout {
                bail!(
                    "timed out after {:?} waiting for lock on {}",
                    timeout,
                    path.display()
                );
            }
            // Sleep a short, bounded interval and try again. 50ms is
            // cheap, gives the original holder time to finish its
            // write cycle, and keeps the wall-clock cap tight.
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Acquire without timeout (block forever). Production callers should
    /// use `acquire` with a bounded wait so a stuck holder surfaces as an
    /// error rather than a hang. Public ONLY for integration tests
    /// (`crates/mp/tests/plan_io_concurrent_writes.rs`) that need to
    /// deliberately hold the lock open; not part of the runtime API.
    #[doc(hidden)]
    pub fn acquire_blocking(path: &Path) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        // SAFETY: see `acquire` — LOCK_EX blocking is what makes this
        // a determinism helper for tests.
        let rc = unsafe { libc::flock(file_as_raw_fd(&file), libc::LOCK_EX) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            bail!("flock({}) failed: {}", path.display(), err);
        }
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for PlanWriteLock {
    fn drop(&mut self) {
        // SAFETY: releasing the flock we acquired. On error (extremely
        // rare; the process is exiting anyway) the kernel reaps it on
        // fd close. Drop never panics in practice.
        unsafe {
            let _ = libc::flock(file_as_raw_fd(&self.file), libc::LOCK_UN);
        }
    }
}

#[cfg(unix)]
fn file_as_raw_fd(file: &File) -> std::os::unix::io::RawFd {
    use std::os::unix::io::AsRawFd;
    file.as_raw_fd()
}

/// F-4: emit the "no advisory lock on this platform" warning at most
/// once per process. Non-Unix builds run writes un-serialized; the
/// warning makes that visible without spamming on every invocation.
#[cfg(not(unix))]
fn warn_no_lock_once() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    if ONCE.set(()).is_ok() {
        eprintln!(
            "warning: mp advisory write-lock is not implemented on this \
             platform; concurrent mp invocations may still race."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_release_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let lock_path = tmp.path().join(".mp-write.lock");
        let guard =
            PlanWriteLock::acquire(&lock_path, Duration::from_secs(1)).expect("first acquire");
        drop(guard);
        let guard2 = PlanWriteLock::acquire(&lock_path, Duration::from_secs(1))
            .expect("second acquire after drop");
        drop(guard2);
    }

    #[test]
    fn acquire_times_out_when_held() {
        let tmp = tempfile::tempdir().unwrap();
        let lock_path = tmp.path().join(".mp-write.lock");
        let holder = PlanWriteLock::acquire_blocking(&lock_path).expect("holder");
        let start = std::time::Instant::now();
        let res = PlanWriteLock::acquire(&lock_path, Duration::from_millis(150));
        let elapsed = start.elapsed();
        assert!(res.is_err(), "second acquire must time out");
        assert!(elapsed >= Duration::from_millis(150), "wait was too short");
        drop(holder);
    }

    #[test]
    fn with_plan_write_lock_runs_closure() {
        let tmp = tempfile::tempdir().unwrap();
        let value =
            with_plan_write_lock(tmp.path(), || Ok::<_, anyhow::Error>(42)).expect("closure runs");
        assert_eq!(value, 42);
    }
}
