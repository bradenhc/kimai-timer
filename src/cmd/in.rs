// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt in` subcommand for punching in to a project.
//!
//! If no project is specified, resumes the one the timer last stopped on. If a different project is
//! already active, the store closes it in the same append that opens the new one, so a switch can
//! never be left half-done. Uses fuzzy search to suggest similar names when an unrecognized project
//! is provided.

use anyhow::{Result, bail};
use clap::Parser;
use colored::Colorize;
use simsearch::SimSearch;

use crate::store::{ProjectId, Store};

/// Arguments for the `kt in` subcommand.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT_ARG, styles = crate::STYLES)]
pub(crate) struct CommandIn {
    /// The project to punch in to. When not provided the last project will be used.
    project: Option<String>,
}

impl CommandIn {
    /// Constructs a `CommandIn` targeting `project` directly, bypassing interactive prompts.
    ///
    /// Used by [`crate::cmd::CommandSwitch`] to reuse the punch-in logic.
    ///
    pub(crate) fn for_project(project: impl Into<String>) -> Self {
        Self {
            project: Some(project.into()),
        }
    }

    /// Punches in to the resolved project, auto-punching out of any active one first if needed.
    ///
    pub(crate) fn execute(self, store: &mut Store) -> Result<()> {
        match self.project {
            None => Self::resume_last(store),
            Some(name) => Self::punch_in(store, &name),
        }
    }

    /// Punches in to the project the timer last stopped on.
    ///
    fn resume_last(store: &mut Store) -> Result<()> {
        if let Some(current) = Self::current_name(store) {
            println!("Already punched in to {}", current.green());
            return Ok(());
        }

        let Some(last) = store.last_project() else {
            bail!("no previous project to punch in to");
        };

        let id = last.id.clone();
        let name = last.name.to_string();

        store.start_session(id)?;

        println!("Punched in to {}", name.green());

        Ok(())
    }

    /// Punches in to a named project, closing any session already open.
    ///
    fn punch_in(store: &mut Store, name: &str) -> Result<()> {
        if Self::current_name(store).is_some_and(|current| current == name) {
            println!("Already punched in to {}", name.green());
            return Ok(());
        }

        let Some(id) = store.projects().get_by_name(name).map(|p| p.id.clone()) else {
            bail!(
                "project does not exist: {name}{}",
                Self::suggestions(store, name)
            );
        };

        if let Some(closed) = store.start_session(id)? {
            let previous = Self::name_of(store, &closed.project_id);
            println!("Punched out of {}", previous.bold());
        }

        println!("Punched in to {}", name.green());

        Ok(())
    }

    /// Returns the name of the project currently being worked, if the timer is running.
    ///
    fn current_name(store: &Store) -> Option<String> {
        let session = store.session()?;
        Some(Self::name_of(store, &session.project_id))
    }

    /// Resolves a project name for display, degrading rather than failing on an unknown reference.
    ///
    fn name_of(store: &Store, id: &ProjectId) -> String {
        store
            .project(id)
            .map_or_else(|| String::from("<unknown>"), |p| p.name.to_string())
    }

    /// Builds a "similar projects" hint for an unrecognized name.
    ///
    /// Rather than failing outright on a typo, we fuzzy-search the known projects so the error can
    /// point at what the user probably meant.
    ///
    fn suggestions(store: &Store, name: &str) -> String {
        let known: Vec<&str> = store.projects().list().map(|p| p.name.as_str()).collect();

        let mut engine = SimSearch::new();
        for (i, candidate) in known.iter().enumerate() {
            engine.insert(i, candidate);
        }

        engine
            .search(name)
            .into_iter()
            .map(|i| known[i])
            .fold(String::new(), |acc, cur| {
                if acc.is_empty() {
                    format!(": similar projects: {cur}")
                } else {
                    format!("{acc}, {cur}")
                }
            })
    }
}
