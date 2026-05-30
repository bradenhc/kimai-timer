// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt new` subcommand for creating a new task alias.
//!
//! Task names must be globally unique and follow the alphanumeric-plus-dash naming rules enforced
//! by the store.

use anyhow::{Result, bail};
use clap::Parser;

use crate::store::Store;

/// Arguments for the `kt new` subcommand.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandNew {
    /// The name of the new task alias to create. Command will fail if the task already exists.
    task: String,
}

impl CommandNew {
    /// Validates and persists the new task alias, failing if the name is already in use.
    ///
    pub fn execute(self) -> Result<()> {
        if self.task.is_empty() {
            bail!("task name cannot be empty");
        }

        let store = Store::new()?;

        let tasks = store.get_tasks()?;

        if tasks.contains(&self.task) {
            bail!("task {} already exists", self.task);
        }

        store.add_task(self.task.clone())?;

        Ok(())
    }
}
