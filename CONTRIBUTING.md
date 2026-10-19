# Contributing

This is the one place to start contributing to `kt`, whether you are a person or
an agent. It covers setup, how the code is laid out and written, how changes
land, and how releases go out.

## Setup

Install [mise](https://mise.jdx.dev), then run `mise install` in the repo to get
the pinned Rust toolchain and `dprint`. Every task lives in `mise.toml`
(`mise tasks` lists them):

- `cargo check` for a fast compile check while iterating
- `mise run fmt` formats Rust, Markdown, and TOML
- `mise run test` runs unit and integration tests
- `mise run ci` runs every check CI runs: formatting, clippy with `-D warnings`,
  tests, and a build

## Before You Change Anything

Read [`CONTEXT.md`](./CONTEXT.md), the glossary. Use its terms in code, docs,
issues, and output, and use the _Avoid_ words nowhere. When a change needs a
term the glossary lacks, add it there first.

Read the ADRs in [`docs/adr/`](./docs/adr/) that touch the area you are
changing. If your change contradicts one, say so in the PR and write a new ADR
that supersedes it.

## Project Layout

- `src/main.rs` parses arguments, opens the `Store`, and dispatches to a
  command.
- `src/cmd/` holds one module per `kt` subcommand.
- `src/report.rs` builds the Report from Intervals, the open Session, and a
  window of Days, independent of the store and the system clock.
- `src/store.rs` and `src/store/` own everything persisted: the Event Log and
  the state derived from it.
- `tests/` holds integration tests that run the `kt` binary.

## Code Style

- Commands receive the `Store` from `main`: `&Store` to read, `&mut Store` to
  write. A command never opens the store itself.
- Output uses ASCII characters only.
- Document every module, summarizing its most important concepts and types.
- Document every public constant, struct, enum, function, trait, macro, and
  struct field.
- Keep other documentation minimal: the code is the source of truth.
- Wrap comments at 100 columns.

## Making a Change

1. Branch from `main`. Keep each PR to one issue where you can.
2. Write the change with tests.
3. Add an entry to `CHANGELOG.md` if users will notice the change (see
   [Changelog](#changelog)).
4. Run `mise run fmt`, then `mise run ci`. It must pass; CI runs the same task
   on every PR.
5. Commit following [Commit Messages](#commit-messages) and open a PR against
   `main`. Title it like a commit subject: a short Conventional Commit line,
   ideally 50 characters or fewer. Fill in the PR template.

## Commit Messages

This repository follows
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```
<type>(<scope>): <description>
```

Commit types and scopes match the `type/*` and `scope/*` issue labels, so an
issue's labels tell you how to write the commit that resolves it.

### Types

- `feat`: New feature or user-facing capability
- `fix`: Something was broken and is now fixed
- `chore`: Maintenance, tooling, or upkeep with no behavior change
- `perf`: Performance improvement
- `refactor`: Code restructuring with no behavior change
- `docs`: Documentation-only change
- `build`: Build system, dependencies, or versioning
- `test`: Adding or fixing tests

### Scopes

- `cli`: Command-line interface: argument parsing, prompts, and output
  (`src/main.rs`, `src/cmd`)
- `store`: Local persistence: event log and data directory (`src/store.rs`)
- `report`: Building the Report: time per Project per Day, rounding, and totals
  (`src/report.rs`)
- `sync`: Synchronization with the Kimai server

If a change spans more than one scope, omit the scope (e.g.
`refactor: use chrono`). Mention the specific subcommand in the description when
relevant (e.g. `feat(cli): add non-interactive mode to add`).

## Changelog

`CHANGELOG.md` follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/). Every PR that
changes what a user sees or relies on adds a line under `## [Unreleased]`, in
the PR itself. Write it for users: what changed for them, not how the code
changed.

Pick the section from the commit type:

- `feat`: Added, or Changed if it alters existing behavior
- `fix`: Fixed
- `perf`: Changed
- Removing a command, flag, or support for old data: Removed
- `chore`, `refactor`, `test`, `docs`, `build`: usually no entry

## Releases

`version` in `Cargo.toml` on `main` is always the last released version. It
changes only in a release PR, so several changes can collect on `main` before
they ship. Between releases, `[Unreleased]` in `CHANGELOG.md` lists what `main`
has beyond the published version.

To release:

1. Choose the version from `[Unreleased]` (or
   `git log <last tag>..HEAD --oneline`). While below 1.0, anything under Added,
   Changed, or Removed bumps the minor version (`0.5.0` to `0.6.0`); only Fixed
   entries bump the patch version (`0.5.0` to `0.5.1`).
2. On a branch, set `version` in `Cargo.toml` and run `cargo check` to update
   `Cargo.lock`. In `CHANGELOG.md`, rename `[Unreleased]` to
   `[X.Y.Z] - YYYY-MM-DD`, add a fresh empty `[Unreleased]` above it, and update
   the compare links at the bottom.
3. Open a PR titled `build: release X.Y.Z` and merge it once CI passes.
4. Tag the merge commit and push the tag:
   `git tag vX.Y.Z && git push origin vX.Y.Z` (tags have a `v` prefix; the
   version in `Cargo.toml` and `CHANGELOG.md` does not).
5. From that commit, run `cargo publish --locked`.

## Issues

Issues are tracked directly in GitHub. Labels use a `group/item` scheme. When
creating an issue, assign it:

- Exactly one `type/*` label: the Conventional Commit type of the work
- Exactly one `status/*` label: the state of the issue
- One or more `scope/*` labels: every scope the issue touches

### Status

- `status/triage`: Needs maintainer evaluation
- `status/needs-info`: Needs more details before it is ready for implementation
- `status/ready-agent`: Fully specified and ready for an AFK agent
- `status/ready-human`: Ready, but needs a human to implement
- `status/obsolete`: Will not be actioned (out of scope, duplicate, or
  superseded)

New issues start at `status/triage`. To find work, filter by
`status/ready-human` or `status/ready-agent`.

### Wayfinder

`wayfinder/*` labels mark maps and tickets created by the `/wayfinder` agent
skill. Leave them to that workflow.

## Agent Docs

[`docs/agents/`](./docs/agents/) tells agent skills how this repo's issue
tracker, labels, and domain docs work. The labels themselves are defined here,
in [Issues](#issues); update `docs/agents/triage-labels.md` when they change.
