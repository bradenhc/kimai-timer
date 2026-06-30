// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Kimai Timer (kt) - Seriously Simple Time Tracker
//!
//! The `kt` application tracks how much time you spend on tasks. You can "punch in" to a task using
//! `kt in` and "punch out" of a task using `kt out`. Kimai Timer maintains a timer log of punch
//! events and uses the log to report on how much time was spent on each task.
//!
//! See the repo README for more details.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::builder::styling::{Style, Styles};
use clap::{Parser, Subcommand};
use tracing::error;

mod cmd;
mod store;
mod time_ext;
mod trace;

use store::Store;

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
    /// Override the directory used to store kt data files. Also configurable via `KT_DATA_DIR`.
    #[arg(long, global = true, env = "KT_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// The subcommand to execute.
    #[clap(subcommand)]
    command: Command,
}

/// Defines the subcommands you can execute.
///
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a new time interval for a task
    #[clap(visible_alias = "a")]
    Add(cmd::CommandAdd),

    /// Lists the current set of tasks.
    #[clap(visible_alias = "ls")]
    List(cmd::CommandList),

    /// Displays a table of task durations by day.
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

/// Entry point: runs the application and maps any error to a non-zero exit code.
///
fn main() -> ExitCode {
    if let Err(e) = run_main() {
        error!("{e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Parses CLI arguments, builds the store, and dispatches to the appropriate subcommand handler.
///
fn run_main() -> Result<()> {
    let config = CliConfig::parse();

    trace::init();

    let store = match config.data_dir {
        Some(dir) => Store::new(&dir)?,
        None => Store::with_project_dir()?,
    };

    match config.command {
        Command::Add(cmd) => cmd.execute(&store),
        Command::List(cmd) => cmd.execute(&store),
        Command::Log(cmd) => cmd.execute(&store),
        Command::In(cmd) => cmd.execute(&store),
        Command::New(cmd) => cmd.execute(&store),
        Command::Out(cmd) => cmd.execute(&store),
        Command::Switch(cmd) => cmd.execute(&store),
    }
}
