// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Implements the `kt log` command for displaying time-tracking history.
//!
//! The command builds a [`Report`] over a configurable window of days ending today (by default just
//! today) and renders it in one of three formats: a human-readable table, raw one-line records, or
//! JSONL. Deciding what time falls on which day, and rounding it, belongs to the Report; this module
//! only resolves project names and prints. The table highlights the project currently being worked
//! in green.

use std::ops::RangeInclusive;

use anyhow::Result;
use chrono::{Duration, Local, NaiveDate, Utc};
use clap::Parser;
use colored::{Color, Colorize};
use serde::Serialize;
use tabled::builder::Builder;
use tabled::settings::Style;

use crate::report::Report;
use crate::store::{Interval, ProjectId, RoundingMode, Store};

/// Arguments and flags for the `kt log` subcommand.
///
/// Controls the date window and output format. At most one output format is active at a time;
/// `--raw` and `--json` are mutually exclusive with the default table view.
///
#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct CommandLog {
    /// The number of days in the report. Defaults to just one (the current day). Using this flag
    /// will override any other flags that may be used to control the number of days in the
    /// report. Must be from 1 to 36525 (a century).
    #[arg(long, short, allow_negative_numbers = true, value_parser = parse_days)]
    days: Option<u32>,

    /// Formats the output as JSONL. Each JSON object represents a single interval, and each
    /// interval is separated by a newline.
    #[arg(long, short)]
    json: bool,

    /// Display time data for the past two weeks, ending today (sets DAYS to 14).
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
        let now = Utc::now();
        let days = self.day_range(now.with_timezone(&Local).date_naive());

        let report = Report::build(
            store.intervals(),
            store.session(),
            now,
            days,
            &Local,
            RoundingMode::default(),
        );

        if self.raw {
            Self::log_raw(store, report.intervals());
        } else if self.json {
            Self::log_json(store, report.intervals());
        } else {
            Self::log_table(store, &report);
        }

        Ok(())
    }

    /// Resolves the window of days to report on, from the earliest day through `today`.
    ///
    /// The window length is resolved in priority order: `--days` > `--period` > `--week` > 1.
    ///
    fn day_range(&self, today: NaiveDate) -> RangeInclusive<NaiveDate> {
        let days = if let Some(d) = self.days {
            d
        } else if self.period {
            14
        } else if self.week {
            7
        } else {
            1
        };

        (today - Duration::days(i64::from(days) - 1))..=today
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
    /// totals. The current project's row and its non-zero cells are highlighted green to distinguish
    /// in-progress time from completed intervals.
    ///
    fn log_table(store: &Store, report: &Report) {
        if report.is_empty() {
            Self::print_empty_hint();
            return;
        }

        let days = report.days();
        let show_multiday_totals = days.len() > 1;

        let mut builder = Builder::new();

        let mut header = vec![String::new()];
        for day in days {
            header.push(day.format("%a").to_string().bold().to_string());
        }
        builder.push_record(header);

        let mut header = vec!["PROJECT".bold().to_string()];
        for day in days {
            header.push(day.format("%-m/%-d").to_string().bold().to_string());
        }
        if show_multiday_totals {
            header.push("TOTAL".italic().to_string());
        }
        builder.push_record(header);

        // Rows are ordered by project name rather than by ID so the table reads the same way the
        // user thinks about their projects.
        let mut rows: Vec<_> = report.rows().collect();
        rows.sort_by_cached_key(|(id, _)| Self::project_name(store, id));

        for (project_id, cells) in rows {
            let is_current = report.session_project() == Some(project_id);
            let name = Self::project_name(store, project_id);

            let mut row = vec![if is_current {
                name.green().to_string()
            } else {
                name
            }];

            for dur in cells {
                let color = if is_current && !dur.is_zero() {
                    Some(Color::Green)
                } else {
                    None
                };

                row.push(Self::format_duration(dur, color));
            }

            if show_multiday_totals {
                row.push(
                    Self::format_duration(&report.project_total(project_id), None)
                        .italic()
                        .to_string(),
                );
            }

            builder.push_record(row);
        }

        let mut footer = vec!["TOTAL".italic().to_string()];
        for dur in &report.day_totals() {
            footer.push(Self::format_duration(dur, None).italic().to_string());
        }
        if show_multiday_totals {
            footer.push(
                Self::format_duration(&report.total(), None)
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
        println!("No time recorded in this window: use `kt in` and `kt out` to track your time");
        println!();
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

/// The most days `--days` accepts: a century, far beyond any real report, and well short of the
/// dates chrono can represent.
const MAX_DAYS: u32 = 36_525;

/// Parses `--days`, which must be a whole number from 1 to [`MAX_DAYS`].
///
fn parse_days(s: &str) -> Result<u32, String> {
    let days: i64 = s
        .parse()
        .map_err(|_| String::from("must be a whole number of days"))?;

    if days < 1 {
        return Err(String::from("must be at least 1"));
    }

    if days > i64::from(MAX_DAYS) {
        return Err(format!("must be at most {MAX_DAYS}"));
    }

    u32::try_from(days).map_err(|_| format!("must be at most {MAX_DAYS}"))
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

// -------------------------------------------------------------------------------------------------
// END OF LOGIC - MODULE UNIT TESTS BELOW HERE
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CommandLog, clap::Error> {
        CommandLog::try_parse_from(std::iter::once("log").chain(args.iter().copied()))
    }

    #[test]
    fn days_below_one_are_rejected() {
        for days in ["0", "-1"] {
            let err = parse(&["--days", days]).expect_err(days);
            assert!(err.to_string().contains("must be at least 1"), "{err}");
        }
    }

    #[test]
    fn days_beyond_a_century_are_rejected() {
        let err = parse(&["--days", "200000000"]).expect_err("too many days");
        assert!(err.to_string().contains("must be at most"), "{err}");
    }

    #[test]
    fn one_or_more_days_are_accepted() {
        assert_eq!(parse(&["--days", "1"]).unwrap().days, Some(1));
        assert_eq!(parse(&["-d", "30"]).unwrap().days, Some(30));
    }
}
