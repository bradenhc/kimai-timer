// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines completed spans of work and how their durations are reported.
//!
//! An [`Interval`] is always closed: it has both a start and an end. Work currently in progress is
//! not an interval but an open session, which the store exposes separately, so that no consumer can
//! mistake a running timer for finished work.
//!
//! Intervals reference their project by ID rather than by name. Resolving that reference is an
//! in-memory lookup against the projection, and it is allowed to fail: an interval whose project is
//! unknown is still valid time that should be displayed, just without a name.
//!
//! [`TimeDuration`] wraps an accumulated duration together with the [`RoundingMode`] used to
//! present it, so that rounding is applied at display time and never to the stored timestamps.

use core::fmt::Display;

use chrono::{DateTime, Duration, Utc};

use super::event::EventId;
use super::project::ProjectId;

/// An atomic unit of time spent on a project, with a definite start and end.
///
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Interval {
    /// The ID of the event that created this interval.
    pub(crate) id: EventId,

    /// The project the work belongs to.
    pub(crate) project_id: ProjectId,

    /// The start timestamp for the interval.
    pub(crate) start: DateTime<Utc>,

    /// The stop timestamp for the interval.
    pub(crate) end: DateTime<Utc>,
}

impl Interval {
    /// The duration of the interval, ready to be reported under `mode`.
    ///
    pub(crate) fn duration(&self, mode: RoundingMode) -> TimeDuration {
        TimeDuration::new(self.end - self.start, mode)
    }
}

/// An accumulated duration together with the rounding used to report it.
///
/// Holds the unmodified value and exposes [`TimeDuration::rounded`] to obtain the display-ready
/// value without altering the underlying data.
///
pub(crate) struct TimeDuration {
    inner: Duration,

    mode: RoundingMode,
}

impl TimeDuration {
    /// Wraps an accumulated duration with the mode used to report it.
    ///
    pub(crate) fn new(raw: Duration, mode: RoundingMode) -> Self {
        Self { inner: raw, mode }
    }

    /// Returns the duration rounded up to the next boundary defined by the mode.
    ///
    /// Both modes use ceiling rounding: if the duration falls exactly on a boundary it is
    /// returned unchanged; otherwise it is snapped to the next boundary above it.
    ///
    pub(crate) fn rounded(&self) -> Duration {
        let secs = self.inner.num_seconds();

        let boundary: i64 = match self.mode {
            RoundingMode::Decimal => 36,
            RoundingMode::Classic(n) => i64::from(n) * 60,
        };

        let remainder = secs % boundary;

        if remainder == 0 {
            Duration::seconds(secs)
        } else {
            Duration::seconds(secs + boundary - remainder)
        }
    }
}

impl Display for TimeDuration {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let dur = if f.alternate() {
            self.inner
        } else {
            self.rounded()
        };

        write!(f, "{:02}:{:02}", dur.num_hours(), dur.num_minutes() % 60)
    }
}

/// Controls how an aggregated duration is snapped upward for reporting.
///
/// Rounding is applied after all intervals for a project on a given day have been summed, so
/// the stored timestamps are never affected. Configuration of the active mode is deferred to
/// a later change; callers that want the default should use `RoundingMode::default()`.
///
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum RoundingMode {
    /// Round up to the nearest 36 seconds (= 0.01 hours).
    ///
    /// Guarantees the displayed value is an exact multiple of 0.01 h, so
    /// `displayed_duration * hourly_rate` never produces a repeating decimal.
    #[default]
    Decimal,

    /// Round up to the nearest `n` minutes.
    ///
    /// The inner value is the granularity in minutes. Kimai recommends 3 as a starting point
    /// (3 min = 0.05 h), keeping invoice math clean without over-rounding short tasks.
    Classic(u32),
}

// -------------------------------------------------------------------------------------------------
// END OF LOGIC - MODULE UNIT TESTS BELOW HERE
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_rounded_decimal_zero() {
        let td = TimeDuration::new(Duration::zero(), RoundingMode::Decimal);
        assert_eq!(td.rounded(), Duration::zero());
    }

    #[test]
    fn duration_rounded_decimal_exact_boundary() {
        let td = TimeDuration::new(Duration::seconds(72), RoundingMode::Decimal);
        assert_eq!(td.rounded(), Duration::seconds(72));
    }

    #[test]
    fn duration_rounded_decimal_one_over_boundary() {
        let td = TimeDuration::new(Duration::seconds(73), RoundingMode::Decimal);
        assert_eq!(td.rounded(), Duration::seconds(108));
    }

    #[test]
    fn duration_rounded_decimal_just_under_boundary() {
        let td = TimeDuration::new(Duration::seconds(35), RoundingMode::Decimal);
        assert_eq!(td.rounded(), Duration::seconds(36));
    }

    #[test]
    fn duration_rounded_classic_zero() {
        let td = TimeDuration::new(Duration::zero(), RoundingMode::Classic(3));
        assert_eq!(td.rounded(), Duration::zero());
    }

    #[test]
    fn duration_rounded_classic_exact_boundary() {
        let td = TimeDuration::new(Duration::minutes(6), RoundingMode::Classic(3));
        assert_eq!(td.rounded(), Duration::minutes(6));
    }

    #[test]
    fn duration_rounded_classic_one_second_over_boundary() {
        let td = TimeDuration::new(Duration::seconds(181), RoundingMode::Classic(3));
        assert_eq!(td.rounded(), Duration::minutes(6));
    }

    #[test]
    fn duration_rounded_classic_just_under_boundary() {
        let td = TimeDuration::new(Duration::seconds(179), RoundingMode::Classic(3));
        assert_eq!(td.rounded(), Duration::minutes(3));
    }
}
