// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines how to access the store of data tracked by Kimai Timer.

use core::fmt::Display;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_jsonlines::JsonLinesIter;
use time::macros::format_description;
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

    pub fn append_timer_event(&self, event: TimerEvent) -> Result<()> {
        Self::touch_file(&self.timelog)?;

        serde_jsonlines::append_json_lines(&self.timelog, &[event])
            .map_err(|e| anyhow!("failed to write time event to timelog: {e}"))?;
        Ok(())
    }

    pub fn fetch_timer_events(&self) -> Result<TimerEventIterator> {
        Self::touch_file(&self.timelog)?;

        let lines_iter = serde_jsonlines::json_lines(&self.timelog)
            .map_err(|e| anyhow!("failed to read timelog: {e}"))?;

        Ok(TimerEventIterator { inner: lines_iter })
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

    pub fn get_current_task(&self) -> Result<Option<String>> {
        let current = Self::read_file(&self.current_task)?;

        if current.is_empty() {
            Ok(None)
        } else {
            Ok(Some(current))
        }
    }

    pub fn set_current_task(&self, task: &str) -> Result<()> {
        Self::write_file(&self.current_task, task)
    }

    pub fn clear_current_task(&self) -> Result<()> {
        self.set_current_task("")
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

pub struct TimerEventIterator {
    inner: JsonLinesIter<BufReader<File>, TimerEvent>,
}

impl Iterator for TimerEventIterator {
    type Item = Result<TimerEvent, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TimerEvent {
    pub action: TimerAction,
    #[serde(with = "time::serde::timestamp")]
    pub timestamp: OffsetDateTime,
    pub task: String,
}

impl TimerEvent {
    pub fn start(task: impl Into<String>) -> Self {
        Self {
            action: TimerAction::Start,
            timestamp: OffsetDateTime::now_utc().truncate_to_second(),
            task: task.into(),
        }
    }

    pub fn stop(task: impl Into<String>) -> Self {
        Self {
            action: TimerAction::Stop,
            timestamp: OffsetDateTime::now_utc().truncate_to_second(),
            task: task.into(),
        }
    }

    pub fn to_local(&self, offset: UtcOffset) -> Self {
        Self {
            action: self.action,
            timestamp: self.timestamp.to_offset(offset),
            task: self.task.clone(),
        }
    }
}

impl Display for TimerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            write!(
                f,
                "{} ({:#} {})",
                self.task,
                self.action,
                self.timestamp
                    .to_offset(UtcOffset::current_local_offset().unwrap())
                    .format(&format_description!(
                        "[year]-[month]-[day] [hour]:[minute]:[second]"
                    ))
                    .unwrap()
            )
        } else {
            write!(
                f,
                "{} {} @ {}",
                self.action,
                self.task,
                self.timestamp
                    .to_offset(UtcOffset::current_local_offset().unwrap())
                    .format(&format_description!(
                        "[year]-[month]-[day] [hour]:[minute]:[second]"
                    ))
                    .unwrap()
            )
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub enum TimerAction {
    Start,
    Stop,
}

impl Display for TimerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            write!(
                f,
                "{}",
                match self {
                    Self::Start => "started",
                    Self::Stop => "stopped",
                }
            )
        } else {
            write!(
                f,
                "{}",
                match self {
                    Self::Start => "START",
                    Self::Stop => "STOP ",
                }
            )
        }
    }
}
