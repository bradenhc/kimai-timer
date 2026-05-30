// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt switch` subcommand for toggling between the current and last tasks.
//!
//! Punches in to the previously completed task, which implicitly punches out of the active one
//! via [`CommandIn`].

use anyhow::{Result, bail};
use clap::Parser;

use crate::cmd::CommandIn;
use crate::store::Store;

/// Arguments for the `kt switch` subcommand (none currently required).
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub struct CommandSwitch {}

impl CommandSwitch {
    /// Punches in to the last task, which implicitly punches out of the current one.
    ///
    #[allow(clippy::unused_self)]
    pub fn execute(self) -> Result<()> {
        let store = Store::new()?;

        let current_task = store.get_current_task()?;
        if current_task.is_none() {
            bail!("no current task to switch from");
        }

        let last_task = store.get_last_task()?;
        if let Some(last) = last_task {
            CommandIn::for_task(last).execute()
        } else {
            bail!("no last task to switch to");
        }
    }
}
