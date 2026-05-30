// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt out` subcommand for punching out of the current task.
//!
//! Records a completed [`crate::store::TimeInterval`] to the timelog, clears the current task, and
//! saves the task name as the last completed task so `kt in` can resume it with no argument.

use anyhow::{Result, anyhow};
use clap::Parser;
use colored::Colorize;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

use crate::store::{Store, TimeInterval};

/// Arguments for the `kt out` subcommand (none currently required).
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
pub struct CommandOut {}

impl CommandOut {
    /// Punches out of the current task, recording a completed interval to the timelog.
    ///
    #[allow(clippy::unused_self)]
    pub fn execute(self) -> Result<()> {
        let store = Store::new()?;

        match store.get_current_task()? {
            None => {
                println!("No current task");
            }

            Some(current) => {
                let start = OffsetDateTime::from_unix_timestamp(current.start)
                    .map_err(|e| anyhow!("invalid start timestamp in current task: {e}"))?;
                let end = OffsetDateTime::now_utc().truncate_to_second();

                let interval = TimeInterval::new(&current.task, start, end);
                store.append_interval(interval)?;
                store.set_last_task(&current.task)?;
                store.clear_current_task()?;

                let offset = UtcOffset::current_local_offset().unwrap();
                let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
                let local_start = start.to_offset(offset).format(fmt).unwrap();
                let local_end = end.to_offset(offset).format(fmt).unwrap();

                println!(
                    "Punched out of {}: {local_start} - {local_end}",
                    current.task.green()
                );
            }
        }

        Ok(())
    }
}
