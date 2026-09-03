// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines the commands supported by Kimai Timer.
//!
//! Each command is implemented inside of a submodule for maintainability.

mod add;
mod r#in;
mod list;
mod log;
mod new;
mod out;
mod switch;

pub(crate) use add::CommandAdd;
pub(crate) use r#in::CommandIn;
pub(crate) use list::CommandList;
pub(crate) use log::CommandLog;
pub(crate) use new::CommandNew;
pub(crate) use out::CommandOut;
pub(crate) use switch::CommandSwitch;
