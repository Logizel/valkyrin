# Valkyrin

**Local-First Visual Database Architect** — Design schemas visually, generate production-ready ORM code for 10 targets.

Single binary · Embedded React UI · Zero cloud · Zero AI · Zero telemetry

[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org)
[![Bun](https://img.shields.io/badge/bun-1.0%2B-blue.svg)](https://bun.sh)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Build](https://img.shields.io/badge/build-passing-brightgreen.svg)]()
[![Offline](https://img.shields.io/badge/offline-first-red.svg)]()
[![AI-Free](https://img.shields.io/badge/AI--free-000000.svg)]()

## Quick Start

```bash
# Install (via cargo or download release binary)
cargo install valkyrin-cli

# 1. Initialize project
valkyrin init

# 2. Design visually — opens http://localhost:3000
valkyrin canvas

# 3. Generate code
valkyrin generate

# 4. Sync with live database (introspect)
valkyrin sync --url postgresql://user:pass@localhost/db

# 5. Push canvas changes to DB (with safety checks)
valkyrin push --url postgresql://user:pass@localhost/db --confirm

# 6. Validate schema (CI/CD friendly)
valkyrin check validate --strict

# 7. Machine-readable errors for CI/CD
valkyrin --json sync --url postgresql://.../db
```

## Supported Targets

| Language | ORMs | Notes |
|----------|------|-------|
| Go | GORM, Ent | Native PG enums, UUID, shopspring/decimal |
| Python | SQLModel, SQLAlchemy | Optional[], Relationship(), Enum classes |
| Rust | Diesel, SeaORM | Queryable/Insertable, FromSqlRow/ToSql, ActiveEnum |
| JavaScript | Sequelize, TypeORM | DataTypes, decorators, junction tables |
| TypeScript | Prisma, TypeORM | schema.prisma, @@relation, decorators |
| **TypeScript** | **Valkyrin** | **Full type-safe client**: `ValkyrinClient`, `PgDialect`, entity delegates, type-level Select/Include/Omit/Where/Create/Update, SQL AST builder |

## Architecture Diagram

```mermaid
flowchart TB
    subgraph CLI["valkyrin-cli"]
        Commands["Commands:\ninit, canvas, generate,\nsync, migrate, push,\ncheck, rollback"]
    end

    subgraph Core["valkyrin-core"]
        CanvasJSON["Canvas JSON\nschema.vdb.json"]
        IR["IR: EntityGraph"]
        
        subgraph Compiler["Compiler Pipeline"]
            Pass1["Pass 1: Parse &\nDeduplicate"]
            Pass2["Pass 2: FK Injector\n+ Junction Tables"]
            ASTMerger["AST Merger\ntree-sitter / string"]
            Reserved["Reserved Word\nProtection"]
            Prune["Diff-and-Prune\nOrphan Cleanup"]
            Drivers["10 Language Drivers"]
        end
        
        subgraph Sync["Sync Engine"]
            Intro["DB Introspector\nPG / MySQL / SQLite"]
            Diff["Bidirectional\nDiff Engine"]
            ResourceSpan["ResourceSpan\nData-Loss Prediction"]
            Spatial["Safe Spawn\nPoint Calc"]
        end
        
        subgraph Migration["Migration Engine"]
            MigTable["_valkyrin_migrations\ntable"]
            ValkyrinSum["valkyrin.sum\nChained SHA-256"]
            TamperDetect["Tamper Detection\n5 cases"]
            SQLParser["SQL Statement\nState-machine Parser"]
            StmtHash["Statement-level\nHashes"]
            ExecOrder["Execution Order\nEnforcement"]
        end

        subgraph Validate["Schema Validator"]
            Rules["6 Validation Rules"]
            StrictMode["--strict mode"]
        end
    end

    subgraph Server["valkyrin-server"]
        Axum["Axum Server :3000"]
        Embed["rust-embed\nUI Assets"]
        APILoad["GET /api/load"]
        APISave["POST /api/save"]
    end

    subgraph UI["valkyrin-ui (React Flow)"]
        TableNode["TableNode\nconstraint badges"]
        Props["PropertiesSidebar\nreal-time validation"]
        Edge["RelationEdge\n1:1 / 1:N / M:N"]
        AutoSave["Auto-save 2s\n+ Ctrl+S"]
    end

    subgraph External["External Actors"]
        PG["PostgreSQL"]
        MySQL["MySQL"]
        SQLite["SQLite"]
    end

    SchemaVDB["schema.vdb.json"]
    Output["models/*.{go,py,rs,ts,js,prisma}"]
    ClientOut["models/valkyrin-client/\n{enums,types,operations,\nclient,index}.ts"]

    Commands --> CanvasJSON
    CanvasJSON --> IR
    IR --> Pass1
    Pass1 --> Reserved
    Reserved --> Pass2
    Pass2 --> ASTMerger
    ASTMerger --> Drivers
    IR --> Validate
    Validate --> Rules
    Rules --> StrictMode

    Intro --> Diff
    Intro <--> PG
    Intro <--> MySQL
    Intro <--> SQLite
    Diff --> ResourceSpan
    ResourceSpan --> Spatial
    Spatial --> SchemaVDB

    MigTable --> ValkyrinSum
    ValkyrinSum --> TamperDetect
    TamperDetect --> SQLParser
    SQLParser --> StmtHash
    StmtHash --> ExecOrder
    ExecOrder --> PG
    ExecOrder --> MySQL
    ExecOrder --> SQLite

    Axum --> Embed
    Axum --> APILoad
    Axum --> APISave
    APILoad --> SchemaVDB
    APISave --> SchemaVDB
    SchemaVDB <--> TableNode
    TableNode --> Props
    TableNode --> Edge
    AutoSave --> SchemaVDB

    Drivers --> Output
    Drivers --> Prune
    Prune --> Output

    Drivers -. TypeScriptValkyrin .-> ClientOut

    classDef cli fill:#1e293b,stroke:#64748b,stroke-width:2px,color:#f1f5f9;
    classDef core fill:#1e3a5f,stroke:#3b82f6,stroke-width:2px,color:#dbeafe;
    classDef server fill:#7c2d12,stroke:#ea580c,stroke-width:2px,color:#fed7aa;
    classDef ui fill:#4c1d95,stroke:#a855f7,stroke-width:2px,color:#e9d5ff;
    classDef ext fill:#14532d,stroke:#22c55e,stroke-width:2px,color:#dcfce7;
    classDef data fill:#78350f,stroke:#f59e0b,stroke-width:3px,color:#fef3c7;
    
    class CLI cli;
    class Core core;
    class Server server;
    class UI ui;
    class External ext;
    class IR,CanvasJSON,SchemaVDB,Output,ClientOut data;
```

## Core Features Deep-Dive

### A. AST Code Merger — Preserves Your Custom Code

```go
// BEFORE generation (models/user.go)
type User struct {
    ID   uuid.UUID `gorm:"primaryKey"`
    Name string
}

// valkyrin:custom_methods_start
func (u *User) FullName() string {
    return u.FirstName + " " + u.LastName
}
// valkyrin:custom_methods_end
```

After `valkyrin generate` — struct regenerates, your `FullName()` method preserved intact.
- **Go, Python**: Uses `tree-sitter` to parse AST and locate markers
- **Rust, JS, TS, Prisma**: String-based fallback with comment markers

### B. Bidirectional Diff Engine

- `valkyrin sync --url <db>` → introspects live DB, diffs against canvas
- Column-level comparison: type changes, nullable changes, unique changes, default value changes, enum value additions
- New tables injected at calculated safe spawn positions (never overlap existing layout)
- FK relations detected from `information_schema` / `PRAGMA foreign_key_list` → drawn as interactive edges
- `schema.vdb.json` updated with positions and relations preserved

### C. Relational Edge Engine — Visual Edges → Real FKs

- Draw edge between tables → click badge to cycle 1:N → 1:1 → M:N
- Compiler Pass 2 mathematically injects FK columns:
  - 1:N → `table_id` on target entity
  - M:N → auto-generates junction table with composite PK (alphabetically sorted name)
- Generated code includes ORM-specific relation helpers:
  - GORM: `gorm:"foreignKey:...;references:..."`
  - Ent: `field.ToMany()`, `field.BelongsTo()`
  - SQLModel: `Relationship(back_populates=...)`
  - Diesel: `joinable!()` macro
  - SeaORM: `Related` trait links
  - Prisma: `@relation()` attributes
  - TypeORM: `@OneToMany`/`@ManyToOne`/`@JoinColumn` decorators

### D. Enterprise Migration Engine (valkyrin.sum)

```
valkyrin.sum (chained SHA-256):
h1:7P3nY...Q==                    ← Directory root hash
20250101120000_initial.sql h1:abc...==
20250102130000_add_users.sql h1:def...==
```

- **Chained hashing**: Each file's hash incorporates the previous file's hash (tamper-evident ordering)
- **5 tamper detection cases**: Removed / Edited / Injected / Appended / ChecksumNotFound
- **SQL statement parser**: State-machine handles `$tag$` dollar quoting, `DELIMITER` (MySQL), `BEGIN...END` blocks, parentheses depth, quoted strings, backtick identifiers, line/block comments, `#` comments (MySQL)
- **Statement-level checkpoints**: Each statement tracked with chained hash; resume on failure picks up exactly at the failed statement
- **Execution order enforcement**: Rejects gaps and non-linear migration histories (VAL-020)
- **Per-DBMS locking**: PostgreSQL advisory locks, MySQL `GET_LOCK()`, SQLite file locks

### E. Predictive Data-Loss Prevention (ResourceSpan)

**ResourceSpan** (2-bit bitmask lifecycle tracking):
- `00` = Unknown    (existed before, no changes in this diff)
- `01` = Added      (created in this diff)
- `10` = Dropped    (being dropped in this diff)
- `11` = Temporary  (Added|Dropped — created AND dropped same diff, no-op)

Single pre-pass computes spans for all tables, columns, indexes, and foreign keys.
Live DB checks via async sqlx:
- `DROP TABLE`: `SELECT 1 FROM "table" LIMIT 1` → empty = safe
- `DROP COLUMN`: `SELECT 1 WHERE col IS NOT NULL LIMIT 1` → all NULL = safe

Only demands `--confirm` when actual data loss is detected (VAL-019).

### F. TypeScriptValkyrin — Full Type-Safe Client Generation

Generates a complete database client (not just models):

```
models/valkyrin-client/
├── enums.ts         # $Enums namespace with union types
├── types.ts         # Type-level machinery: _GetFindResult, _DefaultSelection, _ApplyOmit, ValkyrinExtensions
├── operations.ts    # Per-entity: Payload, Select, Include, Omit, WhereInput, OrderByInput,
                    #   CreateInput, UpdateInput, FindUnique/Many/Create/Update/Delete/Upsert/Aggregate/GroupBy args
├── client.ts        # Runtime: ValkyrinClient class, entity delegates (findUnique, findMany, count),
                    #   SQL AST types (Column, SelectQuery, WhereNode, Join, OrderBy),
                    #   PgDialect (PostgreSQL parameterized query builder)
└── index.ts         # Barrel exports
```

```typescript
import { ValkyrinClient, PgDialect } from './valkyrin-client';

const client = new ValkyrinClient(new PgDialect(), dbConnection);

// Type-safe queries
const user = await client.user.findUnique({
  where: { id: "abc-123" },
  select: { scalars: { name: true, email: true } },
});

const posts = await client.post.findMany({
  where: {
    scalars: { published: { equals: true } },
    objects: { author: { is: { scalars: { name: { equals: "Alice" } } } } },
  },
  take: 10,
});
```

### G. Schema Validation — 6 Rules

| Rule | Description | Severity |
|------|-------------|----------|
| NoNullablePk | Primary key columns must not be nullable | Error |
| FkIndexed | Foreign key fields (ending in `_id`) should be indexed | Warning |
| EnumHasValues | Enum types must have at least one value | Warning |
| NoDuplicateEntities | Case-insensitive duplicate table names | Warning |
| NoReservedNames | SQL/language keywords as table/column names | Warning |
| TableHasColumns | Tables must have at least one column | Warning |

Run with `--strict` to promote all warnings to errors (exit code 2).

### H. UI Production Features

- **PropertiesSidebar**: Right-side edit panel with real-time validation on every keystroke — identifier regex check (`/^[a-zA-Z_][a-zA-Z0-9_]*$/`), duplicate detection, decimal precision/scale bounds (1-65), enum value validation
- **Auto-save**: 2-second debounced auto-save + Ctrl+S keyboard shortcut
- **RelationEdge**: Interactive badge on every edge — click to cycle `1:1` → `1:N` → `M:N` with hover tooltip
- **TableNode**: Gradient header, constraint badges (PK=yellow, U=purple, IDX=blue, Ø=nullable, D=default), hover-reveal delete buttons
- **Dark theme**: `bg-zinc-950` canvas with `#27272a` grid background, cyan accent highlights
- **Toast notifications**: Sonner toast library for save confirmations and error feedback

### I. Diff-and-Prune — Orphaned File Cleanup

After every `valkyrin generate`, the compiler scans the `models/` directory for managed extensions (`.go`, `.py`, `.rs`, `.ts`, `.js`, `.prisma`) and **automatically removes** any file whose stem doesn't match a current entity name. This prevents stale model files from accumulating when tables are renamed or removed.

## Configuration (valkyrin.yaml)

```yaml
language: go              # go | python | rust | typescript | javascript
orm: gorm                 # gorm/ent | sqlmodel/sqlalchemy | diesel/seaorm | prisma/typeorm/valkyrin | sequelize/typeorm
database_url_env: DATABASE_URL
environments:
  dev:
    database_url_env: DATABASE_URL_DEV
    output_dir: ./models/dev
  prod:
    database_url_env: DATABASE_URL_PROD
    output_dir: ./models/prod
```

- Validation rejects unsupported language/ORM combinations at load time
- `--env` flag selects environment (resolves per-env `.env.<env>` files)

## CLI Reference

| Command | Description | Key Flags |
|---------|-------------|-----------|
| `init` | Scaffold `valkyrin.yaml`, empty `schema.vdb.json`, `models/` | — |
| `canvas` | Start embedded React UI at localhost:3000 | — |
| `generate` | Compile blueprint → ORM code in `models/` | — |
| `sync` | DB → Canvas: introspect, diff, inject new tables | `--url`, `--db-type`, `--confirm`, `--dry-run` |
| `migrate` | Apply pending migrations from `migrations/` | `--url`, `--db-type`, `--file` |
| `push` | Canvas → DB: generate DDL and execute | `--url`, `--db-type`, `--confirm`, `--dry-run` |
| `check` | Dry-run sync diff or schema validation | `--url`, `--db-type`, `validate --strict` |
| `rollback` | Revert last N migrations via DOWN SQL | `--url`, `--db-type`, `--steps N`, `--dry-run` |

Global flags: `--json` outputs structured JSON errors for CI/CD integration.

## Generated Code Examples

### Go (GORM)

```go
package models

import (
	"time"
	"github.com/shopspring/decimal"
	"github.com/google/uuid"
	"gorm.io/datatypes"
)

type User struct {
	ID        uuid.UUID       `gorm:"column:id;primaryKey" json:"id"`
	Email     string          `gorm:"column:email;uniqueIndex" json:"email"`
	Balance   decimal.Decimal `gorm:"column:balance;type:numeric(10,2)" json:"balance"`
	Metadata  datatypes.JSON  `gorm:"column:metadata" json:"metadata"`
	CreatedAt time.Time       `gorm:"column:created_at" json:"created_at"`
	Posts     []Post          `gorm:"foreignKey:UserID;references:ID" json:"posts,omitempty"`
}
```

### Go (Ent)

```go
package models

import (
	"entgo.io/ent"
	"entgo.io/ent/schema/field"
	"github.com/google/uuid"
)

type User struct {
	ent.Schema
}

func (User) Fields() []ent.Field {
	return []ent.Field{
		field.UUID("id", uuid.UUID{}).
			Default(uuid.New).
			StorageKey("id"),
		field.String("email").
			Unique().
			StorageKey("email"),
		field.Time("created_at").
			StorageKey("created_at"),
	}
}
```

### Python (SQLModel)

```python
from typing import Optional
from datetime import datetime
from decimal import Decimal
from sqlmodel import SQLModel, Field, Relationship
from enum import Enum

class UserStatus(str, Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"

class User(SQLModel, table=True):
    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    email: str = Field(unique=True, index=True)
    balance: Decimal = Field(default=0)
    status: UserStatus = Field(default=UserStatus.ACTIVE)
    created_at: datetime = Field(default_factory=datetime.utcnow)
    posts: list["Post"] = Relationship(back_populates="user")
```

### Rust (Diesel)

```rust
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;

#[derive(Queryable, Insertable, Selectable, Serialize, Deserialize, Debug, Clone)]
#[diesel(table_name = users)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub balance: BigDecimal,
    pub status: UserStatusEnum,
    pub created_at: NaiveDateTime,
}

// valkyrin:custom_methods_start
impl User {
    pub fn is_active(&self) -> bool {
        self.status == UserStatusEnum::Active
    }
}
// valkyrin:custom_methods_end
```

### TypeScript (Prisma)

```prisma
model User {
  id        String    @id @default(uuid())
  email     String    @unique
  balance   Decimal   @db.Decimal(10, 2)
  status    UserStatus @default(ACTIVE)
  createdAt DateTime  @default(now()) @map("created_at")
  posts     Post[]
  @@map("users")
}

enum UserStatus {
  ACTIVE
  INACTIVE
}
```

### TypeScript (Valkyrin Client)

```typescript
import { ValkyrinClient, PgDialect, DatabaseConnection } from './valkyrin-client';

// Use with any PostgreSQL driver
const conn: DatabaseConnection = {
  async query(sql, params) {
    const result = await pgPool.query(sql, params);
    return { rows: result.rows };
  },
};

const client = new ValkyrinClient(new PgDialect(), conn);

// Type-safe findUnique
const user = await client.user.findUnique({
  where: { id: "123e4567-e89b-12d3-a456-426614174000" },
  select: { scalars: { name: true, email: true } },
});

// Type-safe findMany with relations
const posts = await client.post.findMany({
  where: {
    scalars: { published: { equals: true } },
    objects: { author: { is: { scalars: { name: { equals: "Alice" } } } } },
  },
  orderBy: { scalars: { createdAt: "desc" } },
  take: 10,
  include: { objects: { comments: true } },
});
```

## Error Codes (VAL-001 to VAL-021)

| Code | Name | Exit | Description |
|------|------|------|-------------|
| VAL-001 | Config | 2 | YAML parse, unsupported language/ORM |
| VAL-002 | Schema | 2 | Invalid schema structure |
| VAL-003 | Database | 2 | Connection failed |
| VAL-004 | Migration | 2 | Apply/rollback failure |
| VAL-005 | Codegen | 2 | Template/render error |
| VAL-006 | Io | 2 | File read/write |
| VAL-007 | Parse | 2 | JSON/YAML/SQL parse |
| VAL-008 | Validation | 1 | Schema warnings (errors with `--strict`) |
| VAL-009 | Introspection | 2 | DB schema fetch failed |
| VAL-010 | Sync | 2 | Diff/sync failure |
| VAL-011 | CliArg | 2 | Invalid CLI arguments |
| VAL-012 | Internal | 2 | Unexpected bug |
| VAL-013 | ChecksumMismatch | 2 | Migration modified after apply |
| VAL-014 | HistoryTampered | 2 | `valkyrin.sum` integrity violation |
| VAL-015 | MigrationRemoved | 2 | Migration file deleted |
| VAL-016 | MigrationEdited | 2 | Migration file edited |
| VAL-017 | MigrationInjected | 2 | Migration inserted out-of-order |
| VAL-018 | ChecksumNotFound | 2 | `valkyrin.sum` missing |
| VAL-019 | DestructiveChange | 2 | Live data would be lost |
| VAL-020 | HistoryNonLinear | 2 | Skipped migration versions |
| VAL-021 | StatementExecError | 2 | Individual SQL failed |

JSON output: `valkyrin --json sync --url postgresql://...` → `{"code":"VAL-019","message":"...","exit_code":2}`

## Build Instructions

```bash
# Development (UI separate, hot reload)
cd valkyrin-ui && bun install && bun run dev
# Terminal 2:
cargo run -- canvas

# Release (UI MUST build FIRST — embedded via rust-embed)
cd valkyrin-ui && bun install && bun run build
cd .. && cargo build --release
# Binary: target/release/valkyrin-cli  (note: NOT just "valkyrin")

# CI: .github/workflows/release.yml
# opt-level="z", lto="fat", strip=true, panic="abort"
# Rust edition 2024 (requires rustc 1.92+)
# Cross-compiles: Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64
```
