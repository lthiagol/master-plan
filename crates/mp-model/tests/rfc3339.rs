use mp_model::parse_rfc3339;

fn epoch(value: &str) -> i64 {
    parse_rfc3339(value).expect("valid RFC3339").unix_seconds()
}

#[test]
fn rfc3339_accepts_leap_day_fraction_z_and_offsets() {
    assert!(parse_rfc3339("2024-02-29T23:59:59Z").is_ok());
    assert!(parse_rfc3339("2026-07-19T12:34:56.123456Z").is_ok());
    assert!(parse_rfc3339("2026-07-19T12:34:56+05:30").is_ok());
    assert!(parse_rfc3339("2026-07-19T12:34:56-03:30").is_ok());
}

#[test]
fn rfc3339_rejects_invalid_calendar_time_and_offset_values() {
    for invalid in [
        "2023-02-29T00:00:00Z",
        "2026-00-01T00:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-01-00T00:00:00Z",
        "2026-01-01T24:00:00Z",
        "2026-01-01T00:60:00Z",
        "2026-01-01T00:00:60Z",
        "2026-01-01T00:00:00+24:00",
        "2026-01-01T00:00:00+00:60",
    ] {
        assert!(
            parse_rfc3339(invalid).is_err(),
            "{invalid} must be rejected"
        );
    }
}

#[test]
fn rfc3339_requires_strict_t_and_complete_suffix() {
    for invalid in [
        "2026-01-01 00:00:00Z",
        "2026-01-01t00:00:00Z",
        "2026-01-01T00:00:00",
        "2026-01-01T00:00:00.",
        "2026-01-01T00:00:00+0000",
        "2026-01-01T00:00:00z",
    ] {
        assert!(
            parse_rfc3339(invalid).is_err(),
            "{invalid} must be rejected"
        );
    }
}

#[test]
fn rfc3339_applies_offsets_across_utc_day_boundaries() {
    assert_eq!(
        epoch("2026-07-10T05:21:41+05:30"),
        epoch("2026-07-09T23:51:41Z")
    );
    assert_eq!(
        epoch("2026-07-10T22:45:00-03:30"),
        epoch("2026-07-11T02:15:00Z")
    );
}
