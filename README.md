# Valkyrin

Valkyrin is a local-first, terminal-driven database schema architect and code generator built using **Rust**. It allows you to design relational database schemas using an interactive visual canvas and automatically compiles them into type-safe, framework-specific backend code.

Unlike traditional diagramming tools that only export static SQL text, Valkyrin functions as a developer utility that writes and maintains ORM models directly within your active local repository.

## Supported Ecosystems

Valkyrin translates a single visual blueprint into native code across five primary programming language environments:

- **Python:** SQLAlchemy and SQLModel
- **Go (Golang):** GORM and Ent
- **Rust:** Diesel and SeaORM
- **JavaScript:** Sequelize and TypeORM
- **TypeScript:** Prisma and TypeORM

## Core Workflow

_Valkyrin_ operates entirely on your local machine through four fundamental CLI commands:

1. `valkyrin init`: Initializes the project context by creating a configuration file that defines your target language, chosen ORM, preferred database engine _(PostgreSQL, MySQL, SQLite)_, and output file paths.
2. `valkyrin canvas`: Spins up a lightweight, local web server and opens a browser-based, drag‑and‑drop node workspace. Here, you visually add tables, define columns, assign data types, and link relationships. Saving updates the local blueprint file.
3. `valkyrin generate`: Compiles the visual blueprint file into actual code files inside your project directory, generating clean structural models or entities for your configured language.
4. `valkyrin sync`: Connects directly to your running development database to inspect the live schema. If changes or manual additions are detected, it updates the visual layout canvas without disturbing your existing configurations.
5. `valkyrin migrate`: Generates migration files from the current canvas, writes both **UP** and **DOWN** SQL statements, and stores them in the `migrations/` directory.
6. `valkyrin push`: Applies the current canvas schema to a live database, creating or altering tables as needed. Use `--confirm` to apply destructive changes and `--dry-run` to preview.
7. `valkyrin check`: Performs a dry‑run diff between the canvas and a live database, reporting mismatches without modifying anything.
8. `valkyrin rollback`: Reverts the most recent migration(s) by executing the generated **DOWN** SQL. Use `--steps <N>` to specify how many migrations to roll back and `--dry‑run` to preview.

## Speciality

1. Does not use any _LLM_ to generate the code.
2. Runs totally _offline and locally_.
3. Multi-language and multi-ORM support (Extra languages and respective ORMs support will be added in further updates).

## Status

_Valkyrin_ is still in it's _early development_ stage so please hang on!!
