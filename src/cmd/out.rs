// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

use anyhow::Result;
use clap::Parser;

use crate::store::{Store, TimerEvent};

#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
pub struct CommandOut {}

impl CommandOut {
    #[allow(clippy::unused_self)]
    pub fn execute(self) -> Result<()> {
        let store = Store::new()?;

        match store.get_current_task()? {
            None => {
                println!("No current task");
            }

            Some(task) => {
                let event = TimerEvent::stop(&task);

                store.append_timer_event(event.clone())?;
                store.set_last_task(&task)?;
                store.clear_current_task()?;

                println!("{event}");
            }
        }

        Ok(())
    }
}
