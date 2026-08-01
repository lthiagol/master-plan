//! M124 (M106 ER-5) AC-02: `pipe_drain_thread` caps its captured buffer
//! at a documented ceiling (1 MiB) and emits a single
//! `<output truncated at N bytes>` sentinel when the cap is hit. Pre-fix
//! the buffer grew unbounded — a child emitting several MB (e.g.
//! `cargo test --workspace`) pushed the verifier past the 2 GiB CI runner
//! memory limit on GitHub Actions.
//!
//! Pins the contract end-to-end through the public `pipe_drain_thread`
//! entry point: feed a synthetic `Read` that emits `> DRAIN_BUF_CAP_BYTES`
//! bytes, drain, then assert the captured buffer is bounded by the cap
//! plus the sentinel and that the sentinel is present exactly once.

use mp::ac_verify::{pipe_drain_thread, DRAIN_BUF_CAP_BYTES};
use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex};

#[test]
fn pipe_drain_thread_caps_buffer_at_documented_ceiling() {
    // Emit 2x the cap so the buffer MUST hit the cap mid-stream and
    // drop the trailing half.
    let payload: Vec<u8> = vec![b'X'; DRAIN_BUF_CAP_BYTES * 2];
    let buf = Arc::new(Mutex::new(Vec::new()));
    let handle = pipe_drain_thread(Cursor::new(payload), Arc::clone(&buf));
    handle.join().expect("drain thread join");

    let captured = buf.lock().expect("drain buf poisoned").clone();
    let cap_plus_sentinel_max =
        DRAIN_BUF_CAP_BYTES + b"<output truncated at 1048576 bytes>".len() + 32;
    assert!(
        captured.len() <= cap_plus_sentinel_max,
        "captured.len() = {} must be bounded by cap + sentinel max = {}",
        captured.len(),
        cap_plus_sentinel_max
    );
    // Bytes that fit before the cap should still be there.
    assert!(
        captured.len() >= DRAIN_BUF_CAP_BYTES,
        "captured.len() = {} should retain at least the cap ({}) before truncation",
        captured.len(),
        DRAIN_BUF_CAP_BYTES
    );
}

#[test]
fn pipe_drain_thread_emits_truncation_sentinel_exactly_once() {
    let payload: Vec<u8> = vec![b'Y'; DRAIN_BUF_CAP_BYTES + (256 * 1024)];
    let buf = Arc::new(Mutex::new(Vec::new()));
    let handle = pipe_drain_thread(Cursor::new(payload), Arc::clone(&buf));
    handle.join().expect("drain thread join");

    let captured = buf.lock().expect("drain buf poisoned").clone();
    let as_str = String::from_utf8_lossy(&captured);
    assert!(
        as_str.contains("<output truncated at"),
        "captured output must contain the truncation sentinel; got: {as_str:?}"
    );
    let sentinel_count = as_str.matches("<output truncated at").count();
    assert_eq!(
        sentinel_count, 1,
        "truncation sentinel must appear exactly once (idempotent); got {sentinel_count} occurrences"
    );
    assert!(
        as_str.contains(&format!("{DRAIN_BUF_CAP_BYTES} bytes")),
        "sentinel must report the documented cap value ({DRAIN_BUF_CAP_BYTES}); got: {as_str:?}"
    );
}

#[test]
fn pipe_drain_thread_does_not_cap_below_ceiling() {
    // Sanity: an emitter well below the cap must not be truncated at
    // all (no sentinel, full payload captured).
    let payload: Vec<u8> = vec![b'Z'; 64 * 1024]; // 64 KiB << 1 MiB
    let buf = Arc::new(Mutex::new(Vec::new()));
    let handle = pipe_drain_thread(Cursor::new(payload.clone()), Arc::clone(&buf));
    handle.join().expect("drain thread join");

    let captured = buf.lock().expect("drain buf poisoned").clone();
    assert_eq!(
        captured, payload,
        "sub-cap payload must round-trip unchanged"
    );
    let as_str = String::from_utf8_lossy(&captured);
    assert!(
        !as_str.contains("<output truncated"),
        "sub-cap payload must NOT carry a truncation sentinel; got: {as_str:?}"
    );
}

#[test]
fn pipe_drain_thread_keeps_reading_pipe_after_cap() {
    // Even after the cap is hit, the drain thread must keep reading from
    // the pipe so the child doesn't block on a full kernel pipe buffer.
    // We confirm this by emitting >cap bytes in small chunks via a custom
    // `Read` impl and asserting the drain thread completes (joins) on
    // its own rather than hanging or panicking. The pre-fix
    // `extend_from_slice` loop would also drain in this case, but the
    // buffer would have ballooned past the cap; the post-fix cap path
    // must complete without buffering the trailing bytes.
    struct ChunkedReader {
        remaining: usize,
        chunk_size: usize,
    }
    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Ok(0);
            }
            let n = self.remaining.min(self.chunk_size).min(buf.len());
            for slot in &mut buf[..n] {
                *slot = b'C';
            }
            self.remaining -= n;
            Ok(n)
        }
    }

    let reader = ChunkedReader {
        remaining: DRAIN_BUF_CAP_BYTES * 3,
        chunk_size: 8 * 1024,
    };
    let buf = Arc::new(Mutex::new(Vec::new()));
    let handle = pipe_drain_thread(reader, Arc::clone(&buf));
    handle.join().expect("drain thread must complete after cap");

    let captured = buf.lock().expect("drain buf poisoned").clone();
    assert!(
        captured.len() <= DRAIN_BUF_CAP_BYTES + b"<output truncated at 1048576 bytes>".len() + 32,
        "captured buffer must remain bounded even when the pipe emits >>cap bytes; got {} bytes",
        captured.len()
    );
    assert!(
        String::from_utf8_lossy(&captured).contains("<output truncated at"),
        "sentinel must be present after cap hit; got tail: {:?}",
        &captured[captured.len().saturating_sub(64)..]
    );
}
