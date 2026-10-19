# Triage Labels

The skills speak in terms of canonical triage roles. This file maps those roles
to the actual label strings used in this repo's issue tracker. Labels use a
`group/item` scheme; see `docs/agents/issue-tracker.md` for the full set of
groups.

## Category roles

| Label in mattpocock/skills | Label in our tracker | Meaning                    |
| -------------------------- | -------------------- | -------------------------- |
| `bug`                      | `type/fix`           | Something is broken        |
| `enhancement`              | `type/feat`          | New feature or improvement |

Other `type/*` labels (`chore`, `perf`, `refactor`, `docs`, `build`, `test`)
also count as the category role for an issue. Pick the one matching the
Conventional Commit type the work will use.

## State roles

| Label in mattpocock/skills | Label in our tracker | Meaning                                                  |
| -------------------------- | -------------------- | -------------------------------------------------------- |
| `needs-triage`             | `status/triage`      | Maintainer needs to evaluate this issue                  |
| `needs-info`               | `status/needs-info`  | Needs more details before it is ready for implementation |
| `ready-for-agent`          | `status/ready-agent` | Fully specified, ready for an AFK agent                  |
| `ready-for-human`          | `status/ready-human` | Requires human implementation                            |
| `wontfix`                  | `status/obsolete`    | Will not be actioned                                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the
corresponding label string from these tables.

## Scope

Every triaged issue should also carry at least one `scope/*` label (`cli`,
`store`, `report`, `sync`). Apply every scope the issue touches.
