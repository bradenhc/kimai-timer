// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use crate::store::Store;

#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandList {}

impl CommandList {
    #[allow(clippy::unused_self)]
    pub fn execute(self) -> Result<()> {
        let store = Store::new()?;

        let tasks = store.get_tasks()?;

        if tasks.is_empty() {
            println!("Task set is empty");
        } else {
            let current_task = store.get_current_task()?;
            let last_task = store.get_last_task()?;

            for t in tasks {
                if current_task.as_ref().is_some_and(|c| *c == t) {
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
