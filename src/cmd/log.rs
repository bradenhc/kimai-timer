// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use clap::Parser;
use colored::{Color, Colorize};
use tabled::builder::Builder;
use tabled::settings::Style;
use time::macros::format_description;
use time::{Duration, OffsetDateTime, UtcOffset};
use tracing::warn;

use crate::store::{Store, TimerAction, TimerEvent, TimerEventIterator};

#[derive(Debug, Parser)]
#[command(help_template = crate::HELP_TEMPLATE_OPT, styles = crate::STYLES)]
#[allow(clippy::struct_excessive_bools)]
pub struct CommandLog {
    /// The number of days to include in the log output. Defaults to just one (the current day).
    /// Using this flag will override any other flags that may be used to control the number of
    /// days in the output.
    #[arg(long, short)]
    days: Option<i64>,

    /// Formats the output as JSONL. Each JSON object represents a single timer event, and each
    /// event is separated by a newline.
    #[arg(long, short)]
    json: bool,

    /// Display time data for the past two-weeks, or a typical pay-period (sets DAYS to 14).
    #[arg(long, short)]
    period: bool,

    /// Formats the output as raw start/stop events (same as when using the 'in' and 'out'
    /// commands).
    #[arg(long, short)]
    raw: bool,

    /// Display time data for the past week (sets DAYS to 7).
    #[arg(long, short)]
    week: bool,
}

impl CommandLog {
    pub fn execute(self) -> Result<()> {
        // Make it easier on the user by showing them time in the local offset. We collect it once
        // at the beginning in case it fails. It shouldn't, but it could. Perhaps later we can
        // handle the failure case more elegantly.

        let offset = UtcOffset::current_local_offset().unwrap();

        // Collect all the events in our window: the range of days for which we are interested in
        // viewing task times.

        let day_range = self.compute_day_range(offset);

        let store = Store::new()?;
        let all_events = store.fetch_timer_events()?;
        let events = Self::filter_events_after_start(all_events, day_range[0])?;

        // Collection complete: display the results based on user preference

        if self.raw {
            Self::log_raw(&events);
        } else if self.json {
            Self::log_json(&events);
        } else {
            Self::log_table(&events, &day_range);
        }

        Ok(())
    }

    fn compute_day_range(&self, offset: UtcOffset) -> Vec<OffsetDateTime> {
        // Truncate the current time to the start of the day and determine what day to start
        // collecting events from. Map it to offset time so that the keys match against the events
        // from the timer log.

        let today = OffsetDateTime::now_utc()
            .to_offset(offset)
            .truncate_to_day();

        // Figure out how many days to go back based on what the user configured

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

        // Build the range of days we are interested in

        (0..=days_to_go_back)
            .map(|i| start_day + Duration::days(i))
            .collect()
    }

    /// Filters all fetched timer log events so that we only have to work with the set of events
    /// that occured on or before the provided start day.
    ///
    /// The events timestamps are adjusted to local time using the same offset as the start time.
    ///
    fn filter_events_after_start(
        all_events: TimerEventIterator,
        start: OffsetDateTime,
    ) -> Result<Vec<TimerEvent>> {
        // Collect events in start/stop pairs and filter based on all events that stopped within
        // the window.

        let mut events = Vec::new();

        let mut last_start = None;

        for fetch_result in all_events {
            match fetch_result {
                Ok(event) => {
                    let offset_event = event.to_local(start.offset());

                    match offset_event.action {
                        TimerAction::Start => {
                            if last_start.is_some() {
                                bail!(
                                    "corrupt timelog: two consecutive start events for {}",
                                    offset_event.task
                                );
                            }

                            last_start = Some(offset_event);
                        }

                        TimerAction::Stop => {
                            if let Some(start_event) = last_start.take() {
                                if offset_event.timestamp >= start {
                                    events.push(start_event);
                                    events.push(offset_event);
                                }
                            } else {
                                warn!("found STOP event without matching START: {offset_event}");
                            }
                        }
                    }
                }

                Err(e) => bail!("failed to fetch timer events from log: {e}"),
            }
        }

        // Edge case: if the last event is a start (in current task) but it started on a day that
        // is outside of our current window, we need to include that information in the list we give
        // back so it shows up in the table.

        if let Some(start_event) = last_start {
            events.push(start_event);
        }

        Ok(events)
    }

    /// Logs all the events in the same format as the in and out commands, one per line.
    ///
    fn log_raw(events: &[TimerEvent]) {
        for ev in events {
            println!("{ev}");
        }
    }

    /// Logs all the events in JSONL format, one JSON record per line.
    ///
    fn log_json(events: &[TimerEvent]) {
        for ev in events {
            println!("{}", serde_json::to_string(&ev).unwrap());
        }
    }

    /// Logs all the events in a table format with one row for each task and a column for each day
    /// in the requested range.
    ///
    /// The events should already be filtered so that they fit inside the day range.
    ///
    fn log_table(events: &[TimerEvent], day_range: &[OffsetDateTime]) {
        if events.is_empty() {
            println!();
            println!("No events in time log: use `kt in` and `kt out` to track your time");
            println!();

            return;
        }

        let event_table = Self::build_table(events, day_range);

        let show_multiday_totals = day_range.len() > 1;

        let mut builder = Builder::new();

        // Build the header first row (days of week)

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

        // Build the header second row (tasks and dates)

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

        // Build the rows. We keep track of the per-task total, per-day total, and the overal total
        // so we can display them for the user to let them know total time spent on everything.
        let mut day_totals: Vec<Duration> = vec![Duration::ZERO; day_range.len()];
        let mut total = Duration::ZERO;

        for (task, day_durations) in event_table.day_durations_by_task {
            let task_name = if event_table
                .current_task
                .as_ref()
                .is_some_and(|c| c.task == task)
            {
                task.clone().green().to_string()
            } else {
                task.clone()
            };

            let mut row = vec![task_name];

            let mut task_total = Duration::ZERO;

            for (i, (day, dur)) in day_durations.iter().enumerate() {
                task_total += *dur;
                total += *dur;
                day_totals[i] += *dur;

                let cur_task_color = event_table.current_task.as_ref().and_then(|t| {
                    if t.task == task && t.timestamp.truncate_to_day() == *day {
                        Some(Color::Green)
                    } else {
                        None
                    }
                });

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

        for dur in day_totals {
            footer.push(Self::format_duration(&dur, None).italic().to_string());
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

        if let Some(c) = event_table.current_task {
            println!();
            println!("{}", format!(" current: {c:#}").green());
        }
        println!();
    }

    fn build_table(events: &[TimerEvent], day_range: &[OffsetDateTime]) -> TimerEventTable {
        // Build up a template map that we will clone for each task we find in our timer events.
        // The map is used to store the accumulated durations for each day in the window of time we
        // are interested in.

        let day_durations_template: BTreeMap<OffsetDateTime, Duration> = day_range
            .iter()
            .map(|ts| (*ts, Duration::new(0, 0)))
            .collect();

        // Build up a map of days to tasks and durations for that day. This is the information
        // we will ultimately print out to the console.
        let mut day_durations_by_task: BTreeMap<String, BTreeMap<OffsetDateTime, Duration>> =
            BTreeMap::new();

        // Keep track of the last time we saw a start for a task so we can compute the duration.
        let mut current_task: Option<TimerEvent> = None;

        // Populate the map
        for event in events {
            match event.action {
                TimerAction::Start => {
                    current_task = Some(event.clone());
                }

                TimerAction::Stop => {
                    if let Some(start) = current_task.take() {
                        Self::update_table(
                            &start,
                            event,
                            &mut day_durations_by_task,
                            &day_durations_template,
                        );
                    } else {
                        warn!("found STOP event without matching START after filter: {event}");
                    }
                }
            }
        }

        // Add in the current task duration if there is one. Also make sure to span multiple
        // days if rollover happens.

        if let Some(cur) = current_task.as_ref() {
            Self::update_table(
                cur,
                &TimerEvent::stop(cur.task.clone()).to_local(day_range[0].offset()),
                &mut day_durations_by_task,
                &day_durations_template,
            );
        }

        TimerEventTable {
            day_durations_by_task,
            current_task,
        }
    }

    fn update_table(
        start: &TimerEvent,
        stop: &TimerEvent,
        day_durations_by_task: &mut BTreeMap<String, BTreeMap<OffsetDateTime, Duration>>,
        day_durations_template: &BTreeMap<OffsetDateTime, Duration>,
    ) {
        // NOTE: We aren't going to handle the case where stop time is before the start time. If
        // that happens the log is corrupted and needs to be manually resolved before this command
        // will work. Maybe in the future we can try and make the tool rich enough to help the user
        // identify and fix these entries, but for now we will just assume they don't happen.

        // We need to make sure the start fits inside our window (this applies for an edge case
        // where the current task started on a day outside the window)

        let day_range_start = day_durations_template
            .keys()
            .next()
            .expect("missing days in template");

        // If the timer log entry spans multiple days, we need to add time to each
        // day appropriately.

        let mut cur_start = start.timestamp;

        let mut start_day = cur_start.truncate_to_day();
        let stop_day = stop.timestamp.truncate_to_day();

        while start_day <= stop_day {
            let next_day = start_day + Duration::DAY;
            let dur_to_next_day = next_day - cur_start;

            let cur_dur_remaining = stop.timestamp - cur_start;

            let dur = dur_to_next_day.min(cur_dur_remaining);

            if start_day >= *day_range_start {
                match day_durations_by_task.get_mut(&stop.task) {
                    None => {
                        let mut day_durations = day_durations_template.clone();
                        let day_dur = day_durations
                            .get_mut(&start_day)
                            .expect("missing day when cloning template");
                        *day_dur = dur;
                        day_durations_by_task.insert(stop.task.clone(), day_durations);
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

/// The result of building the table to display from a filtered view of timer events.
///
struct TimerEventTable {
    /// A map of task name to a map of days/durations for each task that has a definitive start
    /// and stop and thus was able to contribute to the duration.
    day_durations_by_task: BTreeMap<String, BTreeMap<OffsetDateTime, Duration>>,

    /// A map of any tasks that ended on a start event with no stop event. There should only ever
    /// be one. If there are more then it indicates the data may be corrupted.
    current_task: Option<TimerEvent>,
}
