// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Resolves where the store lives on disk.
//!
//! The store is a single append-only log file, so this module is deliberately small: it answers
//! "which directory" ([`StoreRoot`]), "which path" ([`store_path`]), and "make the directory
//! exist" ([`ensure_parent_dir`]). Path resolution is free of side effects so that read-only
//! commands leave no trace on a machine that has never recorded any time; the directory is
//! created lazily by the first write instead.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use directories::ProjectDirs;

/// Indicates whether the store is located at the default path or at a custom user-defined path.
///
pub(crate) enum StoreRoot {
    /// Derived from the platform data directory for the current user.
    Derived,

    /// Pinned by the user via `--data-dir` or `KT_DATA_DIR`.
    Specified(PathBuf),
}

impl StoreRoot {
    /// Uses the platform-appropriate data directory for the current user.
    ///
    pub(crate) fn derived() -> Self {
        Self::Derived
    }

    /// Uses the directory the user pinned explicitly.
    ///
    pub(crate) fn specified(p: impl Into<PathBuf>) -> Self {
        Self::Specified(p.into())
    }
}

/// Constructs an absolute path to a file with the provided name in the store.
///
/// Resolving a path never touches the filesystem, so commands that only read leave nothing behind
/// on a machine where the store does not exist yet. Callers that are about to write must call
/// [`ensure_parent_dir`] first.
///
pub(crate) fn store_path(file_name: &str, root: &StoreRoot) -> Result<PathBuf> {
    let data_dir: PathBuf = match root {
        StoreRoot::Derived => {
            let project_dirs = ProjectDirs::from("codes", "hitchcock", "kimai-timer")
                .ok_or_else(|| anyhow!("failed to derive project directory path"))?;

            project_dirs.data_dir().into()
        }

        StoreRoot::Specified(r) => r.clone(),
    };

    Ok(data_dir.join(file_name))
}

/// Creates the directory containing `p` if it does not already exist.
///
/// Called immediately before the first write to the log so that the store directory springs into
/// existence only when there is something to put in it.
///
pub(crate) fn ensure_parent_dir(p: &Path) -> Result<()> {
    let Some(parent) = p.parent() else {
        return Ok(());
    };

    if parent.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(parent).map_err(|e| {
        anyhow!(
            "failed to create store directory: {}: {e}",
            parent.display()
        )
    })
}
