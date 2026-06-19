## v0.2.0 (2026-06-16)

- Added composite primary key support across all code‑generation drivers.
- Implemented full foreign‑key relationship generation in the IR and driver layer.
- Introduced down‑migration generation with matching **DOWN** statements and a `valkyrin rollback` CLI command.
- Extended sync engine: bidirectional diff, migration file creation, and relation detection for PostgreSQL, MySQL, and SQLite.
- Added SQLite support to CI and integration tests, including enum round‑trip validation.
- Fixed all Clippy warnings and cleaned up unused code.

## v0.3.0 (Upcoming)

- **Enterprise Sync Overhaul:** Replaced naive diffing with a State-Machine Sync engine.
- **DAG Sorter:** Implemented graph-based topological sorting to safely resolve circular foreign key deadlocks during table drops.
- **Chained Hash Integrity:** Introduced `valkyrin.sum` directory hashing to detect out-of-band deletions, reordering, and retrospective tampering of migration files.
- **Statement-Level Checkpoints:** Fixed MySQL implicit DDL commit traps by tracking `applied_statements` sequentially for safe rerun recovery.
- **Predictive Data-Loss Prevention:** Added a 2-bit Resource Span analyzer and live `sqlx` empty-checks to eliminate false-positive `--confirm` warnings.
- **Zero-Downtime Indexing:** Upgraded `ir.rs` to track index signatures, enabling O(1) `ALTER INDEX RENAME TO` metadata operations.
