## 🛠️ COMMIT STRATEGY

**One commit per logical change. No feature commits without passing tests.**

Strict "Clean Room" Constraints:

1. Native Naming Only: Do not use any variable names, struct names, or concepts that sound like they belong to another tool. Invent clean, idiomatic Rust names that fit Valkyrin's existing nomenclature (e.g., use `ValkyrinDependencyGraph`, `CanvasNodeSpan`, etc.).
2. No Trace Comments: Do not add any comments explaining where this logic came from. Document the code purely based on what the mathematical algorithm is doing.
3. Type Adherence: You must strictly use the existing data structures defined in `@ir.rs` (like `CanvasTable` and `CanvasColumn`).
4. Output: Provide the fully rewritten, transaction-safe Rust functions ready to be pasted into the project.
