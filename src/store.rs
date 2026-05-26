// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines how to access the store of data tracked by Kimai Timer.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_jsonlines::JsonLinesIter;
use time::{OffsetDateTime, UtcOffset};

pub struct Store {
    timelog: PathBuf,
    taskset: PathBuf,
    current_task: PathBuf,
    last_task: PathBuf,
}

impl Store {
    pub fn new() -> Result<Self> {
        let pdirs = ProjectDirs::from("codes", "hitchcock", "kimai-timer")
            .ok_or_else(|| anyhow!("failed to derive project directory path"))?;

        let path = pdirs.data_dir();
        if !path.exists() {
            std::fs::create_dir_all(path).map_err(|e| {
                anyhow!(
                    "failed to create project data directory: {}: {e}",
                    path.display()
                )
            })?;
        }

        let path_timelog = path.join("timelog.jsonl");
        let path_taskset = path.join("taskset.json");
        let path_current = path.join("current");
        let path_last = path.join("last");

        Ok(Self {
            timelog: path_timelog,
            taskset: path_taskset,
            current_task: path_current,
            last_task: path_last,
        })
    }

    pub fn append_interval(&self, interval: TimeInterval) -> Result<()> {
        Self::touch_file(&self.timelog)?;

        let event = StoreEvent::CreateInterval(interval);
        serde_jsonlines::append_json_lines(&self.timelog, &[event])
            .map_err(|e| anyhow!("failed to write interval event to timelog: {e}"))?;
        Ok(())
    }

    pub fn fetch_events(&self) -> Result<PersistedEventIterator> {
        Self::touch_file(&self.timelog)?;

        let lines_iter = serde_jsonlines::json_lines(&self.timelog)
            .map_err(|e| anyhow!("failed to read timelog: {e}"))?;

        Ok(PersistedEventIterator { inner: lines_iter })
    }

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

    pub fn get_tasks(&self) -> Result<BTreeSet<String>> {
        let contents = Self::read_file(&self.taskset)?;

        let tasks = if contents.is_empty() {
            BTreeSet::new()
        } else {
            serde_json::from_str(&contents).map_err(|e| anyhow!("failed to parse taskset: {e}"))?
        };

        Ok(tasks)
    }

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

    pub fn set_current_task(&self, task: &str, start: i64) -> Result<()> {
        let current = CurrentTask {
            task: task.to_string(),
            start,
        };
        let contents = serde_json::to_string(&current)
            .map_err(|e| anyhow!("failed to serialize current task: {e}"))?;
        Self::write_file(&self.current_task, &contents)
    }

    pub fn clear_current_task(&self) -> Result<()> {
        Self::write_file(&self.current_task, "")
    }

    pub fn get_last_task(&self) -> Result<Option<String>> {
        let last = Self::read_file(&self.last_task)?;

        if last.is_empty() {
            Ok(None)
        } else {
            Ok(Some(last))
        }
    }

    pub fn set_last_task(&self, task: &str) -> Result<()> {
        Self::write_file(&self.last_task, task)
    }

    fn read_file(p: &Path) -> Result<String> {
        if !std::fs::exists(p)
            .map_err(|e| anyhow!("could not determine if file exists: {}: {e}", p.display()))?
        {
            return Ok(String::new());
        }

        std::fs::read_to_string(p).map_err(|e| anyhow!("failed to read file: {}: {e}", p.display()))
    }

    fn write_file(p: &Path, contents: &str) -> Result<()> {
        std::fs::write(p, contents.as_bytes())
            .map_err(|e| anyhow!("failed to write file: {}: {e}", p.display()))
    }

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
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StoreEvent {
    CreateInterval(TimeInterval),
}

/// An atomic unit of time spent on a task, with a definite start and end.
#[derive(Clone, Serialize, Deserialize)]
pub struct TimeInterval {
    pub id: String,
    #[serde(with = "time::serde::timestamp")]
    pub created_at: OffsetDateTime,
    pub task: String,
    #[serde(with = "time::serde::timestamp")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::timestamp")]
    pub end: OffsetDateTime,
}

impl TimeInterval {
    pub fn new(task: impl Into<String>, start: OffsetDateTime, end: OffsetDateTime) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: OffsetDateTime::now_utc().truncate_to_second(),
            task: task.into(),
            start,
            end,
        }
    }

    pub fn to_local(&self, offset: UtcOffset) -> Self {
        Self {
            id: self.id.clone(),
            created_at: self.created_at.to_offset(offset),
            task: self.task.clone(),
            start: self.start.to_offset(offset),
            end: self.end.to_offset(offset),
        }
    }
}

/// The state of the currently-running task, persisted to the `current` file as JSON.
#[derive(Serialize, Deserialize)]
pub struct CurrentTask {
    pub task: String,
    /// UNIX timestamp of when the task was started.
    pub start: i64,
}
