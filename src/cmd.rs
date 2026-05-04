// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Defines the commands supported by Kimai Timer.
//!
//! Each command is implemented inside of a submodule for maintainability.

mod r#in;
mod list;
mod log;
mod new;
mod out;
mod switch;

pub use r#in::CommandIn;
pub use list::CommandList;
pub use log::CommandLog;
pub use new::CommandNew;
pub use out::CommandOut;
pub use switch::CommandSwitch;
