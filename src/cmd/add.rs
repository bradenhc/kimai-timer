// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt add` subcommand for manually inserting time intervals.
//!
//! Used primarily to amend missed or incorrectly tracked sessions. The command prompts for a task,
//! start date/time, and stop date/time, then appends the resulting interval to the timelog. By
//! default, future dates and times are restricted so accidental future entries cannot be created;
//! pass `--future` to bypass this when pre-entering known events like PTO or holidays.

use anyhow::{Result, anyhow, bail};
use clap::Parser;
use colored::Colorize;
use inquire::{DateSelect, Select};
use time::macros::format_description;
use time::{Date, OffsetDateTime, Time, UtcOffset};

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

        let offset = UtcOffset::current_local_offset().unwrap();
        let now_local = OffsetDateTime::now_local().unwrap();

        let today = chrono::NaiveDate::from_ymd_opt(
            now_local.year(),
            now_local.month() as u32,
            u32::from(now_local.day()),
        )
        .expect("valid local date");

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

        let start = Self::combine_date_time(
            &start_date.format("%Y-%m-%d").to_string(),
            &start_time,
            offset,
        )?;
        let stop = Self::combine_date_time(
            &stop_date.format("%Y-%m-%d").to_string(),
            &stop_time,
            offset,
        )?;

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
    /// Times are in 6-minute increments. When today is selected without `--future`, the list is
    /// capped at the current minute so the user cannot enter a time that has not yet occurred.
    ///
    fn select_time(
        prompt: &str,
        date: chrono::NaiveDate,
        now: OffsetDateTime,
        allow_future: bool,
    ) -> Result<String> {
        let today =
            chrono::NaiveDate::from_ymd_opt(now.year(), now.month() as u32, u32::from(now.day()))
                .expect("valid local date");

        let cap: u16 = if !allow_future && date == today {
            u16::from(now.hour()) * 60 + u16::from(now.minute())
        } else {
            1439
        };

        let times: Vec<String> = (0u16..1440)
            .step_by(6)
            .filter(|&m| m <= cap)
            .map(|m| format!("{:02}:{:02}", m / 60, m % 60))
            .collect();

        Ok(Select::new(prompt, times).prompt()?)
    }

    /// Parses `date_str` and `time_str` into an `OffsetDateTime` at `offset`.
    ///
    fn combine_date_time(
        date_str: &str,
        time_str: &str,
        offset: UtcOffset,
    ) -> Result<OffsetDateTime> {
        let date = Date::parse(date_str, &format_description!("[year]-[month]-[day]"))
            .map_err(|e| anyhow!("invalid date '{date_str}': {e}"))?;

        let mut parts = time_str.split(':');
        let hours: u8 = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("invalid time format: {time_str}"))?;
        let minutes: u8 = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("invalid time format: {time_str}"))?;

        let t = Time::from_hms(hours, minutes, 0).map_err(|e| anyhow!("invalid time: {e}"))?;

        Ok(OffsetDateTime::new_in_offset(date, t, offset))
    }
}
