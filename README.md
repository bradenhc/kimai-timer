# kt - Seriously Simple Time Tracker

The Kimai Timer (`kt`) is a simple CLI application that helps you track the time
you spend on your work. It allows you to "punch in" and "punch out" of projects
and view a log of how much time you spent on each project each day.

Eventually `kt` will also support synchronizing with a [Kimai] server. The
synchronization process should feel very similar to Git, with a push/pull model
that lets you fetch projects and current data from the server and push updates
based on the local timer event logs.

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

# Punch out of the current project
kt out

# Punch in to the last project (only if punched out)
kt in

# View a table showing time spent on projects
kt log        # today
kt log -w     # past week
kt log -p     # past two weeks (typical pay-period)

# View help information for more details
kt help
```

## Tracking Time

`kt` tracks time using a series of **time events**. These events are stored in a
single append-only log and used to calculate the amount of time spent on each
project.

Before you can track your time, you must create a **project** using `kt new`. A
project has a stable identity separate from its name, so it can later be renamed
or linked against a specific contract stored in [Kimai] - which is where `kt`
will push time events to - without rewriting any history you have already
recorded.

Once you have a project, you use the `kt in` and `kt out` commands to capture
when you start and stop working on it. If you have multiple projects you are
working on, you can also use the `kt switch` command to switch between the
current project and the last one you were working on.

Running `kt in` with no arguments will start tracking time again for the last
project you punched out of.

## Viewing Time Logs

Use the `kt log` command to view a table of how much time you have spent on your
projects. By default the command only shows data for the current day. You can
use command options to control how many days are displayed and the format of the
output.

## Integrating with Kimai

> **NOTE** This feature is a work in progress

[Kimai] is a powerful open-source time tracking service. It has tons of features
and a slick web interface for managing the time spent on your work.

`kt` is meant to enhance the Kimai user experience for individuals who find
themselves spending most of their day in a terminal (or just like them) and need
to keep track of how much time they spend on each project they work on.

To use `kt` with your Kimai server, log in via the web interface and create a
[user token] for your profile. Once you have a token, run `kt login` and paste
the token into the prompt.

You can also manually save the token to a file named `token` in the `kt`
configuration directory. Directory location is dependent upon your OS but should
be in one of the following locations:

| OS      | Config Directory                                                  |
| :------ | :---------------------------------------------------------------- |
| Linux   | `${XDG_CONFIG_HOME}/kimai-timer` or `${HOME}/.config/kimai-timer` |
| Windows | `{FOLDERID_RoamingAppData}/hitchcock/kimai-timer/config`          |
| MacOS   | `$HOME/Library/Application Support/codes.hitchcock.kimai-timer`   |

[Kimai]: https://www.kimai.org/en/
[user token]: https://www.kimai.org/documentation/user-api.html
