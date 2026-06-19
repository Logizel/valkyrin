## 🛠️ COMMIT STRATEGY

**One commit per logical change. No feature commits without passing tests.**

**PHASE 1**
@valkyrin-core/src/migration.rs
@valkyrin-core/src/error.rs

Role: You are a Senior Rust Systems Engineer.

Objective: Upgrade my current migration execution engine (`@migration.rs`) to implement enterprise-grade drift detection, chained directory hashing (`valkyrin.sum`), and statement-level partial recovery based on the advanced architectural rules provided below.

Currently, Valkyrin tracks migrations using a simple `_valkyrin_migrations` table with a boolean `success` flag wrapped in a DB transaction. As we know, this fails entirely on MySQL due to implicit DDL commits. We need to upgrade standard of deterministic state tracking.

Tasks to Implement:

1. Directory-Wide Hashing: Write a Rust function using `sha2` and `base64` to calculate the sequential, chained directory hash of the `migrations/` folder, ignoring tampered files, and write it to a `valkyrin.sum` file.
2. Pre-Flight Tamper Detection: Update `run_migrations` to pre-flight validate the local `valkyrin.sum` against the database history _before_ acquiring the database lock. It must detect out-of-band deletions, history reordering, and retrospective edits.
3. Partial Execution Checkpoints (The MySQL Fix): Modify the execution logic to parse a `.sql` file into an array of individual statements (split by `;`). Update the execution loop to track `applied_statements` iteratively. If statement 3 fails, the state should safely save that 2 statements succeeded, allowing a rerun to resume exactly at statement 3 without triggering "Table already exists" errors.
4. Schema Upgrade: Provide the SQL necessary to upgrade the `_valkyrin_migrations` table schema to support this new tracking (e.g., adding `applied_statements` and `partial_hashes`).

Strict Constraints:

- Maintain my existing multi-DBMS lock mechanisms (PostgreSQL advisory lock, MySQL GET_LOCK, SQLite file lock).
- Handle all new failure states natively by adding/using specific variants in my custom `ValkyrinError` enum (`@error.rs`). Do not use `.unwrap()` or panic.
- Ensure the code uses highly idiomatic, optimized Rust `Result` bubbling.

The Advanced State-Machine Algorithms to Implement:
Valkyrin Migration Engine: Formal Analysis

1. Directory-Wide Cryptographic Hashing (valkyrin.sum Payload)
   1.1 Input Dataset
   The hash input for each file is a deterministic concatenation of:
   HASH_INPUT(file_i) = BYTES(file_name) || BYTES(file_content)
   Where:

- file_name: raw UTF-8 byte sequence of the filename (e.g. 20250101120000_initial.sql) — no delimiter or separator between name and content
- file_content: raw UTF-8 byte sequence of the entire file body
  Files containing -- valkyrin:sum ignore at the top are excluded from content hashing, but their filename is still consumed by the running hash.
  1.2 Aggregation Pipeline (Per-File Incremental Hash)
  STATE: sha256_hash ← SHA256_INIT()

FOR EACH file_i IN .sql files (sorted lexicographically by filename):
sha256_hash ← SHA256_UPDATE(sha256_hash, BYTES(file_i.Name()))

```
IF NOT file_i has directive "valkyrin:sum ignore":
    sha256_hash ← SHA256_UPDATE(sha256_hash, BYTES(file_i.Bytes()))

file_hash_i = BASE64_ENCODE(sha256_hash)

```

The key property: each file hash is a cumulative SHA-256 digest of all preceding files in order plus the current file. This is NOT a Merkle tree — it is a sequential chained hash.
After processing N files, the state yields the final directory-level hash.
1.3 Layout Specification of valkyrin.sum
Line 1: h1:<DIRECTORY_SUM>
Line 2..N: h1:<FILE_HASH_i>
Where:

- <DIRECTORY_SUM> = BASE64(SHA256( filename_1 || file_hash_1 || filename_2 || file_hash_2 || ... || filename_N || file_hash_N ))
- <FILE_HASH_i> = the cumulative SHA-256 at the point after file_i was processed (as computed in §1.2)
  Example file:
  h1:7P3nY...Q==
  20250101120000_initial.sql h1:abc123...==
  20250102130000_add_users.sql h1:def456...==
  Diagram of the hash pipeline:
  FILE NAMES + CONTENTS (sorted lexicographically)
  │
  ▼
  ┌─────────────────────────────────────────────────┐
  │ FOR each file (in order): │
  │ h ← SHA256(h || filename_bytes) │
  │ h ← SHA256(h || file_bytes) [unless ignored] │
  │ file_hash[i] = BASE64(h) │
  │ PERSIST file_hash[i] with filename │
  └─────────────────────────────────────────────────┘
  │
  ▼ (after all files processed)
  CURRENT h = DIRECTORY_ROOT_HASH
  │
  ▼
  ┌─────────────────────────────────────────────────┐
  │ DIR_HASH = BASE64(SHA256( │
  │ filename_1 || file_hash_1 || │
  │ filename_2 || file_hash_2 || ... │
  │ filename_N || file_hash_N │
  │ )) │
  └─────────────────────────────────────────────────┘
  │
  ▼
  WRITTEN as line 1 of valkyrin.sum
  1.4 Mathematical Invariant
  Let F = sequence of (name_i, content_i) pairs sorted by name_i lexicographically

For i in [1, N]:
h*0 = SHA256_INIT
h_i = SHA256(h*{i-1} || name_i || content_i) [or content_i omitted if ignored]
sum_i = BASE64(h_i)

DIR_SUM = BASE64(SHA256(name_1 || sum_1 || name_2 || sum_2 || ... || name_N || sum_N))

┌──────────────────────────────────────────────────────┐
│ valkyrin.sum invariant: │
│ Unmarshal(Read(valkyrin.sum)).Sum() == DIR_SUM │
│ │
│ This guarantees that valkyrin.sum has not been tampered │
│ with by comparing the self-consistency of its own │
│ entries against its header hash. │
└──────────────────────────────────────────────────────┘ 2. Tamper & Drift Detection Algorithm
2.1 Validation Sequence
STEP 1: READ stored_hash_file = Unmarshal(Read("valkyrin.sum"))

- If file doesn't exist:
  IF any .sql files exist → return ErrChecksumNotFound
  ELSE (empty dir) → VALID (no-op)

STEP 2: COMPUTE expected_hash_file = Checksum(dir.Files())

- Iterates all .sql files, recomputes incremental hashes from scratch

STEP 3: COMPARE stored_hash_file.Sum() vs expected_hash_file.Sum()

- If equal → VALID (exit)
- If NOT equal → enter mismatch diagnosis (STEP 4)
  2.2 Mismatch Diagnosis — Four Cases
  The algorithm walks the stored hash entries sequentially, comparing against the computed entries. It uses a position pointer (pos) to track byte-offset in the valkyrin.sum file for error reporting.
  LET stored = stored_hash_file (from valkyrin.sum on disk)
  LET actual = expected_hash_file (computed from .sql files on disk)

FOR i = 0 TO stored.length - 1:
IF stored[i] == actual[i]:
pos += len(stored[i].name) + 1 + 3 + 44 + 1
CONTINUE

```
// Mismatch found at index i
CASE actual DOES NOT CONTAIN stored[i].name:
    → "REMOVED" (file was deleted)

CASE actual[i].name == stored[i].name BUT hashes differ:
    → "EDITED" (file was modified in place)

CASE actual[i].name != stored[i].name AND stored[i].name exists at a later index:
    → "ADDED" (the file at actual[i].name was inserted before stored[i].name)
    // i.e. a new file was injected into the middle of the sequence

```

IF loop completes without returning (all stored entries matched exactly):
AND stored.Sum() != actual.Sum():
→ "ADDED" at position stored.length (file appended after the last known entry)
2.3 Detection Rules by Attack Vector
Attack Detection Mechanism
a) Retrospective modification of executed file The file's cumulative hash at its position changes → ReasonEdited detected at the modified file's index
b) Reordered / injected file in middle of sequence When stored[i].name ≠ actual[i].name and stored[i].name is found later in actual → ReasonAdded at index i, with the filename set to actual[i].name (the intruder)
c) Out-of-band deletion stored[i].name not found anywhere in actual → ReasonRemoved
2.4 Byte-Level Position Tracking
valkyrin.sum format:
h1:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB==\n ← 1 line (global hash)
filename1 h1:CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC==\n ← N lines

WHERE:
"h1:" prefix = 3 bytes
base64 SHA-256 = 44 bytes
"==\n" = 3 bytes (in global hash line)
filename + " " = len(N) + 1 bytes
"h1:" + hash + \n = 3 + 44 + 1 = 48 bytes

THUS:
pos = 3 + 44 + 1 = 48 bytes // after global hash line (1-indexed: line 1)
For each matching entry i:
pos += len(stored[i].N) + 1 + 48
Line number = i + 2 // first line is global hash 3. Partial Execution Checkpoint Logic (Statement-Level Failures)
3.1 Statement Parsing & Splitting
The SQL scanner implements a state machine that:
INIT:

- Set scanner.input = file content
- Parse `-- valkyrin:delimiter <D>` directive at top (default: ";")
- Set scanner.delim =

LOOP:
WHILE true: 1. Skip leading whitespace 2. If we reach EOS: - If currently inside '(' (depth > 0) → error: unclosed paren - If a character has been consumed → emit collected text - Else → EOF (no more statements) 3. Track parenthesis depth: - '(' → depth++ - ')' → depth-- 4. Skip quoted strings: - ', ", ` → skipQuote (handle escapes) - $tag$ → skipDollarQuote (PG dollar quoting) 5. Handle comments: - -- → skip to \n - /\* _/ → skip to _/ - # → skip to \n (MySQL mode) 6. Handle DELIMITER command (inline MySQL syntax) 7. At depth == 0, detect terminator: - If current delimiter string is matched → split here - If batch terminator command (T-SQL mode) → split here - If BEGIN ATOMIC...END → treat as single statement - If BEGIN TRY...END CATCH → treat as single statement - If BEGIN...END → treat as single statement 8. emit() → returns Stmt{Text, Pos, Comments}, resets state for next iteration
Each migration file is split into an ordered array of Stmt objects:
STMT_ARRAY[1..n] = Scan(file_content)
3.2 Statement-Level Tracking via PartialHashes
For each statement i in the file:
h_sha = SHA256_INIT
h_sha = SHA256_UPDATE(h_sha, BYTES(stmt[i].Text))
sums[i] = BASE64(h_sha)
The sums[i] is a cumulative hash of all statements so far, NOT individual hashes. This is identical in structure to the per-file hash in §1.2 — a sequential chain.
3.3 State Transitions on Execute
STATE MACHINE for a single migration file:

```
                      ┌─────────────────────────┐
                      │  REVISION NOT IN DB      │
                      │  (first execution)       │
                      └──────────┬──────────────┘
                                 │
                      r = Revision{
                        Version, Description,
                        Type = Execute,
                        Total = len(stmts),
                        Hash = file_hash,
                        Applied = 0,
                        PartialHashes = [],
                        Error = "",
                        ErrorStmt = "",
                      }
                      │
                      ├──────────────────────────────────┐
                      │                                  │
                      ▼                                  ▼
          ┌────────────────────┐              ┌────────────────────┐
          │ WriteRevision(r)    │              │ WriteRevision(r)    │
          │ (mark as started)   │              │ (persist from prev) │
          └────────┬───────────┘              └────────┬───────────┘
                   │                                   │
                   ▼                                   ▼
          ┌────────────────────┐              ┌────────────────────┐
          │ Skip? r.Applied==0 │              │ r.Applied > 0      │
          │ → start at stmt[0] │              │ → VERIFY CHECKSUM  │
          └────────┬───────────┘              │   for i in 0..r.   │
                   │                          │   Applied-1:       │
                   ▼                          │     sums[i] MUST   │
          ┌──────────────────┐                │     match Partial-  │
          │ FOR stmt in      │                │     Hashes[i]       │
          │ stmts[r.Applied:]│                │     else →          │
          │   ExecContext()  │                │     HistoryChanged  │
          │   ON FAIL:       │                └────────┬───────────┘
          │     r.Error = err│                         │
          │     r.ErrorStmt  │                         ▼
          │       = stmt.Text│                ┌────────────────────┐
          │     r.Execution- │                │ CONTINUE from      │
          │       Time = now │                │ stmts[r.Applied:]  │
          │     RETURN err   │                └────────┬───────────┘
          │   ON SUCCESS:    │                          │
          │     r.Applied++  │                          │
          │     Partial-     │                          ▼
          │       Hashes[]   │                ┌──────────────────────┐
          │       += sums[i] │                │ FOR each stmt:       │
          │     WriteRevision│                │   ExecContext()      │
          │       (per stmt) │                │   ON FAIL: same path │
          │                  │                │   ON SUCCESS:        │
          └──────────────────┘                │     Applied++        │
                   │                          │     Partial-         │
                   ▼                          │       Hashes += sum  │
          ┌────────────────────┐              │     WriteRevision()  │
          │ AFTER ALL:         │              └──────────────────────┘
          │ Clear Partial-     │                        │
          │   Hashes = nil     │                        ▼
          │ r.done()           │              ┌──────────────────────┐
          │ WriteRevision(r)   │              │ AFTER ALL: same      │
          │ RETURN nil(err)    │              │ (clear partial,      │
          └────────────────────┘              │ done, WriteRevision) │
                                              └──────────────────────┘

```

3.4 Resume Without Duplicate Execution
The exact deterministic resume logic:
IF revision exists in DB for this version:
LET start_index = revision.Applied

```
// Integrity check: verify already-applied statements haven't changed
FOR i = 0 TO revision.Applied - 1:
    IF sums[i] != trim_prefix(revision.PartialHashes[i], "h1:"):
        RETURN HistoryChangedError{File, i+1}

// Resume: execute from start_index onward
FOR stmt IN stmts[start_index..]:
    ExecContext(stmt)
    IF failure:
        revision.Error = err.Error()
        revision.ErrorStmt = stmt.Text
        revision.ExecutionTime = now - started_at
        RETURN StmtExecError{File, Stmt, Version, Err}
    ELSE:
        revision.Applied++
        revision.PartialHashes.append("h1:" + sums[Applied-1])
        WriteRevision(revision)   // ← PERSISTED AFTER EVERY STATEMENT

// All statements completed successfully
revision.PartialHashes = nil      // ← signal: fully applied
revision.ExecutionTime = now - started_at

```

ELSE (no revision yet):
Create new Revision{Applied=0, Total=len(stmts)}
Execute from stmt[0] with same per-statement checkpointing
The Applied counter and PartialHashes array together form an append-only checkpoint log. On resume:

- Applied = number of successfully completed statements (resume index)
- PartialHashes = cryptographic proof that each completed statement has not changed
- Error → cleared on successful retry (line 882-885 of migrate.go: if r.Error != "" { r.Error = ""; r.ErrorStmt = "" })
- PartialHashes = nil → definitive "fully applied" terminal state
  3.5 Execution Order Enforcement (History Non-Linearity)
  LET applied_revisions = read from database revision table (sorted by version)
  LET pending_files = migrations not yet fully applied

IF len(applied_revisions) == 0:
// First run: check clean DB, handle baseline/allow-dirty
ELSE:
LET last_applied = applied_revisions[-1]

```
IF last_applied.Applied != last_applied.Total:
    // Partially applied: locate file, resume from checkpoint
ELSE:
    // Find all files with version < last_applied.Version
    // Check if any were NOT in applied_revisions:
    FOR file IN files_between(first_applied, last_applied):
        IF file.Version NOT IN applied_revisions:
            SKIPPED.append(file)

    IF len(skipped) > 0:
        IF ExecOrder == Linear:
            RETURN HistoryNonLinearError{skipped, pending}
        IF ExecOrder == LinearSkip:
            // Silently skip the out-of-order files
        IF ExecOrder == NonLinear:
            // Prepend skipped to pending (executed in version order)

```

**PHASE 2**

@valkyrin-core/src/migration.rs
@valkyrin-core/src/error.rs

Role: You are a Senior Rust Systems Engineer.

Objective: Upgrade my current migration execution engine (`@migration.rs`) to implement enterprise-grade drift detection, chained directory hashing (`valkyrin.sum`), and statement-level partial recovery based on the advanced architectural rules provided below.

Currently, Valkyrin tracks migrations using a simple `_valkyrin_migrations` table with a boolean `success` flag wrapped in a DB transaction. As we know, this fails entirely on MySQL due to implicit DDL commits. We need to upgrade to the standard of deterministic state tracking.

Tasks to Implement:

1. Directory-Wide Hashing: Write a Rust function using `sha2` and `base64` to calculate the sequential, chained directory hash of the `migrations/` folder, ignoring tampered files, and write it to a `valkyrin.sum` file.
2. Pre-Flight Tamper Detection: Update `run_migrations` to pre-flight validate the local `valkyrin.sum` against the database history _before_ acquiring the database lock. It must detect out-of-band deletions, history reordering, and retrospective edits.
3. Partial Execution Checkpoints (The MySQL Fix): Modify the execution logic to parse a `.sql` file into an array of individual statements (split by `;`). Update the execution loop to track `applied_statements` iteratively. If statement 3 fails, the state should safely save that 2 statements succeeded, allowing a rerun to resume exactly at statement 3 without triggering "Table already exists" errors.
4. Schema Upgrade: Provide the SQL necessary to upgrade the `_valkyrin_migrations` table schema to support this new tracking (e.g., adding `applied_statements` and `partial_hashes`).

Strict Constraints:

- Maintain my existing multi-DBMS lock mechanisms (PostgreSQL advisory lock, MySQL GET_LOCK, SQLite file lock).
- Handle all new failure states natively by adding/using specific variants in my custom `ValkyrinError` enum (`@error.rs`). Do not use `.unwrap()` or panic.
- Ensure the code uses highly idiomatic, optimized Rust `Result` bubbling.

The Advanced State-Machine Algorithms to Implement:

1. Resource Span Lifecycle Tracking (ResourceSpan Bitmask)
   Every schema object tracked by the analyzer has a span — a tiny state machine encoded as a 2-bit bitmask:
   Value Name Meaning
   0 SpanUnknown Resource existed before this file; no ADD or DROP for it in the current file's changes.
   1 SpanAdded Resource was created in this file (explicit AddSchema, AddTable, AddColumn).
   2 SpanDropped Resource is being dropped in this file.
   3 SpanTemporary Resource was both created AND dropped in the same file.
   Span Computation Algorithm
   The spans are computed via a single linear pass over the file's change list. The algorithm is:
   FOR each ChangeGroup in File.Changes:
   FOR each Change in ChangeGroup:
   IF Change is AddSchema(target) → set SchemaSpan(target).state = SpanAdded
   IF Change is DropSchema(target) → set SchemaSpan(target).state |= SpanDropped

```
    IF Change is AddTable(target)      → set TableSpan(target).state = SpanAdded
                                        set every Column/Index/FK in target = SpanAdded
    IF Change is DropTable(target)     → set TableSpan(target).state |= SpanDropped

    IF Change is ModifyTable(target):
        FOR each sub-Change:
            IF AddColumn(col)          → set ColumnSpan(target, col) = SpanAdded
            IF DropColumn(col)         → set ColumnSpan(target, col) |= SpanDropped
            IF AddIndex(idx)          → set IndexSpan(target, idx) = SpanAdded
            IF DropIndex(idx)         → set IndexSpan(target, idx) |= SpanDropped
            IF AddForeignKey(fk)      → set ForeignKeySpan(target, fk) = SpanAdded
            IF DropForeignKey(fk)     → set ForeignKeySpan(target, fk) |= SpanDropped

```

Key mathematical property: The |= SpanDropped operator is additive — it sets the DROP bit without clearing the ADDED bit. A resource that first gets = SpanAdded (from Add*) and later |= SpanDropped (from Drop*) ends at value 3 = SpanTemporary.
Logical Proof of No Data Loss
A resource is proven safe-to-drop when:
SafeToDrop(Span) ⟺ Span == SpanTemporary
Because: if a resource was both created AND destroyed within the same file, it holds no data that existed before the migration. Its entire lifetime is contained in this single file. 2. Change-Traversal Decision Tree
The Analyze() function evaluates three DROP operations via a top-down type switch. Here is the exact decision tree for each:
2a. DropSchema
DROP SCHEMA S encountered
├─ IF SchemaSpan(S) == SpanTemporary:
│ └─ ALLOW (no diagnostic) — schema created & dropped in same file, no data loss
└─ ELSE:
└─ REPORT DS101 with text depending on len(S.Tables):
├─ 0 tables: "Dropping schema "
├─ 1 table: "Dropping non-empty schema with 1 table"
└─ >1 tables: "Dropping non-empty schema with N tables"
2b. DropTable
DROP TABLE T encountered
├─ Guard condition — suppress diagnostic IF ANY of:
│ (A) SchemaSpan(T.Schema) == SpanDropped → parent schema is being dropped
│ (B) TableSpan(T) == SpanTemporary → table created & dropped in this file
│ (C) hasEmptyTableCheck(pass, T) == true → pre-flight SELECT COUNT(\*) exists (unimplemented stub)
│
├─ (A) TRUE → ALLOW, no diagnostic. Parent schema absorbs the drop.
├─ (B) TRUE → ALLOW, no diagnostic. Table is temporary.
├─ (C) TRUE → ALLOW, no diagnostic. (Currently always FALSE — future work)
│
└─ ALL FALSE:
└─ REPORT DS102 + suggest adding pre-migration empty-check
2c. DropColumn (inside ModifyTable)
MODIFY TABLE T with sub-changes encountered
├─ FOR each sub-change C:
│ ├─ IF C is NOT a DropColumn → skip
│ ├─ IF ColumnSpan(T, C.Column) == SpanTemporary → skip (column added & dropped here)
│ ├─ IF C.Column is a VIRTUAL generated column (GeneratedExpr.Type == "VIRTUAL"):
│ │ └─ skip — virtual columns hold no storage, no data loss
│ └─ ELSE IF hasEmptyColumnCheck(pass, T, C.Column) == false (stub):
│ └─ add C.Column.Name to names[] for reporting
│
└─ After loop:
├─ IF len(names) == 0 → no diagnostic
├─ IF len(names) == 1 → REPORT DS103 "Dropping non-virtual column "
└─ IF len(names) > 1 → REPORT DS103 "Dropping non-virtual columns , " 3. The "Added vs Dropped" Graph Dependency (Lifecycle Tracking)
This is the most elegant part of the proof system. The analyzer does not need a full dataflow graph or dependency resolution. Instead, it relies on the property that changes within a single File are evaluated as a set, and spans are pre-computed before any diagnostic decisions.
How it works: 4. Pre-pass: loadSpans() does a single pass over all Changes in the file, building the span bitmask for every schema, table, column, index, and FK mentioned. 5. Decision pass: The Analyze() method then inspects each change again, consulting the pre-computed spans. 6. Because the span is built additively (ADD sets = SpanAdded, DROP sets |= SpanDropped), the order of changes within the file does NOT matter for the temporary check. A table that appears as CREATE TABLE x (...) at the top of the file and DROP TABLE x at the bottom will correctly get SpanTemporary, regardless of which change appears first in the iteration.
Example decision matrix:
Scenario SchemaSpan TableSpan ColumnSpan Result
Drop table that existed before migration Unknown Dropped — DS102 reported
Create + Drop same table in one file Unknown Temporary — Silent (safe)
Drop schema containing existing tables Dropped (any) — DS101 reported (schema-level)
Drop schema, and also drop its tables Dropped Dropped — DS101 only (SchemaSpan guard on DropTable suppresses DS102)
Drop column that was added in this file — — Temporary Silent (safe)
Drop existing column (stored) — — Dropped DS103 reported
Drop virtual generated column — — Dropped Silent (no storage = no data loss)
Drop column + has hasEmptyColumnCheck = true — — — Silent (future feature) 7. The Two Stub Gates (Unimplemented)
The analyzer defines two guard functions that are currently stubbed to return false:

- hasEmptyTableCheck(pass, table) — Would scan the file's SQL statements for a SELECT COUNT(\*) FROM table or IF EXISTS (SELECT ...) pre-check before the DROP. If found, it proves the table is empty and the drop is safe.
- hasEmptyColumnCheck(pass, table, column) — Same but for a column: looks for a pre-migration check that the column is all-NULL (e.g., SELECT 1 FROM table WHERE column IS NOT NULL LIMIT 1). If the check exists and passes before the DROP, no data is lost.
  These represent the second layer of safe-drop proof: runtime verification via pre-migration SELECT statements, as opposed to the compile-time proof from the span/temporary analysis.

5. Summary: Complete Boolean Terms for Suppressing a Diagnostic
   SUPPRESS_DROP_SCHEMA(S) ⟸ SchemaSpan(S) == SpanTemporary

SUPPRESS_DROP_TABLE(T) ⟸ SchemaSpan(T.Schema) == SpanDropped
OR TableSpan(T) == SpanTemporary
OR hasEmptyTableCheck(pass, T)

SUPPRESS_DROP_COLUMN(T, C) ⟸ ColumnSpan(T, C) == SpanTemporary
OR (C has GeneratedExpr && Type == "VIRTUAL")
OR hasEmptyColumnCheck(pass, T, C)
Where SpanTemporary is the mathematical conjunction:
SpanTemporary(Resource) ⟸ Resource was Added in this file
AND Resource was Dropped in this file
Which is computed as the bitmask SpanAdded | SpanDropped == 3, built incrementally via:

- ADD operations: span = SpanAdded (assignment)
- DROP operations: span |= SpanDropped (bitwise OR)

**PHASE 3**

@valkyrin/valkyrin-core/src/sync.rs
@valkyrin/valkyrin-core/src/ir.rs
@valkyrin-core/src/error.rs

Role: You are a Senior Rust Systems Engineer and Database Architect.

Objective: Upgrade Valkyrin’s schema diffing and migration engine (`@sync.rs`) to implement an intelligent, predictive data-loss prevention system based on the architectural rules provided below.

Currently, Valkyrin relies on a blanket `--confirm` flag for all drops. We need to implement the 2-bit `ResourceSpan` logic to silently allow mathematically "safe" drops, and strictly warn on destructive drops. Furthermore, because Valkyrin has a live `sqlx` connection pool during sync, we will implement the actual live data check that was stubbed out.

Tasks to Implement:

1. The Lifecycle Span Tracker: Implement the 2-bit bitmask logic (`SpanUnknown`, `SpanAdded`, `SpanDropped`, `SpanTemporary`). Analyze the calculated diffs to see if a Table or Column was created and dropped in the same pass (safe).
2. The Live DB Empty Check: If a Drop is NOT `SpanTemporary`, write async `sqlx` queries to dynamically check the live database for data:

- For `DropTable`: Execute `SELECT 1 FROM "table" LIMIT 1`. If it returns no rows, it is safe to drop without `--confirm`.
- For `DropColumn`: Execute `SELECT 1 FROM "table" WHERE "column" IS NOT NULL LIMIT 1`. If no rows, it is safe to drop.

3. Integration: Update the execution/diff flow. If a drop is identified as destructive (contains live data and isn't temporary), it must bubble up a specific `ValkyrinError::DestructiveChange` (add this to `@error.rs`) containing the exact names of the tables/columns that will lose data, demanding the `--confirm` flag from the CLI.

Strict Constraints:

- Maintain all existing transaction logic.
- Use only the types defined in `@ir.rs`.
- Do not use `.unwrap()`; gracefully bubble all `sqlx` errors into `ValkyrinError`.

The Destructive Analysis Algorithms to Implement:
Here is the full architectural analysis of the destructive change detection engine.

1. Resource Span Lifecycle Tracking (ResourceSpan Bitmask)
   Every schema object tracked by the analyzer has a span — a tiny state machine encoded as a 2-bit bitmask:
   Value Name Meaning
   0 SpanUnknown Resource existed before this file; no ADD or DROP for it in the current file's changes.
   1 SpanAdded Resource was created in this file (explicit AddSchema, AddTable, AddColumn).
   2 SpanDropped Resource is being dropped in this file.
   3 SpanTemporary Resource was both created AND dropped in the same file.
   Span Computation Algorithm
   The spans are computed via a single linear pass over the file's change list. The algorithm is:
   FOR each ChangeGroup in File.Changes:
   FOR each Change in ChangeGroup:
   IF Change is AddSchema(target) → set SchemaSpan(target).state = SpanAdded
   IF Change is DropSchema(target) → set SchemaSpan(target).state |= SpanDropped

```
    IF Change is AddTable(target)      → set TableSpan(target).state = SpanAdded
                                        set every Column/Index/FK in target = SpanAdded
    IF Change is DropTable(target)     → set TableSpan(target).state |= SpanDropped

    IF Change is ModifyTable(target):
        FOR each sub-Change:
            IF AddColumn(col)          → set ColumnSpan(target, col) = SpanAdded
            IF DropColumn(col)         → set ColumnSpan(target, col) |= SpanDropped
            IF AddIndex(idx)          → set IndexSpan(target, idx) = SpanAdded
            IF DropIndex(idx)         → set IndexSpan(target, idx) |= SpanDropped
            IF AddForeignKey(fk)      → set ForeignKeySpan(target, fk) = SpanAdded
            IF DropForeignKey(fk)     → set ForeignKeySpan(target, fk) |= SpanDropped

```

Key mathematical property: The |= SpanDropped operator is additive — it sets the DROP bit without clearing the ADDED bit. A resource that first gets = SpanAdded (from Add*) and later |= SpanDropped (from Drop*) ends at value 3 = SpanTemporary.
Logical Proof of No Data Loss
A resource is proven safe-to-drop when:
SafeToDrop(Span) ⟺ Span == SpanTemporary
Because: if a resource was both created AND destroyed within the same file, it holds no data that existed before the migration. Its entire lifetime is contained in this single file. 2. Change-Traversal Decision Tree
The Analyze() function evaluates three DROP operations via a top-down type switch. Here is the exact decision tree for each:
2a. DropSchema
DROP SCHEMA S encountered
├─ IF SchemaSpan(S) == SpanTemporary:
│ └─ ALLOW (no diagnostic) — schema created & dropped in same file, no data loss
└─ ELSE:
└─ REPORT DS101 with text depending on len(S.Tables):
├─ 0 tables: "Dropping schema "
├─ 1 table: "Dropping non-empty schema with 1 table"
└─ >1 tables: "Dropping non-empty schema with N tables"
2b. DropTable
DROP TABLE T encountered
├─ Guard condition — suppress diagnostic IF ANY of:
│ (A) SchemaSpan(T.Schema) == SpanDropped → parent schema is being dropped
│ (B) TableSpan(T) == SpanTemporary → table created & dropped in this file
│ (C) hasEmptyTableCheck(pass, T) == true → pre-flight SELECT COUNT(\*) exists (unimplemented stub)
│
├─ (A) TRUE → ALLOW, no diagnostic. Parent schema absorbs the drop.
├─ (B) TRUE → ALLOW, no diagnostic. Table is temporary.
├─ (C) TRUE → ALLOW, no diagnostic. (Currently always FALSE — future work)
│
└─ ALL FALSE:
└─ REPORT DS102 + suggest adding pre-migration empty-check
2c. DropColumn (inside ModifyTable)
MODIFY TABLE T with sub-changes encountered
├─ FOR each sub-change C:
│ ├─ IF C is NOT a DropColumn → skip
│ ├─ IF ColumnSpan(T, C.Column) == SpanTemporary → skip (column added & dropped here)
│ ├─ IF C.Column is a VIRTUAL generated column (GeneratedExpr.Type == "VIRTUAL"):
│ │ └─ skip — virtual columns hold no storage, no data loss
│ └─ ELSE IF hasEmptyColumnCheck(pass, T, C.Column) == false (stub):
│ └─ add C.Column.Name to names[] for reporting
│
└─ After loop:
├─ IF len(names) == 0 → no diagnostic
├─ IF len(names) == 1 → REPORT DS103 "Dropping non-virtual column "
└─ IF len(names) > 1 → REPORT DS103 "Dropping non-virtual columns , " 3. The "Added vs Dropped" Graph Dependency (Lifecycle Tracking)
This is the most elegant part of the proof system. The analyzer does not need a full dataflow graph or dependency resolution. Instead, it relies on the property that changes within a single File are evaluated as a set, and spans are pre-computed before any diagnostic decisions.
How it works: 4. Pre-pass: loadSpans() does a single pass over all Changes in the file, building the span bitmask for every schema, table, column, index, and FK mentioned. 5. Decision pass: The Analyze() method then inspects each change again, consulting the pre-computed spans. 6. Because the span is built additively (ADD sets = SpanAdded, DROP sets |= SpanDropped), the order of changes within the file does NOT matter for the temporary check. A table that appears as CREATE TABLE x (...) at the top of the file and DROP TABLE x at the bottom will correctly get SpanTemporary, regardless of which change appears first in the iteration.
Example decision matrix:
Scenario SchemaSpan TableSpan ColumnSpan Result
Drop table that existed before migration Unknown Dropped — DS102 reported
Create + Drop same table in one file Unknown Temporary — Silent (safe)
Drop schema containing existing tables Dropped (any) — DS101 reported (schema-level)
Drop schema, and also drop its tables Dropped Dropped — DS101 only (SchemaSpan guard on DropTable suppresses DS102)
Drop column that was added in this file — — Temporary Silent (safe)
Drop existing column (stored) — — Dropped DS103 reported
Drop virtual generated column — — Dropped Silent (no storage = no data loss)
Drop column + has hasEmptyColumnCheck = true — — — Silent (future feature) 7. The Two Stub Gates (Unimplemented)
The analyzer defines two guard functions that are currently stubbed to return false:

- hasEmptyTableCheck(pass, table) — Would scan the file's SQL statements for a SELECT COUNT(\*) FROM table or IF EXISTS (SELECT ...) pre-check before the DROP. If found, it proves the table is empty and the drop is safe.
- hasEmptyColumnCheck(pass, table, column) — Same but for a column: looks for a pre-migration check that the column is all-NULL (e.g., SELECT 1 FROM table WHERE column IS NOT NULL LIMIT 1). If the check exists and passes before the DROP, no data is lost.
  These represent the second layer of safe-drop proof: runtime verification via pre-migration SELECT statements, as opposed to the compile-time proof from the span/temporary analysis.

5. Summary: Complete Boolean Terms for Suppressing a Diagnostic
   SUPPRESS_DROP_SCHEMA(S) ⟸ SchemaSpan(S) == SpanTemporary

SUPPRESS_DROP_TABLE(T) ⟸ SchemaSpan(T.Schema) == SpanDropped
OR TableSpan(T) == SpanTemporary
OR hasEmptyTableCheck(pass, T)

SUPPRESS_DROP_COLUMN(T, C) ⟸ ColumnSpan(T, C) == SpanTemporary
OR (C has GeneratedExpr && Type == "VIRTUAL")
OR hasEmptyColumnCheck(pass, T, C)
Where SpanTemporary is the mathematical conjunction:
SpanTemporary(Resource) ⟸ Resource was Added in this file
AND Resource was Dropped in this file
Which is computed as the bitmask SpanAdded | SpanDropped == 3, built incrementally via:

- ADD operations: span = SpanAdded (assignment)
- DROP operations: span |= SpanDropped (bitwise OR)
