//! Shared timestamp-editor helpers.
//!
//! The GUI stores timestamps as canonical 24-hour local values (the same form
//! the CLI/TUI and presets use). These helpers are the single place that turns
//! the 12-hour editor fields into that canonical form, and the single place
//! that decides whether they are valid — used both when applying the editor and
//! when drawing its validation state.

/// Parse one clock component (1–2 digits, optional zero-padding) and range-check it.
///
/// The token rule is shared with the core parser via
/// [`awara::parse_short_numeric`], so the GUI and CLI accept exactly the same
/// forms (`5`/`05` yes, `015`/`+5` no).
fn clock_part(s: &str, max: u32) -> Option<u32> {
    let v = awara::parse_short_numeric(s)?;
    (v <= max as u64).then_some(v as u32)
}

/// Split `hh:mm[:ss]` into (hour, minute, second).
///
/// Used both for canonical 24-hour stored values (hour 0–23) and for the raw
/// editor field. Empty, malformed, or out-of-range input returns `None`.
pub(crate) fn split_hms(t: &str) -> Option<(u32, u32, u32)> {
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    let mut it = t.split(':');
    let h = clock_part(it.next()?, 23)?;
    let m = match it.next() {
        Some(x) => clock_part(x, 59)?,
        None => 0,
    };
    let s = match it.next() {
        Some(x) => clock_part(x, 59)?,
        None => 0,
    };
    if it.next().is_some() {
        return None;
    }
    Some((h, m, s))
}

/// Editor time (`hh:mm:ss` + AM/PM) → canonical 24-hour `HH:MM:SS`.
///
/// The editor is a 12-hour clock, so the hour must be `1..=12`; `12 AM` is
/// midnight and `12 PM` is noon. Anything else is rejected.
pub(crate) fn ui_time_to_24h(t: &str, pm: bool) -> Option<String> {
    let (h, m, s) = split_hms(t)?;
    if !(1..=12).contains(&h) {
        return None;
    }
    let h24 = (h % 12) + if pm { 12 } else { 0 };
    Some(format!("{h24:02}:{m:02}:{s:02}"))
}

/// Build a canonical local datetime from the editor's date + 12-hour time
/// fields, or return a human-readable reason it is invalid.
///
/// Validation/normalization is delegated to the core SSoT
/// (`normalize_local_datetime`), so the GUI accepts exactly what the CLI/TUI
/// accept.
pub(crate) fn build_fixed_datetime(date: &str, time: &str, pm: bool) -> Result<String, String> {
    let date = date.trim();
    if date.is_empty() {
        return Err("Date is required (YYYY-MM-DD)".into());
    }
    let candidate = if time.trim().is_empty() {
        date.to_string()
    } else {
        let t24 = ui_time_to_24h(time, pm)
            .ok_or_else(|| format!("Invalid time '{}' (expected 1-12 hh:mm:ss)", time.trim()))?;
        format!("{} {}", date, t24)
    };
    awara::normalize_local_datetime(&candidate)
        .ok_or_else(|| format!("Invalid date/time '{}'", candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_time_accepts_only_12_hour() {
        assert_eq!(
            ui_time_to_24h("12:00:00", false).as_deref(),
            Some("00:00:00")
        );
        assert_eq!(
            ui_time_to_24h("12:00:00", true).as_deref(),
            Some("12:00:00")
        );
        assert_eq!(ui_time_to_24h("2:30:00", true).as_deref(), Some("14:30:00"));
        assert_eq!(
            ui_time_to_24h("02:30:00", true).as_deref(),
            Some("14:30:00")
        );
        assert_eq!(ui_time_to_24h("11:59", false).as_deref(), Some("11:59:00"));
    }

    #[test]
    fn ui_time_rejects_malformed_or_out_of_range() {
        for bad in [
            "015:23:20", // 3-digit hour
            "15:23:20",  // 24-hour value in a 12-hour field
            "13:00:00",
            "0:30:00",
            "+5:00:00",
            "12:015:00",
            "12:00:060",
            "1:2:3:4",
            "12::00",
            "",
            "abc",
            "12:60:00",
        ] {
            assert!(
                ui_time_to_24h(bad, false).is_none(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn split_hms_accepts_canonical_24h_for_stored_values() {
        assert_eq!(split_hms("00:00:00"), Some((0, 0, 0)));
        assert_eq!(split_hms("14:30:00"), Some((14, 30, 0)));
        assert_eq!(split_hms("23:59:59"), Some((23, 59, 59)));
        assert_eq!(split_hms("9:5:6"), Some((9, 5, 6)));
        // Zero-padding to 2 digits is optional.
        assert_eq!(split_hms("05:30:00"), Some((5, 30, 0)));
        assert_eq!(split_hms("5:30:00"), Some((5, 30, 0)));
        assert_eq!(split_hms("15"), Some((15, 0, 0)));
        // Malformed tokens are rejected, not reinterpreted.
        assert_eq!(split_hms("015:00:00"), None);
        assert_eq!(split_hms("+5"), None);
    }

    #[test]
    fn build_fixed_rejects_bad_editor_time() {
        assert!(build_fixed_datetime("2025-06-15", "015:23:20", true).is_err());
        assert!(build_fixed_datetime("2025-06-15", "15:23:20", true).is_err());
        assert_eq!(
            build_fixed_datetime("2025-06-15", "02:30:00", true).unwrap(),
            "2025-06-15 14:30:00"
        );
        assert_eq!(
            build_fixed_datetime("2025-06-15", "", false).unwrap(),
            "2025-06-15"
        );
    }
}
