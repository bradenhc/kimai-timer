// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines how to access the store of data tracked by Kimai Timer.
//!
//! The store is a single append-only event log. [`Store`] folds that log once when it is opened,
//! producing an in-memory projection of everything the application knows: the set of projects, the
//! session currently being worked, and every completed interval. Reads borrow from the projection;
//! writes append to the log and then apply the very same events to the projection, so the state
//! held in memory is always exactly what re-opening the store would produce.
//!
//! Deriving rather than storing is what removes a whole class of bug. There is no second file to
//! fall out of step with the log, so punching out cannot half-succeed, and losing project metadata
//! can no longer make historical time unreadable - an interval whose project is unknown is still
//! reported, just without a name.

mod event;
mod fs;
mod interval;
mod project;

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};

use crate::time_ext::DateTimeExt;

use event::{Envelope, Event, EventId};

pub(crate) use fs::StoreRoot;
pub(crate) use interval::{Interval, RoundingMode, TimeDuration};
pub(crate) use project::{Project, ProjectId, ProjectName, ProjectSet};

/// The name of the log file inside the store directory.
const FILE_NAME: &str = "events.jsonl";

/// Provides read and write access to everything Kimai Timer persists.
///
pub(crate) struct Store {
    /// Absolute path to the event log.
    path: PathBuf,

    /// The projection folded from the log.
    state: State,

    /// Byte offset of a torn final line, to be dropped by the next write.
    torn_tail_at: Option<u64>,
}

impl Store {
    /// Opens the store rooted at `root`, folding the log into memory.
    ///
    /// Opening never creates anything on disk, so commands that only read leave no trace on a
    /// machine where no time has been recorded yet.
    ///
    pub(crate) fn open(root: &StoreRoot) -> Result<Self> {
        let path = fs::store_path(FILE_NAME, root)?;
        let contents = event::read_all(&path)?;

        let mut state = State::default();
        for envelope in contents.envelopes {
            state.apply(envelope)?;
        }

        Ok(Self {
            path,
            state,
            torn_tail_at: contents.torn_tail_at,
        })
    }

    /// Returns every known project.
    ///
    pub(crate) fn projects(&self) -> &ProjectSet {
        &self.state.projects
    }

    /// Resolves a project reference held by an interval or session.
    ///
    /// Returns `None` when the log references a project this store has never seen, which is
    /// possible after a hand edit or a future merge. Callers should degrade rather than fail.
    ///
    pub(crate) fn project(&self, id: &ProjectId) -> Option<&Project> {
        self.state.projects.get_by_id(id)
    }

    /// Returns every completed interval, in the order it was recorded.
    ///
    /// Work in progress is deliberately excluded; use [`Store::session`] for that.
    ///
    pub(crate) fn intervals(&self) -> &[Interval] {
        &self.state.intervals
    }

    /// Returns the session currently being worked, if the timer is running.
    ///
    pub(crate) fn session(&self) -> Option<&Session> {
        self.state.session.as_ref()
    }

    /// Returns the project the timer most recently stopped on.
    ///
    /// This is what `kt in` with no argument resumes. It follows the timer only, so recording an
    /// interval with `kt add` never changes it.
    ///
    pub(crate) fn last_project(&self) -> Option<&Project> {
        self.state.last.as_ref().and_then(|id| self.project(id))
    }

    /// Creates a new project.
    ///
    /// Returns an error if a project with the same name already exists.
    ///
    pub(crate) fn add_project(&mut self, name: ProjectName) -> Result<&Project> {
        if self.state.projects.contains_name(name.as_str()) {
            bail!("project '{name}' already exists");
        }

        let project_id = ProjectId::new();

        self.commit(vec![Event::ProjectCreated {
            project_id: project_id.clone(),
            name,
        }])?;

        self.project(&project_id)
            .ok_or_else(|| anyhow!("newly created project is missing from the store"))
    }

    /// Starts the timer on a project, closing any session already open.
    ///
    /// Both events are written in a single append, so a switch between projects cannot be observed
    /// or interrupted half-done. Returns the interval that closing the previous session produced,
    /// if there was one.
    ///
    pub(crate) fn start_session(&mut self, id: ProjectId) -> Result<Option<Interval>> {
        if self.project(&id).is_none() {
            bail!("no project with ID {id}");
        }

        let at = Utc::now().truncate_to_second();
        let closing = self.state.session.is_some();

        let mut events = Vec::new();
        if closing {
            events.push(Event::TimerStopped { at });
        }
        events.push(Event::TimerStarted { project_id: id, at });

        self.commit(events)?;

        Ok(if closing {
            self.state.intervals.last().cloned()
        } else {
            None
        })
    }

    /// Stops the timer, recording the interval that the open session produced.
    ///
    pub(crate) fn stop_session(&mut self) -> Result<Interval> {
        if self.state.session.is_none() {
            bail!("no current session");
        }

        self.commit(vec![Event::TimerStopped {
            at: Utc::now().truncate_to_second(),
        }])?;

        self.state
            .intervals
            .last()
            .cloned()
            .ok_or_else(|| anyhow!("stopping the session did not produce an interval"))
    }

    /// Records a completed interval directly, without running the timer.
    ///
    pub(crate) fn add_interval(
        &mut self,
        id: ProjectId,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<&Interval> {
        if self.project(&id).is_none() {
            bail!("no project with ID {id}");
        }

        if end <= start {
            bail!("interval end must be after its start");
        }

        self.commit(vec![Event::IntervalCreated {
            project_id: id,
            start,
            end,
        }])?;

        self.state
            .intervals
            .last()
            .ok_or_else(|| anyhow!("newly added interval is missing from the store"))
    }

    /// Appends events to the log and folds them into the projection.
    ///
    /// Routing every write through the same [`State::apply`] the fold uses is what guarantees the
    /// in-memory state cannot drift from what a fresh read of the log would produce.
    ///
    fn commit(&mut self, events: Vec<Event>) -> Result<()> {
        let envelopes: Vec<Envelope> = events.into_iter().map(Envelope::new).collect();

        event::append_all(&self.path, &envelopes, self.torn_tail_at)?;
        self.torn_tail_at = None;

        for envelope in envelopes {
            self.state.apply(envelope)?;
        }

        Ok(())
    }
}

/// Everything the application knows, derived from the log.
///
#[derive(Debug, Default)]
struct State {
    /// Every project that has been created.
    projects: ProjectSet,

    /// Completed intervals, in the order they were recorded.
    intervals: Vec<Interval>,

    /// The session currently open, if the timer is running.
    session: Option<Session>,

    /// The project the timer most recently stopped on.
    last: Option<ProjectId>,
}

impl State {
    /// Folds a single event into the projection.
    ///
    /// A reference to an unknown project is deliberately not an error here: the interval or session
    /// is kept so the time is never lost, and resolving the name is left to fail gracefully at
    /// display time.
    ///
    fn apply(&mut self, envelope: Envelope) -> Result<()> {
        match envelope.event {
            Event::ProjectCreated { project_id, name } => {
                self.projects.insert(Project {
                    id: project_id,
                    name,
                })?;
            }

            Event::TimerStarted { project_id, at } => {
                self.session = Some(Session {
                    id: envelope.id,
                    project_id,
                    start: at,
                });
            }

            // An interval derived from the timer inherits the ID of the event that started it, so
            // it stays addressable by a future edit or delete.
            Event::TimerStopped { at } => {
                if let Some(session) = self.session.take() {
                    self.last = Some(session.project_id.clone());

                    self.intervals.push(Interval {
                        id: session.id,
                        project_id: session.project_id,
                        start: session.start,
                        end: at,
                    });
                }
            }

            Event::IntervalCreated {
                project_id,
                start,
                end,
            } => {
                self.intervals.push(Interval {
                    id: envelope.id,
                    project_id,
                    start,
                    end,
                });
            }
        }

        Ok(())
    }
}

/// A timer that is currently running.
///
#[derive(Clone, Debug)]
pub(crate) struct Session {
    /// The ID of the event that started the session; becomes the ID of the resulting interval.
    id: EventId,

    /// The project being worked on.
    pub(crate) project_id: ProjectId,

    /// When work began.
    pub(crate) start: DateTime<Utc>,
}

// -------------------------------------------------------------------------------------------------
// END OF LOGIC - MODULE UNIT TESTS BELOW HERE
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::DateTime;
    use tempfile::{TempDir, tempdir};

    use super::*;

    /// Opens a store over a fresh temporary directory.
    fn store_in(dir: &TempDir) -> Store {
        Store::open(&StoreRoot::specified(dir.path())).unwrap()
    }

    /// Writes raw log lines so tests can exercise logs this binary did not produce.
    fn seed(dir: &Path, lines: &[&str]) {
        let mut contents = String::new();
        for line in lines {
            contents.push_str(line);
            contents.push('\n');
        }
        std::fs::write(dir.join(FILE_NAME), contents).unwrap();
    }

    /// Returns the `type` of every event recorded in the log, in order.
    fn event_types(dir: &Path) -> Vec<String> {
        let contents = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        contents
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["event"]["type"].as_str().unwrap().to_string()
            })
            .collect()
    }

    fn name(n: &str) -> ProjectName {
        ProjectName::new(n).unwrap()
    }

    #[test]
    fn open_creates_nothing_on_disk() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b");

        let store = Store::open(&StoreRoot::specified(&nested)).unwrap();

        assert!(!nested.exists(), "read-only open must not touch the disk");
        assert!(store.projects().is_empty());
        assert!(store.intervals().is_empty());
        assert!(store.session().is_none());
    }

    #[test]
    fn add_project_roundtrips_by_id_and_name() {
        let dir = tempdir().unwrap();
        let mut store = store_in(&dir);

        let id = store.add_project(name("my-project")).unwrap().id.clone();

        assert_eq!(store.project(&id).unwrap().name, name("my-project"));
        assert_eq!(store.projects().get_by_name("my-project").unwrap().id, id);
        assert!(store.projects().contains_name("my-project"));
    }

    #[test]
    fn duplicate_project_name_is_rejected() {
        let dir = tempdir().unwrap();
        let mut store = store_in(&dir);

        store.add_project(name("alpha")).unwrap();

        assert!(store.add_project(name("alpha")).is_err());
        assert_eq!(store.projects().list().count(), 1);
    }

    #[test]
    fn start_and_stop_session_roundtrip() {
        let dir = tempdir().unwrap();
        let mut store = store_in(&dir);
        let id = store.add_project(name("alpha")).unwrap().id.clone();

        assert!(store.start_session(id.clone()).unwrap().is_none());
        assert_eq!(store.session().unwrap().project_id, id);

        let interval = store.stop_session().unwrap();

        assert_eq!(interval.project_id, id);
        assert!(store.session().is_none());
        assert_eq!(store.intervals().len(), 1);
        assert_eq!(store.last_project().unwrap().id, id);
    }

    #[test]
    fn stop_without_a_session_is_an_error() {
        let dir = tempdir().unwrap();
        let mut store = store_in(&dir);

        assert!(store.stop_session().is_err());
    }

    #[test]
    fn switch_records_stop_and_start_together() {
        let dir = tempdir().unwrap();
        let mut store = store_in(&dir);
        let alpha = store.add_project(name("alpha")).unwrap().id.clone();
        let beta = store.add_project(name("beta")).unwrap().id.clone();

        store.start_session(alpha.clone()).unwrap();
        let closed = store.start_session(beta.clone()).unwrap();

        assert_eq!(closed.unwrap().project_id, alpha);
        assert_eq!(store.session().unwrap().project_id, beta);

        // The stop and the start are adjacent in the log because they were written as one batch.
        assert_eq!(
            event_types(dir.path()),
            [
                "ProjectCreated",
                "ProjectCreated",
                "TimerStarted",
                "TimerStopped",
                "TimerStarted",
            ]
        );
    }

    #[test]
    fn add_interval_does_not_change_last_project() {
        let dir = tempdir().unwrap();
        let mut store = store_in(&dir);
        let alpha = store.add_project(name("alpha")).unwrap().id.clone();
        let beta = store.add_project(name("beta")).unwrap().id.clone();

        store.start_session(alpha.clone()).unwrap();
        store.stop_session().unwrap();

        let start = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let end = DateTime::from_timestamp(1_003_600, 0).unwrap();
        store.add_interval(beta, start, end).unwrap();

        assert_eq!(store.intervals().len(), 2);
        assert_eq!(store.last_project().unwrap().id, alpha);
    }

    #[test]
    fn add_interval_rejects_a_non_positive_span() {
        let dir = tempdir().unwrap();
        let mut store = store_in(&dir);
        let id = store.add_project(name("alpha")).unwrap().id.clone();

        let start = DateTime::from_timestamp(1_000_000, 0).unwrap();

        assert!(store.add_interval(id.clone(), start, start).is_err());
        assert!(store.intervals().is_empty());
    }

    #[test]
    fn state_survives_reopen() {
        let dir = tempdir().unwrap();
        let id = {
            let mut store = store_in(&dir);
            let id = store.add_project(name("alpha")).unwrap().id.clone();
            store.start_session(id.clone()).unwrap();
            store.stop_session().unwrap();
            id
        };

        let store = store_in(&dir);

        assert_eq!(store.intervals().len(), 1);
        assert_eq!(store.intervals()[0].project_id, id);
        assert_eq!(store.last_project().unwrap().id, id);
        assert!(store.session().is_none());
    }

    #[test]
    fn derived_interval_inherits_the_start_event_id() {
        let dir = tempdir().unwrap();
        seed(
            dir.path(),
            &[
                r#"{"v":1,"id":"e-proj","ts":1000,"event":{"type":"ProjectCreated","data":{"project_id":"p1","name":"alpha"}}}"#,
                r#"{"v":1,"id":"e-start","ts":2000,"event":{"type":"TimerStarted","data":{"project_id":"p1","at":2000}}}"#,
                r#"{"v":1,"id":"e-stop","ts":5600,"event":{"type":"TimerStopped","data":{"at":5600}}}"#,
            ],
        );

        let store = store_in(&dir);

        assert_eq!(store.intervals().len(), 1);
        assert_eq!(store.intervals()[0].id.to_string(), "e-start");
    }

    #[test]
    fn interval_with_unknown_project_is_kept_but_unresolved() {
        let dir = tempdir().unwrap();
        seed(
            dir.path(),
            &[
                r#"{"v":1,"id":"e1","ts":1000,"event":{"type":"IntervalCreated","data":{"project_id":"ghost","start":1000,"end":4600}}}"#,
            ],
        );

        let store = store_in(&dir);

        assert_eq!(store.intervals().len(), 1, "time must never be dropped");
        assert!(store.project(&store.intervals()[0].project_id).is_none());
    }

    #[test]
    fn unrecognized_event_is_skipped() {
        let dir = tempdir().unwrap();
        seed(
            dir.path(),
            &[
                r#"{"v":1,"id":"e1","ts":1000,"event":{"type":"ProjectCreated","data":{"project_id":"p1","name":"alpha"}}}"#,
                r#"{"v":1,"id":"e2","ts":2000,"event":{"type":"ProjectRenamed","data":{"project_id":"p1","name":"beta"}}}"#,
                r#"{"v":1,"id":"e3","ts":3000,"event":{"type":"IntervalCreated","data":{"project_id":"p1","start":3000,"end":6600}}}"#,
            ],
        );

        let store = store_in(&dir);

        assert_eq!(store.projects().list().count(), 1);
        assert_eq!(
            store.intervals().len(),
            1,
            "events after the unknown one still apply"
        );
    }

    #[test]
    fn event_from_a_newer_schema_version_is_skipped() {
        let dir = tempdir().unwrap();
        seed(
            dir.path(),
            &[
                r#"{"v":99,"id":"e1","ts":1000,"event":{"type":"ProjectCreated","data":{"project_id":"p1","name":"alpha"}}}"#,
                r#"{"v":1,"id":"e2","ts":2000,"event":{"type":"ProjectCreated","data":{"project_id":"p2","name":"beta"}}}"#,
            ],
        );

        let store = store_in(&dir);

        assert_eq!(store.projects().list().count(), 1);
        assert!(store.projects().contains_name("beta"));
    }

    #[test]
    fn corrupt_line_before_the_end_is_fatal() {
        let dir = tempdir().unwrap();
        seed(
            dir.path(),
            &[
                r#"{"v":1,"id":"e1","ts":1000,"event":{"type":"ProjectCreated","data":{"project_id":"p1","na"#,
                r#"{"v":1,"id":"e2","ts":2000,"event":{"type":"ProjectCreated","data":{"project_id":"p2","name":"beta"}}}"#,
            ],
        );

        assert!(Store::open(&StoreRoot::specified(dir.path())).is_err());
    }

    #[test]
    fn torn_trailing_line_is_skipped_then_repaired() {
        let dir = tempdir().unwrap();
        let good = r#"{"v":1,"id":"e1","ts":1000,"event":{"type":"ProjectCreated","data":{"project_id":"p1","name":"alpha"}}}"#;

        // A crash mid-append leaves a partial final line with no newline terminator.
        std::fs::write(
            dir.path().join(FILE_NAME),
            format!("{good}\n{{\"v\":1,\"id\":\"e2\",\"ts\":20"),
        )
        .unwrap();

        let mut store = store_in(&dir);

        assert_eq!(store.projects().list().count(), 1, "good lines still load");

        store.add_project(name("beta")).unwrap();

        let contents = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(
            !contents.contains(r#""ts":20"#),
            "torn tail must be dropped"
        );
        assert_eq!(
            event_types(dir.path()),
            ["ProjectCreated", "ProjectCreated"]
        );

        // Re-opening sees a clean log with both projects and no warning-worthy remnant.
        let reopened = store_in(&dir);
        assert_eq!(reopened.projects().list().count(), 2);
    }
}
