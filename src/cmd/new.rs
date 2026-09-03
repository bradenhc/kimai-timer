// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt new` subcommand for creating a new project.
//!
//! Project names must be unique and follow the naming rules enforced by [`ProjectName`]. Both
//! constraints live in the store, so this command only has to hand the name over.

use anyhow::Result;
use clap::Parser;

use crate::store::{ProjectName, Store};

/// Arguments for the `kt new` subcommand.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub(crate) struct CommandNew {
    /// The name of the new project to create. Fails if the project already exists.
    project: String,
}

impl CommandNew {
    /// Validates and persists the new project, failing if the name is already in use.
    ///
    pub(crate) fn execute(self, store: &mut Store) -> Result<()> {
        store.add_project(ProjectName::new(self.project)?)?;

        Ok(())
    }
}
