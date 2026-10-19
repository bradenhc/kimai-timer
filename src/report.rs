// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Builds the Report: how much time each Project received on each Day of a window of Days.
//!
//! A [`Report`] is built from plain data (the Intervals, the open Session, the current time, the
//! window, and a time zone) rather than from the store or the system clock, so that every subtle
//! case can be tested: work that crosses midnight, the window's edges, Days that DST makes 23 or
//! 25 hours long, and rounding.
//!
//! Each Day is the half-open span from its first instant to the next Day's, in the given time zone.
//! Only the part of an Interval, or of the open Session up to now, that falls inside a Day counts
//! toward it. Per ADR-0002, rounding is applied once to each Project's total on a Day, and every
//! other total is a sum of those rounded values.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Offset, TimeZone, Utc};

use crate::store::{Interval, ProjectId, RoundingMode, Session, TimeDuration};

/// Time per Project per Day over a window of Days, plus the Intervals that fall in it.
///
pub(crate) struct Report<'a> {
    /// The Days in the window, in order.
    days: Vec<NaiveDate>,

    /// Each Project's rounded time on each Day, one cell per Day. Only Projects with time in the
    /// window, or the open Session's Project, have a row.
    rows: BTreeMap<ProjectId, Vec<Duration>>,

    /// The Project of the open Session, if one is open.
    session_project: Option<ProjectId>,

    /// Every Interval with any part inside the window, whole and in Event Log order.
    intervals: Vec<&'a Interval>,
}

impl<'a> Report<'a> {
    /// Builds the Report for the Days in `days`, counting the open Session up to `now`.
    ///
    /// Never fails: a span that ends before it starts, or a Session that starts after `now`,
    /// contributes no time, and an empty range of Days yields an empty Report.
    ///
    pub(crate) fn build<Tz: TimeZone>(
        intervals: &'a [Interval],
        session: Option<&Session>,
        now: DateTime<Utc>,
        days: RangeInclusive<NaiveDate>,
        tz: &Tz,
        mode: RoundingMode,
    ) -> Self {
        let (first, last) = days.into_inner();
        let days: Vec<NaiveDate> = first.iter_days().take_while(|d| *d <= last).collect();

        // One boundary per Day plus the start of the Day after the last, so Day `i` is the
        // half-open span `bounds[i]..bounds[i + 1]`.
        let bounds: Vec<DateTime<Utc>> = days
            .iter()
            .chain(days.last().and_then(NaiveDate::succ_opt).as_ref())
            .map(|d| day_start(*d, tz))
            .collect();

        let window = bounds.first().zip(bounds.last());

        // Listed Intervals stay whole: they are records, so only whether they touch the window
        // matters, not how much of them falls inside it.
        let listed = intervals
            .iter()
            .filter(|i| window.is_some_and(|(start, end)| i.start < *end && i.end > *start))
            .collect();

        let mut raw: BTreeMap<ProjectId, Vec<Duration>> = BTreeMap::new();

        for interval in intervals {
            let cells = split(interval.start, interval.end, &bounds);

            if cells.iter().any(|c| !c.is_zero()) {
                add_cells(&mut raw, &interval.project_id, &cells);
            }
        }

        // The open Session runs up to now. Its Project keeps a row even when none of that time
        // falls in the window, so the Project being worked on is always shown.
        if let Some(session) = session {
            let cells = split(session.start, now, &bounds);
            add_cells(&mut raw, &session.project_id, &cells);
        }

        let rows = raw
            .into_iter()
            .map(|(id, cells)| {
                let cells = cells
                    .into_iter()
                    .map(|c| TimeDuration::new(c, mode).rounded())
                    .collect();
                (id, cells)
            })
            .collect();

        Self {
            days,
            rows,
            session_project: session.map(|s| s.project_id.clone()),
            intervals: listed,
        }
    }

    /// Each Project's rounded time on each Day of the window, in Project ID order.
    ///
    pub(crate) fn rows(&self) -> impl Iterator<Item = (&ProjectId, &[Duration])> {
        self.rows.iter().map(|(id, cells)| (id, cells.as_slice()))
    }

    /// One Project's rounded time on each Day, if it has a row.
    ///
    #[cfg(test)]
    fn row(&self, project: &ProjectId) -> Option<Vec<Duration>> {
        self.rows.get(project).cloned()
    }

    /// The Days in the window, in order.
    ///
    pub(crate) fn days(&self) -> &[NaiveDate] {
        &self.days
    }

    /// Every Interval with any part inside the window, whole rather than cut to it.
    ///
    pub(crate) fn intervals(&self) -> &[&'a Interval] {
        &self.intervals
    }

    /// The time on each Day across all Projects, as the sum of each Project's rounded time.
    ///
    pub(crate) fn day_totals(&self) -> Vec<Duration> {
        let mut totals = vec![Duration::zero(); self.days.len()];

        for cells in self.rows.values() {
            for (total, cell) in totals.iter_mut().zip(cells) {
                *total += *cell;
            }
        }

        totals
    }

    /// A Project's time across the window, as the sum of its rounded time on each Day.
    ///
    pub(crate) fn project_total(&self, project: &ProjectId) -> Duration {
        self.rows
            .get(project)
            .map_or_else(Duration::zero, |cells| cells.iter().sum())
    }

    /// All time in the window, as the sum of every Project's rounded time on each Day.
    ///
    pub(crate) fn total(&self) -> Duration {
        self.rows.keys().map(|id| self.project_total(id)).sum()
    }

    /// The Project of the open Session, if one is open.
    ///
    pub(crate) fn session_project(&self) -> Option<&ProjectId> {
        self.session_project.as_ref()
    }

    /// Whether there is nothing to report: no Project has time in the window and no Session is
    /// open.
    ///
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The first instant of a Day in the time zone.
///
/// Where clocks skip over midnight (a DST gap starting at 00:00), the Day starts at the instant
/// the clocks jump: the moment midnight would have been under the offset in force just before.
///
fn day_start<Tz: TimeZone>(day: NaiveDate, tz: &Tz) -> DateTime<Utc> {
    let midnight = day.and_time(NaiveTime::MIN);

    if let Some(start) = tz.from_local_datetime(&midnight).earliest() {
        return start.with_timezone(&Utc);
    }

    let offset_before = tz
        .offset_from_utc_datetime(&(midnight - Duration::days(1)))
        .fix();

    (midnight - offset_before).and_utc()
}

/// Splits `start..end` into the time it shares with each Day delimited by `bounds`.
///
fn split(start: DateTime<Utc>, end: DateTime<Utc>, bounds: &[DateTime<Utc>]) -> Vec<Duration> {
    bounds
        .windows(2)
        .map(|w| {
            let overlap = end.min(w[1]) - start.max(w[0]);
            overlap.max(Duration::zero())
        })
        .collect()
}

/// Adds one span's per-Day time to a Project's row, creating the row on first use.
///
fn add_cells(rows: &mut BTreeMap<ProjectId, Vec<Duration>>, id: &ProjectId, cells: &[Duration]) {
    let row = rows
        .entry(id.clone())
        .or_insert_with(|| vec![Duration::zero(); cells.len()]);

    for (total, cell) in row.iter_mut().zip(cells) {
        *total += *cell;
    }
}

// -------------------------------------------------------------------------------------------------
// END OF LOGIC - MODULE UNIT TESTS BELOW HERE
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use chrono_tz::America::{Denver, Santiago};
    use chrono_tz::Tz;

    use super::*;
    use crate::store::EventId;

    /// A zone west of UTC, so a Day's boundaries never coincide with UTC midnight.
    fn tz() -> FixedOffset {
        FixedOffset::west_opt(7 * 3600).unwrap()
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, d).unwrap()
    }

    /// The instant at `hh:mm` local time on a date in `tz`.
    fn local_on<Z: TimeZone>(tz: &Z, date: NaiveDate, hh: u32, mm: u32) -> DateTime<Utc> {
        tz.from_local_datetime(&date.and_time(NaiveTime::from_hms_opt(hh, mm, 0).unwrap()))
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
    }

    /// The instant at `hh:mm` local time on the given Day of March 2026, in [`tz`].
    fn at(d: u32, hh: u32, mm: u32) -> DateTime<Utc> {
        local_on(&tz(), day(d), hh, mm)
    }

    fn interval(project: &ProjectId, start: DateTime<Utc>, end: DateTime<Utc>) -> Interval {
        Interval {
            id: EventId::new(),
            project_id: project.clone(),
            start,
            end,
        }
    }

    fn mins(m: i64) -> Duration {
        Duration::minutes(m)
    }

    /// Builds a Report over March 1-3 at noon on the 3rd, the window used by most tests.
    fn report<'a>(intervals: &'a [Interval], session: Option<&Session>) -> Report<'a> {
        Report::build(
            intervals,
            session,
            at(3, 12, 0),
            day(1)..=day(3),
            &tz(),
            RoundingMode::Decimal,
        )
    }

    #[test]
    fn interval_inside_one_day_counts_toward_that_day() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(2, 9, 0), at(2, 10, 30))];

        let report = report(&intervals, None);

        assert_eq!(report.row(&p), Some(vec![mins(0), mins(90), mins(0)]));
    }

    #[test]
    fn interval_crossing_midnight_is_split_between_days() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(1, 23, 0), at(2, 1, 30))];

        let report = report(&intervals, None);

        assert_eq!(report.row(&p), Some(vec![mins(60), mins(90), mins(0)]));
    }

    #[test]
    fn interval_starting_before_the_window_counts_only_inside_it() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(1, 0, 0) - Duration::hours(2), at(1, 2, 0))];

        let report = report(&intervals, None);

        assert_eq!(report.row(&p), Some(vec![mins(120), mins(0), mins(0)]));
        assert_eq!(report.intervals().len(), 1);
    }

    #[test]
    fn interval_after_the_window_is_ignored() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(4, 9, 0), at(4, 17, 0))];

        let report = report(&intervals, None);

        assert_eq!(report.row(&p), None);
        assert_eq!(report.intervals().len(), 0);
        assert!(report.is_empty());
    }

    #[test]
    fn interval_ending_after_the_window_counts_only_inside_it() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(3, 23, 0), at(4, 1, 0))];

        let report = report(&intervals, None);

        assert_eq!(report.row(&p), Some(vec![mins(0), mins(0), mins(60)]));
        assert_eq!(report.intervals().len(), 1);
    }

    #[test]
    fn intervals_touching_the_window_edges_are_left_out() {
        let p = ProjectId::new();
        let intervals = [
            interval(&p, at(1, 0, 0) - Duration::hours(1), at(1, 0, 0)),
            interval(&p, at(4, 0, 0), at(4, 1, 0)),
        ];

        let report = report(&intervals, None);

        assert_eq!(report.row(&p), None);
        assert_eq!(report.intervals().len(), 0);
    }

    #[test]
    fn open_session_counts_up_to_now() {
        let p = ProjectId::new();
        let session = Session::new(p.clone(), at(3, 10, 0));

        let report = report(&[], Some(&session));

        assert_eq!(report.row(&p), Some(vec![mins(0), mins(0), mins(120)]));
        assert_eq!(report.session_project(), Some(&p));
        assert_eq!(report.intervals().len(), 0);
    }

    #[test]
    fn open_session_started_before_the_window_counts_only_inside_it() {
        let p = ProjectId::new();
        let session = Session::new(p.clone(), at(1, 0, 0) - Duration::hours(5));

        let report = report(&[], Some(&session));

        assert_eq!(
            report.row(&p),
            Some(vec![mins(24 * 60), mins(24 * 60), mins(12 * 60)])
        );
    }

    #[test]
    fn open_session_adds_to_the_same_projects_intervals() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(3, 8, 0), at(3, 9, 0))];
        let session = Session::new(p.clone(), at(3, 11, 0));

        let report = report(&intervals, Some(&session));

        assert_eq!(report.row(&p), Some(vec![mins(0), mins(0), mins(120)]));
    }

    #[test]
    fn open_session_starting_after_now_contributes_nothing_but_keeps_its_row() {
        let p = ProjectId::new();
        let session = Session::new(p.clone(), at(3, 13, 0));

        let report = report(&[], Some(&session));

        assert_eq!(report.row(&p), Some(vec![mins(0), mins(0), mins(0)]));
    }

    #[test]
    fn window_without_time_is_empty() {
        let report = report(&[], None);

        assert!(report.is_empty());
        assert_eq!(report.days().len(), 3);
        assert_eq!(report.day_totals(), vec![mins(0); 3]);
        assert_eq!(report.total(), mins(0));
    }

    #[test]
    fn window_without_days_is_empty() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(2, 9, 0), at(2, 10, 0))];

        let report = Report::build(
            &intervals,
            None,
            at(3, 12, 0),
            day(3)..=day(1),
            &tz(),
            RoundingMode::Decimal,
        );

        assert_eq!(report.days().len(), 0);
        assert_eq!(report.intervals().len(), 0);
        assert!(report.is_empty());
    }

    #[test]
    fn degenerate_intervals_contribute_nothing() {
        let p = ProjectId::new();
        let intervals = [
            interval(&p, at(2, 10, 0), at(2, 9, 0)),
            interval(&p, at(2, 10, 0), at(2, 10, 0)),
        ];

        let report = report(&intervals, None);

        assert_eq!(report.row(&p), None);
        assert!(report.is_empty());
        // They are still records, so they are listed rather than hidden.
        assert_eq!(report.intervals().len(), 2);
    }

    #[test]
    fn each_projects_total_for_a_day_is_rounded_once_and_totals_add_rounded_cells() {
        let secs = Duration::seconds;
        let (a, b) = (ProjectId::new(), ProjectId::new());
        let intervals = [
            // A has 61s + 10s = 71s on the 2nd: one rounding gives 72s, rounding each gives 108s.
            interval(&a, at(2, 9, 0), at(2, 9, 0) + secs(61)),
            interval(&a, at(2, 10, 0), at(2, 10, 0) + secs(10)),
            interval(&a, at(3, 9, 0), at(3, 9, 0) + secs(1)),
            interval(&b, at(2, 9, 0), at(2, 9, 0) + secs(1)),
        ];

        let report = report(&intervals, None);

        assert_eq!(report.row(&a), Some(vec![secs(0), secs(72), secs(36)]));
        assert_eq!(report.row(&b), Some(vec![secs(0), secs(36), secs(0)]));
        assert_eq!(report.day_totals(), vec![secs(0), secs(108), secs(36)]);
        assert_eq!(report.project_total(&a), secs(108));
        assert_eq!(report.project_total(&b), secs(36));
        assert_eq!(report.total(), secs(144));
    }

    #[test]
    fn classic_rounding_mode_is_applied() {
        let p = ProjectId::new();
        let intervals = [interval(&p, at(2, 9, 0), at(2, 9, 4))];

        let report = Report::build(
            &intervals,
            None,
            at(3, 12, 0),
            day(2)..=day(2),
            &tz(),
            RoundingMode::Classic(3),
        );

        assert_eq!(report.row(&p), Some(vec![mins(6)]));
    }

    /// The instant at a local date and time in a zone with DST.
    fn local(tz: Tz, (y, m, d): (i32, u32, u32), hh: u32, mm: u32) -> DateTime<Utc> {
        local_on(&tz, NaiveDate::from_ymd_opt(y, m, d).unwrap(), hh, mm)
    }

    /// Reports one whole Day, using an Interval from noon the Day before to noon the Day after.
    fn whole_day(tz: Tz, (y, m, d): (i32, u32, u32)) -> Option<Vec<Duration>> {
        let p = ProjectId::new();
        let date = NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let start = local_on(&tz, date.pred_opt().unwrap(), 12, 0);
        let end = local_on(&tz, date.succ_opt().unwrap(), 12, 0);
        let intervals = [interval(&p, start, end)];

        Report::build(
            &intervals,
            None,
            end,
            date..=date,
            &tz,
            RoundingMode::Decimal,
        )
        .row(&p)
    }

    #[test]
    fn day_when_clocks_spring_forward_is_23_hours() {
        assert_eq!(
            whole_day(Denver, (2026, 3, 8)),
            Some(vec![Duration::hours(23)])
        );
    }

    #[test]
    fn day_when_clocks_fall_back_is_25_hours() {
        assert_eq!(
            whole_day(Denver, (2026, 11, 1)),
            Some(vec![Duration::hours(25)])
        );
    }

    #[test]
    fn day_whose_midnight_is_skipped_starts_when_clocks_jump() {
        // Santiago springs forward from 00:00 straight to 01:00, so the Day has no midnight.
        assert_eq!(
            whole_day(Santiago, (2026, 9, 6)),
            Some(vec![Duration::hours(23)])
        );
    }

    #[test]
    fn interval_across_a_dst_change_counts_real_time() {
        let p = ProjectId::new();
        // 23:00 to 03:30 on the clock, but 02:00-03:00 never happened.
        let intervals = [interval(
            &p,
            local(Denver, (2026, 3, 7), 23, 0),
            local(Denver, (2026, 3, 8), 3, 30),
        )];
        let days = NaiveDate::from_ymd_opt(2026, 3, 7).unwrap()
            ..=NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();

        let report = Report::build(
            &intervals,
            None,
            local(Denver, (2026, 3, 8), 12, 0),
            days,
            &Denver,
            RoundingMode::Decimal,
        );

        assert_eq!(report.row(&p), Some(vec![mins(60), mins(150)]));
    }
}
