// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use clap::Parser;
use colored::{Color, Colorize};
use tabled::builder::Builder;
use tabled::settings::Style;
use time::macros::format_description;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::store::{CurrentTask, PersistedEventIterator, Store, StoreEvent, TimeInterval};

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
    pub fn execute(self) -> Result<()> {
        let offset = UtcOffset::current_local_offset().unwrap();

        let day_range = self.compute_day_range(offset);

        let store = Store::new()?;
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

    fn log_json(intervals: &[TimeInterval]) {
        for interval in intervals {
            let event = StoreEvent::CreateInterval(interval.clone());
            println!("{}", serde_json::to_string(&event).unwrap());
        }
    }

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
                task_total += *dur;
                total += *dur;
                day_totals[i] += *dur;

                let cur_task_color = if is_current && !dur.is_zero() {
                    Some(Color::Green)
                } else {
                    None
                };

                row.push(Self::format_duration(dur, cur_task_color));
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

    fn format_duration(dur: &Duration, color: Option<Color>) -> String {
        let s = format!("{:02}:{:02}", dur.whole_hours(), dur.whole_minutes() % 60);

        if let Some(c) = color {
            s.color(c).to_string()
        } else {
            s
        }
    }
}

struct IntervalTable {
    day_durations_by_task: BTreeMap<String, BTreeMap<OffsetDateTime, Duration>>,
    current_task: Option<String>,
}
