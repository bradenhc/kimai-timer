# Developer Guidelines

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
- `sync`: Synchronization with the Kimai server

If a change spans more than one scope, omit the scope (e.g.
`refactor: use chrono`). Mention the specific subcommand in the description when
relevant (e.g. `feat(cli): add non-interactive mode to add`).

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

### Agent docs

`docs/agents/` is the source of truth for how agents interact with issues and
domain docs:

- `issue-tracker.md`: How to create, read, and label issues
- `triage-labels.md`: Mapping from agent skill triage roles to this repo's
  labels
- `domain.md`: Where to find `CONTEXT.md` and ADRs

Keep this file and `docs/agents/` in sync when labels change.
