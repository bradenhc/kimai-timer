// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt switch` subcommand for toggling between the current and last projects.
//!
//! Punches in to the project the timer last stopped on, which implicitly punches out of the active
//! one via [`CommandIn`].

use anyhow::{Result, bail};
use clap::Parser;

use crate::cmd::CommandIn;
use crate::store::Store;

/// Arguments for the `kt switch` subcommand (none currently required).
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub(crate) struct CommandSwitch {}

impl CommandSwitch {
    /// Punches in to the last project, which implicitly punches out of the current one.
    ///
    #[allow(clippy::unused_self)]
    pub(crate) fn execute(self, store: &mut Store) -> Result<()> {
        if store.session().is_none() {
            bail!("no current project to switch from");
        }

        let Some(last) = store.last_project() else {
            bail!("no last project to switch to");
        };

        CommandIn::for_project(last.name.to_string()).execute(store)
    }
}
