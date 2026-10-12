# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `kt add` reports the exact duration of the interval it added, instead of
  rounding it. Rounding now happens only in `kt log`.

## [0.5.0] - 2026-09-03

### Changed

- Everything `kt` records is kept in a single append-only event log,
  `events.jsonl`. Projects, the current session, and every interval are derived
  from it, so a crash or power loss can no longer lose a punch or leave the
  store half-updated.
- A project has a stable ID separate from its name.
- Punching in to a project while punched in to another records the punch out and
  the punch in together, in one write.
- Commands that only read, such as `kt log`, no longer create the data
  directory.

### Removed

- Data files written by 0.4.0 and earlier (`timelog.jsonl`, `taskset.json`,
  `current`, `last`) are no longer read, and are not migrated.

## [0.4.0] - 2026-06-30

### Added

- `kt log` rounds each project's total for a day up, using Kimai's decimal
  rounding (steps of 0.01 hours).
- `kt add` accepts `--start-date`, `--start-time`, `--stop-date`, and
  `--stop-time`, so it can run without prompts.
- `kt add --future` allows dates and times in the future, for entering planned
  leave such as PTO or holidays.

### Changed

- `kt add` rejects dates and times in the future unless `--future` is passed.
- The `kt add` time picker offers times in 3-minute steps instead of 6.

## [0.3.0] - 2026-05-30

### Added

- `--data-dir` and the `KT_DATA_DIR` environment variable choose where `kt`
  stores its data.

## [0.2.0] - 2026-05-30

### Added

- `kt add` records an interval after the fact, for time you forgot to track.

## [0.1.0] - 2026-05-04

### Added

- Local time tracking: `kt new`, `kt in`, `kt out`, `kt switch`, `kt list`, and
  `kt log`.

[unreleased]: https://github.com/bradenhc/kimai-timer/compare/0.5.0...HEAD
[0.5.0]: https://github.com/bradenhc/kimai-timer/compare/0.4.0...0.5.0
[0.4.0]: https://github.com/bradenhc/kimai-timer/compare/0.3.0...0.4.0
[0.3.0]: https://github.com/bradenhc/kimai-timer/compare/0.2.0...0.3.0
[0.2.0]: https://github.com/bradenhc/kimai-timer/compare/0.1.0...0.2.0
[0.1.0]: https://github.com/bradenhc/kimai-timer/releases/tag/0.1.0
