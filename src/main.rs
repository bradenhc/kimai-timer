// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Kimai Timer (kt) - Seriously Simple Time Tracker
//!
//! The `kt` application tracks how much time you spend on tasks. You can "punch in" to a task using
//! `kt in` and "punch out" of a task using `kt out`. Kimai Timer maintains a timer log of punch
//! events and uses the log to report on how much time was spent on each task.
//!
//! See the repo README for more details.

use std::process::ExitCode;

use anyhow::Result;
use clap::builder::styling::{Style, Styles};
use clap::{Parser, Subcommand};
use tracing::error;

mod cmd;
mod store;
mod trace;

/// Removes all styles from the command-line help text to keep things simple.
const STYLES: Styles = Styles::styled().usage(Style::new());

/// Defines a template that shows options and arguments.
const HELP_TEMPLATE_OPT_ARG: &str = "\
{about}

USAGE
  {usage}

ARGUMENTS
{positionals}

OPTIONS
{options}
";

/// Defines a template that shows only options.
const HELP_TEMPLATE_OPT: &str = "\
{about}

USAGE
  {usage}

OPTIONS
{options}
";

/// Defines a template that shows subcommands.
const HELP_TEMPLATE_CMD: &str = "\
{about}

USAGE
  {usage}

COMMANDS
{subcommands}

OPTIONS
{options}
";

/// The max width of help text in the terminal.
const TERM_WIDTH: usize = 80;

/// kt - Seriously Simple Time Tracker
///
/// Allows you to punch in and out of various tasks to track time spent on them.
///
#[derive(Debug, Parser)]
#[clap(
    about,
    help_template = HELP_TEMPLATE_CMD,
    styles = STYLES,
    term_width = TERM_WIDTH,
    version
)]
pub struct CliConfig {
    /// The subcommand to execute.
    #[clap(subcommand)]
    command: Command,
}

/// Defines the subcommands you can execute.
///
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Lists the current set of tasks.
    #[clap(visible_alias = "ls")]
    List(cmd::CommandList),

    /// Displays a table of task durations organized by days.
    #[clap(visible_alias = "l")]
    Log(cmd::CommandLog),

    /// Punch in to a task (start)
    #[clap(visible_alias = "i")]
    In(cmd::CommandIn),

    /// Create a new task alias
    #[clap(visible_alias = "n")]
    New(cmd::CommandNew),

    /// Punch out of the current task (stop)
    #[clap(visible_alias = "o")]
    Out(cmd::CommandOut),

    /// Switch between current and last task
    #[clap(visible_alias = "s")]
    Switch(cmd::CommandSwitch),
}

fn main() -> ExitCode {
    if let Err(e) = run_main() {
        error!("{e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_main() -> Result<()> {
    let config = CliConfig::parse();

    trace::init();

    match config.command {
        Command::List(cmd) => cmd.execute(),
        Command::Log(cmd) => cmd.execute(),
        Command::In(cmd) => cmd.execute(),
        Command::New(cmd) => cmd.execute(),
        Command::Out(cmd) => cmd.execute(),
        Command::Switch(cmd) => cmd.execute(),
    }
}
