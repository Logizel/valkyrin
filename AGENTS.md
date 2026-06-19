# AGENTS.md

Valkyrin is a Rust CLI tool for designing database schemas visually and generating ORM code. This file captures context that agents are likely to miss without help.

## Project Structure

**Workspace** (Cargo.toml): `valkyrin-cli`, `valkyrin-core`, `valkyrin-server`

- **valkyrin-cli** (`src/main.rs`): Binary entrypoint. Defines four commands: `init`, `canvas`, `generate`, `sync`.
- **valkyrin-core**: Compilation logic, code generators (Go, Python, Rust, JavaScript, TypeScript), database introspection, sync engine, config parsing.
- **valkyrin-server**: Lightweight Axum web server (port 3000). Serves the embedded React UI.
- **valkyrin-ui**: React + TypeScript + Vite SPA. Drag-and-drop schema canvas. Uses Bun package manager. Built to static files and embedded into the server binary via `rust-embed`.

## Build Order & Commands

**UI must build before Rust**, because `valkyrin-server` embeds the compiled UI:

```bash
# Full build (release):
cd valkyrin-ui && bun install && bun run build
cd .. && cargo build --release

# Dev (Rust only; UI dev runs separately):
cargo build

# UI development server:
cd valkyrin-ui && bun install && bun run dev
```

## Rust Edition & Quirks

- **Edition: `2024`** — Valid in Rust 1.92+. Do not "fix" to 2021.
- **No test suites** in main codebase (valkyrin-ui has eslint but no vitest).
- **No dev-dependencies** defined in Cargo.toml files.
- **Binary name**: `valkyrin-cli` (not `valkyrin`).

## Release Build

CI uses multi-platform cross-compilation. Release profile prioritizes binary size (`opt-level = "z"`, `lto = "fat"`, `strip = true`, `panic = "abort"`). See `.github/workflows/release.yml` for the full build sequence.

## CLI Commands

```bash
valkyrin init                           # Create valkyrin.yaml config + empty schema
valkyrin canvas                         # Start web server at localhost:3000
valkyrin generate                       # Compile blueprint → ORM code
valkyrin sync --url <conn>              # Introspect live DB and update canvas
valkyrin sync --url <conn> --confirm    # Apply destructive changes (remove tables)
valkyrin sync --url <conn> --dry-run    # Preview changes without modifying canvas
```

## Key Dependencies

- **Tokio** (full features) for async runtime.
- **sqlx** (postgres, mysql, sqlite; runtime-tokio-rustls) for database introspection.
- **tree-sitter** (Python, Go) for code parsing during sync.
- **rust-embed** for embedding UI assets.
- **Axum** for web server.
- **Clap** (derive) for CLI argument parsing.
- **sonner** (UI) for toast notifications.

## Important Files

- `.github/workflows/release.yml` — Release build procedure; shows UI build step.
- `valkyrin-ui/package.json` — Defines `bun run build` (and `dev`, `lint`, `preview`).
- `schema.vdb.json` — The single source of truth (JSON canvas blueprint).

## Implementation Status

All 6 master plan phases are **complete**:

1. **IR Expansion** — DataType enum now has 11 variants (Decimal, Enum, IntSize), Constraints has 5 fields
2. **All Codegen Drivers** — 10 working drivers: GoGorm, GoEnt, PythonSqlModel, PythonSqlAlchemy, RustDiesel, RustSeaORM, JavaScriptSequelize, JavaScriptTypeORM, TypeScriptPrisma, TypeScriptTypeORM
3. **Multi-DBMS Sync Engine** — PostgreSQL, MySQL, SQLite introspection with auto-detect, PK detection, --db-type override
4. **UI Production Hardening** — Strict types, PropertiesSidebar, validation, toast notifications, auto-save, enhanced TableNode, interactive RelationEdge, dark-first palette
5. **Bidirectional Diff Engine** — Column-level diff, FK detection from all 3 DBs, migration SQL generation, --confirm, --dry-run
6. **Edge Case Fixes** — Config validation (supported languages/ORMs), duplicate entity detection, empty table warnings, reserved keyword protection, safe filename sanitization

## Potential Agent Pitfalls

1. **Forgetting to build UI first** — Rust will compile but server won't serve anything.
2. **Assuming tests exist** — There are none to run.
3. **Edition mismatch** — Do not downgrade `edition = "2024"`.
4. **Rust version** — Requires 1.92+.
5. **Bun vs npm** — UI uses Bun, not npm or pnpm.
6. **Sync command** now requires `SyncMode` enum (not just url and db_type).
7. **config.rs** validates supported language/ORM combinations with `ensure!`.
8. Sync Engine Complexity: sync.rs and migration.rs use highly advanced DAG topological sorting, chained cryptographic hashing (valkyrin.sum), and statement-level execution checkpoints. Do NOT replace these with naive loops.

_VERY IMPORTANT_

## The main thing about Valkyrin is that it is supposed to not include AI and should be local and should run without any internet

## Don't add bloated features

## Keep everything production grade

## Commit the code after every successful change
