# Kimai Timer

A local, terminal-first record of the time one person spends on their work,
which can be reported on and, eventually, synchronized with a Kimai server.

## Language

### Work

**Project**:\
A named body of work that time is tracked against. Its identity is stable and
independent of its name, so it can be renamed without rewriting history. Its
name is unique among Projects. A Project exists locally, independent of any
Kimai server.\
_Avoid_: Task, Activity, Contract

**Session**:\
The single, currently running period of work on one Project. It has a start but
no end, and at most one exists at a time.\
_Avoid_: Current interval, open interval, active task

**Interval**:\
A period of work on one Project, with both a start and an end. It may lie in the
future, such as planned leave recorded on a Project made for it. Intervals
should not overlap one another or the Session: `kt` refuses to create an overlap
where it can, and warns where it cannot rather than lose time.\
_Avoid_: Time entry, timesheet, record

**Last Project**:\
The Project the most recent Session closed on. Adding an Interval directly never
changes it.

### Actions

**Punch In**:\
Opening a Session on a Project.\
_Avoid_: Start, clock in

**Punch Out**:\
Closing the open Session, which turns it into an Interval.\
_Avoid_: Stop, clock out

**Switch**:\
Punching In while a Session is already open. The old Session is closed and the
new one opened together, never one without the other. `kt switch` is one
specific Switch: back to the Last Project.

### Record

**Event**:\
A single immutable fact about something that happened, such as a Project being
created or a Session being opened. Every Event has a recorded time (when it was
written) that is distinct from the times it describes.\
_Avoid_: Time event, timer event, punch event

**Event Log**:\
The ordered, append-only collection of every Event. It is the only authoritative
record; everything else is derived from it.\
_Avoid_: Time log, timelog, store file

### Reporting

**Report**:\
A view of Intervals (and any open Session) over a window of Days, in any format,
as shown by `kt log`.\
_Avoid_: Log

**Day**:\
A calendar day in the user's local time zone. Work that crosses midnight counts
toward each Day only for the part that falls within it.

**Rounding Mode**:\
The rule for rounding a Project's total time on a Day up to a fixed step when it
is reported: either hundredths of an hour (Decimal) or a whole number of minutes
(Classic). Recorded times are never rounded.

**Pay Period**:\
A recurring, fixed-length span of Days, starting from a set date, after which
the time in it is settled so that payment can be made. Fourteen Days by
default.\
_Avoid_: Pay cycle, billing period
