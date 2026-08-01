//! M144: small humanize helpers for the TUI. Currently exposes
//! `humanize_relative` which converts an RFC3339 timestamp into a
//! coarse relative-time string ("3d ago", "2w ago") for the Milestones
//! lane's "since" column. We deliberately keep this minimal — the
//! full timestamp is still available via `mp show <id>`, and locale
//! formatting is out of scope.

/// Convert an RFC3339 timestamp into a short relative-time string.
///
/// Buckets:
///   * < 60s    -> "just now"
///   * < 60m    -> "{n}m ago"
///   * < 24h    -> "{n}h ago"
///   * < 14d    -> "{n}d ago"
///   * < 8w     -> "{n}w ago"
///   * else     -> "{YYYY-MM-DD}" (full date as the fallback)
///
/// Returns `"unknown"` when the input is malformed (not RFC3339) — the
/// TUI caller can render that directly without an extra match arm.
pub fn humanize_relative(rfc3339: &str) -> String {
    let Ok(dt) = mp_model::parse_rfc3339(rfc3339) else {
        return "unknown".to_string();
    };
    let dt = dt.unix_seconds();
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => return "unknown".to_string(),
    };
    let delta = now - dt;
    if delta < 0 {
        // Future-dated timestamp — likely a clock skew, but render the
        // absolute date so the user sees something rather than "now".
        return format_date(dt);
    }
    if delta < 60 {
        return "just now".to_string();
    }
    if delta < 60 * 60 {
        return format!("{}m ago", delta / 60);
    }
    if delta < 60 * 60 * 24 {
        return format!("{}h ago", delta / 3600);
    }
    if delta < 60 * 60 * 24 * 14 {
        return format!("{}d ago", delta / 86_400);
    }
    if delta < 60 * 60 * 24 * 7 * 8 {
        return format!("{}w ago", delta / (86_400 * 7));
    }
    format_date(dt)
}

#[cfg(test)]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    // Howard Hinnant's days_from_civil (public domain). Computes the
    // number of days since 1970-01-01 (Unix epoch) for the given
    // proleptic-Gregorian date.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32; // [0, 399]
    let m = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe as i64 - 719468
}

fn format_date(epoch_secs: i64) -> String {
    // Convert epoch_secs back to YYYY-MM-DD. Inverse of days_from_civil.
    let days = epoch_secs.div_euclid(86_400);
    let z = days + 719468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_returns_unknown_for_garbage() {
        assert_eq!(humanize_relative("not a date"), "unknown");
        assert_eq!(humanize_relative(""), "unknown");
    }

    #[test]
    fn humanize_formats_recent_timestamp_as_just_now() {
        // 30 seconds ago — well inside the < 60s bucket.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let ts = format_iso(now - 30);
        let s = humanize_relative(&ts);
        assert_eq!(
            s, "just now",
            "a 30s-old timestamp must render as 'just now' (got {s:?})"
        );
    }

    #[test]
    fn humanize_formats_minute_old_timestamp_with_minutes_suffix() {
        // 5 minutes ago — should land in the < 1h bucket as "5m ago".
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let ts = format_iso(now - 5 * 60);
        let s = humanize_relative(&ts);
        assert_eq!(
            s, "5m ago",
            "a 5m-old timestamp must render as '5m ago' (got {s:?})"
        );
    }

    #[test]
    fn humanize_formats_old_timestamp_as_date() {
        // 2020-01-01.
        let days = days_from_civil(2020, 1, 1);
        let ts = format_iso(days * 86_400);
        let s = humanize_relative(&ts);
        assert_eq!(s, "2020-01-01");
    }

    #[test]
    fn parse_and_format_roundtrip() {
        let ts = "2026-07-04T12:34:56Z";
        let secs = mp_model::parse_rfc3339(ts).expect("parse").unix_seconds();
        let formatted = format_iso(secs);
        assert_eq!(formatted, ts);
    }

    #[test]
    fn parse_handles_chrono_utc_now_shape() {
        // `chrono::Utc::now().to_rfc3339()` produces
        // `YYYY-MM-DDTHH:MM:SS.fff+00:00` — fractional seconds + offset.
        let ts = "2026-07-10T05:21:41.937696+00:00";
        let secs = mp_model::parse_rfc3339(ts)
            .expect("parse chrono shape")
            .unix_seconds();
        let formatted = format_iso(secs);
        assert_eq!(formatted, "2026-07-10T05:21:41Z");
    }

    #[test]
    fn humanize_parser_applies_positive_offset_across_day_boundary() {
        let ts = "2026-07-10T05:21:41+05:30";
        let secs = mp_model::parse_rfc3339(ts)
            .expect("parse with offset")
            .unix_seconds();
        let formatted = format_iso(secs);
        assert_eq!(formatted, "2026-07-09T23:51:41Z");
    }

    #[test]
    fn humanize_parser_applies_negative_offset_across_day_boundary() {
        let ts = "2026-07-10T22:45:00-03:30";
        let secs = mp_model::parse_rfc3339(ts)
            .expect("parse with offset")
            .unix_seconds();
        assert_eq!(format_iso(secs), "2026-07-11T02:15:00Z");
    }

    fn format_iso(secs: i64) -> String {
        let days = secs.div_euclid(86_400);
        let z = days + 719468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        let sod = secs.rem_euclid(86_400);
        let hh = sod / 3600;
        let mm = (sod / 60) % 60;
        let ss = sod % 60;
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hh, mm, ss)
    }
}
