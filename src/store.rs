// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines how to access the store of data tracked by Kimai Timer.
//!
//! The central type is [`Store`], which provides read/write access to each piece of persisted
//! state: the task set, the active task, the last completed task, and the append-only timelog.
//! [`StoreEvent`] and [`TimeInterval`] describe the data written to and read from the timelog.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_jsonlines::JsonLinesIter;
use time::{OffsetDateTime, UtcOffset};

/// Manages access to all persisted state for the Kimai Timer application.
///
pub struct Store {
    /// Path to the append-only JSONL event log file.
    timelog: PathBuf,

    /// Path to the JSON file storing the set of task names.
    taskset: PathBuf,

    /// Path to the file tracking the in-progress task.
    current_task: PathBuf,

    /// Path to the file storing the most recently completed task name.
    last_task: PathBuf,
}

impl Store {
    /// Creates a `Store` rooted at `data_dir`, creating it if it does not already exist.
    ///
    pub fn new(data_dir: &Path) -> Result<Self> {
        if !data_dir.exists() {
            std::fs::create_dir_all(data_dir).map_err(|e| {
                anyhow!(
                    "failed to create project data directory: {}: {e}",
                    data_dir.display()
                )
            })?;
        }

        Ok(Self {
            timelog: data_dir.join("timelog.jsonl"),
            taskset: data_dir.join("taskset.json"),
            current_task: data_dir.join("current"),
            last_task: data_dir.join("last"),
        })
    }

    /// Resolves the platform-appropriate data directory for this application and delegates to
    /// [`Store::new`].
    ///
    pub fn with_project_dir() -> Result<Self> {
        let pdirs = ProjectDirs::from("codes", "hitchcock", "kimai-timer")
            .ok_or_else(|| anyhow!("failed to derive project directory path"))?;
        Self::new(pdirs.data_dir())
    }

    /// Appends a `CreateInterval` event to the timelog.
    ///
    pub fn append_interval(&self, interval: TimeInterval) -> Result<()> {
        Self::touch_file(&self.timelog)?;

        let event = StoreEvent::CreateInterval(interval);
        serde_jsonlines::append_json_lines(&self.timelog, &[event])
            .map_err(|e| anyhow!("failed to write interval event to timelog: {e}"))?;
        Ok(())
    }

    /// Opens the timelog and returns a lazy iterator over stored events.
    ///
    pub fn fetch_events(&self) -> Result<PersistedEventIterator> {
        Self::touch_file(&self.timelog)?;

        let lines_iter = serde_jsonlines::json_lines(&self.timelog)
            .map_err(|e| anyhow!("failed to read timelog: {e}"))?;

        Ok(PersistedEventIterator { inner: lines_iter })
    }

    /// Validates the task name, adds it to the persisted task set, and writes the result to disk.
    ///
    /// Names must start with an ASCII letter and contain only alphanumerics or dashes.
    pub fn add_task(&self, task: impl Into<String>) -> Result<()> {
        let task = task.into();

        if !task.chars().next().unwrap().is_ascii_alphabetic()
            || task
                .chars()
                .any(|c| !(c.is_ascii_alphanumeric() || c == '-'))
        {
            bail!(
                "invalid task name: must start with letter and only contain alphanumerics or dashes"
            );
        }

        let mut tasks = self.get_tasks()?;

        tasks.insert(task);

        let contents = serde_json::to_string(&tasks)
            .map_err(|e| anyhow!("failed to save task to taskset: {e}"))?;

        Self::write_file(&self.taskset, &contents)
    }

    /// Reads and deserializes the full set of task names from disk.
    ///
    pub fn get_tasks(&self) -> Result<BTreeSet<String>> {
        let contents = Self::read_file(&self.taskset)?;

        let tasks = if contents.is_empty() {
            BTreeSet::new()
        } else {
            serde_json::from_str(&contents).map_err(|e| anyhow!("failed to parse taskset: {e}"))?
        };

        Ok(tasks)
    }

    /// Returns the in-progress task state, or `None` if no task is active.
    ///
    pub fn get_current_task(&self) -> Result<Option<CurrentTask>> {
        let contents = Self::read_file(&self.current_task)?;

        if contents.is_empty() {
            Ok(None)
        } else {
            let current = serde_json::from_str(&contents)
                .map_err(|e| anyhow!("failed to parse current task: {e}"))?;
            Ok(Some(current))
        }
    }

    /// Persists a new current task with its start timestamp.
    ///
    pub fn set_current_task(&self, task: &str, start: i64) -> Result<()> {
        let current = CurrentTask {
            task: task.to_string(),
            start,
        };
        let contents = serde_json::to_string(&current)
            .map_err(|e| anyhow!("failed to serialize current task: {e}"))?;
        Self::write_file(&self.current_task, &contents)
    }

    /// Clears the current task by writing an empty file, signaling that no task is active.
    ///
    pub fn clear_current_task(&self) -> Result<()> {
        Self::write_file(&self.current_task, "")
    }

    /// Returns the name of the most recently completed task, or `None` if none has been set.
    ///
    pub fn get_last_task(&self) -> Result<Option<String>> {
        let last = Self::read_file(&self.last_task)?;

        if last.is_empty() {
            Ok(None)
        } else {
            Ok(Some(last))
        }
    }

    /// Persists `task` as the last completed task so `kt in` can resume it with no argument.
    ///
    pub fn set_last_task(&self, task: &str) -> Result<()> {
        Self::write_file(&self.last_task, task)
    }

    /// Reads a file as a UTF-8 string, returning an empty string if the file does not yet exist.
    ///
    fn read_file(p: &Path) -> Result<String> {
        if !std::fs::exists(p)
            .map_err(|e| anyhow!("could not determine if file exists: {}: {e}", p.display()))?
        {
            return Ok(String::new());
        }

        std::fs::read_to_string(p).map_err(|e| anyhow!("failed to read file: {}: {e}", p.display()))
    }

    /// Writes `contents` to a file, creating or truncating it as necessary.
    ///
    fn write_file(p: &Path, contents: &str) -> Result<()> {
        std::fs::write(p, contents.as_bytes())
            .map_err(|e| anyhow!("failed to write file: {}: {e}", p.display()))
    }

    /// Creates `p` (and any missing parent directories) without truncating it if it already exists.
    ///
    fn touch_file(p: &Path) -> Result<()> {
        let exists = std::fs::exists(p)
            .map_err(|e| anyhow!("could not determine if file exists: {}: {e}", p.display()))?;

        if !exists {
            std::fs::create_dir_all(p.parent().unwrap())
                .map_err(|e| anyhow!("could not create store directory: {e}"))?;
        }

        let _ = File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .open(p)
            .map_err(|e| anyhow!("failed to touch store file: {}: {e}", p.display()))?;

        Ok(())
    }
}

/// A lazy iterator over [`StoreEvent`] records decoded from the timelog JSONL file.
///
pub struct PersistedEventIterator {
    inner: JsonLinesIter<BufReader<File>, StoreEvent>,
}

impl Iterator for PersistedEventIterator {
    type Item = Result<StoreEvent, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// An event stored in the append-only timelog.
///
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StoreEvent {
    /// Records the addition of a completed time interval.
    CreateInterval(TimeInterval),
}

/// An atomic unit of time spent on a task, with a definite start and end.
///
#[derive(Clone, Serialize, Deserialize)]
pub struct TimeInterval {
    /// The unique ID for the interval (allows deduplication and future modification).
    pub id: String,

    /// The time the interval was created.
    #[serde(with = "time::serde::timestamp")]
    pub created_at: OffsetDateTime,

    /// The time the interval was last updated; `None` if it has never been modified after creation.
    pub updated_at: Option<OffsetDateTime>,

    /// The name of the task to add the interval to.
    pub task: String,

    /// The start timestamp for the interval.
    #[serde(with = "time::serde::timestamp")]
    pub start: OffsetDateTime,

    /// The stop timestamp for the interval.
    #[serde(with = "time::serde::timestamp")]
    pub end: OffsetDateTime,
}

impl TimeInterval {
    /// Constructs a new interval with a fresh UUID and the current UTC time as `created_at`.
    ///
    pub fn new(task: impl Into<String>, start: OffsetDateTime, end: OffsetDateTime) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: OffsetDateTime::now_utc().truncate_to_second(),
            updated_at: None,
            task: task.into(),
            start,
            end,
        }
    }

    /// Returns a copy of this interval with all timestamps converted to `offset`.
    ///
    pub fn to_local(&self, offset: UtcOffset) -> Self {
        Self {
            id: self.id.clone(),
            created_at: self.created_at.to_offset(offset),
            updated_at: self.updated_at,
            task: self.task.clone(),
            start: self.start.to_offset(offset),
            end: self.end.to_offset(offset),
        }
    }
}

/// The state of the currently-running task, persisted to the `current` file as JSON.
///
#[derive(Serialize, Deserialize)]
pub struct CurrentTask {
    /// Name of the task being tracked.
    pub task: String,

    /// UNIX timestamp (seconds since epoch) of when the task was started.
    pub start: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn new_creates_directory_if_missing() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        assert!(!nested.exists());
        Store::new(&nested).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn add_and_get_tasks_roundtrip() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        store.add_task("my-task").unwrap();
        let tasks = store.get_tasks().unwrap();
        assert!(tasks.contains("my-task"));
    }

    #[test]
    fn set_and_get_current_task_roundtrip() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        store.set_current_task("my-task", 1_000_000).unwrap();
        let current = store.get_current_task().unwrap().unwrap();
        assert_eq!(current.task, "my-task");
        assert_eq!(current.start, 1_000_000);
    }

    #[test]
    fn clear_current_task_returns_none() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        store.set_current_task("my-task", 1_000_000).unwrap();
        store.clear_current_task().unwrap();
        assert!(store.get_current_task().unwrap().is_none());
    }

    #[test]
    fn set_and_get_last_task_roundtrip() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        store.set_last_task("my-task").unwrap();
        let last = store.get_last_task().unwrap().unwrap();
        assert_eq!(last, "my-task");
    }

    #[test]
    fn append_and_fetch_interval_roundtrip() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        let start = OffsetDateTime::from_unix_timestamp(1_000_000).unwrap();
        let end = OffsetDateTime::from_unix_timestamp(1_003_600).unwrap();
        let interval = TimeInterval::new("my-task", start, end);
        let id = interval.id.clone();
        store.append_interval(interval).unwrap();
        let events: Vec<_> = store.fetch_events().unwrap().collect();
        assert_eq!(events.len(), 1);
        let StoreEvent::CreateInterval(fetched) = events[0].as_ref().unwrap().clone();
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.task, "my-task");
    }
}
