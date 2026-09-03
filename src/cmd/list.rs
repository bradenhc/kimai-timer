// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt list` subcommand for displaying all known projects.
//!
//! The project currently being worked is marked with `*` (green), and the project the timer last
//! stopped on is marked with `-`, giving a quick visual overview of timer state at a glance.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use crate::store::Store;

/// Arguments for the `kt list` subcommand (none currently required).
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub(crate) struct CommandList {}

impl CommandList {
    /// Prints all projects, marking the active one with `*` and the last one with `-`.
    ///
    /// Returns `Result` even though nothing here can fail, so that every command shares one
    /// signature and `main` can dispatch to them uniformly.
    ///
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    pub(crate) fn execute(self, store: &Store) -> Result<()> {
        if store.projects().is_empty() {
            println!("Project set is empty");
            return Ok(());
        }

        let current = store.session().map(|s| s.project_id.clone());
        let last = store.last_project().map(|p| p.id.clone());

        for project in store.projects().list() {
            let name = project.name.as_str();

            if current.as_ref() == Some(&project.id) {
                println!("* {}", name.bold().green());
            } else if last.as_ref() == Some(&project.id) {
                println!("- {}", name.bold());
            } else {
                println!("  {name}");
            }
        }

        Ok(())
    }
}
