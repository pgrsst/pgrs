---
name: code-review
description: Use when asked to review Rust code, check code quality, audit a file or module, or evaluate code structure, naming, error handling, or architecture in the pgrs codebase. Triggers: /code-review, "review this code", "any issues in this code", "clean code check".
---

# Code Review — pgrs

Review Rust code across these dimensions. Be direct and prioritize issues by severity.

**When NOT to use:** For non-Rust code or code outside this repo.

## Quick Reference

| Dimension | Key Questions |
|-----------|--------------|
| Structure & Architecture | Correct dependency direction? God structs? |
| Clean Code | Names clear? Functions >30 lines? Magic values? |
| Rust-Specific | Unnecessary `clone()`? `.unwrap()` in prod? |
| Testing | Business logic covered? Descriptive test names? |
| Idiomatic Rust | Iterators over loops? Exhaustive pattern matching? |

---

## 1. Structure & Architecture

- Is the module/crate separation logical and meaningful?
- Is there unnecessary coupling between modules?
- Is the dependency direction correct (inner layers must not depend on outer layers)?
- Are there any god structs/functions that should be split up?

**pgrs rules:**
- Does any file under `core/` import `crate::adapters::**`? This is a hard violation.
- Are new port traits defined in `core/ports/`, not inside adapters?
- Is `app.rs` the only place `FileConnectionRepository` / `PostgresDb` are constructed?
- The `shell` command is intentionally handled in `app.rs` (needs `PostgresDb`) — is a new command doing the same without reason?
- New service structs must follow the `Service<R: SomePort>` generic pattern, not `dyn`.

## 2. Clean Code

- Naming: are variable, function, struct, and module names clear and descriptive?
- Function size: are functions too long (> ~30 lines is suspicious)?
- Are there magic numbers or magic strings that should be constants?
- Is there duplicated logic that could be extracted?
- Do comments explain *why*, not *what*?

**pgrs rules:**
- Validation belongs in `ConnectionService`, not in `Cli`. Is `require_field` called from CLI instead of service?
- Are new fields added to `Connection` without a corresponding `require_field` call in `add_connection`?
- Input structs like `AddConnectionInput` are the boundary between CLI and service — are they used consistently for new commands?

## 3. Rust-Specific

- **Ownership & borrowing**: are there unnecessary `clone()` or `Arc`/`Rc` usages?
- **Error handling**: is the `?` operator used correctly? Are error types appropriate?
- **Option/Result**: are there `.unwrap()` or `.expect()` calls in production code that should be properly handled?
- **Trait design**: are traits focused enough (Interface Segregation Principle)?
- **Lifetimes**: are there lifetime annotations that could be simplified?
- **Performance**: are there unnecessary heap allocations? Iterator chains that could be optimized?

**pgrs rules:**
- Error type in this codebase is plain `String`. Is `anyhow`, `thiserror`, or `Box<dyn Error>` introduced where `Result<T, String>` should be used?
- Is `dyn ConnectionRepository` used where `<R: ConnectionRepository>` should be used?
- File I/O must use the atomic write pattern: write to `.tmp` → `set_permissions(0o600)` → `rename`. Is a new write path bypassing `write_connections()`?
- Mutating operations on `FileConnectionRepository` (`add`, `delete`) must go through `with_lock()`. Read-only operations (`list`, `get_connection`) must NOT lock. Is this respected?

## 4. Testing

- Are there unit tests for business logic?
- Are edge cases covered?
- Are test names descriptive?

**pgrs rules:**
- Stub pattern: tests use hand-written `StubRepository { connections: RefCell<Vec<Connection>> }` — not `mockall` or other mock libraries. Is a new test introducing a mock framework?
- Test naming: flat verb-first names (`add_persists_connection`, `delete_returns_error_when_not_found`) — not `should_X_when_Y`.
- `FileConnectionRepository` tests must use `tempfile::tempdir()`, never real `~/.pgrs`.
- Are factory helpers used (`sample_connection`, `valid_input`, `cli_with`, `repo`, `service`) to keep test bodies short?
- Edge cases to check for any new command: empty name, duplicate entry, missing file, not-found error.

## 5. Idiomatic Rust

- Are iterators used instead of manual loops where appropriate?
- Is pattern matching exhaustive and idiomatic?
- Is `impl Trait` leveraged where appropriate?

**pgrs rules:**
- CLI arg parsing uses `optional_option` / `required_option` helpers — no external arg-parsing library. Are new flags following the `--key=value` prefix pattern?
- Unknown/invalid input must return a user-readable `Err(String)`, never panic.

---

## Output Format

```
## Summary
<1-2 sentence overall assessment>

## Critical Issues 🔴
<must fix before merging>

## Improvements 🟡
<worth doing>

## Minor / Style 🟢
<optional>

## Positives ✅
<what's done well — do not skip>
```

If no argument is provided, ask the user to paste the code or specify the file.
If an argument is provided: `$ARGUMENTS` — use it as the target file or review scope.
