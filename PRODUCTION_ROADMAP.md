# VALKYRIN: BRUTALLY HONEST PRODUCTION ROADMAP

**Philosophy: No AI, local-only, zero bloat, production-grade, offline-first, professional + hobby user focused.**

---

## 🔍 BRUTAL REALITY CHECK

### What the OLD roadmap CLAIMED was missing vs ACTUAL CODEBASE STATE:

| Roadmap Item               | Claimed Status       | Actual Status | Notes                                                                                               |
| -------------------------- | -------------------- | ------------- | --------------------------------------------------------------------------------------------------- |
| Test infrastructure        | "Missing - no tests" | **PARTIAL**   | Rust tests exist, CI workflow with DB services, but **no UI tests exist** (bun test finds 0 tests)  |
| Migration execution engine | "Missing"            | **PARTIAL**   | execute_migration() exists but NO migration lock table, NO version tracking, NO checksum validation |
| Push command               | "Missing"            | **EXISTS**    | push_to_database() implemented but uses raw SQL execution, no proper migration tracking             |
| Enum introspection         | "Broken"             | **PARTIAL**   | MySQL/PostgreSQL exist but PostgreSQL uses VARCHAR fallback, no native CREATE TYPE                  |
| Full relationship codegen  | "Missing"            | **PARTIAL**   | generate_relations() exists in all 10 drivers but may be stubbed                                    |
| M:N junction tables        | "Missing"            | **MISSING**   | No auto-generation in compiler                                                                      |
| Index emission             | "Missing"            | **MISSING**   | is_indexed flag in IR but NOT emitted in any driver output                                          |
| Composite PK support       | "Missing"            | **PARTIAL**   | primary_key_order exists but drivers may not fully utilize it                                       |
| Structured errors          | "Missing"            | **MISSING**   | Using anyhow::Error everywhere, no error codes                                                      |
| Multi-env config           | "Missing"            | **PARTIAL**   | dotenvy loads .env but no --env CLI flag, no config commands                                        |
| valkyrin check             | "Missing"            | **EXISTS**    | check_sync exists but is dry-run, NOT schema validation                                             |
| Documentation generation   | "Missing"            | **MISSING**   | No --docs flag                                                                                      |

**CONCLUSION: ~60% of "Phase 1-2" work is already done or partial. The old roadmap is outdated.**

---

## 🎯 REFINED PRODUCTION ROADMAP (REALISTIC)

### 🔥 IMMEDIATE BLOCKERS (Week 1)

**Blocking all production use: Migration safety is non-existent**

1. **Migration Lock Table** - Without this, concurrent runs corrupt DB
   - File: `valkyrin-core/src/migration.rs`
   - No migration history being tracked - just raw SQL execution
   - Risk: Production DB corruption

2. **Index Emission in Drivers** - Indexed columns silently ignored
   - All 10 drivers ignore `is_indexed: true`
   - Risk: Performance issues in generated code

3. **M:N Junction Auto-Generation** - Many-to-many relations incomplete
   - Relation exists in IR but no junction table created
   - Risk: Incomplete ORM code for complex relationships

### ⚙️ CORE PRODUCT (Weeks 2-3)

4. **Structured Error Types** - CI/CD integration impossible
   - File: `valkyrin-core/src/error.rs`
   - Replace anyhow::Error with ValkyrinError enum + error codes
   - Add `--json` flag for machine-readable output

5. **Schema Validation Command** - PR gates don't exist
   - File: `valkyrin-core/src/validate.rs`, CLI `check validate` subcommand
   - Rules: no nullable PK, FK columns indexed, enum has values, no reserved names

### 🏗️ PRODUCT MATURITY (Weeks 4-5)

6. **Native Enum Types** - PostgreSQL enum support incomplete
   - Current: Falls back to VARCHAR(255)
   - Fix: CREATE TYPE in PostgreSQL, proper MySQL enum

7. **Multi-Environment Config** - Production workflows need this
   - Add `--env` flag to all DB commands
   - Support .env.local, .env.{environment}

8. **Documentation Generation** - Adoption friction
   - File: `valkyrin-core/src/docs.rs`
   - `valkyrin generate --docs` → models/README.md with connection examples

### 🚀 DIFFERENTIATORS (Ongoing, Quality-of-Life)

9. **Diff Visualization Overlay** - UX improvement
   - Show pending changes on canvas before apply

10. **Export Formats** - Documentation and sharing
    - Mermaid, DBML, PlantUML export

---

## 📋 PRIORITY ORDERED TASKS

### PRIORITY 1: MIGRATION SAFETY (BLOCKER)

```
1.1: Create migration history table and tracking
     - _valkyrin_migrations table with version, name, checksum, applied_at, success
     - Lock mechanism per DB (pg_advisory_lock, GET_LOCK, file lock)
     - Method: ApplyMigration::with_lock()

1.2: Add migration checksum validation
     - SHA256 hash of migration SQL stored on apply
     - Reject if migration file modified after apply

1.3: Track applied migrations in run_migrations
     - Read from _valkyrin_migrations, execute only pending
     - Mark success/failure in table
```

### PRIORITY 2: INDEX EMISSION (BLOCKER)

```
2.1: Emit indexes in all 10 drivers
     - GoGORM: gorm:"index" + composite with separate index tags
     - GoEnt: .Index() field modifier
     - Python SqlModel: index=True on Field
     - Python SqlAlchemy: index=True on Column
     - Rust Diesel: #[diesel(index)] attribute or index! macro
     - Rust SeaORM: #[sea_orm(index)] attribute
     - JS Sequelize: index: true in field definition
     - JS TypeORM: @Index() decorator
     - TS Prisma: @@index([field])
     - TS TypeORM: @Index() decorator

COMMIT AFTER EACH DRIVER - allows incremental verification
```

### PRIORITY 3: M:N JUNCTION TABLES (BLOCKER)

```
3.1: Auto-generate junction entities in compiler
     - When RelationType::ManyToMany detected
     - Create Entity with two FK columns + composite unique index
     - Naming: alphabetical join (e.g., user_group for User↔Group)

3.2: Emit junction + relation helpers in all drivers
     - Foreign keys on both sides
     - Navigation methods (user.groups(), group.users())
```

### PRIORITY 4: STRUCTURED ERRORS + JSON (BLOCKER)

```
4.1: Create ValkyrinError enum
     - VAL-001 Config, VAL-002 Schema, VAL-003 Database, etc.
     - thiserror crate

4.2: Add --json flag to CLI
     - Serialize errors as JSON for CI/CD parsing
     - Exit codes: 0=success, 1=warning, 2=error
```

### PRIORITY 5: SCHEMA VALIDATION (BLOCKER)

```
5.1: Create validate.rs module
     - ValidationRule enum with check() method
     - Rules: PK not nullable, FK indexed, enum values present, no duplicates, no reserved words

5.2: Add validate subcommand to CLI
     - valkyrin check validate --strict
     - Returns exit code 2 on errors, 1 on warnings
```

---

## 🚫 WHAT TO NOT BUILD (ANTI-FEATURES)

- **AI features** - Explicitly forbidden
- **Cloud sync** - Local-only requirement
- **Plugin marketplace** - Plugin system adds complexity, defer
- **Web-based auth** - Security and offline breach
- **Real-time collaboration** - Unnecessary bloat
- **ERD import beyond introspection** - Scope creep

---

## ✅ DEFINITION OF "PRODUCTION-READY"

**Required before v1.0:**

- [ ] Migration lock table prevents concurrent corruption
- [ ] Migration checksums prevent tampering
- [ ] Index emission works in all drivers
- [ ] M:N relations auto-generate junction tables
- [ ] `--json` flag for all commands
- [ ] `valkyrin check validate` for PR gates
- [ ] All tests pass in CI (cargo test + bun test)
  > - **UI tests**: CI runs `bun test` but zero test files exist

**Nice-to-have for v1.0:**

- [ ] PostgreSQL native enum types
- [ ] Multi-environment config with --env flag
- [ ] Documentation generation

---

## 📊 CURRENT TECHNICAL DEBT (VERIFIED)

1. **Migration engine** - execute_migration has no transaction safety, no migration lock table, no version tracking
2. **Index emission** - `is_indexed` completely ignored in all 10 drivers (verified: no `index` emission in any driver)
3. **Index introspection** - All introspectors set `is_indexed: false` hardcoded, no index detection query
4. **M:N junction** - RelationType::ManyToMany exists but explicitly skipped in GoGormDriver (line 211: `// skip for now`)
5. **Error handling** - All errors use anyhow::Error, no structured codes, no `--json` output
6. **Check command** - Currently `check_sync` (diff preview), NOT schema validation
7. **CLI test gaps** - codegen_tests.rs and property_roundtrip_tests.rs missing JavaScriptTypeOrmDriver (10 drivers exist, 9 tested)
8. **UI test gap** - vitest configured but zero test files exist (CI passes vacuously)

---

## 🛠️ COMMIT STRATEGY

Format: `<priority>.<task>: <action>`

Examples:

- `1.1: create migration history table and tracking`
- `2.1.1: emit gorm index tags for indexed columns`
- `3.1: auto-generate junction entity for many-to-many relations`
- `4.1: add ValkyrinError enum with structured error codes`

**One commit per logical change. No feature commits without passing tests.**

