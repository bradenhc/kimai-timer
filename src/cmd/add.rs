// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt add` subcommand for manually inserting time intervals.
//!
//! Used primarily to amend missed or incorrectly tracked sessions. The command prompts for a task,
//! start date/time, and stop date/time, then appends the resulting interval to the timelog.

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
}

impl CommandAdd {
    /// Runs the add flow: selects a task, prompts for start/stop date+time, and saves the interval.
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

        let start_date = DateSelect::new("Start date?").prompt()?;
        let start_time = Self::select_time("Start time?")?;
        let stop_date = DateSelect::new("Stop date? ").prompt()?;
        let stop_time = Self::select_time("Stop time? ")?;

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

    /// Displays a selection list of times in 6-minute increments and returns the chosen `HH:MM` string.
    ///
    fn select_time(prompt: &str) -> Result<String> {
        let times = (0..1440)
            .step_by(6)
            .map(|m| format!("{:02}:{:02}", m / 60, m % 60))
            .collect();

        let start_time = Select::new(prompt, times).prompt()?;
        Ok(start_time)
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
