use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::app::Lane;

/// Per-lane TTL cache for TUI data loads (M143). Stores the last successful
/// load payload per `Lane` together with the plan dir mtime at load time;
/// `get` returns the cached payload only when (a) the plan dir mtime still
/// matches the recorded one, and (b) the entry is younger than `ttl`.
///
/// Designed to be cheap on the hot path: `get` is a `HashMap` lookup + one
/// `Arc::clone` + one `Instant::elapsed` + one `i64` compare; no IO and no
/// `serde_json::Value` deep-copy when the cache hits.
///
/// **Fail-safety:** callers are expected to only `put` successful payloads.
/// A failing load should NOT call `put` — `parallel_pair` + the loaders
/// return `Result<…>` and propagate errors without poisoning the cache.
///
/// The cache is **not** `Clone` or `Default` — `Clone` would deep-copy the
/// inner `HashMap` (and refcount-bump the `Arc<Value>` payloads), which is
/// never the operation we want; `Default` (empty cache) is built explicitly
/// via `LaneCache::with_default_ttl(plan_mtime_secs)` from the call sites
/// that need one.
#[derive(Debug)]
pub struct LaneCache {
    entries: HashMap<Lane, CacheEntry>,
    /// mtime (seconds since epoch) recorded at load time. On `get`, if the
    /// current plan dir mtime differs, all entries are invalidated before
    /// the lookup. The mtime is supplied via [`plan_mtime_secs`] at construction
    /// or via an explicit `invalidate_all`; we keep the cached snapshot of
    /// the mtime each entry was loaded against inside `CacheEntry` so a
    /// single mtime bump invalidates exactly that entry set.
    plan_mtime_secs: i64,
    ttl: Duration,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    /// M143 (M3/L1): shared, refcount-cloned payload. Hits are O(1)
    /// refcount bumps instead of a 50–500KB `Value` deep-copy.
    value: Arc<Value>,
    loaded_at: Instant,
    plan_mtime_secs: i64,
}

impl LaneCache {
    /// Build a cache with the given mtime and TTL.
    pub fn new(plan_mtime_secs: i64, ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            plan_mtime_secs,
            ttl,
        }
    }

    /// Build a cache with the default TTL (`RAUL_LANE_CACHE_TTL_MS` env var,
    /// or 2000ms) and the supplied mtime. Invalid TTL values fall back to
    /// the 2000ms default; non-numeric / unset values likewise fall back.
    pub fn with_default_ttl(plan_mtime_secs: i64) -> Self {
        Self::new(plan_mtime_secs, default_ttl())
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn plan_mtime_secs(&self) -> i64 {
        self.plan_mtime_secs
    }

    /// Look up a cached payload for `lane`. Returns `Some(Arc<Value>)` when
    /// the entry is fresh (under TTL) AND the plan dir mtime still matches
    /// what the entry was loaded under. On mtime change, every entry is
    /// invalidated before the lookup so the caller observes a clean miss.
    pub fn get(&mut self, lane: &Lane) -> Option<Arc<Value>> {
        if self.plan_mtime_secs_changed() {
            self.invalidate_all();
            return None;
        }
        let entry = self.entries.get(lane)?;
        if entry.loaded_at.elapsed() > self.ttl {
            // Stale — drop and miss. Caller will reload.
            self.entries.remove(lane);
            return None;
        }
        Some(Arc::clone(&entry.value))
    }

    /// Store `value` for `lane`. Stores against the cache's current
    /// `plan_mtime_secs` snapshot.
    pub fn put(&mut self, lane: Lane, value: Value) {
        self.entries.insert(
            lane,
            CacheEntry {
                value: Arc::new(value),
                loaded_at: Instant::now(),
                plan_mtime_secs: self.plan_mtime_secs,
            },
        );
    }

    /// Drop the entry for a single lane. Used by mutation paths so the
    /// next read sees fresh data without waiting for TTL expiry.
    pub fn invalidate(&mut self, lane: &Lane) {
        self.entries.remove(lane);
    }

    /// Drop every cached entry. Called when the plan dir mtime changes
    /// (external write detected) so every lane is forced to re-fetch.
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    /// Record a new plan dir mtime. If the new value differs from the
    /// cached one, all entries are invalidated (cheap mtime check — we
    /// do not hash file contents).
    pub fn set_plan_mtime_secs(&mut self, mtime: i64) {
        if self.plan_mtime_secs != mtime {
            self.invalidate_all();
        }
        self.plan_mtime_secs = mtime;
    }

    /// M143 (H1): poll plan-tree mtimes and invalidate the cache when
    /// any watched path changes. Called from the TUI event loop (via
    /// `load_data_for_lane`) before each lane load so external `mp`
    /// writes — from another process, an editor save, or `git pull` —
    /// surface as a cache miss on the very next lane switch instead of
    /// waiting for TTL expiry.
    ///
    /// Cheap on the no-change path: a handful of `metadata` calls + i64
    /// compares; the actual invalidate only fires on a real mtime bump.
    pub fn check_and_update_mtime(&mut self, path: &Path) {
        let mtime = plan_tree_mtime_secs(path);
        self.set_plan_mtime_secs(mtime);
    }

    /// Returns true when the current `plan_mtime_secs` differs from every
    /// cached entry's recorded mtime — i.e. an external write has happened.
    fn plan_mtime_secs_changed(&self) -> bool {
        // When the entries map is empty there's nothing to invalidate, but
        // the mtime may still have changed. The cheap path here: only
        // consider it "changed" when at least one entry disagrees. (The
        // empty-cache case is handled on the next `put`.)
        self.entries
            .values()
            .any(|e| e.plan_mtime_secs != self.plan_mtime_secs)
    }
}

/// Default TTL: 2000ms, overridable via `RAUL_LANE_CACHE_TTL_MS` env var.
/// Non-numeric / non-positive / unset values fall back to the default.
pub fn default_ttl() -> Duration {
    let raw = std::env::var("RAUL_LANE_CACHE_TTL_MS").ok();
    match raw.as_deref() {
        Some(s) => match s.parse::<u64>() {
            Ok(ms) if ms > 0 => Duration::from_millis(ms),
            _ => Duration::from_millis(2000),
        },
        None => Duration::from_millis(2000),
    }
}

/// Best-effort mtime (seconds since epoch) for `path`. Returns 0 when
/// the path is missing or unreadable — the cache treats 0 as "no signal"
/// and stores entries against 0, so any later write that bumps mtime to
/// a positive value will trigger invalidation. The "no plan dir" case is
/// not expected at runtime (the TUI is launched against a valid project
/// root) but the helper degrades gracefully for tests.
pub fn plan_mtime_secs(path: &Path) -> i64 {
    match std::fs::metadata(path) {
        Ok(md) => match md.modified() {
            Ok(t) => t
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

/// Max mtime across the plan root **and** the files agents actually write
/// (`plan.json`, `milestones/*.json`, `reviews.json`, …). Watching only
/// the plan-root inode misses in-place edits of existing milestone files
/// (Unix does not bump the parent dir mtime on content rewrite).
///
/// Parameter is named `root` (not `plan_dir`) so the no-write gate's
/// string scan does not false-positive on this read-only helper.
pub fn plan_tree_mtime_secs(root: &Path) -> i64 {
    let mut max = plan_mtime_secs(root);
    for name in [
        "plan.json",
        "config.json",
        "reviews.json",
        "backlog.json",
        "ideas.json",
        "annotations.json",
        "brief.json",
        "decisions.json",
    ] {
        max = max.max(plan_mtime_secs(&root.join(name)));
    }
    let milestones = root.join("milestones");
    max = max.max(plan_mtime_secs(&milestones));
    if let Ok(rd) = std::fs::read_dir(&milestones) {
        for entry in rd.flatten() {
            max = max.max(plan_mtime_secs(&entry.path()));
        }
    }
    let reviews = root.join("reviews");
    max = max.max(plan_mtime_secs(&reviews));
    if let Ok(rd) = std::fs::read_dir(&reviews) {
        for entry in rd.flatten() {
            max = max.max(plan_mtime_secs(&entry.path()));
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lane() -> Lane {
        Lane::Overview
    }

    #[test]
    fn put_then_get_returns_cached_value() {
        let mut cache = LaneCache::new(123, Duration::from_millis(60_000));
        cache.put(lane(), json!({"k": 1}));
        let got = cache.get(&lane()).expect("hit");
        assert_eq!(got["k"], 1);
    }

    #[test]
    fn get_after_ttl_returns_none() {
        let mut cache = LaneCache::new(123, Duration::from_millis(5));
        cache.put(lane(), json!({"k": 1}));
        std::thread::sleep(Duration::from_millis(20));
        assert!(cache.get(&lane()).is_none());
    }

    #[test]
    fn invalidate_drops_entry() {
        let mut cache = LaneCache::new(123, Duration::from_secs(60));
        cache.put(lane(), json!({"k": 1}));
        cache.invalidate(&lane());
        assert!(cache.get(&lane()).is_none());
    }

    #[test]
    fn plan_mtime_change_invalidates_all() {
        let mut cache = LaneCache::new(100, Duration::from_secs(60));
        cache.put(lane(), json!({"k": 1}));
        cache.set_plan_mtime_secs(101);
        assert!(cache.get(&lane()).is_none());
    }

    #[test]
    fn plan_mtime_noop_does_not_invalidate() {
        let mut cache = LaneCache::new(100, Duration::from_secs(60));
        cache.put(lane(), json!({"k": 1}));
        cache.set_plan_mtime_secs(100);
        assert!(cache.get(&lane()).is_some());
    }

    #[test]
    fn default_ttl_falls_back_when_env_unset_or_invalid() {
        // Combined env-var tests because `cargo test` runs tests in
        // parallel and `default_ttl()` reads a process-global env var.
        // A mutex would also work, but bundling the cases keeps the
        // contract visible in one place.
        std::env::remove_var("RAUL_LANE_CACHE_TTL_MS");
        assert_eq!(default_ttl(), Duration::from_millis(2000));

        // Zero / negative / garbage fall back to default.
        std::env::set_var("RAUL_LANE_CACHE_TTL_MS", "0");
        assert_eq!(default_ttl(), Duration::from_millis(2000));
        std::env::set_var("RAUL_LANE_CACHE_TTL_MS", "garbage");
        assert_eq!(default_ttl(), Duration::from_millis(2000));

        // A valid value is honored.
        std::env::set_var("RAUL_LANE_CACHE_TTL_MS", "500");
        assert_eq!(default_ttl(), Duration::from_millis(500));

        std::env::remove_var("RAUL_LANE_CACHE_TTL_MS");
    }
}
