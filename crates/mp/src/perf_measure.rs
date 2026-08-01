//! M176: structured wall-clock measurement for quantitative performance ACs.
//!
//! The AC verifier is exit-code-only; it cannot parse numeric thresholds.
//! Spec authors use `mp_measure!` (or the free function [`measure`]) to run a
//! shell command N times via `/usr/bin/time -p`, collect real seconds, and
//! assert a threshold with `#[track_caller]` so failures point at the call site.
//!
//! Verification fields on plan ACs should either:
//! - wrap the claim in `mp_measure!(…)` (and usually `manual:` for historical
//!   milestones whose numbers already live in mp-dogfood-log.md), or
//! - use an explicit `manual:` prefix with a documented re-measure schedule.
//!
//! See `docs/concepts/03 - Planning Methodology/PERF-ACS.md`.

use std::process::Command;
use std::time::Instant;

/// One multi-run measurement record.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureRecord {
    pub label: String,
    pub mean_s: f64,
    pub stddev_s: f64,
    pub runs: Vec<f64>,
}

impl MeasureRecord {
    /// Assert mean seconds is at least `min_s`. Panics at the call site on failure.
    #[track_caller]
    pub fn assert_mean_at_least(&self, min_s: f64) {
        assert!(
            self.mean_s + f64::EPSILON >= min_s,
            "mp_measure![{}] mean {:.3}s < required {:.3}s (runs={:?})",
            self.label,
            self.mean_s,
            min_s,
            self.runs
        );
    }

    /// Assert mean seconds is at most `max_s`. Panics at the call site on failure.
    #[track_caller]
    pub fn assert_mean_at_most(&self, max_s: f64) {
        assert!(
            self.mean_s <= max_s + f64::EPSILON,
            "mp_measure![{}] mean {:.3}s > allowed {:.3}s (runs={:?})",
            self.label,
            self.mean_s,
            max_s,
            self.runs
        );
    }

    /// Assert mean is at least `pct` percent lower than `baseline_mean_s`.
    #[track_caller]
    pub fn assert_drop_at_least_pct(&self, baseline_mean_s: f64, pct: f64) {
        assert!(
            baseline_mean_s > 0.0,
            "mp_measure![{}] baseline mean must be > 0",
            self.label
        );
        let drop_pct = (baseline_mean_s - self.mean_s) / baseline_mean_s * 100.0;
        assert!(
            drop_pct + f64::EPSILON >= pct,
            "mp_measure![{}] drop {:.2}% < required {:.2}% (baseline={:.3}s mean={:.3}s runs={:?})",
            self.label,
            drop_pct,
            pct,
            baseline_mean_s,
            self.mean_s,
            self.runs
        );
    }
}

/// Run `command` through `sh -c` `runs` times and return mean / stddev of
/// wall-clock seconds. Prefers `/usr/bin/time -p` real-time when available;
/// falls back to process wall-clock via [`Instant`].
pub fn measure(label: &str, command: &str, runs: usize) -> std::io::Result<MeasureRecord> {
    assert!(runs > 0, "mp_measure! requires runs >= 1");
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        samples.push(time_once(command)?);
    }
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    // Population stddev (/N), not sample stddev (/(N-1)).
    let var = samples
        .iter()
        .map(|s| {
            let d = s - mean;
            d * d
        })
        .sum::<f64>()
        / samples.len() as f64;
    Ok(MeasureRecord {
        label: label.to_string(),
        mean_s: mean,
        stddev_s: var.sqrt(),
        runs: samples,
    })
}

fn time_once(command: &str) -> std::io::Result<f64> {
    // Prefer /usr/bin/time -p for portable real-seconds parsing.
    // Trusted commands only — `sh -c` is intentional for measure helpers.
    if std::path::Path::new("/usr/bin/time").exists() {
        let output = Command::new("/usr/bin/time")
            .args(["-p", "sh", "-c", command])
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "command failed (exit {:?}): {command}",
                output.status.code()
            )));
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return parse_time_p_real(&stderr).ok_or_else(|| {
            std::io::Error::other(format!(
                "/usr/bin/time -p ran but real-seconds could not be parsed from stderr: {stderr:?}"
            ))
        });
    }
    // Fallback when /usr/bin/time is absent: Instant wall-clock around sh -c.
    let start = Instant::now();
    let status = Command::new("sh").args(["-c", command]).status()?;
    let elapsed = start.elapsed().as_secs_f64();
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "command failed (exit {:?}): {command}",
            status.code()
        )));
    }
    Ok(elapsed)
}

/// Parse `real N.NN` from `/usr/bin/time -p` stderr.
pub fn parse_time_p_real(stderr: &str) -> Option<f64> {
    for line in stderr.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("real ") {
            if let Ok(v) = rest.trim().parse::<f64>() {
                return Some(v);
            }
        }
        // Some platforms emit `real\tN.NN`.
        if let Some(rest) = line.strip_prefix("real\t") {
            if let Ok(v) = rest.trim().parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

/// Measure a shell command `runs` times (default 3) and return a [`MeasureRecord`].
///
/// ```ignore
/// let rec = mp_measure!("echo_ok", "echo ok");
/// rec.assert_mean_at_most(1.0);
/// ```
///
/// With explicit run count:
/// ```ignore
/// let rec = mp_measure!("echo_ok", "echo ok", runs = 5);
/// ```
#[macro_export]
macro_rules! mp_measure {
    ($label:expr, $command:expr) => {
        $crate::perf_measure::measure($label, $command, 3).expect("mp_measure! command failed")
    };
    ($label:expr, $command:expr, runs = $runs:expr) => {
        $crate::perf_measure::measure($label, $command, $runs).expect("mp_measure! command failed")
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_p_real_reads_real_line() {
        let sample = "real 1.25\nuser 0.10\nsys 0.05\n";
        assert_eq!(parse_time_p_real(sample), Some(1.25));
    }

    #[test]
    fn parse_time_p_real_reads_tab_separated() {
        assert_eq!(parse_time_p_real("real\t2.50\n"), Some(2.50));
    }

    #[test]
    fn parse_time_p_real_returns_none_when_missing() {
        assert_eq!(parse_time_p_real("user 0.1\nsys 0.0\n"), None);
    }

    #[test]
    fn measure_echo_is_fast() {
        let rec = measure("echo_ok", "echo ok", 2).expect("measure");
        assert_eq!(rec.runs.len(), 2);
        assert!(rec.mean_s < 2.0, "echo should be sub-second-ish: {rec:?}");
        rec.assert_mean_at_most(5.0);
    }

    #[test]
    #[should_panic(expected = "mean")]
    fn assert_mean_at_least_panics_when_below() {
        let rec = MeasureRecord {
            label: "tiny".into(),
            mean_s: 0.1,
            stddev_s: 0.0,
            runs: vec![0.1],
        };
        rec.assert_mean_at_least(10.0);
    }

    #[test]
    fn assert_drop_pct_passes() {
        let rec = MeasureRecord {
            label: "warm".into(),
            mean_s: 80.0,
            stddev_s: 0.0,
            runs: vec![80.0],
        };
        rec.assert_drop_at_least_pct(100.0, 15.0);
    }
}
