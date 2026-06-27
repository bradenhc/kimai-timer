// Copyright (c) 2026 Braden Hitchcock - MIT License (see LICENSE file for details)

//! Extension trait for [`chrono::DateTime`] providing truncation helpers absent from the chrono
//! public API.
//!
//! Import [`DateTimeExt`] wherever truncation is needed; implementations are provided for
//! [`DateTime<Utc>`].

use chrono::{DateTime, Timelike, Utc};

/// Adds truncation methods missing from chrono's standard [`DateTime`] API.
///
pub trait DateTimeExt: Sized {
    /// Strips sub-second precision, returning a value with nanoseconds set to zero.
    ///
    fn truncate_to_second(self) -> Self;
}

impl DateTimeExt for DateTime<Utc> {
    fn truncate_to_second(self) -> Self {
        self.with_nanosecond(0).unwrap()
    }
}
