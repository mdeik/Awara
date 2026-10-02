use super::*;
use crate::gui::helpers::{build_fixed_datetime, split_hms};

/// 24-hour hour → 12-hour hour + AM/PM flag.
fn hour_to_12h(h24: u32) -> (u32, bool) {
    let pm = h24 >= 12;
    let h12 = match h24 % 12 {
        0 => 12,
        h => h,
    };
    (h12, pm)
}

/// Format a 24-hour clock time as 12-hour `hh:mm:ss`.
fn time_to_12h(h24: u32, m: u32, s: u32) -> String {
    let (h12, _) = hour_to_12h(h24);
    format!("{h12:02}:{m:02}:{s:02}")
}

impl GuiApp {
    pub fn init_timestamps_ui(&mut self) {
        let now = Self::now_datetime_string();
        let (now_date, now_time) = now.split_once(' ').unwrap_or((&now, ""));
        let now_date = now_date.to_string();
        let (now_h, now_mi, now_s) = split_hms(now_time).unwrap_or((0, 0, 0));
        let now_time_12 = time_to_12h(now_h, now_mi, now_s);
        let now_pm = now_h >= 12;

        // Helper: init a timestamp group
        let init_one = |config_val: &Option<TimestampSpec>, ts: &mut GuiTimestampState| {
            ts.delta_days = 0;
            ts.delta_hours = 0;
            ts.delta_mins = 0;
            ts.delta_secs = 0;
            ts.negative = false;
            ts.fixed_pm = now_pm;
            match config_val {
                Some(TimestampSpec::Current) => {
                    ts.mode = "current".into();
                    ts.fixed_date = now_date.clone();
                    ts.fixed_time = now_time_12.clone();
                }
                Some(TimestampSpec::Fixed(val)) => {
                    ts.mode = "fixed".into();
                    if let Some((date, time)) = val.split_once(' ') {
                        ts.fixed_date = date.to_string();
                        match split_hms(time) {
                            Some((h, m, s)) => {
                                ts.fixed_time = time_to_12h(h, m, s);
                                ts.fixed_pm = h >= 12;
                            }
                            None => ts.fixed_time = time.to_string(),
                        }
                    } else {
                        ts.fixed_date = val.clone();
                        ts.fixed_time = now_time_12.clone();
                    }
                }
                Some(TimestampSpec::CopyFrom(ref_name)) => {
                    ts.mode = ref_name.clone();
                    ts.fixed_date = now_date.clone();
                    ts.fixed_time = now_time_12.clone();
                }
                Some(TimestampSpec::Taken) => {
                    ts.mode = "taken".into();
                    ts.fixed_date = now_date.clone();
                    ts.fixed_time = now_time_12.clone();
                }
                Some(TimestampSpec::Delta(secs)) => {
                    ts.mode = "delta".into();
                    ts.fixed_date = now_date.clone();
                    ts.fixed_time = now_time_12.clone();
                    let total = *secs;
                    let neg = total < 0;
                    let abs_secs = if neg { -total } else { total };
                    let d = abs_secs / 86400;
                    let r = abs_secs % 86400;
                    let h = r / 3600;
                    let r2 = r % 3600;
                    let m = r2 / 60;
                    let s = r2 % 60;
                    ts.delta_days = d as u32;
                    ts.delta_hours = h as u32;
                    ts.delta_mins = m as u32;
                    ts.delta_secs = s as u32;
                    ts.negative = neg;
                }
                None => {
                    ts.mode = "no_change".into();
                    ts.fixed_date = now_date.clone();
                    ts.fixed_time = now_time_12.clone();
                }
            }
        };

        // Created
        init_one(
            &self.config.special.set_created,
            &mut self.timestamps.created,
        );

        // Modified
        init_one(
            &self.config.special.set_modified,
            &mut self.timestamps.modified,
        );

        // Accessed
        init_one(
            &self.config.special.set_accessed,
            &mut self.timestamps.accessed,
        );
    }

    /// Build a formatted current datetime string "YYYY-MM-DD HH:MM:SS" from the
    /// system clock, in **local time** (SSoT: [`awara::format_date`]).
    pub(super) fn now_datetime_string() -> String {
        awara::format_date(
            "YYYY-MM-DD hh:mm:ss",
            std::time::SystemTime::now(),
            true,
            None,
        )
    }

    /// Apply the timestamp UI state back to the config.
    /// Called when the user clicks OK in the timestamps editor.
    pub fn apply_timestamps_ui(&mut self) {
        // The editor disables Confirm when a Fixed value is invalid; if this is
        // still called with bad input, leave the field untouched rather than
        // writing an unusable value.
        let fixed_value =
            |date: &str, time: &str, pm: bool| build_fixed_datetime(date, time, pm).ok();

        let build_delta =
            |days: u32, hours: u32, mins: u32, secs: u32, neg: bool| -> Option<TimestampSpec> {
                let total =
                    days as i64 * 86400 + hours as i64 * 3600 + mins as i64 * 60 + secs as i64;
                if total == 0 {
                    None
                } else {
                    Some(TimestampSpec::Delta(if neg { -total } else { total }))
                }
            };

        // Store per-file increment (applies to all timestamp fields)
        self.config.special.timestamp_incr_secs = self.timestamps.created.incr_by;

        // Created
        match self.timestamps.created.mode.as_str() {
            "no_change" => self.config.special.set_created = None,
            "current" => {
                self.config.special.set_created = Some(TimestampSpec::Current);
            }
            "fixed" => {
                if let Some(v) = fixed_value(
                    &self.timestamps.created.fixed_date,
                    &self.timestamps.created.fixed_time,
                    self.timestamps.created.fixed_pm,
                ) {
                    self.config.special.set_created = Some(TimestampSpec::Fixed(v));
                }
            }
            "modified" => {
                self.config.special.set_created = Some(TimestampSpec::CopyFrom("modified".into()));
            }
            "taken" => {
                self.config.special.set_created = Some(TimestampSpec::Taken);
            }
            "delta" => {
                self.config.special.set_created = build_delta(
                    self.timestamps.created.delta_days,
                    self.timestamps.created.delta_hours,
                    self.timestamps.created.delta_mins,
                    self.timestamps.created.delta_secs,
                    self.timestamps.created.negative,
                );
            }
            _ => self.config.special.set_created = None,
        }

        // Modified
        match self.timestamps.modified.mode.as_str() {
            "no_change" => self.config.special.set_modified = None,
            "current" => {
                self.config.special.set_modified = Some(TimestampSpec::Current);
            }
            "fixed" => {
                if let Some(v) = fixed_value(
                    &self.timestamps.modified.fixed_date,
                    &self.timestamps.modified.fixed_time,
                    self.timestamps.modified.fixed_pm,
                ) {
                    self.config.special.set_modified = Some(TimestampSpec::Fixed(v));
                }
            }
            "created" => {
                self.config.special.set_modified = Some(TimestampSpec::CopyFrom("created".into()));
            }
            "taken" => {
                self.config.special.set_modified = Some(TimestampSpec::Taken);
            }
            "delta" => {
                self.config.special.set_modified = build_delta(
                    self.timestamps.modified.delta_days,
                    self.timestamps.modified.delta_hours,
                    self.timestamps.modified.delta_mins,
                    self.timestamps.modified.delta_secs,
                    self.timestamps.modified.negative,
                );
            }
            _ => self.config.special.set_modified = None,
        }

        // Accessed
        match self.timestamps.accessed.mode.as_str() {
            "no_change" => self.config.special.set_accessed = None,
            "current" => {
                self.config.special.set_accessed = Some(TimestampSpec::Current);
            }
            "fixed" => {
                if let Some(v) = fixed_value(
                    &self.timestamps.accessed.fixed_date,
                    &self.timestamps.accessed.fixed_time,
                    self.timestamps.accessed.fixed_pm,
                ) {
                    self.config.special.set_accessed = Some(TimestampSpec::Fixed(v));
                }
            }
            "created" => {
                self.config.special.set_accessed = Some(TimestampSpec::CopyFrom("created".into()));
            }
            "taken" => {
                self.config.special.set_accessed = Some(TimestampSpec::Taken);
            }
            "delta" => {
                self.config.special.set_accessed = build_delta(
                    self.timestamps.accessed.delta_days,
                    self.timestamps.accessed.delta_hours,
                    self.timestamps.accessed.delta_mins,
                    self.timestamps.accessed.delta_secs,
                    self.timestamps.accessed.negative,
                );
            }
            _ => self.config.special.set_accessed = None,
        }

        self.preview_dirty = true;
    }

    /// True when every timestamp section's editor input is valid. Used to
    /// disable Confirm so an invalid Fixed value can't be applied.
    pub fn timestamps_valid(&self) -> bool {
        [
            &self.timestamps.created,
            &self.timestamps.modified,
            &self.timestamps.accessed,
        ]
        .iter()
        .all(|ts| {
            ts.mode != "fixed"
                || build_fixed_datetime(&ts.fixed_date, &ts.fixed_time, ts.fixed_pm).is_ok()
        })
    }
}
