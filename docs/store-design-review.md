# Store Design Review

## Summary

The current direction — three files (`projects.json`, `session.json`,
`timelog.jsonl`) with project data normalized out of the time log and re-joined
on load — is a step backward on the two things you said matter most: crash
safety and fitness for the future sync story. It converts a one-file consistency
problem into a three-file one, without buying anything the alternatives don't
also provide.

**Recommendation: collapse the store to a single append-only event log that is
the sole source of truth, and derive everything else as an in-memory
projection.**

This is not a new idea imported from outside — it is what `CLAUDE.md` already
says the domain model is ("Intervals are created/modified/removed via timer
events. Timer events stored in append-only timer event log"). The current file
layout is the part that drifted from the stated model, not the model itself.
Adopting it _deletes_ the `Linked`/`Resolved` join machinery rather than adding
a new subsystem.

## Constraints this review was scored against

Gathered before analysis, not derived from the code:

| Constraint                                                                                     | Status                                  |
| :--------------------------------------------------------------------------------------------- | :-------------------------------------- |
| No C / native dependencies (`cargo install --locked` must work with no C toolchain)            | **Hard gate**                           |
| Crash safety — a kill or power loss must never lose a punch-in or corrupt the store            | **Hard requirement**                    |
| Binary size and cold start — `kt` runs dozens of times a day, interactively                    | **Hard requirement**                    |
| Edit and delete of past intervals must be supported                                            | **Committed roadmap**                   |
| Near term: local store only. Mid term: Kimai server push/pull. Far term: other remote backends | **Committed roadmap**                   |
| Distributed/peer time-keeping without a Kimai server                                           | **Not planned, must not be foreclosed** |
| Human-readable on-disk format                                                                  | **Not a constraint** (nice to have)     |

Two facts about scale, measured rather than assumed:

- The live data directory currently holds 10 intervals in a 1.7 KB
  `timelog.jsonl`.
- A heavy user punching in and out ~10x/day across 250 workdays generates
  roughly 2,500 events per year, on the order of 600 KB/year of JSONL.

**Every option below is fast enough.** Performance is not a differentiator at
this scale and should not drive the decision. Correctness under crash, and
fitness for sync, should.

## What exists today

### On `main` (v0.4.0, shipping)

One `Store` struct over four files: `timelog.jsonl` (append-only
`CreateInterval` events), `taskset.json` (a `BTreeSet<String>`), `current` (JSON
blob), `last` (bare task name). Tasks are identified by their display name, and
that name is **denormalized into every interval**.

### On `feat/refine-store` (in flight, does not compile — 20 errors)

Introduces a first-class `Project` with a stable UUID separate from its display
name — the right call, and the reason the refactor started. Splits the store
into `fs`/`project`/`session`/ `timelog` modules. Replaces `current` + `last`
with a single `session.json`. Adds `Linked<T>`/`Resolved<T>`: records in the
session and time logs persist only a `ProjectId` and are "resolved" against
`ProjectSet` on load.

The branch also contains two conflicting designs — `add.rs` calls through a
`Store` facade while `in.rs`/`out.rs` were rewritten to open
`ProjectsLog`/`SessionLog`/`TimeLog` directly. The unit tests still in
`src/store.rs` describe the facade, so the facade is the intended shape.

## Findings against the current approach

These are defects and structural risks in the direction as written, in severity
order. They are the evidence behind the scoring that follows.

### 1. Punch-out is not atomic across files, and the failure mode is double-counted time

`in.rs:57-63` closes the previous session, appends the resulting interval to
`timelog.jsonl`, and _then_ writes `session.json`. A crash, `SIGKILL`, or full
disk between those two writes leaves the interval recorded in the time log while
`session.json` still shows the session as open. The next `kt out` records the
same span again.

Silent duplicate time on a tool whose entire output is billable hours is the
worst class of bug this application can have. It exists because a single logical
operation spans two files with no transaction between them.

### 2. Losing or truncating `projects.json` makes the entire time log unreadable

`fs.rs:77-78` returns `T::default()` when a file reads as empty, so a
zero-length `projects.json` becomes an empty `ProjectSet` with no error.
`project.rs:291-294` then fails resolution for every interval, and
`timelog.rs:71` propagates that with `?` — so `TimeLog::intervals()` fails
wholesale. `kt log`, `kt list`, and `kt out` all stop working.

This is a regression introduced by normalization. On `main`, each interval
carries its own task name, so losing `taskset.json` costs you the alias list and
nothing else — the history still reads. The refactor makes a small metadata file
into a single point of failure for all historical data, and it is the file most
exposed to failure mode 3.

### 3. All state writes are non-atomic truncations

`fs.rs:40` writes via `std::fs::write`, which opens with `O_TRUNC` — the file is
emptied _before_ the new content lands. There is no temp-file-plus-rename, and
no `fsync`. Any interruption mid-write leaves a zero-length file, which per
finding 2 is then silently read back as "no projects." Given crash safety is a
stated hard requirement, this is the single most mechanical gap in the design.

### 4. No schema version anywhere on disk

None of `projects.json`, `session.json`, or the `timelog.jsonl` lines carry a
version field. The data model is actively in flux — `TimeInterval` just dropped
its `task` field, `taskset.json` is becoming `projects.json` — and there is no
way for a future binary to detect which layout it is looking at, nor to migrate
forward safely. Every future format change becomes a guess based on file names
and duck-typed JSON.

For a tool distributed via `cargo install` (where users update at unpredictable
intervals and can easily run an old binary against a new store), this is the
most consequential _maintainability_ gap on the list.

### 5. `ProjectSet::add` does not enforce the uniqueness it documents

`project.rs:78-79` relies on `BTreeSet::insert` returning `false` for a
duplicate. But `Project` derives `Ord`/`Eq` over both `id` and `name`
(`project.rs:144`), and `Project::new` always mints a fresh UUID — so two
projects with the same name are never equal, and `add` never returns the
documented `"project '{p}' already exists"` error. The invariant in the doc
comment ("Each project must have a unique name") is not enforced by the type
that claims to own it.

End-user impact is currently masked because `new.rs` checks `contains` first,
but the guard now lives in the command layer rather than the store, which is
exactly the inversion the refactor set out to fix.

### 6. A torn trailing line fails the whole log read

`timelog.rs:60-71` iterates every line and `?`-propagates any deserialization
error. A partial final line — the realistic outcome of a crash during append —
makes the entire history unreadable rather than costing one event.

### 7. Unknown event variants are a hard error

`TimeEvent` is a closed enum with `#[serde(tag = "type", content = "data")]`. An
older binary reading a log written by a newer one fails to deserialize instead
of skipping forward. Benign today with one variant; a genuine problem the moment
sync exists and two machines run different versions.

### 8. Incidental issues

- `fs.rs:120` `store_path` calls `create_dir_all` **and** `canonicalize` on
  every path construction, so computing a path has filesystem side effects and
  costs syscalls on each of the several calls per command.
- No file locking. Two concurrent `kt in` invocations can interleave
  read-modify-write on `session.json`.
- `src/store/round.rs` is empty, dead, and not in the `mod` tree.
- `TimeInterval::updated_at` is written by nothing and read by nothing.

## Alternatives considered

### A. Current direction — normalized multi-file (`projects.json` + `session.json` + `timelog.jsonl`)

Two mutable state documents plus one append-only event log, joined at load time
via `Linked`/`Resolved`.

The hybrid is the problem. It pays the full cost of normalization (cross-file
joins, resolution failures, a hard dependency from history onto metadata) and
the full cost of multi-file consistency (findings 1 and 2), while the event log
— the part that would justify that complexity — is not actually the source of
truth. Session state and project state live outside it, so the log cannot be
replayed to reconstruct the store, cannot be shipped as a self-contained change
feed, and cannot be merged.

### B. Single-file snapshot (one `store.json`, read-all / rewrite-all)

Everything in one document; every command loads it, mutates in memory, and
atomically rewrites via temp-file-plus-rename.

Genuinely appealing for simplicity, and it eliminates findings 1, 2, 3, and 6
outright — one file cannot be internally inconsistent with itself if writes are
atomic. At 600 KB/year the rewrite cost is irrelevant. Its weakness is the
future: a snapshot has no change feed, so push/pull degrades to whole-store
last-writer-wins, and edit/delete leave no trace to reconcile. You would be
re-adding an event log later to get sync, having removed one you already had.

### C. Single append-only event log as sole source of truth **(recommended)**

One `events.jsonl`. Projects, session state, and intervals are all _derived_ by
folding the log into an in-memory projection. Nothing else on disk is
authoritative.

Every write becomes exactly one atomic append, which is what kills the entire
torn-state class of bug. The log is simultaneously the storage format and the
sync unit: push/pull is "send events after cursor X, dedup by event id," which
is precisely the git-like model the README promises. Edit and delete are
first-class events rather than in-place mutations. Cost: every read replays the
log, and the log grows without bound absent compaction. Both are non-issues at
this scale and have known, deferrable answers.

### D. Embedded pure-Rust key-value store (`redb`, `fjall`, `sled`)

Real ACID transactions and crash safety for free, indexed range scans by date,
and no C toolchain (all three are pure Rust, though `sled` has been effectively
stalled for some time).

This is a serious engineering answer to findings 1-3 and 6. It is also enormous
overkill for a dataset that fits in a single 4 KB page for the first several
months, and it actively hurts the roadmap: an opaque page-file has no natural
change feed, so you would end up writing an event log _inside_ the database to
get sync, arriving at option C with a storage engine bolted underneath it. It
adds a dependency, a file format you cannot inspect, and a nontrivial share of
the binary, in exchange for durability guarantees that atomic appends already
provide at this size.

### E. SQLite (`rusqlite`)

The conventional answer, and on the merits a strong one: transactions, real
queries for the `log` aggregation, and `PRAGMA user_version` as a battle-tested
migration mechanism that directly addresses finding 4.

**Eliminated by the no-native-dependency gate.** `rusqlite`, even with the
`bundled` feature, compiles the SQLite C amalgamation and therefore requires a C
toolchain at install time. If that constraint ever relaxes and the data model
outgrows a log, this is the option to revisit first.

### F. Event log plus derived snapshot/index cache

Option C with a rebuildable cache file so reads skip the replay.

The right _eventual_ shape if replay cost ever becomes visible, and cheap to add
later precisely because the cache is derived and therefore disposable. Adding it
now would be optimizing a sub-millisecond operation while introducing a second
file that can disagree with the first — reintroducing the class of bug option C
exists to remove.

## Scoring

Weights reflect the stated constraints. 1 = poor, 5 = excellent. The
no-native-dependency constraint is a gate, not a weighted criterion.

| Criterion                                | Weight | A. Multi-file (current) | B. Single snapshot | C. Event log | D. Embedded KV | E. SQLite |
| :--------------------------------------- | -----: | ----------------------: | -----------------: | -----------: | -------------: | --------: |
| Crash safety / atomicity                 |      5 |                       1 |                  4 |            5 |              5 |         5 |
| Fit for push/pull sync + future backends |      5 |                       2 |                  1 |            5 |              2 |         3 |
| Edit / delete support                    |      4 |                       3 |                  3 |            5 |              4 |         5 |
| Simplicity & maintainability             |      4 |                       2 |                  5 |            4 |              2 |         3 |
| Schema evolution / migration             |      3 |                       1 |                  3 |            4 |              3 |         5 |
| Cold start & binary size                 |      3 |                       5 |                  5 |            5 |              3 |         3 |
| Debuggability (nice to have)             |      1 |                       4 |                  5 |            5 |              1 |         2 |
| **Weighted total (max 125)**             |        |                  **57** |             **86** |      **118** |         **78** |    **98** |
| **Gate: no native dependency**           |      — |                    pass |               pass |         pass |           pass |  **FAIL** |

Option C wins on the two heaviest criteria simultaneously, and is the only
option scoring 5 on both crash safety and sync fitness. Option B is the
runner-up and the sensible fallback if you decide the sync roadmap is genuinely
speculative — but it trades away the exact capability the README commits to, and
it is the one option you would have to unwind later rather than extend.

Two honest caveats about this table. First, SQLite scores second-highest and
would likely place first if the native-dependency gate were lifted — it is
removed by the gate, not on the merits. Second, the current direction (A) scores
lowest not because multi-file storage is inherently bad but because _this
particular_ multi-file split combines normalization costs with multi-file
consistency costs while leaving the event log non-authoritative. Fixing findings
1-3 in place would raise A materially; it would also amount to rebuilding it as
C.

## Recommended target design

### One authoritative file

```
<data_dir>/events.jsonl     # the only source of truth
<data_dir>/events.jsonl.bak # written only by compaction, before rename
```

`projects.json`, `session.json`, and the separate `timelog.jsonl` all go away.

### One versioned envelope per line

```json
{"v":1,"id":"<uuid-v4>","ts":1756800000,"event":{"type":"IntervalCreated","data":{...}}}
```

- `v` — per-line schema version. Closes finding 4 and allows a mixed-version
  file to migrate forward incrementally, without a rewrite.
- `id` — a UUID per _event_, distinct from any domain id. This is what makes
  sync idempotent: merging is "append events whose id I haven't seen," and
  replaying a duplicated push is a no-op.
- `ts` — when the event was _recorded_, kept distinct from the domain timestamps
  inside `data` (when the work happened). `kt add` for last Tuesday has today's
  `ts` and Tuesday's interval times. Conflating the two makes reconciliation
  ambiguous.

### Event vocabulary

| Event             | Carries                                     | Notes                                                             |
| :---------------- | :------------------------------------------ | :---------------------------------------------------------------- |
| `ProjectCreated`  | `project_id`, `name`                        |                                                                   |
| `ProjectRenamed`  | `project_id`, `name`                        | The rename-without-rewriting-history case that motivated the UUID |
| `ProjectLinked`   | `project_id`, remote ref                    | Kimai contract linkage, when that lands                           |
| `ProjectArchived` | `project_id`                                | Prefer archive over delete; deletion orphans intervals            |
| `TimerStarted`    | `project_id`, `at`                          |                                                                   |
| `TimerStopped`    | `at`                                        |                                                                   |
| `IntervalCreated` | `interval_id`, `project_id`, `start`, `end` | `kt add`                                                          |
| `IntervalUpdated` | `interval_id`, changed fields               |                                                                   |
| `IntervalDeleted` | `interval_id`                               |                                                                   |

Punch-out becomes a **single append** of `TimerStopped`. The projection pairs it
with the open `TimerStarted` and derives the interval, using the `TimerStarted`
event's `id` as the interval's stable id — so the derived interval is
addressable by later `IntervalUpdated`/`IntervalDeleted` events. This is what
eliminates finding 1: there is no second write to fall out of sync with.

### Read path

`Store::open` folds the log once into a projection:

```rust
struct State {
    projects:  ProjectSet,
    session:   Session,                       // derived, not stored
    intervals: BTreeMap<IntervalId, Interval>,
}
```

`Linked<T>` and `Resolved<T>` are **deleted**. Resolution stops being an
I/O-time join that can fail (finding 2) and becomes an in-memory lookup against
state that was built from the same file in the same pass. There is no longer a
second file to lose.

### Durability rules

1. **Append:** open with `O_APPEND`, write one complete line with a single
   `write_all`, then `sync_data()`. `O_APPEND` makes the offset update atomic,
   so concurrent invocations cannot interleave within a line — which covers the
   realistic concurrency case (two terminals) without a lock file or a new
   dependency.
2. **Rewrite** (compaction only): write to a temp file in the same directory,
   `sync_data`, then `rename` over the original. Never truncate in place. Closes
   finding 3.
3. **Tolerant reads:** a malformed _trailing_ line is truncated with a warning
   rather than failing the read (finding 6); a malformed line anywhere else is a
   hard error, since that indicates real corruption rather than an interrupted
   append.
4. **Forward compatibility:** deserialize the envelope with `event` as a raw
   value and match the tag manually, so an unrecognized `type` is skipped with a
   warning instead of failing (finding 7). Non-negotiable once two machines can
   run different versions.

### Keep the `Store` facade

The facade shape asserted by the existing `src/store.rs` tests (`open`,
`projects`, `add_project`, `session`, `start_session`, `stop_session`,
`append_interval`, `intervals`) is still correct and should be the only way
commands touch storage. The `in.rs`/`out.rs` direction of opening logs directly
should be reverted. This also settles Phase 0 of the refactor plan.

## What this means for the in-flight branch

**Keep — the domain modeling is right and is the valuable part of the
refactor:**

- `Project`, `ProjectId`, `ProjectName` newtypes and their validation.
- `TimeInterval`, `TimeDuration`, `RoundingMode` (already well-tested).
- `StoreRoot` and the `Derived`/`Specified` split.
- The `store/` module decomposition and the `Store` facade shape from the tests.

**Change:**

- `timelog.rs` becomes the single event log: envelope type, wider event enum,
  tolerant reads, atomic appends.
- `project.rs` keeps the types, loses `ProjectsLog` (no separate file) and
  `Linked`/`Resolved`.
- `session.rs` keeps `SessionInfo` as a _projection_ type, loses `SessionLog`
  and the file.
- `fs.rs` loses the JSON document helpers entirely and `store_path` stops
  calling `create_dir_all`/`canonicalize` on every invocation. It gains no
  `atomic_write`: once the log is the only file, nothing is ever rewritten in
  place, so durability comes from an appending write plus `fsync`, and the
  torn-tail repair truncates with `set_len` rather than rebuilding the file. An
  unused temp-and-rename helper would have been exactly the speculative dead
  code this review argues against.
- `ProjectSet::add` enforces name uniqueness explicitly rather than leaning on
  `BTreeSet` identity (finding 5).

**Delete:**

- `src/store/round.rs` (empty, dead).
- `TimeInterval::updated_at` in its current form — supersede it with
  `IntervalUpdated` events.

Net effect on the refactor plan: Phase 1 grows modestly (the envelope and the
fold), Phase 2 shrinks (commands hit one facade over one file), and an entire
subsystem — the `Linked`/ `Resolved` join — is removed rather than finished.

## Migration: none — cut deliberately

**Decision: ship this as a hard break with no migration path.**

The live data directory holds four legacy files and ten intervals, and those
intervals are scratch data, not history — zero-second entries, a 96-hour `foo`,
and a `navy` interval that ends before it starts. There is no user-visible time
history to protect, and at this stage the tool has a single known user who has
accepted the break.

Cutting migration costs nothing rather than saving a little: the new format's
filename (`events.jsonl`) collides with none of the legacy four, so a fresh
`Store` simply never looks at them. There is no legacy-detection branch, no
`.bak` rename, no startup check — the code that isn't written is the code that
doesn't have to be maintained or tested. The old files are inert and can be
deleted by hand.

What survives from this section is only the `"v"` field in the event envelope,
and that is a version _stamp_, not migration machinery: one integer per line, no
logic. It costs nothing now and it is what makes the first real migration — once
there is data worth migrating — a `match` on an integer rather than an
archaeology exercise on unversioned lines. Keep the stamp; skip everything that
would read it today.

Independent of this decision, `tests/data_dir.rs` asserts that `taskset.json`
exists and must be updated to the new filename.

## Deferred, with explicit triggers

Do **not** build these now. Each has a concrete condition that should trigger
revisiting it:

| Deferred                                                  | Trigger                                                                                           |
| :-------------------------------------------------------- | :------------------------------------------------------------------------------------------------ |
| Compaction (`Snapshot` event at head, or yearly rotation) | Log exceeds ~25k events or replay exceeds ~20 ms                                                  |
| Derived snapshot/index cache (option F)                   | Replay becomes visible in interactive use                                                         |
| Advisory file locking                                     | A real interleaving bug is observed, or a command needs read-modify-write across multiple appends |
| Embedded KV or SQLite (options D/E)                       | Data outgrows a single log, _or_ the no-native-dependency constraint is lifted                    |

## Answering the roadmap directly

- **Near term (local only):** one file, atomic appends, tolerant reads. Strictly
  safer than today's four files or the branch's three.
- **Mid term (Kimai push/pull):** the log is already the change feed. Sync is a
  cursor plus dedup-by-event-id, with a `synced_upto` marker. The git-like model
  in the README stops being an aspiration and becomes the natural consequence of
  the storage format.
- **Far term (other backends):** backends consume and produce the same event
  stream. Adding one is an adapter, not a storage change.
- **Not foreclosed (peer-to-peer, no server):** an append-only log of
  uniquely-identified events is the standard substrate for exactly this. Two
  machines merge by unioning their logs and re-folding. This is the one property
  options B and D cannot offer without being rebuilt around a log first — which
  is the strongest argument for choosing C now, while the cost of choosing it is
  a few hundred lines rather than a data migration under load.
