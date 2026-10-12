# A single append-only Event Log is the only source of truth

Everything `kt` knows (Projects, the open Session, every Interval, the Last
Project) is derived by replaying one append-only file, `events.jsonl`, into
memory each time `kt` runs. Nothing else on disk is authoritative. We chose this
because the two requirements that matter most are crash safety (a kill or power
loss must never lose a punch or corrupt the store) and fitness for a future
push/pull sync. Every write is a single atomic append, so no operation can
half-succeed. The log doubles as the sync change feed: merging means appending
the Events not yet seen, de-duplicated by Event ID.

## Considered Options

- **Several mutable files joined on load** (the v0.4 layout and its first
  refactor): a punch-out could half-succeed and double-count time, and losing
  project metadata made the history unreadable.
- **A single snapshot file, rewritten whole on each command**: crash-safe, but
  it has no change feed, so sync would degrade to last-writer-wins and we would
  have to re-add an event log later.
- **An embedded key-value store (`redb`, `fjall`)**: overkill at roughly 2,500
  Events a year, an opaque format, and sync would still need an event log built
  inside it.
- **SQLite**: ruled out by the no-native-dependency gate
  ([ADR-0003](./0003-no-native-dependencies.md)). If that gate ever lifts and
  the data outgrows a log, this is the first option to revisit.

## Consequences

- **Tolerant reads.** An Event from a newer schema version, or of an unknown
  kind, is skipped with a warning, so an older `kt` can still report on a log a
  newer one wrote. New Event kinds are therefore backward compatible without a
  schema bump. A torn final line (a crash mid-append) is dropped by the next
  write. Malformed lines anywhere else are real corruption and are fatal.
- **Unknown Project references are kept.** An Interval or Session whose Project
  is not in the log is still valid time, and is reported without a name rather
  than dropped.
- **Separate start and stop Events, with no "switch" Event.** A Switch is a stop
  and a start written in one atomic append, so no consumer has to understand a
  third way of saying the same thing.
- **Interval identity is an Event ID.** An Interval takes the ID of the Event
  that created it (`SessionOpened` or `IntervalCreated`), giving a single ID
  space for future edit and delete Events to refer to.
- **No migration from v0.4.** The old files (`timelog.jsonl`, `taskset.json`,
  `current`, `last`) held only scratch data and are ignored. Each line carries a
  schema version `v` so that the first real migration can branch on it.
- **Renames are aliases, not migrations.** `SessionOpened` and `SessionClosed`
  were first written as `TimerStarted` and `TimerStopped`. The old names are
  still accepted on read, so renaming an Event kind needs no version bump; a
  bump is reserved for a change to an Event's shape.

## Deferred, with triggers

| Deferred                                               | Revisit when                                                                  |
| :----------------------------------------------------- | :---------------------------------------------------------------------------- |
| Compaction (a snapshot Event at the head, or rotation) | The log exceeds ~25k Events or replay exceeds ~20 ms                          |
| A derived, disposable snapshot/index cache             | Replay becomes noticeable in interactive use                                  |
| Advisory file locking                                  | A real interleaving bug is seen, or a command needs several dependent appends |
| An embedded key-value store or SQLite                  | The data outgrows a single log, or the no-native-dependency gate is lifted    |
