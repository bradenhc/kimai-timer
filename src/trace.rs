// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines logic to initialize a tracing subscriber to format log records.

use colored::Colorize;
use std::fmt;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

/// Custom formatter that prints simple, consistent log records for kt
struct Formatter;

impl<S, N> FormatEvent<S, N> for Formatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();

        // Convert level to lowercase string
        let level = match *metadata.level() {
            Level::ERROR => "error:".bold().red(),
            Level::WARN => "warning:".bold().yellow(),
            Level::INFO => "info:".bold().green(),
            Level::DEBUG => "debug:".bold().purple(),
            Level::TRACE => "trace:".bold().cyan(),
        };

        write!(writer, "{level} ")?;
        write!(writer, "{}: ", metadata.target())?;
        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

/// Initializes the tracing subscriber used to print log records to the console.
pub fn init() {
    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .event_format(Formatter)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("failed to set global subscriber");
}
