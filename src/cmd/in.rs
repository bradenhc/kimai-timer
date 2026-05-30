// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt in` subcommand for punching in to a task.
//!
//! If no task is specified, resumes the last punched-out task. If a different task is already
//! active, it automatically punches out first. Uses fuzzy search to suggest similar task names
//! when an unrecognized task is provided.

use anyhow::{Result, anyhow, bail};
use clap::Parser;
use colored::Colorize;
use simsearch::SimSearch;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

use crate::cmd::CommandOut;
use crate::store::Store;

/// Arguments for the `kt in` subcommand.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandIn {
    /// The task to punch in to. When not provided the last punched task will be used.
    task: Option<String>,
}

impl CommandIn {
    /// Constructs a `CommandIn` targeting `task` directly, bypassing interactive prompts.
    ///
    /// Used by [`crate::cmd::CommandSwitch`] to reuse the punch-in logic programmatically.
    pub fn for_task(task: impl Into<String>) -> Self {
        Self {
            task: Some(task.into()),
        }
    }

    /// Punches in to the resolved task, auto-punching out of any active task first if needed.
    ///
    pub fn execute(self) -> Result<()> {
        let store = Store::new()?;

        let tasks = store.get_tasks()?;
        let current_task = store.get_current_task()?;

        let task = match self.task {
            None => {
                if let Some(c) = current_task {
                    println!("Already punched in to {}", c.task.green());
                    return Ok(());
                }

                store
                    .get_last_task()?
                    .ok_or_else(|| anyhow!("missing task and no last task is set"))?
            }

            Some(task) => {
                if !tasks.contains(&task) {
                    let mut engine = SimSearch::new();
                    let indexed_tasks: Vec<_> = tasks.into_iter().collect();
                    for (i, t) in indexed_tasks.iter().enumerate() {
                        engine.insert(i, t);
                    }
                    let results = engine.search(&task);
                    let similar = results.into_iter().map(|i| indexed_tasks[i].clone()).fold(
                        String::new(),
                        |acc, cur| {
                            if acc.is_empty() {
                                format!(": similar tasks: {cur}")
                            } else {
                                format!("{acc}, {cur}")
                            }
                        },
                    );

                    bail!("task does not exist: {task}{similar}");
                }

                if let Some(c) = current_task {
                    if c.task == task {
                        println!("Already punched in to {task}");
                        return Ok(());
                    }

                    // Switching tasks: punch out of the current one
                    CommandOut {}.execute()?;
                }
                task
            }
        };

        let start = OffsetDateTime::now_utc().truncate_to_second();
        let start_ts = start.unix_timestamp();

        store.set_current_task(&task, start_ts)?;

        let local_time = start
            .to_offset(UtcOffset::current_local_offset().unwrap())
            .format(&format_description!(
                "[year]-[month]-[day] [hour]:[minute]:[second]"
            ))
            .unwrap();

        println!("Punched in to {} at {local_time}", task.green());

        Ok(())
    }
}
