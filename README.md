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

1. **valkyrin init** Initializes the project context by creating a configuration file that defines your target language, chosen ORM, preferred database engine (PostgreSQL, MySQL, SQLite), and output file paths.
2. **valkyrin canvas** Spins up a lightweight, local web server and opens a browser-based, drag-and-drop node workspace. Here, you visually add tables, define columns, assign data types, and link relationships. Saving updates the local blueprint file.
3. **valkyrin generate** Compiles the visual blueprint file into actual code files inside your project directory, generating clean structural models or entities for your configured language.
4. **valkyrin sync** Connects directly to your running development database to inspect the live schema. If changes or manual additions are detected, it updates the visual layout canvas without disturbing your existing configurations.

## Key Technical Features

### Split-State Architecture

Valkyrin separates the relational schema logic from the visual layout data. Database structures are saved in a clean blueprint file committed to version control, while visual coordinates are kept in a separate file added to your `.gitignore`. This ensures multiple team members can work on the database simultaneously without ever encountering Git merge conflicts caused by shifting visual boxes.

### Abstract Syntax Tree (AST) Merging

Valkyrin reads your existing code files using language-specific syntax trees before updating them. It identifies, extracts, and preserves custom hand-written code—such as custom methods, validation routines, or password-hashing hooks. When you update the schema via the canvas, Valkyrin safely stitches your custom logic back into place, ensuring your work is never overwritten.

### Rename and Data Loss Protection

The visual engine tracks tables and columns using internal, immutable unique identifiers under the hood rather than tracking them strictly by their text names. If you rename a table or column on the canvas to fix a typo, Valkyrin recognizes it as the same entity and generates a safe SQL rename statement instead of interpretative drop-and-add commands that cause data loss in production.

### Two-Pass Compilation

To handle complex relational architectures where tables point to one another in circular loops, the generation engine decouples base structures from relational constraints. It builds the primary tables first and injects foreign key constraints in a secondary phase, eliminating circular dependency errors and deadlocks during compilation.
