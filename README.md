# kt - Seriously Simple Time Tracker

The Kimai Timer (`kt`) is a simple CLI application that helps you track the time
you spend on your work. It allows you to "punch in" and "punch out" of projects
and view a log of how much time you spent on each project each day.

Eventually `kt` will also support synchronizing with a [Kimai] server. The
synchronization process should feel very similar to Git, with a push/pull model
that lets you fetch projects and current data from the server and push updates
based on the local event log.

## Installation

```sh
cargo install --locked kimai-timer
```

## Quick Start

```sh
# Create a couple of projects
kt new my-project
kt new my-other-project

# Punch in to a project
kt in my-project

# Automatically punch out of the current project and into another
kt in my-other-project

# Punch back in to the project you just left
kt switch

# Punch out of the current project
kt out

# Punch in to the last project (only if punched out)
kt in

# View a table showing time spent on projects
kt log        # today
kt log -w     # past week
kt log -p     # past two weeks

# View help information for more details
kt help
```

## Tracking Time

`kt` records everything that happens, such as punching in or out, as an
**event** in a single append-only **event log**. The time spent on each project
is calculated from that log.

Before you can track your time, you must create a **project** using `kt new`. A
project has a stable identity separate from its name, so it can later be renamed
or linked to its counterpart on a [Kimai] server without rewriting any history
you have already recorded.

Once you have a project, you use the `kt in` and `kt out` commands to capture
when you start and stop working on it. Running `kt in` with another project
while you are punched in punches you out of the first and in to the second in
one step.

Running `kt in` with no arguments punches you back in to the last project you
punched out of. While punched in, `kt switch` does the same thing, which makes
it easy to bounce between two projects.

To record work after the fact, use `kt add`.

## Viewing Reports

Use the `kt log` command to view a report of how much time you have spent on
your projects. By default the command only shows data for the current day. You
can use command options to control how many days are displayed and the format of
the output.

## Integrating with Kimai

> **NOTE** This feature is a work in progress

[Kimai] is a powerful open-source time tracking service. It has tons of features
and a slick web interface for managing the time spent on your work.

`kt` is meant to enhance the Kimai user experience for individuals who find
themselves spending most of their day in a terminal (or just like them) and need
to keep track of how much time they spend on each project they work on.

Planned: you will authenticate `kt` with a Kimai [user token], then push and
pull time much like a Git remote. Until then, `kt` works entirely offline.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

[Kimai]: https://www.kimai.org/en/
[user token]: https://www.kimai.org/documentation/user-api.html
