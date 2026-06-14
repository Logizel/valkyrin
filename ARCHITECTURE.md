1. Project Overview

Valkyrin is a Local-First Visual Database Architect. It allows developers to visually design database schemas (tables, columns, and relationships) on a local browser canvas, and mathematically compiles that visual graph into production-ready, strictly typed backend code (e.g., Golang structs with GORM tags).

Tech Stack:

    Backend/Compiler: Rust (Crates: clap, tokio, axum, sqlx, anyhow, serde).

    Frontend: React + TypeScript + Vite + Tailwind CSS + @xyflow/react (React Flow).

    Distribution: Single, highly optimized standalone cross-compiled binary. The React frontend is bundled directly into the Rust binary using rust-embed.

2. Workspace Architecture

The project is structured as a Rust Cargo Workspace containing three Rust crates and one Node/Bun frontend project.
📦 valkyrin-ui (The Frontend)

A React Flow application that acts as the visual designer.

    App.tsx: Manages state (nodes, edges), fetches /api/load on mount to restore spatial memory, and POSTs to /api/save to persist the graph to disk. Includes CRUD logic for adding/deleting tables and columns.

    TableNode.tsx: A custom React Flow node representing a database table. Uses Tailwind group-hover classes to reveal ✕ buttons for deleting the table or individual columns.

    RelationEdge.tsx: A custom interactive React Flow edge. It places a clickable badge on the connection line to cycle through relationship types (1:N, 1:1, M:N).

📦 valkyrin-server (The Local Bridge)

An axum web server that binds to 127.0.0.1:3000.

    Serves the bundled valkyrin-ui/dist folder via rust-embed.

    GET /api/load: Reads schema.vdb.json from the user's disk and sends it to React.

    POST /api/save: Receives the JSON payload from React and writes it to schema.vdb.json with pretty-formatting.

📦 valkyrin-core (The Compiler Engine)

The brain of the application. It handles parsing, code generation, and database synchronization.

    ir.rs & canvas.rs: Defines the Intermediate Representation (IR). Transforms the "dumb" Canvas JSON into strict Rust memory types (EntityGraph, Entity, Field, Constraints). Preserves NodePosition (X/Y coordinates).

    codegen.rs: Implements the Strategy Pattern using a LanguageDriver trait. Currently implements GoDriver to map IR types to Golang types and generate GORM tags.

    ast.rs: The CodeMerger. When generating code, it reads existing .go files, extracts any developer code written inside // valkyrin:custom_methods_start boundaries, and stitches it back into the newly generated schema.

    sync.rs: The Database Introspector. Connects to live PostgreSQL databases via sqlx, queries information_schema.columns, diffs the live database against schema.vdb.json, and calculates safe X/Y spawn points to inject missing production tables into the visual layout without overlapping existing nodes.

    config.rs: Handles scaffolding and parsing of valkyrin.yaml (Project Name, Target Language, DB URL).

    compiler.rs: The Master Orchestrator.

        Pass 1: Parses JSON to IR.

        Pass 2 (Constraint Injector): Analyzes relationships and mathematically injects Foreign Keys (e.g., user_id) into target tables.

        Pass 3: Dynamically loads the LanguageDriver based on valkyrin.yaml, generates code, merges AST, and writes to the models/ directory.

📦 valkyrin-cli (The Command Router)

The user-facing executable built with clap and tokio. Contains strict error/panic boundaries using anyhow and colored.

    valkyrin init: Scaffolds valkyrin.yaml, an empty schema.vdb.json, and the models/ folder.

    valkyrin canvas: Boots the valkyrin-server and holds the terminal open.

    valkyrin generate: Triggers the valkyrin_core::compiler::compile_blueprint() loop.

    valkyrin sync --url <DB>: Connects to PostgreSQL, diffs the schema, and updates the local JSON visual layout.

3. Data Flow & Core Files
   A. The Single Source of Truth: schema.vdb.json

This file lives in the user's project root. It is updated by the React Canvas and read by the Rust Compiler.
JSON

{
"tables": [
{
"id": "uuid",
"name": "Users",
"columns": [{ "id": "uuid", "name": "email", "raw_type": "string", "is_primary": false, "is_nullable": false }],
"position": { "x": 100, "y": 100 }
}
],
"relations": [
{
"id": "uuid",
"source_table_id": "uuid",
"target_table_id": "uuid",
"relation_type": "1:N"
}
]
}

B. The Configuration: valkyrin.yaml
YAML

project_name: my_backend_service
language: go # Determines which LanguageDriver is instantiated (go, python, rust, typescript)
database_url_env: DATABASE_URL

4. Current State & Known Limitations

The application is structurally complete, robust, and capable of end-to-end codeless database generation. However, if expanding the tool, note the following areas for development:

    Missing Language Drivers: The LanguageDriver trait is fully implemented for Golang (GoDriver). Python is partially stubbed. Rust and TypeScript drivers return unimplemented!().

    Sync Engine Limitations: The sync.rs engine currently only introspects PostgreSQL. MySQL and SQLite traits are scaffolded but not implemented. Furthermore, it only reads columns; it does not currently parse live Foreign Key constraints from pg_constraint.

    Advanced Types: The React UI currently limits users to standard primitive types (string, int, boolean, datetime, uuid, json) via a browser prompt. Enums or deeply nested Postgres arrays are not yet fully modeled in the UI.

5. Build & Deployment

   Frontend assets must be compiled prior to Rust compilation (cd valkyrin-ui && bun run build).

   Cross-compilation is handled via GitHub Actions (.github/workflows/release.yml), outputting stripped, lto="fat", opt-level="z" binaries for Linux, macOS, and Windows.

Agent Instructions:
When modifying this codebase, strictly adhere to the established architectural boundaries:

    DO NOT put code generation logic in the CLI crate.

    DO NOT put network/server logic in the Core crate.

    Treat schema.vdb.json as the absolute source of truth; any UI modifications must successfully serialize back into this schema, and any Core modifications must read from it natively.

## Key Architectural Differentiators

### 1. The AST Code Merger (Non-Destructive Generation)

- **The Problem:** Traditional code generators are notoriously destructive. If you add a custom helper function (like `func (u *User) GetFullName()`) to an automatically generated `user.go` file, running the generator again will wipe out your custom code.
- **The Valkyrin Solution:** Valkyrin uses an Abstract Syntax Tree (AST) parser before it writes to your disk. It scans your existing files for `// valkyrin:custom_methods_start` boundaries, extracts your custom business logic, generates the new database struct, and seamlessly stitches your custom logic back into the file. You get visual schema management without sacrificing code ownership.

### 2. True Local-First & Zero-Friction Architecture

- **The Problem:** Most visual database designers are SaaS products. They require accounts, cloud syncing, and internet connections, and your database architecture is trapped on their servers.
- **The Valkyrin Solution:** Valkyrin is a single, cross-compiled Rust binary with the entire React frontend embedded directly inside it via `rust-embed`. You run `valkyrin canvas` and the UI is served locally at `localhost:3000`. The absolute source of truth is a `schema.vdb.json` file that lives inside your Git repository. If GitHub goes down, Valkyrin still works.

### 3. Live Introspection with Spatial Memory (Two-Way Sync)

- **The Problem:** If a DBA manually alters a table in production via pgAdmin or psql, your visual diagram immediately becomes outdated. When you import a database into other visualizers, it usually drops all the tables in a messy, overlapping pile.
- **The Valkyrin Solution:** The `valkyrin sync --url <DB>` command tunnels into a live PostgreSQL database, mathematically diffs the live `information_schema` against your local canvas, and injects only the missing tables. Furthermore, it utilizes **Spatial Memory**. It remembers the exact X/Y pixel coordinates of where you dragged your tables, calculating "safe spawn points" for new tables so they never overlap your beautifully organized layout.

### 4. Direct-to-Native Code Orchestration

- **The Problem:** Visual tools usually export raw `.sql` files. You still have to spend hours writing the Go, Python, or Rust structs and carefully adding the correct ORM tags (like `gorm:"primaryKey"`) to map your application to the database.
- **The Valkyrin Solution:** Through its dynamic `LanguageDriver` trait and `valkyrin.yaml` configuration, Valkyrin translates your visual graph into strict Intermediate Representation (IR), and then physically generates the exact native language files your backend needs. A visual change to a table on the canvas instantly becomes a perfectly formatted Go struct or Python SQLAlchemy model.

### 5. The Relational Edge Engine (Visual Constraint Injection)

- **The Problem:** In many tools, drawing a line between tables is purely cosmetic. It doesn't actually affect the generated code, leaving the developer to figure out the Foreign Keys manually.
- **The Valkyrin Solution:** Valkyrin features an interactive Relational Edge. When you connect two tables in the UI, you can click a badge on the connecting line to toggle between `1:N`, `1:1`, and `M:N`. When you hit Generate, the Compiler Engine's "Pass 2 Constraint Injector" intercepts this graph and mathematically injects the correct Foreign Key fields (e.g., `user_id *string`) directly into the target structs.
