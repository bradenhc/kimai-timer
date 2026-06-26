# Project: Kimai Timer (kt)

Seriously simple Rust CLI that locally tracks time spent on projects and can
sync with a Kimai server backend.

See @README.md for more overview/design/usage information.

## Concepts

- A **project** defines work done by a user and are explicitly created
- An **interval** associates a project with start/stop timestamps
- Only one current interval may be active at a time (partial, no stop timestamp)
- Intervals are created/modified/removed via timer events
- Timer events stored in append-only timer event log for fast updates

## Structure

- Subcommands of `kt` are isolated as individual modules in `src/cmd`
- The `src/store.rs` module defines app interactions with persisted data

## Toolchain

- Rust 1.95
- Rust Edition 2024

## Commands

- `cargo check` to make sure code compiles without building artifact (faster)
- `cargo build` to build in debug mode
- `cargo run` to build and run
- `cargo test` to run unit and integration tests after updates
- `cargo clippy` to lint code for quality

## Code Style

- Doc comments should focus more on "why" and less on "what"
- Add doc comments to every module summarizing purpose and key elements
- Add concise doc comments to every struct, enum, and function (summarize what,
  highlight why and how it fits in)
- Add concise doc comments to every public struct field (summarize what)
- Add empty doc comment line at end of documentation before code for doc
  comments on structs, enums, and functions to improve readability
- Separate struct fields with doc comments with newline to improve readability
- Wrap comments at 100 columns
- Use `cargo fmt` to format Rust code (after every update)
- use `dprint` to format markdown files
- Prefer stateless functions over stateful struct methods, but keep code simple
