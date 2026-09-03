// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines projects and the in-memory index used to look them up.
//!
//! A [`Project`] pairs a stable [`ProjectId`] with the [`ProjectName`] the user types. Keeping the
//! two separate is what allows a project to be renamed, or linked to a remote contract, without
//! rewriting any history: events reference the ID, never the name.
//!
//! [`ProjectSet`] is rebuilt from the event log every time the store is opened and is never
//! persisted on its own. It indexes projects both ways - by ID for resolving references out of the
//! log, and by name for the lookups the CLI performs - and the name index is what lets it enforce
//! the "names are unique" rule the application depends on.

use core::fmt::Display;
use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Information about a single project a user is working on.
///
/// Projects are uniquely identified locally by their name and globally by their UUID. The global
/// identifier allows project information to be exported and imported when moving data across
/// machines, so a diffing algorithm can detect name collisions from different sources and prompt
/// the user for action.
///
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Project {
    /// A UUID that uniquely identifies the project.
    pub(crate) id: ProjectId,

    /// The name of the project.
    pub(crate) name: ProjectName,
}

impl Display for Project {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// The UUID for a project.
///
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct ProjectId(String);

impl ProjectId {
    /// Mints a fresh project ID.
    ///
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4().into())
    }
}

impl Display for ProjectId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated project name.
///
/// Project names must start with an alphabetic character and only contain alphanumeric
/// characters, dashes, and slashes. Slashes are permitted so that names can mirror the hierarchy
/// a remote backend may use.
///
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct ProjectName(String);

impl ProjectName {
    /// Creates a new validated project name.
    ///
    /// See the struct-level docs for validation rules.
    ///
    pub(crate) fn new(n: impl Into<String>) -> Result<Self> {
        let name = n.into();
        let mut char_iter = name.chars();

        match char_iter.next() {
            None => bail!("empty project name"),

            Some(c) => {
                if !c.is_ascii_alphabetic() {
                    bail!("the first letter of a project name must be ASCII alphabetic")
                }
            }
        }

        if char_iter.any(|c| !(c.is_alphanumeric() || c == '-' || c == '/')) {
            bail!("project names must only include alphanumeric, dash, and slash characters");
        }

        Ok(Self(name))
    }

    /// Returns the inner name value as a string slice.
    ///
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ProjectName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Allows the name index to be queried with a plain string slice.
///
/// The derived ordering on the wrapper matches the inner string's, which is what makes borrowing
/// as a `str` sound here and keeps name lookups logarithmic rather than a scan.
///
impl core::borrow::Borrow<str> for ProjectName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// An index of every known project, keyed both ways.
///
/// Holding a name index alongside the ID index is what makes uniqueness enforceable: a set keyed
/// only by the whole project would happily accept two projects sharing a name but differing in
/// ID, which is indistinguishable from a duplicate to the person typing the name.
///
#[derive(Debug, Default)]
pub(crate) struct ProjectSet {
    by_id: BTreeMap<ProjectId, Project>,

    by_name: BTreeMap<ProjectName, ProjectId>,
}

impl ProjectSet {
    /// Adds a project to the index.
    ///
    /// Returns an error if a project with the same name is already present.
    ///
    pub(crate) fn insert(&mut self, p: Project) -> Result<()> {
        if self.by_name.contains_key(&p.name) {
            bail!("project '{p}' already exists");
        }

        self.by_name.insert(p.name.clone(), p.id.clone());
        self.by_id.insert(p.id.clone(), p);

        Ok(())
    }

    /// Returns the project with the provided ID, if one is known.
    ///
    pub(crate) fn get_by_id(&self, id: &ProjectId) -> Option<&Project> {
        self.by_id.get(id)
    }

    /// Returns the project with the provided name, if one is known.
    ///
    pub(crate) fn get_by_name(&self, name: &str) -> Option<&Project> {
        self.by_name.get(name).and_then(|id| self.by_id.get(id))
    }

    /// Returns true if a project with the provided name is known.
    ///
    pub(crate) fn contains_name(&self, name: &str) -> bool {
        self.get_by_name(name).is_some()
    }

    /// Returns every known project in name order.
    ///
    /// Ordering comes free from the name index, so callers never need to sort.
    ///
    pub(crate) fn list(&self) -> impl Iterator<Item = &Project> {
        self.by_name.values().filter_map(|id| self.by_id.get(id))
    }

    /// Returns true when no projects have been created yet.
    ///
    pub(crate) fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}
