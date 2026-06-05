# Valkyrin

Valkyrin is a local-first, terminal-driven database schema architect and code generator. It allows you to design relational database schemas using an interactive visual canvas and automatically compiles them into type-safe, framework-specific backend code.

Unlike traditional diagramming tools that only export static SQL text, Valkyrin functions as a developer utility that writes and maintains ORM models directly within your active local repository.

## Supported Ecosystems

Valkyrin translates a single visual blueprint into native code across five primary programming language environments:

* **Python:** SQLAlchemy and SQLModel
* **Go (Golang):** GORM and Ent
* **Rust:** Diesel and SeaORM
* **JavaScript:** Sequelize and TypeORM
* **TypeScript:** Prisma and TypeORM

## Core Workflow

Valkyrin operates entirely on your local machine through four fundamental CLI commands:

1. `**valkyrin init**` Initializes the project context by creating a configuration file that defines your target language, chosen ORM, preferred database engine (PostgreSQL, MySQL, SQLite), and output file paths.
3. `**valkyrin canvas**` Spins up a lightweight, local web server and opens a browser-based, drag-and-drop node workspace. Here, you visually add tables, define columns, assign data types, and link relationships. Saving updates the local blueprint file.
4. `**valkyrin generate**` Compiles the visual blueprint file into actual code files inside your project directory, generating clean structural models or entities for your configured language.
5. `**valkyrin sync**` Connects directly to your running development database to inspect the live schema. If changes or manual additions are detected, it updates the visual layout canvas without disturbing your existing configurations.
