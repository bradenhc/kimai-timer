// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt out` subcommand for punching out of the current project.
//!
//! Stopping the timer is a single append to the event log, which is what makes the interval it
//! records and the fact that the session ended impossible to observe separately.

use anyhow::Result;
use chrono::Local;
use clap::Parser;
use colored::Colorize;

use crate::store::Store;

/// Arguments for the `kt out` subcommand (none currently required).
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
pub(crate) struct CommandOut;

impl CommandOut {
    /// Punches out of the current project, recording the completed interval.
    ///
    #[allow(clippy::unused_self)]
    pub(crate) fn execute(self, store: &mut Store) -> Result<()> {
        if store.session().is_none() {
            println!("No current project");
            return Ok(());
        }

        let interval = store.stop_session()?;

        let name = store
            .project(&interval.project_id)
            .map_or_else(|| String::from("<unknown>"), |p| p.name.to_string());

        let fmt = "%Y-%m-%d %H:%M:%S";
        let local_start = interval.start.with_timezone(&Local).format(fmt);
        let local_end = interval.end.with_timezone(&Local).format(fmt);

        println!(
            "Punched out of {}: {local_start} - {local_end}",
            name.green()
        );

        Ok(())
    }
}
