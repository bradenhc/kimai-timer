// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt add` subcommand for manually inserting time intervals.
//!
//! Used primarily to amend missed or incorrectly tracked sessions. The command prompts for a task,
//! start date/time, and stop date/time, then appends the resulting interval to the timelog. By
//! default, future dates and times are restricted so accidental future entries cannot be created;
//! pass `--future` to bypass this when pre-entering known events like PTO or holidays.

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use clap::Parser;
use colored::Colorize;
use inquire::{DateSelect, Select};

use crate::store::{Store, TimeInterval};

/// Arguments for the `kt add` subcommand.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandAdd {
    /// The task to add a time event for (must have been previously created with `new`).
    task: Option<String>,

    /// Allow selecting dates and times in the future (e.g. for pre-entering PTO or holidays).
    #[arg(long, short)]
    future: bool,

    /// Start date in YYYY-MM-DD format. Skips the interactive date picker when provided.
    #[arg(long)]
    start_date: Option<String>,

    /// Start time in HH:MM format. Skips the interactive time picker when provided.
    #[arg(long)]
    start_time: Option<String>,

    /// Stop date in YYYY-MM-DD format. Skips the interactive date picker when provided.
    #[arg(long)]
    stop_date: Option<String>,

    /// Stop time in HH:MM format. Skips the interactive time picker when provided.
    #[arg(long)]
    stop_time: Option<String>,
}

impl CommandAdd {
    /// Runs the add flow: selects a task, prompts for start/stop date+time, and saves the interval.
    ///
    /// Snapshots the current local time once at entry so all pickers share a consistent "now"
    /// boundary. When `--future` is not set, the date pickers are capped at today and the time
    /// pickers are filtered to only show times at or before the current time when today is selected.
    ///
    pub fn execute(self, store: &Store) -> Result<()> {
        let tasks = store.get_tasks()?;

        let task = match self.task {
            Some(t) => t,

            None => Select::new("Task?      ", tasks.iter().collect())
                .prompt()?
                .clone(),
        };

        if !tasks.contains(&task) {
            bail!("invalid task: {task} (must use previously created task)");
        }

        let now_local = Local::now();
        let today = now_local.date_naive();

        let start_date = match self.start_date {
            Some(s) => Self::parse_date(&s, "--start-date", today, self.future)?,
            None => Self::select_date("Start date?", today, None, self.future)?,
        };
        let start_time = match self.start_time {
            Some(s) => {
                Self::parse_time_flag(&s, "--start-time", start_date, now_local, self.future)?
            }
            None => Self::select_time("Start time?", start_date, now_local, self.future)?,
        };
        let stop_date = match self.stop_date {
            Some(s) => Self::parse_date(&s, "--stop-date", today, self.future)?,
            None => Self::select_date("Stop date? ", today, Some(start_date), self.future)?,
        };
        let stop_time = match self.stop_time {
            Some(s) => Self::parse_time_flag(&s, "--stop-time", stop_date, now_local, self.future)?,
            None => Self::select_time("Stop time? ", stop_date, now_local, self.future)?,
        };

        let start = Self::combine_date_time(start_date, &start_time)?;
        let stop = Self::combine_date_time(stop_date, &stop_time)?;

        if stop <= start {
            bail!("stop time must be after start time");
        }

        let interval = TimeInterval::new(&task, start, stop);
        store.append_interval(interval)?;

        println!(
            "Added interval for {}: {start_time} - {stop_time}",
            task.green()
        );

        Ok(())
    }

    /// Prompts the user to select a date from a picker in the console.
    ///
    /// The `today` and `allow_future` parameters control whether dates are capped at today. When
    /// `starting` is `Some`, the picker opens on that date — used to pre-seed the stop date with
    /// the already-chosen start date.
    ///
    fn select_date(
        prompt: &str,
        today: NaiveDate,
        starting: Option<NaiveDate>,
        allow_future: bool,
    ) -> Result<NaiveDate> {
        let mut picker = DateSelect::new(prompt);
        if let Some(sd) = starting {
            picker = picker.with_starting_date(sd);
        }
        if !allow_future {
            picker = picker.with_max_date(today);
        }
        Ok(picker.prompt()?)
    }

    /// Parses a date string supplied via a CLI flag, enforcing the future restriction when needed.
    ///
    /// Returns a descriptive error if the string is malformed or the date is in the future without
    /// `--future` being set.
    ///
    fn parse_date(s: &str, flag: &str, today: NaiveDate, allow_future: bool) -> Result<NaiveDate> {
        let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| anyhow!("invalid {flag}: '{s}' (expected YYYY-MM-DD)"))?;
        if !allow_future && date > today {
            bail!("{flag} '{s}' is in the future: pass --future to allow it");
        }
        Ok(date)
    }

    /// Parses a time string supplied via a CLI flag, enforcing the future restriction when needed.
    ///
    /// Accepts any valid `HH:MM` (not restricted to 3-minute boundaries, unlike the interactive
    /// picker). Returns a normalised zero-padded `HH:MM` string consistent with `select_time`.
    ///
    fn parse_time_flag(
        s: &str,
        flag: &str,
        date: NaiveDate,
        now: DateTime<Local>,
        allow_future: bool,
    ) -> Result<String> {
        let t = NaiveTime::parse_from_str(s, "%H:%M")
            .map_err(|_| anyhow!("invalid {flag}: '{s}' (expected HH:MM)"))?;
        if !allow_future && date == now.date_naive() {
            let now_mins = now.hour() * 60 + now.minute();
            let provided_mins = t.hour() * 60 + t.minute();
            if provided_mins > now_mins {
                bail!("{flag} '{s}' is in the future: pass --future to allow it");
            }
        }
        Ok(format!("{:02}:{:02}", t.hour(), t.minute()))
    }

    /// Prompts the user to select a time for the event using a list in the console..
    ///
    /// Times are in 3-minute increments, matching `RoundingMode::Classic(3)` in `store.rs` so
    /// every selectable time aligns with the minimum billing boundary. When today is selected
    /// without `--future`, the cap is snapped down to the nearest 3-minute boundary at or before
    /// the current minute.
    ///
    fn select_time(
        prompt: &str,
        date: NaiveDate,
        now: DateTime<Local>,
        allow_future: bool,
    ) -> Result<String> {
        let today = now.date_naive();

        let raw_cap: u16 = if !allow_future && date == today {
            u16::try_from(now.hour() * 60 + now.minute()).unwrap_or(1439)
        } else {
            1439
        };
        let cap = (raw_cap / 3) * 3;

        let times = Self::build_time_options(cap);

        Ok(Select::new(prompt, times).prompt()?)
    }

    /// Builds the list of selectable time strings from `00:00` up to and including `cap_minutes`.
    ///
    /// `cap_minutes` must already be snapped to a 3-minute boundary by the caller; this function
    /// only filters and formats.
    ///
    fn build_time_options(cap_minutes: u16) -> Vec<String> {
        (0u16..1440)
            .step_by(3)
            .filter(|&m| m <= cap_minutes)
            .map(|m| format!("{:02}:{:02}", m / 60, m % 60))
            .collect()
    }

    /// Combines `date` and `time_str` (HH:MM) into a UTC timestamp using the local timezone.
    ///
    fn combine_date_time(date: NaiveDate, time_str: &str) -> Result<DateTime<Utc>> {
        let mut parts = time_str.split(':');
        let hours: u32 = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("invalid time format: {time_str}"))?;
        let minutes: u32 = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("invalid time format: {time_str}"))?;

        let t = NaiveTime::from_hms_opt(hours, minutes, 0)
            .ok_or_else(|| anyhow!("invalid time: {hours:02}:{minutes:02}"))?;

        NaiveDateTime::new(date, t)
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| anyhow!("ambiguous or invalid local datetime: {date} {time_str}"))
            .map(|dt| dt.with_timezone(&Utc))
    }
}

// -------------------------------------------------------------------------------------------------
// END OF LOGIC - MODULE UNIT TESTS BELOW HERE
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_time_options_full_day() {
        let times = CommandAdd::build_time_options(1437);
        assert_eq!(times.len(), 480);
        assert_eq!(times.first().map(String::as_str), Some("00:00"));
        assert_eq!(times.last().map(String::as_str), Some("23:57"));
    }

    #[test]
    fn build_time_options_past_date_snaps_to_1437() {
        // cap 1439 floors to (1439/3)*3 = 1437, giving the same full grid as allow_future
        let cap = (1439u16 / 3) * 3;
        let times = CommandAdd::build_time_options(cap);
        assert_eq!(times.len(), 480);
        assert_eq!(times.last().map(String::as_str), Some("23:57"));
    }

    #[test]
    fn build_time_options_now_on_3min_boundary() {
        // now = 14:06 → raw_cap = 846, snaps to (846/3)*3 = 846; last entry must be "14:06"
        let cap = (846u16 / 3) * 3;
        let times = CommandAdd::build_time_options(cap);
        assert_eq!(times.last().map(String::as_str), Some("14:06"));
        assert!(!times.contains(&"14:09".to_string()));
    }

    #[test]
    fn build_time_options_now_between_boundaries() {
        // now = 14:07 → raw_cap = 847, snaps to (847/3)*3 = 846; last entry must be "14:06"
        let cap = (847u16 / 3) * 3;
        let times = CommandAdd::build_time_options(cap);
        assert_eq!(times.last().map(String::as_str), Some("14:06"));
        assert!(!times.contains(&"14:09".to_string()));
    }

    #[test]
    fn build_time_options_midnight() {
        // now = 00:00 → raw_cap = 0, snaps to 0; only "00:00" in the list
        let cap = 0u16;
        let times = CommandAdd::build_time_options(cap);
        assert_eq!(times.len(), 1);
        assert_eq!(times[0], "00:00");
    }

    #[test]
    fn parse_date_valid_past() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let past = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let result = CommandAdd::parse_date("2026-06-01", "--start-date", today, false);
        assert_eq!(result.unwrap(), past);
    }

    #[test]
    fn parse_date_rejects_future_without_flag() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let err = CommandAdd::parse_date("2099-01-01", "--start-date", today, false).unwrap_err();
        assert!(err.to_string().contains("--start-date"));
        assert!(err.to_string().contains("--future"));
    }

    #[test]
    fn parse_date_accepts_future_with_flag() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let result = CommandAdd::parse_date("2099-01-01", "--start-date", today, true);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_date_rejects_malformed_string() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let err = CommandAdd::parse_date("27/06/2026", "--start-date", today, false).unwrap_err();
        assert!(err.to_string().contains("expected YYYY-MM-DD"));
    }

    #[test]
    fn parse_date_rejects_invalid_month() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let err = CommandAdd::parse_date("2026-13-01", "--start-date", today, false).unwrap_err();
        assert!(err.to_string().contains("expected YYYY-MM-DD"));
    }

    #[test]
    fn parse_time_flag_valid_time() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let now: DateTime<Local> = Local::now();
        let result = CommandAdd::parse_time_flag("09:00", "--start-time", date, now, false);
        assert_eq!(result.unwrap(), "09:00");
    }

    #[test]
    fn parse_time_flag_normalises_output() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let now: DateTime<Local> = Local::now();
        let result = CommandAdd::parse_time_flag("9:00", "--start-time", date, now, false);
        assert_eq!(result.unwrap(), "09:00");
    }

    #[test]
    fn parse_time_flag_accepts_off_boundary() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let now: DateTime<Local> = Local::now();
        let result = CommandAdd::parse_time_flag("14:07", "--start-time", date, now, false);
        assert_eq!(result.unwrap(), "14:07");
    }

    #[test]
    fn parse_time_flag_rejects_malformed() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let now: DateTime<Local> = Local::now();
        let err = CommandAdd::parse_time_flag("9am", "--start-time", date, now, false).unwrap_err();
        assert!(err.to_string().contains("expected HH:MM"));
    }

    #[test]
    fn parse_time_flag_rejects_invalid_hour() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let now: DateTime<Local> = Local::now();
        let err =
            CommandAdd::parse_time_flag("25:00", "--start-time", date, now, false).unwrap_err();
        assert!(err.to_string().contains("expected HH:MM"));
    }

    #[test]
    fn parse_time_flag_rejects_future_on_today_without_flag() {
        // Use a fixed time of 09:00 today; 23:59 should be rejected.
        let today = Local::now().date_naive();
        let now = today
            .and_hms_opt(9, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap();
        let err =
            CommandAdd::parse_time_flag("23:59", "--stop-time", today, now, false).unwrap_err();
        assert!(err.to_string().contains("--future"));
    }

    #[test]
    fn parse_time_flag_accepts_future_on_today_with_flag() {
        let today = Local::now().date_naive();
        let now = today
            .and_hms_opt(9, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap();
        let result = CommandAdd::parse_time_flag("23:59", "--stop-time", today, now, true);
        assert_eq!(result.unwrap(), "23:59");
    }

    #[test]
    fn parse_time_flag_allows_future_time_on_past_date() {
        // "Future" time on a past date is fine even without --future.
        let past_date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let now: DateTime<Local> = Local::now();
        let result = CommandAdd::parse_time_flag("23:59", "--stop-time", past_date, now, false);
        assert!(result.is_ok());
    }
}
