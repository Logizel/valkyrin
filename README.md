# Valkyrin

**Local-First Visual Database Architect** — Design schemas visually, generate production-ready ORM code for 10 targets.

Single binary · Embedded React UI · Zero cloud · Zero AI

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
```

## Supported Targets

| Language | ORMs | Native Features |
|----------|------|-----------------|
| Go | GORM, Ent | Native PG enums, UUID, shopspring/decimal |
| Python | SQLModel, SQLAlchemy | Optional[], Relationship(), Enum |
| Rust | Diesel, SeaORM | Queryable/Insertable, FromSqlRow/ToSql |
| JavaScript | Sequelize, TypeORM | DataTypes, decorators, junction tables |
| TypeScript | Prisma, TypeORM | schema.prisma, @@relation, strict types |

## Architecture Diagram (PlantUML)

```plantuml
@startuml
package "valkyrin-cli" {
  [Commands: init, canvas, generate, sync, migrate, push, check, rollback]
}
package "valkyrin-server" {
  [Axum Server :3000] --> [rust-embed UI Assets]
  [GET /api/load] --> [schema.vdb.json]
  [POST /api/save] --> [schema.vdb.json]
}
package "valkyrin-ui (React Flow)" {
  [TableNode] --> [PropertiesSidebar]
  [RelationEdge] --> [1:N / 1:1 / M:N]
  [Auto-save 2s] --> [schema.vdb.json]
}
package "valkyrin-core" {
  [Canvas JSON] --> [IR: EntityGraph]
  [IR] --> [Pass 2: Constraint Injector]
  [Constraint Injector] --> [FK Columns + Junction Tables]
  [IR] --> [10 LanguageDrivers] --> [models/*.{go,py,rs,ts,js,prisma}]
  [AST Merger] --> [Preserves // valkyrin:custom_methods]
  [Sync Engine] <--> [PostgreSQL / MySQL / SQLite]
  [Migration Engine] --> [valkyrin.sum + _valkyrin_migrations]
  [Validator] --> [VAL-001..VAL-021]
}
[schema.vdb.json] <--> [Canvas]
[schema.vdb.json] <--> [Compiler]
@enduml
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

After `valkyrin generate` — struct regenerates, your `FullName()` method preserved intact. Uses tree-sitter (Go/Python) or string fallback (Rust/JS/TS/Prisma).

### B. Bidirectional Sync with Spatial Memory

- `valkyrin sync --url <db>` → introspects live DB, diffs against canvas
- New tables injected at calculated safe spawn points (never overlap existing layout)
- FK relations detected from `information_schema` → drawn as interactive edges
- `schema.vdb.json` updated with positions preserved

### C. Relational Edge Engine — Visual Edges → Real FKs

- Draw edge between tables → click badge to cycle 1:N → 1:1 → M:N
- Compiler Pass 2 mathematically injects FK columns:
  - 1:N → `user_id` on target
  - M:N → auto-generates junction table (`user_group`) with composite PK
- Generated code includes ORM-specific relation helpers (`belongs_to`, `has_many`, `many2many`)

### D. Enterprise Migration Engine (valkyrin.sum)

```
valkyrin.sum (chained SHA-256):
h1:7P3nY...Q==                    ← Directory root hash
20250101120000_initial.sql h1:abc...==
20250102130000_add_users.sql h1:def...==
```

- Tamper detection (4 cases): Removed / Edited / Injected / Appended
- Statement-level checkpoints: SQL parsed by state machine (handles `$tag$`, `DELIMITER`, `BEGIN...END`)
- MySQL-safe: Each statement tracked in `_valkyrin_migrations` with `applied_statements` + `partial_hashes`
- Resume on failure: Re-run picks up exactly at failed statement

### E. Predictive Data-Loss Prevention (ResourceSpan)

**ResourceSpan** (2-bit bitmask):
- 0 = Unknown    (existed before, no changes)
- 1 = Added      (created in this diff)
- 2 = Dropped    (being dropped)
- 3 = Temporary  (Added | Dropped — created AND dropped same diff)

- Single pre-pass computes spans for all tables/columns/indexes/FKs
- Live DB checks (async sqlx):
  - `DROP TABLE`: `SELECT 1 FROM "table" LIMIT 1` → empty = safe
  - `DROP COLUMN`: `SELECT 1 WHERE col IS NOT NULL LIMIT 1` → all NULL = safe
- Only demands `--confirm` when actual data loss detected (VAL-019)

### F. DAG Topological Sorting — Zero-Deadlock Migrations

- Schema changes → Directed Acyclic Graph
- Emits `DROP CONSTRAINT` before `DROP TABLE`
- Resolves circular FK dependencies mathematically

## Configuration (valkyrin.yaml)

```yaml
language: go              # go | python | rust | typescript | javascript
orm: gorm                 # gorm/ent | sqlmodel/sqlalchemy | diesel/seaorm | prisma/typeorm | sequelize/typeorm
database_url_env: DATABASE_URL
environments:
  dev:
    database_url_env: DATABASE_URL_DEV
    output_dir: ./models/dev
  prod:
    database_url_env: DATABASE_URL_PROD
    output_dir: ./models/prod
```

- Validation: rejects unsupported language/ORM combos
- `--env` flag selects environment

## CLI Reference

| Command | Description | Key Flags |
|---------|-------------|-----------|
| `init` | Scaffold `valkyrin.yaml`, empty `schema.vdb.json`, `models/` | — |
| `canvas` | Start embedded React UI at localhost:3000 | — |
| `generate` | Compile blueprint → ORM code in `models/` | — |
| `sync` | DB → Canvas: introspect, diff, inject new tables | `--url`, `--db-type`, `--confirm`, `--dry-run` |
| `migrate` | Apply pending migrations from `migrations/` | `--url`, `--db-type`, `--file` |
| `push` | Canvas → DB: apply changes, create migration files | `--url`, `--db-type`, `--confirm`, `--dry-run` |
| `check` | Dry-run diff or schema validation | `--url`, `--db-type`, `validate --strict` |
| `rollback` | Revert last N migrations via DOWN SQL | `--url`, `--db-type`, `--steps N`, `--dry-run` |

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
	
	// Relations (injected by Pass 2)
	Posts []Post `gorm:"foreignKey:UserID;references:ID" json:"posts,omitempty"`
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
    __tablename__ = "users"
    
    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    email: str = Field(unique=True, index=True)
    balance: Decimal = Field(default=0, sa_column=Column(Numeric(10, 2)))
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
    pub status: UserStatusEnum,  // Native PG enum or generated enum
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
  id        String   @id @default(uuid())
  email     String   @unique
  balance   Decimal  @db.Decimal(10, 2)
  status    UserStatus @default(ACTIVE)
  createdAt DateTime @default(now()) @map("created_at")
  
  posts     Post[]
  
  @@map("users")
}

enum UserStatus {
  ACTIVE
  INACTIVE
}
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

JSON output: `valkyrin --json migrate ...` → `{"code":"VAL-019","message":"...","exit_code":2}`

## Build Instructions

```bash
# Development (UI separate, hot reload)
cd valkyrin-ui && bun install && bun run dev
# Terminal 2:
cargo run -- canvas

# Release (UI MUST build FIRST - embedded via rust-embed)
cd valkyrin-ui && bun install && bun run build
cd .. && cargo build --release
# Binary: target/release/valkyrin-cli

# CI: .github/workflows/release.yml
# opt-level="z", lto="fat", strip=true, panic="abort"
# Cross-compiles: Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64
```