use std::fmt;

/// A validated RFC3339 timestamp, normalized to Unix epoch seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rfc3339Timestamp {
    unix_seconds: i64,
}

impl Rfc3339Timestamp {
    pub fn unix_seconds(self) -> i64 {
        self.unix_seconds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rfc3339Error;

impl fmt::Display for Rfc3339Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid RFC3339 timestamp")
    }
}

impl std::error::Error for Rfc3339Error {}

/// Parse a strict RFC3339 timestamp and convert it to Unix epoch seconds.
///
/// The parser requires the `T` date/time separator, validates Gregorian
/// calendar dates and time/offset ranges, accepts fractional seconds, and
/// applies numeric offsets when computing the epoch value.
pub fn parse_rfc3339(value: &str) -> Result<Rfc3339Timestamp, Rfc3339Error> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return Err(Rfc3339Error);
    }

    let year = parse_digits(bytes, 0, 4)? as i64;
    let month = parse_digits(bytes, 5, 2)?;
    let day = parse_digits(bytes, 8, 2)?;
    let hour = parse_digits(bytes, 11, 2)? as i64;
    let minute = parse_digits(bytes, 14, 2)? as i64;
    let second = parse_digits(bytes, 17, 2)? as i64;

    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(Rfc3339Error);
    }

    let mut cursor = 19;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == fraction_start {
            return Err(Rfc3339Error);
        }
    }

    let offset_seconds = match bytes.get(cursor) {
        Some(b'Z') if cursor + 1 == bytes.len() => 0,
        Some(sign @ (b'+' | b'-')) if cursor + 6 == bytes.len() => {
            if bytes.get(cursor + 3) != Some(&b':') {
                return Err(Rfc3339Error);
            }
            let offset_hour = parse_digits(bytes, cursor + 1, 2)? as i64;
            let offset_minute = parse_digits(bytes, cursor + 4, 2)? as i64;
            if offset_hour > 23 || offset_minute > 59 {
                return Err(Rfc3339Error);
            }
            let magnitude = offset_hour * 3600 + offset_minute * 60;
            if *sign == b'+' {
                magnitude
            } else {
                -magnitude
            }
        }
        _ => return Err(Rfc3339Error),
    };

    let local_seconds = days_from_civil(year, month, day)
        .checked_mul(86_400)
        .and_then(|base| base.checked_add(hour * 3600 + minute * 60 + second))
        .ok_or(Rfc3339Error)?;
    let unix_seconds = local_seconds
        .checked_sub(offset_seconds)
        .ok_or(Rfc3339Error)?;
    Ok(Rfc3339Timestamp { unix_seconds })
}

fn parse_digits(bytes: &[u8], start: usize, length: usize) -> Result<u32, Rfc3339Error> {
    let slice = bytes.get(start..start + length).ok_or(Rfc3339Error)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return Err(Rfc3339Error);
    }
    Ok(slice
        .iter()
        .fold(0_u32, |value, digit| value * 10 + u32::from(digit - b'0')))
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = (year - era * 400) as u32;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + i64::from(day_of_era) - 719_468
}
