// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

use anyhow::{Result, anyhow, bail};
use clap::Parser;
use colored::Colorize;
use simsearch::SimSearch;

use crate::cmd::CommandOut;
use crate::store::{Store, TimerEvent};

#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandIn {
    /// The task to punch in to. When not provided the last punched task will be used.
    task: Option<String>,
}

impl CommandIn {
    pub fn for_task(task: impl Into<String>) -> Self {
        Self {
            task: Some(task.into()),
        }
    }

    pub fn execute(self) -> Result<()> {
        let store = Store::new()?;

        let tasks = store.get_tasks()?;
        let current_task = store.get_current_task()?;

        let task = match self.task {
            None => {
                if let Some(c) = current_task {
                    println!("Already punched in to {}", c.green());
                    return Ok(());
                }

                store
                    .get_last_task()?
                    .ok_or_else(|| anyhow!("missing task and no last task is set"))?
            }

            Some(task) => {
                if !tasks.contains(&task) {
                    // Help the user out be suggesting close matches to what they typed
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
                    if c == task {
                        println!("Already punched in to {task}");
                        return Ok(());
                    }

                    // Switching tasks: punch out of the current one
                    CommandOut {}.execute()?;
                }
                task
            }
        };

        let event = TimerEvent::start(&task);

        store.add_task(&task)?;
        store.append_timer_event(event.clone())?;
        store.set_current_task(&task)?;

        println!("{event}");

        Ok(())
    }
}
