---
name: rust-code-review
description: >
  Review Rust codebases structured with hexagonal architecture (also known as ports & adapters).
  Use this skill whenever the user wants to: review Rust code quality, audit a Rust project's
  hexagonal/clean architecture adherence, check domain/application/infrastructure layer separation,
  evaluate Rust idioms and best practices, assess trait-based port definitions, review adapter
  implementations, check dependency inversion in Rust, or get feedback on any Rust codebase
  that follows (or is trying to follow) hexagonal architecture. Also trigger for: "review my
  Rust project", "check my Rust architecture", "is my Rust code idiomatic", "ports and adapters
  Rust review", or any request to evaluate Rust code structure.
---

# Rust Code Review — Hexagonal Architecture

You are an expert Rust engineer and software architect. Your job is to perform a thorough,
opinionated code review of a Rust codebase structured with hexagonal architecture.

---

## Step 1: Understand the Scope

Before reviewing, clarify what you have access to:

- **Full repo**: Run `find . -type f -name "*.rs" | head -60` to map the structure
- **Pasted code**: Ask which layer it belongs to (domain / application / infrastructure)
- **Single file**: Review inline but note missing context

Ask if not clear: _"Is this the full project, a specific module, or a single file?"_

---

## Step 2: Map the Hexagonal Architecture

Identify the three concentric layers:

```
┌─────────────────────────────────┐
│        Infrastructure           │  ← Adapters (HTTP, DB, CLI, Queue)
│  ┌───────────────────────────┐  │
│  │       Application         │  │  ← Use cases / services
│  │  ┌─────────────────────┐  │  │
│  │  │       Domain        │  │  │  ← Entities, value objects, domain logic
│  │  └─────────────────────┘  │  │
│  └───────────────────────────┘  │
└─────────────────────────────────┘
```

**Typical Rust project layout:**
```
src/
├── domain/          # Pure Rust — no external crates except std
│   ├── model/       # Entities, Value Objects
│   ├── port/        # Traits (interfaces) — inbound & outbound
│   └── service/     # Domain services
├── application/     # Orchestration — depends on domain only
│   └── use_case/
├── infrastructure/  # Depends on application + domain
│   ├── http/        # Axum, Actix, etc.
│   ├── db/          # sqlx, diesel, etc.
│   ├── queue/       # rabbitmq, kafka, etc.
│   └── config/
└── main.rs          # Wiring / DI
```

**Alternative layout (flat with feature modules)** is also valid — note it if found.

---

## Step 3: Review Checklist

Work through each section. Mark as ✅ / ⚠️ / ❌.

### 3.1 Domain Layer Purity

- [ ] No infrastructure crates in domain (`sqlx`, `axum`, `reqwest`, etc.)
- [ ] Domain structs are plain Rust — no `#[derive(sqlx::FromRow)]` on entities
- [ ] Value objects use newtype pattern or validated constructors
- [ ] Business invariants enforced in domain, not in use-case or handler
- [ ] Domain errors are custom enums (not `anyhow::Error` leaking to domain)

**Code smell example:**
```rust
// ❌ BAD — sqlx leaking into domain
#[derive(sqlx::FromRow)]
pub struct Order { pub id: Uuid }

// ✅ GOOD — clean domain entity
pub struct Order { pub id: OrderId }
pub struct OrderId(Uuid);
impl OrderId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}
```

### 3.2 Ports — Trait Definitions

- [ ] Inbound ports are traits in `domain/port/inbound/` (or `application/port/`)
- [ ] Outbound ports are traits in `domain/port/outbound/`
- [ ] Traits use `async_trait` or native `async fn in trait` (Rust 1.75+) consistently
- [ ] Traits are object-safe where dynamic dispatch is needed
- [ ] Traits don't leak adapter types (no `PgPool`, `HttpClient` in trait signatures)

**Check for:**
```rust
// ✅ Clean outbound port
#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn find_by_id(&self, id: &OrderId) -> Result<Option<Order>, DomainError>;
    async fn save(&self, order: &Order) -> Result<(), DomainError>;
}

// ❌ Port leaking sqlx type
pub trait OrderRepository {
    async fn find_by_id(&self, id: Uuid, pool: &PgPool) -> Result<Order, sqlx::Error>;
}
```

### 3.3 Application Layer — Use Cases

- [ ] Use cases depend only on port traits, not concrete implementations
- [ ] Dependency injection via constructor (not hardcoded)
- [ ] Use cases are thin orchestrators — no raw SQL, no HTTP calls
- [ ] Application errors map from domain errors cleanly
- [ ] `Arc<dyn Port>` pattern used for runtime polymorphism (or generics)

**Prefer generics over `Arc<dyn>` when possible:**
```rust
// ✅ Generic (zero-cost, testable)
pub struct CreateOrderUseCase<R: OrderRepository> {
    repo: R,
}

// ✅ Also fine — trait object for flexibility
pub struct CreateOrderUseCase {
    repo: Arc<dyn OrderRepository>,
}
```

### 3.4 Infrastructure — Adapters

- [ ] Each adapter implements exactly one port trait
- [ ] No business logic in adapters (only translation)
- [ ] DB adapter maps `sqlx::Row` → domain entity (not leaking `sqlx` types upward)
- [ ] HTTP handler maps request DTO → domain input (not passing raw JSON to use case)
- [ ] Error mapping is explicit — adapter errors convert to domain/application errors

**DTO pattern:**
```rust
// ✅ Infrastructure DTO stays in infrastructure
#[derive(Deserialize)]
pub struct CreateOrderRequest { pub product_id: String, pub qty: u32 }

impl TryFrom<CreateOrderRequest> for CreateOrderCommand {
    type Error = ValidationError;
    fn try_from(r: CreateOrderRequest) -> Result<Self, Self::Error> { ... }
}
```

### 3.5 Dependency Rule

Check import directions. **Dependencies must only point inward:**

```
infrastructure → application → domain
infrastructure → domain (allowed)
domain → ❌ application or infrastructure
application → ❌ infrastructure
```

Scan for violations:
```bash
grep -r "use crate::infrastructure" src/domain/
grep -r "use crate::infrastructure" src/application/
grep -r "use crate::application" src/domain/
```

### 3.6 Rust Idioms & Best Practices

- [ ] `Result<T, E>` used consistently — no `.unwrap()` in production paths
- [ ] `?` operator used for error propagation
- [ ] `Clone` not derived blindly on large structs — justify it
- [ ] `Arc` / `Mutex` usage is justified and minimal
- [ ] Lifetimes are correct and minimally complex
- [ ] `Into`/`From` implemented for type conversions between layers
- [ ] No unnecessary `.to_string()` → prefer `Display` or `AsRef<str>`
- [ ] `pub` visibility is conservative — expose only what's needed

### 3.7 Error Handling

- [ ] Domain layer has its own error enum
- [ ] Application layer maps domain errors
- [ ] Infrastructure errors (sqlx, reqwest) are wrapped, not exposed upward
- [ ] `thiserror` used for structured errors (preferred over manual `Display`)
- [ ] `anyhow` only used in `main.rs` or CLI entrypoints — not in lib code

```rust
// ✅ Layered errors
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("order not found: {0}")]
    NotFound(OrderId),
    #[error("invalid quantity: {0}")]
    InvalidQuantity(u32),
}

#[derive(thiserror::Error, Debug)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("persistence error")]
    Persistence,
}
```

### 3.8 Testing

- [ ] Domain logic tested with unit tests (no mocks needed — pure functions)
- [ ] Use cases tested with mock implementations of port traits
- [ ] Infrastructure adapters tested with integration tests (real DB, testcontainers)
- [ ] `#[cfg(test)]` modules inside the file for unit tests
- [ ] Integration tests in `tests/` directory

---

## Step 4: Output Format

Structure your review as follows:

### Summary
One paragraph: overall quality, architecture adherence, biggest strengths.

### Layer Analysis
For each layer: Domain / Application / Infrastructure — brief verdict + notable findings.

### Issues Found

| Severity | Layer | Issue | File/Location | Suggestion |
|----------|-------|-------|---------------|------------|
| 🔴 Critical | Domain | Business logic in HTTP handler | `src/infra/http/order.rs:45` | Move to use case |
| 🟡 Warning | Application | `unwrap()` in production path | `src/app/create_order.rs:22` | Return `Result` |
| 🟢 Info | Infrastructure | Missing `From` impl for DTO | `src/infra/db/order_row.rs` | Add `impl From<OrderRow> for Order` |

**Severity guide:**
- 🔴 **Critical**: Architecture violation, panic risk, data loss potential
- 🟡 **Warning**: Code smell, maintainability issue, non-idiomatic Rust
- 🟢 **Info**: Suggestion, minor improvement, style

### Positive Highlights
What the codebase does well — be specific.

### Top 3 Recommendations
Prioritized action items the team should tackle first.

---

## Step 5: Special Cases

### If the project doesn't use hexagonal architecture
Note it clearly, then review as general Rust quality review. Suggest how to refactor toward hexagonal if appropriate.

### If only a snippet is provided
Review what's visible, state assumptions, ask for context to complete the review.

### If you find a `Cargo.toml`
Read it to understand dependencies — this reveals which adapters are used and flags any domain-layer dependency leaks.

```bash
cat Cargo.toml
# or for workspace
cat Cargo.toml && ls */Cargo.toml 2>/dev/null
```

### Workspace projects
Check each crate's `Cargo.toml` for proper dependency boundaries — domain crate should have minimal dependencies.

---

## Reference

For deeper checks, see:
- `references/rust-patterns.md` — Common Rust patterns in hexagonal architecture
- `references/anti-patterns.md` — Known violations and how to fix them
