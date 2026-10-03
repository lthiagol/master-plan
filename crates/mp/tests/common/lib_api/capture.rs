//! Stdout/stderr capture for the in-process CLI runner.
//!
//! `capture_stdio` redirects fds 1/2 to temp files for the duration of
//! `f`, then restores them. It is used by [`crate::common::lib_api::io`]
//! so that tests can compare stdout/stderr against subprocess-shaped
//! `Output` values without polluting the test process's terminal.

/// Redirect stdout+stderr to temp files for the duration of `f`, then restore.
/// Fds are always restored even if `f` panics (`catch_unwind` + Drop guard).
pub fn capture_stdio<F: FnOnce() -> i32 + std::panic::UnwindSafe>(f: F) -> (Vec<u8>, Vec<u8>, i32) {
    #[cfg(unix)]
    {
        use std::io::Read;
        use std::os::fd::AsRawFd;

        let out_path = std::env::temp_dir().join(format!(
            "mp-lib-api-stdout-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let err_path = out_path.with_extension("err.log");

        let out_file = std::fs::File::create(&out_path).expect("stdout capture file");
        let err_file = std::fs::File::create(&err_path).expect("stderr capture file");
        let out_fd = out_file.as_raw_fd();
        let err_fd = err_file.as_raw_fd();

        // SAFETY: single-threaded critical section under RUN_LOCK.
        // F-07: FdRestore is installed *before* any dup2, with live
        // flags so a failed second dup2 still restores fd 1.
        struct FdRestore {
            saved_out: i32,
            saved_err: i32,
            out_redirected: bool,
            err_redirected: bool,
        }
        impl Drop for FdRestore {
            fn drop(&mut self) {
                unsafe {
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                    let _ = std::io::Write::flush(&mut std::io::stderr());
                    if self.out_redirected && self.saved_out >= 0 {
                        let _ = libc::dup2(self.saved_out, 1);
                    }
                    if self.err_redirected && self.saved_err >= 0 {
                        let _ = libc::dup2(self.saved_err, 2);
                    }
                    if self.saved_out >= 0 {
                        libc::close(self.saved_out);
                    }
                    if self.saved_err >= 0 {
                        libc::close(self.saved_err);
                    }
                }
            }
        }
        // Clean capture temps even if `f` panics.
        struct CaptureFiles {
            out: std::path::PathBuf,
            err: std::path::PathBuf,
        }
        impl Drop for CaptureFiles {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.out);
                let _ = std::fs::remove_file(&self.err);
            }
        }
        let _capture_files = CaptureFiles {
            out: out_path.clone(),
            err: err_path.clone(),
        };

        let code = unsafe {
            let saved_out = libc::dup(1);
            let saved_err = libc::dup(2);
            let mut restore = FdRestore {
                saved_out,
                saved_err,
                out_redirected: false,
                err_redirected: false,
            };
            assert!(saved_out >= 0 && saved_err >= 0, "dup stdout/stderr");
            assert_eq!(libc::dup2(out_fd, 1), 1);
            restore.out_redirected = true;
            assert_eq!(libc::dup2(err_fd, 2), 2);
            restore.err_redirected = true;
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = std::io::Write::flush(&mut std::io::stderr());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            match result {
                Ok(code) => {
                    drop(restore);
                    code
                }
                Err(payload) => {
                    drop(restore);
                    std::panic::resume_unwind(payload);
                }
            }
        };
        drop(out_file);
        drop(err_file);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let _ = std::fs::File::open(&out_path).and_then(|mut f| f.read_to_end(&mut stdout));
        let _ = std::fs::File::open(&err_path).and_then(|mut f| f.read_to_end(&mut stderr));
        (stdout, stderr, code)
    }
    #[cfg(not(unix))]
    {
        let code = f();
        (Vec::new(), Vec::new(), code)
    }
}
