// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt list` subcommand for displaying all known task aliases.
//!
//! The currently active task is marked with `*` (green), and the last completed task is marked
//! with `-`, giving a quick visual overview of task state at a glance.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use crate::store::Store;

/// Arguments for the `kt list` subcommand (none currently required).
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandList {}

impl CommandList {
    /// Prints all tasks, marking the active one with `*` and the last completed one with `-`.
    ///
    #[allow(clippy::unused_self)]
    pub fn execute(self, store: &Store) -> Result<()> {
        let tasks = store.get_tasks()?;

        if tasks.is_empty() {
            println!("Task set is empty");
        } else {
            let current_task = store.get_current_task()?;
            let last_task = store.get_last_task()?;

            for t in tasks {
                if current_task.as_ref().is_some_and(|c| c.task == t) {
                    println!("* {}", t.bold().green());
                } else if last_task.as_ref().is_some_and(|l| *l == t) {
                    println!("- {}", t.bold());
                } else {
                    println!("  {t}");
                }
            }
        }

        Ok(())
    }
}
