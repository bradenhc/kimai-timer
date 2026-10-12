# Project: Kimai Timer (kt)

Seriously simple Rust CLI that locally tracks time spent on projects and can
sync with a Kimai server backend.

See README.md for more overview and details on design and usage.

## Structure

- Subcommands of `kt` are isolated as individual modules in `src/cmd`
- The `src/store.rs` module defines app interactions with persisted data

## Commands

Tools and tasks are managed with `mise` (see `mise.toml`):

- `cargo check` for a fast compile check while iterating
- `mise run fmt` to format Rust, Markdown, and TOML
- `mise run test` to run unit and integration tests
- `mise run ci` to run every check CI runs (format, clippy `-D warnings`, tests,
  build)

Before committing, run `mise run fmt` then `mise run ci`; it must pass.

## Code Style

- Use only ASCII characters for all output
- Code is the ultimate source of truth: keep extra documentation minimal
- Document every module, summarizing the most important concepts and types
- Document every public constant, struct, enum, function, trait, and macro
- Document public struct fields
- Wrap comments at 100 columns

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues via the `gh` CLI. See
`docs/agents/issue-tracker.md`.

### Triage labels

Labels use a `group/item` scheme (`type/*`, `status/*`, `scope/*`,
`wayfinder/*`) mirroring Conventional Commits. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` and `docs/adr/` at the repo root. See
`docs/agents/domain.md`.
