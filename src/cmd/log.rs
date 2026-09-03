// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt log` command for displaying time-tracking history.
//!
//! The command reads completed intervals from the store, filters them to a configurable date window
//! (defaulting to today), and renders the result in one of three formats: a human-readable table,
//! raw one-line records, or JSONL. The table format groups time by project and day, shows per-day
//! and per-project totals, and highlights the project currently being worked in green. Intervals
//! that span midnight are split across calendar days so every day column reflects only the portion
//! of work done that day.
//!
//! Work in progress is not an interval, so the running session is merged in separately by treating
//! "now" as its end. That keeps the table honest without inventing a completed record that was
//! never written to the log.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use clap::Parser;
use colored::{Color, Colorize};
use serde::Serialize;
use tabled::builder::Builder;
use tabled::settings::Style;

use crate::store::{Interval, ProjectId, RoundingMode, Store, TimeDuration};

/// Arguments and flags for the `kt log` subcommand.
///
/// Controls the date window and output format. At most one output format is active at a time;
/// `--raw` and `--json` are mutually exclusive with the default table view.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct CommandLog {
    /// The number of days to include in the log output. Defaults to just one (the current day).
    /// Using this flag will override any other flags that may be used to control the number of
    /// days in the output.
    #[arg(long, short)]
    days: Option<i64>,

    /// Formats the output as JSONL. Each JSON object represents a single interval, and each
    /// interval is separated by a newline.
    #[arg(long, short)]
    json: bool,

    /// Display time data for the past two-weeks, or a typical pay-period (sets DAYS to 14).
    #[arg(long, short)]
    period: bool,

    /// Formats the output as raw interval records, one per line.
    #[arg(long, short)]
    raw: bool,

    /// Display time data for the past week (sets DAYS to 7).
    #[arg(long, short)]
    week: bool,
}

impl CommandLog {
    /// Filters stored intervals to the requested date window and renders output.
    ///
    /// Returns `Result` even though nothing here can fail, so that every command shares one
    /// signature and `main` can dispatch to them uniformly.
    ///
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn execute(self, store: &Store) -> Result<()> {
        let day_range = self.compute_day_range();
        let window_start = day_range[0];

        let intervals: Vec<&Interval> = store
            .intervals()
            .iter()
            // Filtering by end time keeps intervals that began before the window but finished
            // inside it, which is common for work that spans midnight.
            .filter(|i| i.end.with_timezone(&Local).date_naive() >= window_start)
            .collect();

        if self.raw {
            Self::log_raw(store, &intervals);
        } else if self.json {
            Self::log_json(store, &intervals);
        } else {
            Self::log_table(store, &intervals, &day_range);
        }

        Ok(())
    }

    /// Builds the ordered list of days to display, from the earliest day through today.
    ///
    /// The window length is resolved in priority order: `--days` > `--period` > `--week` > 1.
    /// Returns a `Vec` rather than a range so callers can index into it for column headers and
    /// template construction without re-evaluating the priority logic.
    ///
    fn compute_day_range(&self) -> Vec<NaiveDate> {
        let today = Local::now().date_naive();

        let days = if let Some(d) = self.days {
            d
        } else if self.period {
            14
        } else if self.week {
            7
        } else {
            1
        };

        let days_to_go_back = days - 1;
        let start_day = today - Duration::days(days_to_go_back);

        (0..=days_to_go_back)
            .map(|i| start_day + Duration::days(i))
            .collect()
    }

    /// Prints each interval as a single human-readable line: `<project> <start> - <end> (HH:MM)`.
    ///
    fn log_raw(store: &Store, intervals: &[&Interval]) {
        let fmt = "%Y-%m-%d %H:%M:%S";

        for interval in intervals {
            let local_start = interval.start.with_timezone(&Local);
            let local_end = interval.end.with_timezone(&Local);
            let dur = local_end - local_start;

            println!(
                "{} {} - {} ({:02}:{:02})",
                Self::project_name(store, &interval.project_id),
                local_start.format(fmt),
                local_end.format(fmt),
                dur.num_hours(),
                dur.num_minutes() % 60
            );
        }
    }

    /// Serializes each interval as a JSON object, one per line (JSONL).
    ///
    /// This is a reporting format rather than a backup format: it reports the derived interval, not
    /// the events that produced it. A faithful copy of the store is the log file itself.
    ///
    fn log_json(store: &Store, intervals: &[&Interval]) {
        for interval in intervals {
            let record = IntervalRecord {
                id: interval.id.to_string(),
                project: ProjectRecord {
                    id: interval.project_id.to_string(),
                    name: store.project(&interval.project_id).map(|p| p.name.as_str()),
                },
                start: interval.start.timestamp(),
                end: interval.end.timestamp(),
            };

            println!("{}", serde_json::to_string(&record).unwrap_or_default());
        }
    }

    /// Renders a human-readable table of project durations grouped by day.
    ///
    /// Prints a helpful hint and returns early when there is nothing to show. When the window spans
    /// more than one day, an extra TOTAL column is appended on the right for per-project and grand
    /// totals. The active project's row and its non-zero cells are highlighted green to distinguish
    /// in-progress time from completed intervals.
    ///
    #[allow(clippy::too_many_lines)]
    fn log_table(store: &Store, intervals: &[&Interval], day_range: &[NaiveDate]) {
        if intervals.is_empty() && store.session().is_none() {
            Self::print_empty_hint();
            return;
        }

        let table_data = Self::build_table(store, intervals, day_range);

        if table_data.day_durations_by_project.is_empty() {
            Self::print_empty_hint();
            return;
        }

        let show_multiday_totals = day_range.len() > 1;

        let mut builder = Builder::new();

        let mut header = vec![String::new()];
        for ts in day_range {
            header.push(ts.format("%a").to_string().bold().to_string());
        }
        builder.push_record(header);

        let mut header = vec!["PROJECT".bold().to_string()];
        for ts in day_range {
            header.push(ts.format("%-m/%-d").to_string().bold().to_string());
        }
        if show_multiday_totals {
            header.push("TOTAL".italic().to_string());
        }
        builder.push_record(header);

        let mut day_totals: Vec<Duration> = vec![Duration::zero(); day_range.len()];
        let mut total = Duration::zero();

        // Rows are ordered by project name rather than by ID so the table reads the same way the
        // user thinks about their projects.
        let mut rows: Vec<_> = table_data.day_durations_by_project.into_iter().collect();
        rows.sort_by_cached_key(|(id, _)| Self::project_name(store, id));

        for (project_id, day_durations) in rows {
            let is_current = table_data.current_project.as_ref() == Some(&project_id);
            let name = Self::project_name(store, &project_id);

            let mut row = vec![if is_current {
                name.green().to_string()
            } else {
                name
            }];

            let mut project_total = Duration::zero();

            for (i, (_day, dur)) in day_durations.iter().enumerate() {
                let rounded = TimeDuration::new(*dur, RoundingMode::default()).rounded();

                project_total += rounded;
                total += rounded;
                day_totals[i] += rounded;

                let color = if is_current && !dur.is_zero() {
                    Some(Color::Green)
                } else {
                    None
                };

                row.push(Self::format_duration(&rounded, color));
            }

            if show_multiday_totals {
                row.push(
                    Self::format_duration(&project_total, None)
                        .italic()
                        .to_string(),
                );
            }

            builder.push_record(row);
        }

        let mut footer = vec!["TOTAL".italic().to_string()];
        for dur in &day_totals {
            footer.push(Self::format_duration(dur, None).italic().to_string());
        }
        if show_multiday_totals {
            footer.push(
                Self::format_duration(&total, None)
                    .italic()
                    .bold()
                    .to_string(),
            );
        }
        builder.push_record(footer);

        let mut table = builder.build();
        table.with(Style::blank());

        println!();
        println!("{table}");

        if let Some(session) = store.session() {
            let local_start = session
                .start
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S");

            println!();
            println!(
                "{}",
                format!(
                    " current: {} (started {local_start})",
                    Self::project_name(store, &session.project_id)
                )
                .green()
            );
        }

        println!();
    }

    /// Prints the hint shown when the window contains no time at all.
    ///
    fn print_empty_hint() {
        println!();
        println!("No events in time log window: use `kt in` and `kt out` to track your time");
        println!();
    }

    /// Aggregates intervals and the running session into per-project, per-day duration buckets.
    ///
    /// Pre-populates every project row with zero-duration entries for each day using
    /// `day_durations_template`, so days with no activity print as `00:00` rather than being
    /// absent from the table. The running session is included by treating "now" as its end.
    ///
    fn build_table(
        store: &Store,
        intervals: &[&Interval],
        day_range: &[NaiveDate],
    ) -> IntervalTable {
        let day_durations_template: BTreeMap<NaiveDate, Duration> =
            day_range.iter().map(|d| (*d, Duration::zero())).collect();

        let mut day_durations_by_project: BTreeMap<ProjectId, BTreeMap<NaiveDate, Duration>> =
            BTreeMap::new();

        for interval in intervals {
            Self::update_table(
                interval.start.with_timezone(&Local),
                interval.end.with_timezone(&Local),
                &interval.project_id,
                &mut day_durations_by_project,
                &day_durations_template,
            );
        }

        let current_project = store.session().map(|session| {
            Self::update_table(
                session.start.with_timezone(&Local),
                Utc::now().with_timezone(&Local),
                &session.project_id,
                &mut day_durations_by_project,
                &day_durations_template,
            );

            session.project_id.clone()
        });

        IntervalTable {
            day_durations_by_project,
            current_project,
        }
    }

    /// Adds the duration from `[start, end)` to the appropriate per-day bucket for a project.
    ///
    /// Intervals that cross midnight are split so each calendar day receives only the portion of
    /// work that falls within it. Days before the template's earliest key are skipped - they are
    /// outside the display window but can appear when work started before the window opened.
    ///
    fn update_table(
        start: DateTime<Local>,
        end: DateTime<Local>,
        project_id: &ProjectId,
        day_durations_by_project: &mut BTreeMap<ProjectId, BTreeMap<NaiveDate, Duration>>,
        day_durations_template: &BTreeMap<NaiveDate, Duration>,
    ) {
        let day_range_start = day_durations_template
            .keys()
            .next()
            .expect("missing days in template");

        let mut cur_start = start;
        let mut start_day = cur_start.date_naive();
        let stop_day = end.date_naive();

        while start_day <= stop_day {
            let next_day = start_day.succ_opt().expect("date overflow");

            let next_day_dt = Local
                .from_local_datetime(&next_day.and_time(NaiveTime::MIN))
                .earliest()
                .unwrap_or_else(|| {
                    // DST gap: use the first valid time on next_day instead
                    Local
                        .from_local_datetime(
                            &next_day.and_time(NaiveTime::from_hms_opt(1, 0, 0).unwrap()),
                        )
                        .earliest()
                        .unwrap()
                });

            let dur_to_next_day = next_day_dt - cur_start;
            let cur_dur_remaining = end - cur_start;
            let dur = dur_to_next_day.min(cur_dur_remaining);

            if start_day >= *day_range_start {
                let day_durations = day_durations_by_project
                    .entry(project_id.clone())
                    .or_insert_with(|| day_durations_template.clone());

                let duration = day_durations
                    .get_mut(&start_day)
                    .expect("missing day when accumulating durations");

                *duration += dur;
            }

            start_day = next_day;
            cur_start += dur;
        }
    }

    /// Resolves a project name for display, degrading rather than failing on an unknown reference.
    ///
    fn project_name(store: &Store, id: &ProjectId) -> String {
        store
            .project(id)
            .map_or_else(|| String::from("<unknown>"), |p| p.name.to_string())
    }

    /// Formats a `Duration` as `HH:MM`, optionally applying a terminal color.
    ///
    /// Uses whole hours and remaining minutes so 90 minutes prints as `01:30`, not `00:90`.
    ///
    fn format_duration(dur: &Duration, color: Option<Color>) -> String {
        let s = format!("{:02}:{:02}", dur.num_hours(), dur.num_minutes() % 60);

        if let Some(c) = color {
            s.color(c).to_string()
        } else {
            s
        }
    }
}

/// Intermediate aggregation produced by `build_table` and consumed by `log_table`.
///
struct IntervalTable {
    /// Per-project map of calendar dates to the accumulated duration for that day.
    day_durations_by_project: BTreeMap<ProjectId, BTreeMap<NaiveDate, Duration>>,

    /// The project currently being worked, used to apply green highlighting in the table.
    current_project: Option<ProjectId>,
}

/// One interval as emitted by `--json`.
///
#[derive(Serialize)]
struct IntervalRecord<'a> {
    /// The ID of the event that created the interval.
    id: String,

    /// The project the work belongs to.
    project: ProjectRecord<'a>,

    /// Start of the interval, as seconds since the UNIX epoch.
    start: i64,

    /// End of the interval, as seconds since the UNIX epoch.
    end: i64,
}

/// The project an emitted interval belongs to.
///
#[derive(Serialize)]
struct ProjectRecord<'a> {
    /// The project's stable ID.
    id: String,

    /// The project's name, or `null` when the store has no record of it.
    name: Option<&'a str>,
}
