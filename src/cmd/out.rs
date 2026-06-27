// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt out` subcommand for punching out of the current task.
//!
//! Records a completed [`crate::store::TimeInterval`] to the timelog, clears the current task, and
//! saves the task name as the last completed task so `kt in` can resume it with no argument.

use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, Utc};
use clap::Parser;
use colored::Colorize;

use crate::store::{Store, TimeInterval};
use crate::time_ext::DateTimeExt;

/// Arguments for the `kt out` subcommand (none currently required).
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
pub struct CommandOut {}

impl CommandOut {
    /// Punches out of the current task, recording a completed interval to the timelog.
    ///
    #[allow(clippy::unused_self)]
    pub fn execute(self, store: &Store) -> Result<()> {
        match store.get_current_task()? {
            None => {
                println!("No current task");
            }

            Some(current) => {
                let start = DateTime::from_timestamp(current.start, 0)
                    .ok_or_else(|| anyhow!("invalid start timestamp in current task"))?;
                let end = Utc::now().truncate_to_second();

                let interval = TimeInterval::new(&current.task, start, end);
                store.append_interval(interval)?;
                store.set_last_task(&current.task)?;
                store.clear_current_task()?;

                let fmt = "%Y-%m-%d %H:%M:%S";
                let local_start = start.with_timezone(&Local).format(fmt);
                let local_end = end.with_timezone(&Local).format(fmt);

                println!(
                    "Punched out of {}: {local_start} - {local_end}",
                    current.task.green()
                );
            }
        }

        Ok(())
    }
}
