# No native dependencies in the default build

`cargo install --locked kimai-timer` must succeed on a machine with a Rust
toolchain but no C compiler, so no dependency may compile C or C++ code or link
a system library. `kt` is meant to be trivially installable by anyone who
already has Rust, and this constraint is not visible from the code: it is why
SQLite was rejected for the store
([ADR-0001](./0001-single-append-only-event-log.md)) and it must be checked
whenever a dependency is added.

## Consequences

- Kimai sync puts pressure on this gate, because every production-grade HTTPS
  stack in Rust needs at least a C compiler. One option under consideration is
  to put sync behind a default-on Cargo feature, so that `--no-default-features`
  still gives a local-only build with no native dependencies. Whatever the sync
  design decides should supersede this ADR rather than quietly break the gate.
