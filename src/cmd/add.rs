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

        let start_date = if self.future {
            DateSelect::new("Start date?").prompt()?
        } else {
            DateSelect::new("Start date?")
                .with_max_date(today)
                .prompt()?
        };

        let start_time = Self::select_time("Start time?", start_date, now_local, self.future)?;

        let stop_date = if self.future {
            DateSelect::new("Stop date? ").prompt()?
        } else {
            DateSelect::new("Stop date? ")
                .with_max_date(today)
                .prompt()?
        };

        let stop_time = Self::select_time("Stop time? ", stop_date, now_local, self.future)?;

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

    /// Returns a list of selectable times for a given date, filtered to exclude future times when
    /// the date is today and `allow_future` is false.
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
// MODULE UNIT TESTS BELOW HERE
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
}
