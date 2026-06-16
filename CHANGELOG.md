## v0.2.0 (2026-06-16)

- Added composite primary key support across all code‑generation drivers.
- Implemented full foreign‑key relationship generation in the IR and driver layer.
- Introduced down‑migration generation with matching **DOWN** statements and a `valkyrin rollback` CLI command.
- Extended sync engine: bidirectional diff, migration file creation, and relation detection for PostgreSQL, MySQL, and SQLite.
- Added SQLite support to CI and integration tests, including enum round‑trip validation.
- Fixed all Clippy warnings and cleaned up unused code.
