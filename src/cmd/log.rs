// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt log` command for displaying time-tracking history.
//!
//! The command reads all store events, filters them to a configurable date window (defaulting to
//! today), and renders the result in one of three formats: a human-readable table, raw one-line
//! records, or JSONL. The table format groups time by task and day, shows per-day and per-task
//! totals, and highlights the currently active task in green. Intervals that span midnight are
//! split across calendar days so every day column reflects only the portion of work done that day.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use clap::Parser;
use colored::{Color, Colorize};
use tabled::builder::Builder;
use tabled::settings::Style;
use time::macros::format_description;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::store::{
    CurrentTask, PersistedEventIterator, RoundingMode, Store, StoreEvent, TaskDuration,
    TimeInterval,
};

/// Arguments and flags for the `kt log` subcommand.
///
/// Controls the date window and output format. At most one output format is active at a time;
/// `--raw` and `--json` are mutually exclusive with the default table view.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
#[allow(clippy::struct_excessive_bools)]
pub struct CommandLog {
    /// The number of days to include in the log output. Defaults to just one (the current day).
    /// Using this flag will override any other flags that may be used to control the number of
    /// days in the output.
    #[arg(long, short)]
    days: Option<i64>,

    /// Formats the output as JSONL. Each JSON object represents a single store event, and each
    /// event is separated by a newline.
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
    /// Fetches events from the store, filters to the requested date window, and renders output.
    ///
    pub fn execute(self, store: &Store) -> Result<()> {
        let offset = UtcOffset::current_local_offset().unwrap();

        let day_range = self.compute_day_range(offset);

        let current = store.get_current_task()?;
        let all_events = store.fetch_events()?;
        let intervals = Self::filter_events_in_range(all_events, day_range[0])?;

        if self.raw {
            Self::log_raw(&intervals);
        } else if self.json {
            Self::log_json(&intervals);
        } else {
            Self::log_table(&intervals, current.as_ref(), &day_range);
        }

        Ok(())
    }

    /// Builds the ordered list of days to display, from the earliest day through today.
    ///
    /// The window length is resolved in priority order: `--days` > `--period` > `--week` > 1.
    /// Returns a `Vec` rather than a range so callers can index into it for column headers and
    /// template construction without re-evaluating the priority logic.
    ///
    fn compute_day_range(&self, offset: UtcOffset) -> Vec<OffsetDateTime> {
        let today = OffsetDateTime::now_utc()
            .to_offset(offset)
            .truncate_to_day();

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

    /// Walks all persisted events and keeps only `CreateInterval` entries whose end time falls on
    /// or after `start`.
    ///
    /// Filtering by end time (rather than start time) ensures intervals that began before the
    /// window but finished inside it are still counted — a common occurrence for long-running
    /// tasks that span midnight.
    ///
    fn filter_events_in_range(
        all_events: PersistedEventIterator,
        start: OffsetDateTime,
    ) -> Result<Vec<TimeInterval>> {
        let mut intervals = Vec::new();

        for fetch_result in all_events {
            match fetch_result {
                Ok(StoreEvent::CreateInterval(interval)) => {
                    let local_end = interval.end.to_offset(start.offset());
                    if local_end >= start {
                        intervals.push(interval);
                    }
                }
                Err(e) => bail!("failed to fetch events from log: {e}"),
            }
        }

        Ok(intervals)
    }

    /// Prints each interval as a single human-readable line: `<task> <start> - <end> (HH:MM)`.
    ///
    fn log_raw(intervals: &[TimeInterval]) {
        let offset = UtcOffset::current_local_offset().unwrap();
        let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
        for interval in intervals {
            let local = interval.to_local(offset);
            let start = local.start.format(fmt).unwrap();
            let end = local.end.format(fmt).unwrap();
            let dur = local.end - local.start;
            println!(
                "{} {} - {} ({:02}:{:02})",
                local.task,
                start,
                end,
                dur.whole_hours(),
                dur.whole_minutes() % 60
            );
        }
    }

    /// Serializes each interval as a `StoreEvent::CreateInterval` JSON object, one per line (JSONL).
    ///
    /// Emitting the full `StoreEvent` envelope keeps the output compatible with the store's own
    /// event format, making it straightforward to pipe into other tools or re-import.
    ///
    fn log_json(intervals: &[TimeInterval]) {
        for interval in intervals {
            let event = StoreEvent::CreateInterval(interval.clone());
            println!("{}", serde_json::to_string(&event).unwrap());
        }
    }

    /// Renders a human-readable table of task durations grouped by day.
    ///
    /// Prints a helpful hint and returns early when there are no events or the aggregated table is
    /// empty. When the window spans more than one day, an extra TOTAL column is appended on the
    /// right for per-task and grand totals. The currently active task row and its non-zero cells
    /// are highlighted green to distinguish in-progress time from completed intervals.
    ///
    #[allow(clippy::too_many_lines)]
    fn log_table(
        intervals: &[TimeInterval],
        current: Option<&CurrentTask>,
        day_range: &[OffsetDateTime],
    ) {
        if intervals.is_empty() && current.is_none() {
            println!();
            println!("No events in time log window: use `kt in` and `kt out` to track your time");
            println!();

            return;
        }

        let interval_table = Self::build_table(intervals, current, day_range);

        if interval_table.day_durations_by_task.is_empty() {
            println!();
            println!("No events in time log window: use `kt in` and `kt out` to track your time");
            println!();

            return;
        }

        let show_multiday_totals = day_range.len() > 1;

        let mut builder = Builder::new();

        let mut header = vec![String::new()];

        for ts in day_range {
            header.push(
                ts.format(&format_description!("[weekday repr:short]"))
                    .unwrap()
                    .bold()
                    .to_string(),
            );
        }

        builder.push_record(header);

        let mut header = vec!["TASK".bold().to_string()];

        for ts in day_range {
            header.push(
                ts.format(&format_description!("[month]/[day]"))
                    .unwrap()
                    .bold()
                    .to_string(),
            );
        }

        if show_multiday_totals {
            header.push("TOTAL".italic().to_string());
        }

        builder.push_record(header);

        let mut day_totals: Vec<Duration> = vec![Duration::ZERO; day_range.len()];
        let mut total = Duration::ZERO;

        for (task, day_durations) in &interval_table.day_durations_by_task {
            let is_current = interval_table
                .current_task
                .as_deref()
                .is_some_and(|c| c == task);

            let task_name = if is_current {
                task.clone().green().to_string()
            } else {
                task.clone()
            };

            let mut row = vec![task_name];

            let mut task_total = Duration::ZERO;

            for (i, (_day, dur)) in day_durations.iter().enumerate() {
                let rounded = TaskDuration::new(*dur).rounded(&RoundingMode::default());
                task_total += rounded;
                total += rounded;
                day_totals[i] += rounded;

                let cur_task_color = if is_current && !dur.is_zero() {
                    Some(Color::Green)
                } else {
                    None
                };

                row.push(Self::format_duration(&rounded, cur_task_color));
            }

            if show_multiday_totals {
                row.push(
                    Self::format_duration(&task_total, None)
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

        if let Some(cur) = current
            && let Ok(start) = OffsetDateTime::from_unix_timestamp(cur.start)
        {
            let offset = day_range[0].offset();
            let local_start = start
                .to_offset(offset)
                .format(&format_description!(
                    "[year]-[month]-[day] [hour]:[minute]:[second]"
                ))
                .unwrap();
            println!();
            println!(
                "{}",
                format!(" current: {} (started {})", cur.task, local_start).green()
            );
        }
        println!();
    }

    /// Aggregates `intervals` and the live in-progress task (if any) into per-task, per-day
    /// duration buckets ready for rendering.
    ///
    /// Pre-populates every task row with zero-duration entries for each day using
    /// `day_durations_template`, so days with no activity print as `00:00` rather than being
    /// absent from the table. The in-progress task is included by treating "now" as its end time.
    ///
    fn build_table(
        intervals: &[TimeInterval],
        current: Option<&CurrentTask>,
        day_range: &[OffsetDateTime],
    ) -> IntervalTable {
        let day_durations_template: BTreeMap<OffsetDateTime, Duration> = day_range
            .iter()
            .map(|ts| (*ts, Duration::new(0, 0)))
            .collect();

        let mut day_durations_by_task: BTreeMap<String, BTreeMap<OffsetDateTime, Duration>> =
            BTreeMap::new();

        let offset = day_range[0].offset();

        for interval in intervals {
            let local = interval.to_local(offset);
            Self::update_table(
                local.start,
                local.end,
                &local.task,
                &mut day_durations_by_task,
                &day_durations_template,
            );
        }

        let current_task_name = if let Some(cur) = current {
            if let Ok(start) = OffsetDateTime::from_unix_timestamp(cur.start) {
                let local_start = start.to_offset(offset);
                let local_end = OffsetDateTime::now_utc()
                    .to_offset(offset)
                    .truncate_to_second();
                Self::update_table(
                    local_start,
                    local_end,
                    &cur.task,
                    &mut day_durations_by_task,
                    &day_durations_template,
                );
            }
            Some(cur.task.clone())
        } else {
            None
        };

        IntervalTable {
            day_durations_by_task,
            current_task: current_task_name,
        }
    }

    /// Adds the duration from `[start, end)` to the appropriate per-day bucket for `task`.
    ///
    /// Intervals that cross midnight are split so each calendar day receives only the portion of
    /// work that falls within it. Days before the template's earliest key are skipped — they are
    /// outside the display window but can appear when a task was started before the window opened.
    ///
    fn update_table(
        start: OffsetDateTime,
        end: OffsetDateTime,
        task: &str,
        day_durations_by_task: &mut BTreeMap<String, BTreeMap<OffsetDateTime, Duration>>,
        day_durations_template: &BTreeMap<OffsetDateTime, Duration>,
    ) {
        let day_range_start = day_durations_template
            .keys()
            .next()
            .expect("missing days in template");

        let mut cur_start = start;
        let mut start_day = cur_start.truncate_to_day();
        let stop_day = end.truncate_to_day();

        while start_day <= stop_day {
            let next_day = start_day + Duration::DAY;
            let dur_to_next_day = next_day - cur_start;
            let cur_dur_remaining = end - cur_start;
            let dur = dur_to_next_day.min(cur_dur_remaining);

            if start_day >= *day_range_start {
                match day_durations_by_task.get_mut(task) {
                    None => {
                        let mut day_durations = day_durations_template.clone();
                        let day_dur = day_durations
                            .get_mut(&start_day)
                            .expect("missing day when cloning template");
                        *day_dur = dur;
                        day_durations_by_task.insert(task.to_string(), day_durations);
                    }

                    Some(day_durations) => {
                        let duration = day_durations
                            .get_mut(&start_day)
                            .expect("missing day when accumulating durations");
                        *duration += dur;
                    }
                }
            }

            start_day = next_day;
            cur_start += dur;
        }
    }

    /// Formats a `Duration` as `HH:MM`, optionally applying a terminal color.
    ///
    /// Uses whole hours and remaining minutes so 90 minutes prints as `01:30`, not `00:90`.
    ///
    fn format_duration(dur: &Duration, color: Option<Color>) -> String {
        let s = format!("{:02}:{:02}", dur.whole_hours(), dur.whole_minutes() % 60);

        if let Some(c) = color {
            s.color(c).to_string()
        } else {
            s
        }
    }
}

/// Intermediate aggregation produced by `build_table` and consumed by `log_table`.
///
/// Keeps tasks in sorted order via `BTreeMap` so table rows are stable across runs.
///
struct IntervalTable {
    /// Per-task map of day-start timestamps to the accumulated duration for that day.
    day_durations_by_task: BTreeMap<String, BTreeMap<OffsetDateTime, Duration>>,

    /// Name of the currently running task, if any; used to apply green highlighting in the table.
    current_task: Option<String>,
}
