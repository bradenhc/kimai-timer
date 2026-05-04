// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

use anyhow::{Result, bail};
use clap::Parser;

use crate::store::Store;

#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandNew {
    /// The name of the new task alias to create. Command will fail if the task already exists.
    task: String,
}

impl CommandNew {
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
