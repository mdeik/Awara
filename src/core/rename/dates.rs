use super::*;
use crate::core::config::{
    INSERT_META_FMT_DEFAULT, MetaSource, auto_date_type_source, insert_meta_tag_source,
};

/// Compute the local UTC offset in seconds **at the given instant** (the offset
/// in effect then, including DST), or `0` if unknown.
///
/// This is the display-side counterpart of [`local_datetime_to_utc_secs`]: a
/// timestamp must be rendered with the offset that applied when it occurred.
pub fn local_utc_offset_secs_at(time: SystemTime) -> i64 {
    let secs = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        #[cfg(not(windows))]
        {
            libc::localtime_r(&secs, &mut tm);
            // tm_gmtoff is the UTC offset in seconds (positive = east of UTC).
            // It's available on Linux (glibc), macOS, and BSD.
            tm.tm_gmtoff
        }
        #[cfg(windows)]
        {
            // `_timezone`/`_dstbias` are only assigned by `_tzset()`. Unlike
            // `localtime`, the secure `localtime_s` does not call it, so the
            // first call in a process can read zeroed globals and return 0
            // instead of the real offset.
            libc::tzset();
            if libc::localtime_s(&mut tm, &secs) != 0 {
                return 0;
            }
            // CRT globals: `_timezone` is standard-time seconds *west* of UTC and
            // `_dstbias` is the DST adjustment (negative, e.g. -3600). `tm_isdst`
            // tells us whether DST is in effect at this instant.
            let mut tz_secs: libc::c_long = 0;
            let mut dst_secs: libc::c_long = 0;
            if libc::get_timezone(&mut tz_secs) != 0 || libc::get_dstbias(&mut dst_secs) != 0 {
                return 0;
            }
            if tm.tm_isdst > 0 {
                -(tz_secs as i64 + dst_secs as i64)
            } else {
                -(tz_secs as i64)
            }
        }
    }
}

/// Compute the local UTC offset in seconds for *now*, or `0` if unknown.
pub fn local_utc_offset_secs() -> i64 {
    local_utc_offset_secs_at(SystemTime::now())
}

// The `libc` crate doesn't expose `mktime` on Windows, so bind the CRT's
// explicit 64-bit variant directly. `_mktime64` is independent of the
// process-wide `time_t` width, so it works on every Windows target (including
// x86 GNU, where `time_t` is 32-bit).
#[cfg(windows)]
unsafe extern "C" {
    #[link_name = "_mktime64"]
    fn mktime64(tm: *mut libc::tm) -> i64;
}

/// Convert a local wall-clock date/time to UTC epoch seconds, applying the
/// local offset **in effect at that date** (including DST).
///
/// [`local_utc_offset_secs`] returns the offset for *now*, which is wrong for a
/// fixed timestamp that falls in a different DST period. The C runtime's
/// `mktime` resolves the correct offset for the given wall-clock time
/// (`tm_isdst = -1` lets the system decide). Returns `None` for invalid input or
/// a pre-epoch result.
pub fn local_datetime_to_utc_secs(
    year: i64,
    month: u64,
    day: u64,
    hour: u64,
    minute: u64,
    second: u64,
) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = (year - 1900) as libc::c_int;
    tm.tm_mon = (month as i32 - 1) as libc::c_int;
    tm.tm_mday = day as libc::c_int;
    tm.tm_hour = hour as libc::c_int;
    tm.tm_min = minute as libc::c_int;
    tm.tm_sec = second as libc::c_int;
    tm.tm_isdst = -1;
    #[cfg(not(windows))]
    let t = unsafe { libc::mktime(&mut tm) };
    #[cfg(windows)]
    let t = unsafe { mktime64(&mut tm) };
    if t < 0 { None } else { Some(t as i64) }
}

/// Split a user-entered datetime string into parts.
///
/// Accepts `YYYY-MM-DD[ HH:MM[:SS]][ AM|PM]` with any mix of `-`, space, `:` or
/// `T` as separators. The meridiem is optional and case-insensitive; when
/// present the hour must be a 12-hour value (1–12), with `12 AM` = 00 and
/// `12 PM` = 12. Without a meridiem the hour is treated as 24-hour (0–23).
pub(crate) fn parse_datetime_parts(input: &str) -> Option<(i64, u64, u64, u64, u64, u64)> {
    let clean = input.trim();
    let mut parts: Vec<&str> = clean
        .split(&['-', ' ', ':', 'T'][..])
        .filter(|s| !s.is_empty())
        .collect();

    // Pull out an optional, case-insensitive AM/PM token so it isn't counted as
    // an extra component.
    let meridiem = parts
        .iter()
        .position(|t| t.eq_ignore_ascii_case("am") || t.eq_ignore_ascii_case("pm"));
    let pm = match meridiem {
        Some(idx) => parts.remove(idx).eq_ignore_ascii_case("pm"),
        None => false,
    };

    // At most `YYYY MM DD HH MM SS`.
    if parts.len() < 3 || parts.len() > 6 {
        return None;
    }
    let year = parse_year(parts[0])?;
    let month = crate::core::validate::parse_short_numeric(parts[1])?;
    let day = crate::core::validate::parse_short_numeric(parts[2])?;
    let mut hour = component(&parts, 3)?;
    let minute = component(&parts, 4)?;
    let second = component(&parts, 5)?;

    // A meridiem requires a 12-hour value (1–12); combining it with a 24-hour
    // hour is contradictory, so reject it rather than silently ignoring it.
    if meridiem.is_some() {
        if !(1..=12).contains(&hour) {
            return None;
        }
        hour = (hour % 12) + if pm { 12 } else { 0 };
    }

    Some((year, month, day, hour, minute, second))
}

/// Parse a 1–4 digit year with no sign (delegates to the shared token rule).
fn parse_year(s: &str) -> Option<i64> {
    crate::core::validate::parse_digits(s, 4).map(|v| v as i64)
}

/// Read optional datetime component `idx` from `parts`, defaulting to `0` when
/// absent. Present-but-malformed tokens return `None`.
fn component(parts: &[&str], idx: usize) -> Option<u64> {
    match parts.get(idx) {
        Some(s) => crate::core::validate::parse_short_numeric(s),
        None => Some(0),
    }
}

/// Normalize a user-entered local datetime to the canonical stored form:
/// `"YYYY-MM-DD HH:MM:SS"` (or `"YYYY-MM-DD"` when no time was given).
///
/// Returns `None` for anything that isn't a valid date/time, so callers can
/// reject bad input instead of persisting it. The instant itself is not
/// computed here — that happens when the timestamp is written.
pub fn normalize_local_datetime(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let (year, month, day, hour, minute, second) = parse_datetime_parts(trimmed)?;
    if year < 1970 {
        return None;
    }
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    // A time component is present when the input carries more than the three
    // date tokens (e.g. `2024-01-15 14:30` vs `2024-01-15`).
    let has_time = trimmed
        .split(['-', ' ', ':', 'T'])
        .filter(|s| !s.is_empty())
        .count()
        > 3;
    Some(if has_time {
        format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
    } else {
        format!("{year:04}-{month:02}-{day:02}")
    })
}

/// Number of days in `month` (1–12) of `year`, accounting for leap years.
pub(crate) fn days_in_month(year: i64, month: u64) -> u64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year as u64) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Whole days from 1970-01-01 to `year-month-day`, or `None` if the date fields
/// are out of range. Shared by the UTC and local datetime parsers and the EXIF
/// date conversion, so all agree on leap years and month lengths.
pub(crate) fn days_from_civil(year: i64, month: u64, day: u64) -> Option<i64> {
    if year < 1970 || !(1..=12).contains(&month) {
        return None;
    }
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let mut total_days: i64 = 0;
    for y in 1970..year {
        total_days += if is_leap(y as u64) { 366 } else { 365 };
    }
    for m in 1..month {
        total_days += days_in_month(year, m) as i64;
    }
    Some(total_days + (day - 1) as i64)
}

/// Inverse of [`days_from_civil`]: whole days since the epoch → `(year, month, day)`.
pub(crate) fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let mut month = 1u64;
    loop {
        let dim = days_in_month(year as i64, month);
        if remaining < dim {
            break;
        }
        remaining -= dim;
        month += 1;
    }
    (year, month, remaining + 1)
}

/// Parse a user-entered local wall-clock datetime into a UTC instant.
///
/// All frontends (GUI editor, CLI `--set-*`, TUI console) interpret fixed
/// timestamps as **local time**; this is the one place that converts them.
pub fn parse_local_datetime(input: &str) -> Option<SystemTime> {
    let (y, mo, d, h, mi, s) = parse_datetime_parts(input)?;
    let secs = local_datetime_to_utc_secs(y, mo, d, h, mi, s)?;
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64))
}

/// Format a `SystemTime` as a date string.
///
/// Always applies the local timezone offset **in effect at `time`** first, then
/// adds any extra `offset_hours` on top. `None` or `Some(0)` means no extra
/// offset (pure local time).
pub fn format_date(
    fmt: &str,
    time: SystemTime,
    century: bool,
    offset_hours: Option<i32>,
) -> String {
    // Local offset at the instant being rendered (not "now"), then any extra
    // offset on top.
    let local_offset = local_utc_offset_secs_at(time);
    let extra_secs = offset_hours.unwrap_or(0) as i64 * 3600;
    let offset_secs = local_offset + extra_secs;

    // Format using a pre-computed total offset in seconds.
    // Exposed as a separate function for testing.
    format_date_offset(fmt, time, century, offset_secs)
}

/// Format a `SystemTime` using a raw offset in seconds (for testing).
/// `offset_secs` is the total offset to apply (e.g. 0 = UTC).
pub(crate) fn format_date_offset(
    fmt: &str,
    time: SystemTime,
    century: bool,
    offset_secs: i64,
) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let adjusted = if offset_secs >= 0 {
        time.checked_add(Duration::from_secs(offset_secs as u64))
    } else {
        time.checked_sub(Duration::from_secs((-offset_secs) as u64))
    };
    let time = adjusted.unwrap_or(time);

    let dur = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    let (year, month, day) = civil_from_days(secs / 86400);
    let rem_secs = secs % 86400;
    let hour = rem_secs / 3600;
    let minute = (rem_secs % 3600) / 60;
    let second = rem_secs % 60;

    let year_fmt = if century {
        format!("{:04}", year)
    } else {
        format!("{:02}", year % 100)
    };

    fmt.replace("YYYY", &year_fmt)
        .replace("YY", &format!("{:02}", year % 100))
        .replace("MM", &format!("{:02}", month))
        .replace("DD", &format!("{:02}", day))
        .replace("hh", &format!("{:02}", hour))
        .replace("mm", &format!("{:02}", minute))
        .replace("ss", &format!("{:02}", second))
}

pub(crate) fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Parse a formatted date string as a **UTC** instant (no timezone conversion).
///
/// For user-entered local timestamps use [`parse_local_datetime`] instead.
pub fn parse_date(input: &str) -> Option<SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};
    let clean = input.trim();
    let parts: Vec<&str> = clean
        .split(&['-', ' ', ':'][..])
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 3 || parts.len() > 6 {
        return None;
    }
    let year = parse_year(parts[0])? as u64;
    let month = crate::core::validate::parse_short_numeric(parts[1])?;
    let day = crate::core::validate::parse_short_numeric(parts[2])?;
    let hour = component(&parts, 3)?;
    let minute = component(&parts, 4)?;
    let second = component(&parts, 5)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let total_days = days_from_civil(year as i64, month, day)?;
    let total_secs = total_days * 86400 + (hour * 3600 + minute * 60 + second) as i64;
    if total_secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(total_secs as u64))
}

/// Resolve the `TimestampSpec` to an actual `SystemTime`, or `None` for Current/absent.
fn resolve_timestamp_spec(spec: &Option<TimestampSpec>) -> Option<SystemTime> {
    match spec {
        Some(TimestampSpec::Fixed(s)) => parse_local_datetime(s),
        Some(TimestampSpec::CopyFrom(_))
        | Some(TimestampSpec::Taken)
        | Some(TimestampSpec::Delta(_)) => {
            // These require file-level resolution; fall back to None (current time).
            None
        }
        _ => None,
    }
}

/// Render EXIF epoch seconds (see `read_exif_date`) as a `SystemTime`.
fn exif_date_time(epoch: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(epoch.max(0) as u64)
}

/// The file's EXIF capture date, or its mtime when the image has none — the one
/// fallback every EXIF-backed tag and date type shares.
fn exif_date_or_mtime(meta: &FileMeta) -> Option<SystemTime> {
    meta.exif_date.map(exif_date_time).or(meta.modified)
}

/// Map a date-type discriminator to the appropriate `Option<SystemTime>`.
/// Returns `None` for types that should use the current time, or when the
/// requested time source isn't available.
///
/// The `_new` keys are the only ones that consult the timestamp editor; every
/// other key resolves through [`auto_date_type_source`], the shared source
/// table. That is what keeps a key `sanitize` accepts from falling through to
/// the wall clock in silence — the EXIF-backed keys used to.
pub(crate) fn date_type_time_source(
    date_type: Option<&str>,
    meta: &FileMeta,
    config: &RenameConfig,
) -> Option<SystemTime> {
    match date_type {
        // New timestamps from the timestamp editor, falling back to the file's.
        Some("creation_new") => {
            resolve_timestamp_spec(&config.special.set_created).or(meta.created)
        }
        Some("modified_new") => {
            resolve_timestamp_spec(&config.special.set_modified).or(meta.modified)
        }
        Some("accessed_new") => {
            resolve_timestamp_spec(&config.special.set_accessed).or(meta.accessed)
        }
        other => match auto_date_type_source(other.unwrap_or_default()) {
            Some(MetaSource::Exif) => exif_date_or_mtime(meta),
            Some(MetaSource::Mtime) => meta.modified,
            Some(MetaSource::Created) => meta.created,
            Some(MetaSource::Accessed) => meta.accessed,
            // `current` — and an absent or unknown key — means the wall clock.
            Some(MetaSource::Clock) | None => None,
        },
    }
}

pub(crate) fn apply_add_date(
    s: &str,
    fmt: &str,
    file_modified: Option<SystemTime>,
    suffix: bool,
    config: &RenameConfig,
) -> String {
    let time = file_modified.unwrap_or_else(SystemTime::now);
    let date_str = format_date(
        fmt,
        time,
        config.auto_date.auto_date_century,
        config.auto_date.auto_date_offset,
    );
    let sep = config
        .auto_date
        .auto_date_sep_filename
        .as_deref()
        .unwrap_or("");
    if suffix {
        format!("{}{}{}", s, sep, date_str)
    } else {
        format!("{}{}{}", date_str, sep, s)
    }
}

/// Return the insert-meta date format string, preferring the configured
/// `insert_meta_fmt` and falling back to [`INSERT_META_FMT_DEFAULT`].
fn insert_meta_fmt_str(config: &RenameConfig) -> &str {
    config
        .auto_date
        .insert_meta_fmt
        .as_deref()
        .unwrap_or(INSERT_META_FMT_DEFAULT)
}

/// Format a `SystemTime` using the insert-meta format.
fn fmt_meta_time(time: SystemTime, config: &RenameConfig) -> String {
    let fmt = insert_meta_fmt_str(config);
    format_date(
        fmt,
        time,
        config.auto_date.auto_date_century,
        config.auto_date.auto_date_offset,
    )
}

pub(crate) fn apply_insert_meta(
    s: &str,
    tag: &str,
    meta: &FileMeta,
    suffix: bool,
    config: &RenameConfig,
) -> String {
    let sep = config
        .auto_date
        .auto_date_sep_filename
        .as_deref()
        .unwrap_or("");
    let insert_val = |val: &str, sep: &str, stem: &str, suffix: bool| -> String {
        if suffix {
            format!("{}{}{}", stem, sep, val)
        } else {
            format!("{}{}{}", val, sep, stem)
        }
    };
    // The tag decides the timestamp; nothing to insert means leave the name be.
    let with_time = |time: Option<SystemTime>| match time {
        Some(time) => insert_val(&fmt_meta_time(time, config), sep, s, suffix),
        None => s.to_string(),
    };

    // Routed on the shared tag table, so this behaviour and `config_needs_exif_date`
    // (which decides whether EXIF was read at all) cannot drift apart and
    // silently drop a date. The EXIF-backed tags differ only in intent: all of
    // them fall back to the file's mtime when there is no usable EXIF date.
    match insert_meta_tag_source(tag) {
        Some(MetaSource::Exif) => with_time(exif_date_or_mtime(meta)),
        Some(MetaSource::Mtime) => with_time(meta.modified),
        Some(MetaSource::Created) => with_time(meta.created),
        Some(MetaSource::Accessed) => with_time(meta.accessed),
        // The Insert Meta tags are all file-derived; a wall-clock tag would make
        // a rename non-deterministic, so there is none. An unknown tag — which
        // `sanitize` rejects, but a stacking `command_order` can still carry —
        // leaves the name alone.
        Some(MetaSource::Clock) | None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The local→UTC conversion must resolve the offset for the *target* date:
    /// converting a wall-clock time to epoch and back through `localtime` must
    /// yield the same wall clock. The old implementation subtracted the current
    /// offset, so it was off by an hour whenever the target date's DST state
    /// differed from today's.
    #[test]
    fn local_datetime_round_trips_through_localtime() {
        for (y, mo, d, h, mi, s) in [
            (2020, 1u64, 15u64, 12u64, 30u64, 0u64),
            (2020, 7, 15, 12, 30, 0),
        ] {
            let epoch = local_datetime_to_utc_secs(y, mo, d, h, mi, s).unwrap();
            let t = epoch as libc::time_t;
            let mut tm: libc::tm = unsafe { std::mem::zeroed() };
            #[cfg(not(windows))]
            let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
            #[cfg(windows)]
            let ok = unsafe { libc::localtime_s(&mut tm, &t) == 0 };
            assert!(ok, "localtime failed");
            assert_eq!(
                (
                    tm.tm_year + 1900,
                    tm.tm_mon + 1,
                    tm.tm_mday,
                    tm.tm_hour,
                    tm.tm_min,
                ),
                (y as i32, mo as i32, d as i32, h as i32, mi as i32),
                "wall clock must round-trip for {y}-{mo}-{d} {h}:{mi}"
            );
        }
    }

    /// Display and input must agree: an instant rendered with `format_date` must
    /// parse back (as local time) to the same instant. The old display path
    /// rendered UTC civil time, so this failed on any non-UTC timezone.
    #[test]
    fn local_display_round_trips_through_parse() {
        let now = SystemTime::now();
        let displayed = format_date("YYYY-MM-DD hh:mm:ss", now, true, None);
        let parsed = parse_local_datetime(&displayed)
            .unwrap_or_else(|| panic!("could not parse displayed time {displayed:?}"));
        let diff = parsed
            .duration_since(now)
            .or_else(|_| now.duration_since(parsed))
            .unwrap();
        assert!(
            diff.as_secs() < 2,
            "local display must round-trip to the same instant (diff {diff:?})"
        );
    }

    /// Fixed timestamps are interpreted as local wall-clock time.
    #[test]
    fn parse_local_datetime_matches_local_conversion() {
        let parsed = parse_local_datetime("2020-01-02 03:04:05").unwrap();
        let secs = local_datetime_to_utc_secs(2020, 1, 2, 3, 4, 5).unwrap();
        assert_eq!(
            parsed,
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64)
        );
    }

    /// The raw **UTC** parser must apply the same token rules and reject
    /// out-of-range fields (previously `2024-00-10` underflowed and panicked).
    #[test]
    fn parse_date_validates_and_rejects_malformed() {
        assert!(parse_date("2024-01-15 05:30:00").is_some());
        assert!(parse_date("2024-1-5 5:3:2").is_some());
        for bad in [
            "2024-00-10",
            "2024-13-01",
            "2024-01-00",
            "2024-01-15 24:00:00",
            "2024-01-15 10:30:60",
            "2024-015-15",
            "02024-01-15",
            "+2024-01-15",
            "2024-01-15 5:3:2:1",
            "2023-02-29",
            "2024-02-30",
        ] {
            assert_eq!(parse_date(bad), None, "should reject {bad:?}");
        }
    }

    /// The civil-date helpers are inverses, and both reject impossible dates
    /// rather than silently normalising them.
    #[test]
    fn civil_conversions_round_trip() {
        for (y, m, d) in [
            (1970i64, 1u64, 1u64),
            (1970, 12, 31),
            (2000, 2, 29),
            (2024, 2, 29),
            (2023, 3, 1),
            (2024, 12, 31),
        ] {
            let days = days_from_civil(y, m, d).unwrap();
            assert_eq!(civil_from_days(days as u64), (y as u64, m, d));
        }
        assert_eq!(days_from_civil(2023, 2, 29), None);
        assert_eq!(days_from_civil(2024, 2, 30), None);
        assert_eq!(days_from_civil(2024, 13, 1), None);
        assert_eq!(days_from_civil(2024, 0, 1), None);
        assert_eq!(days_from_civil(1969, 12, 31), None);
    }

    /// `normalize_local_datetime` is the validation/normalization gate for user
    /// input: it canonicalizes accepted forms and rejects invalid ones.
    #[test]
    fn normalize_local_datetime_canonicalizes_and_validates() {
        assert_eq!(
            normalize_local_datetime("2024-01-15").as_deref(),
            Some("2024-01-15")
        );
        assert_eq!(
            normalize_local_datetime("2024-01-15T14:30:00").as_deref(),
            Some("2024-01-15 14:30:00")
        );
        assert_eq!(
            normalize_local_datetime("2024-02-29").as_deref(),
            Some("2024-02-29")
        );
        // AM/PM is converted to 24-hour form.
        assert_eq!(
            normalize_local_datetime("2024-01-15 02:30:00 PM").as_deref(),
            Some("2024-01-15 14:30:00")
        );
        assert_eq!(
            normalize_local_datetime("2024-01-15 12:00:00 AM").as_deref(),
            Some("2024-01-15 00:00:00")
        );
        assert_eq!(
            normalize_local_datetime("2024-01-15 12:00:00 PM").as_deref(),
            Some("2024-01-15 12:00:00")
        );
        assert_eq!(
            normalize_local_datetime("2024-01-15 2:5:6 pm").as_deref(),
            Some("2024-01-15 14:05:06")
        );
        // Zero-padding to the canonical 2-digit width is optional: `5` == `05`.
        assert_eq!(
            normalize_local_datetime("2024-01-15 05:30:00").as_deref(),
            Some("2024-01-15 05:30:00")
        );
        assert_eq!(
            normalize_local_datetime("2024-01-15 5:30:00").as_deref(),
            Some("2024-01-15 05:30:00")
        );
        assert_eq!(
            normalize_local_datetime("2024-1-5 5:3:2").as_deref(),
            Some("2024-01-05 05:03:02")
        );

        for bad in [
            "",
            "garbage",
            "2024-13-01",
            "2024-00-10",
            "2024-02-30",
            "2023-02-29",
            "1969-12-31",
            "2024-01-15 25:00:00",
            "2024-01-15 10:60:00",
            // A meridiem requires a 12-hour value (1-12).
            "2024-01-15 14:30:00 PM",
            "2024-01-15 015:23:20 PM",
            "2024-01-15 0:30:00 AM",
            // More than 2 digits is not padding; it's malformed.
            "2024-01-15 005:30:00",
            "2024-015-15",
            "02024-01-15",
            "+2024-01-15",
            // Too many components.
            "2024-01-15 5:3:2:1",
            // Seconds are 0-59 (leap seconds aren't representable).
            "2024-01-15 10:30:60",
        ] {
            assert_eq!(normalize_local_datetime(bad), None, "should reject {bad:?}");
        }
    }
}
