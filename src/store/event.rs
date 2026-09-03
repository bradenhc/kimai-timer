// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines the append-only event log that is the sole source of truth for the store.
//!
//! Everything the application knows - projects, the open session, completed intervals - is derived
//! by folding the [`Event`] values read from this log. Nothing else on disk is authoritative.
//!
//! Each line is an [`Envelope`] carrying a schema version, a unique event ID, and the moment the
//! event was recorded. The ID is what makes a future push/pull sync idempotent (merging is "append
//! the events I have not seen"), and it doubles as the identity of any interval the event creates.
//! The recorded timestamp is deliberately separate from the domain timestamps inside the event:
//! `kt add` for last Tuesday is recorded today but describes Tuesday.
//!
//! Reads are tolerant by design. A line from a newer schema version, or one naming an event this
//! binary does not recognize, is skipped with a warning rather than failing the whole read, so an
//! older `kt` can still report on a log a newer one has written. A truncated final line - the
//! signature of a crash mid-append - is reported back to the caller so the next write can repair
//! it. Malformed JSON anywhere other than the final line is real corruption and is fatal.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::time_ext::DateTimeExt;

use super::project::{ProjectId, ProjectName};

/// The schema version written on every new line.
///
/// Lines claiming a higher version are skipped on read, since this binary cannot know how to
/// interpret them.
///
const SCHEMA_VERSION: u32 = 1;

/// The unique ID of a single event in the log.
///
/// Also serves as the identity of whatever the event creates: an interval derived from a
/// [`Event::TimerStarted`] / [`Event::TimerStopped`] pair inherits the ID of the event that started
/// it, and an interval added directly inherits the ID of its [`Event::IntervalCreated`]. That gives
/// one ID space rather than two, and makes "which event produced this interval" always answerable.
///
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct EventId(String);

impl EventId {
    /// Mints a fresh event ID.
    ///
    fn new() -> Self {
        Self(Uuid::new_v4().into())
    }
}

impl core::fmt::Display for EventId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single recorded event together with its identity and recording time.
///
#[derive(Clone, Debug)]
pub(crate) struct Envelope {
    /// Uniquely identifies this event across machines.
    pub(crate) id: EventId,

    /// When the event was recorded, as opposed to the domain times inside `event`.
    pub(crate) ts: DateTime<Utc>,

    /// What happened.
    pub(crate) event: Event,
}

impl Envelope {
    /// Wraps `event` with a fresh ID and the current time.
    ///
    pub(crate) fn new(event: Event) -> Self {
        Self {
            id: EventId::new(),
            ts: Utc::now().truncate_to_second(),
            event,
        }
    }
}

/// Something that happened, as recorded in the log.
///
/// The vocabulary is deliberately orthogonal: starting and stopping the timer are separate events
/// rather than a combined "switch", so that a switch is expressed by writing both in a single
/// atomic append and no consumer has to understand a third way of saying the same thing.
///
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum Event {
    /// A new project was created.
    ProjectCreated {
        /// Identity of the new project.
        project_id: ProjectId,

        /// Display name of the new project.
        name: ProjectName,
    },

    /// The timer started running against a project.
    TimerStarted {
        /// The project being worked on.
        project_id: ProjectId,

        /// When work began.
        #[serde(with = "chrono::serde::ts_seconds")]
        at: DateTime<Utc>,
    },

    /// The timer stopped, closing whichever session was open.
    TimerStopped {
        /// When work ended.
        #[serde(with = "chrono::serde::ts_seconds")]
        at: DateTime<Utc>,
    },

    /// A completed interval was recorded directly, without running the timer.
    IntervalCreated {
        /// The project the work belongs to.
        project_id: ProjectId,

        /// When the work began.
        #[serde(with = "chrono::serde::ts_seconds")]
        start: DateTime<Utc>,

        /// When the work ended.
        #[serde(with = "chrono::serde::ts_seconds")]
        end: DateTime<Utc>,
    },
}

/// The recognized contents of the log, plus any repair the next write should perform.
///
pub(crate) struct LogContents {
    /// Every event this binary understood, in the order it was written.
    pub(crate) envelopes: Vec<Envelope>,

    /// Byte offset to truncate to before the next append, set when the final line was torn.
    pub(crate) torn_tail_at: Option<u64>,
}

/// Reads and parses the whole log.
///
/// A missing file is not an error; it simply means nothing has been recorded yet. See the module
/// docs for which malformed lines are tolerated and which are fatal.
///
pub(crate) fn read_all(path: &Path) -> Result<LogContents> {
    if !path.exists() {
        return Ok(LogContents {
            envelopes: Vec::new(),
            torn_tail_at: None,
        });
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("failed to read event log: {}: {e}", path.display()))?;

    let lines: Vec<&str> = contents.split_inclusive('\n').collect();
    let last_index = lines.len().saturating_sub(1);

    let mut envelopes = Vec::new();
    let mut torn_tail_at = None;
    let mut offset: u64 = 0;

    for (i, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim();

        if line.is_empty() {
            offset += raw_line.len() as u64;
            continue;
        }

        match serde_json::from_str::<WireIn>(line) {
            Ok(wire) => {
                if let Some(event) = decode(wire.v, wire.event, i + 1) {
                    envelopes.push(Envelope {
                        id: wire.id,
                        ts: wire.ts,
                        event,
                    });
                }
            }

            // A truncated final line is what a crash mid-append leaves behind. Report the offset so
            // the next write can drop it. The same damage anywhere else means the file was
            // corrupted rather than interrupted, and silently discarding history would be worse
            // than refusing to run.
            Err(e) => {
                if i == last_index {
                    warn!("skipping malformed trailing line in event log (incomplete write)");
                    torn_tail_at = Some(offset);
                    break;
                }

                bail!(
                    "corrupt event log at line {}: {}: {e}",
                    i + 1,
                    path.display()
                );
            }
        }

        offset += raw_line.len() as u64;
    }

    Ok(LogContents {
        envelopes,
        torn_tail_at,
    })
}

/// Turns a raw event body into an [`Event`], or `None` if this binary cannot interpret it.
///
/// Both rejections are deliberate forward-compatibility paths rather than errors: a newer `kt` may
/// have written a schema this one predates, or an event variant it does not have.
///
fn decode(version: u32, body: serde_json::Value, line_no: usize) -> Option<Event> {
    if version > SCHEMA_VERSION {
        warn!("skipping event at line {line_no} from newer schema version {version}");
        return None;
    }

    match serde_json::from_value::<Event>(body) {
        Ok(event) => Some(event),

        Err(e) => {
            warn!("skipping unrecognized event at line {line_no}: {e}");
            None
        }
    }
}

/// Appends every envelope to the log as a single atomic write.
///
/// Serializing the whole batch into one buffer and issuing one `write_all` is what makes a
/// multi-event operation - punching out of one project and into another - indivisible. Because the
/// file is opened in append mode, concurrent `kt` invocations cannot interleave within the batch.
/// The write is flushed to disk before returning so that a crash cannot lose an acknowledged punch.
///
/// When `truncate_to` is set, the torn tail left by an earlier crash is dropped first, so the log
/// repairs itself on first use rather than warning forever.
///
pub(crate) fn append_all(
    path: &Path,
    envelopes: &[Envelope],
    truncate_to: Option<u64>,
) -> Result<()> {
    if envelopes.is_empty() {
        return Ok(());
    }

    super::fs::ensure_parent_dir(path)?;

    let mut buf = String::new();
    for env in envelopes {
        let wire = WireOut {
            v: SCHEMA_VERSION,
            id: &env.id,
            ts: env.ts,
            event: &env.event,
        };

        let line =
            serde_json::to_string(&wire).map_err(|e| anyhow!("failed to serialize event: {e}"))?;

        buf.push_str(&line);
        buf.push('\n');
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| anyhow!("failed to open event log: {}: {e}", path.display()))?;

    if let Some(len) = truncate_to {
        file.set_len(len)
            .map_err(|e| anyhow!("failed to repair torn event log: {}: {e}", path.display()))?;
    }

    file.write_all(buf.as_bytes())
        .map_err(|e| anyhow!("failed to append to event log: {}: {e}", path.display()))?;

    file.sync_data()
        .map_err(|e| anyhow!("failed to flush event log: {}: {e}", path.display()))
}

/// The on-disk shape of a line as written.
///
#[derive(Serialize)]
struct WireOut<'a> {
    v: u32,
    id: &'a EventId,

    #[serde(with = "chrono::serde::ts_seconds")]
    ts: DateTime<Utc>,

    event: &'a Event,
}

/// The on-disk shape of a line as read.
///
/// The event body stays an uninterpreted value until [`decode`] has checked the schema version,
/// which is what lets an unknown variant be skipped instead of failing the surrounding line.
///
#[derive(Deserialize)]
struct WireIn {
    v: u32,
    id: EventId,

    #[serde(with = "chrono::serde::ts_seconds")]
    ts: DateTime<Utc>,

    event: serde_json::Value,
}
