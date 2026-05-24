# Anti-Patterns in Rust Hexagonal Architecture

## Table of Contents
1. [Domain Layer Violations](#1-domain-layer-violations)
2. [Leaky Ports](#2-leaky-ports)
3. [Fat Adapters with Business Logic](#3-fat-adapters-with-business-logic)
4. [Error Mishandling](#4-error-mishandling)
5. [Dependency Direction Violations](#5-dependency-direction-violations)
6. [Rust-Specific Bad Practices](#6-rust-specific-bad-practices)

---

## 1. Domain Layer Violations

### ❌ Infrastructure types in domain

```rust
// BAD — sqlx leaks into domain entity
use sqlx::FromRow;

#[derive(FromRow, Debug)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub price: f64, // f64 for money is also wrong
}
```

**Fix:**
```rust
// GOOD — clean domain entity with value objects
pub struct Product {
    pub id: ProductId,
    pub name: ProductName,
    pub price: Money,
}

// Separate DB row in infrastructure
#[derive(sqlx::FromRow)]
struct ProductRow { id: Uuid, name: String, price_cents: i64 }
```

---

### ❌ `f64` for money

```rust
// BAD — floating point for monetary value
pub struct Order {
    pub total: f64,
}
```

**Fix:**
```rust
// GOOD — use integer cents or a Money type
pub struct Money {
    amount_cents: i64,
    currency: Currency,
}
```

---

### ❌ Business logic in constructor only (no invariant enforcement)

```rust
// BAD — anyone can construct invalid state
pub struct Email(pub String);
```

**Fix:**
```rust
// GOOD — validated constructor
pub struct Email(String);
impl Email {
    pub fn new(raw: &str) -> Result<Self, DomainError> {
        if !raw.contains('@') {
            return Err(DomainError::InvalidEmail);
        }
        Ok(Self(raw.to_lowercase()))
    }
    pub fn value(&self) -> &str { &self.0 }
}
```

---

## 2. Leaky Ports

### ❌ Port returns infrastructure type

```rust
// BAD — sqlx type in port signature
#[async_trait]
pub trait UserRepository {
    async fn find(&self, id: Uuid) -> Result<sqlx::postgres::PgRow, sqlx::Error>;
}
```

**Fix:**
```rust
// GOOD — port returns domain type
#[async_trait]
pub trait UserRepository {
    async fn find(&self, id: &UserId) -> Result<Option<User>, DomainError>;
}
```

---

### ❌ Port takes pool/connection as parameter

```rust
// BAD — adapter detail leaks into port
#[async_trait]
pub trait OrderRepository {
    async fn save(&self, order: Order, pool: &PgPool) -> Result<(), sqlx::Error>;
}
```

**Fix:**
```rust
// GOOD — pool is an implementation detail, hidden in the adapter
#[async_trait]
pub trait OrderRepository {
    async fn save(&self, order: &Order) -> Result<(), DomainError>;
}

// Adapter holds the pool internally
pub struct PostgresOrderRepository { pool: PgPool }
```

---

## 3. Fat Adapters with Business Logic

### ❌ Validation and business rules in HTTP handler

```rust
// BAD — handler doing domain work
async fn create_order(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Business logic in handler — WRONG
    if req["quantity"].as_u64().unwrap_or(0) == 0 {
        return (StatusCode::BAD_REQUEST, "quantity must be > 0").into_response();
    }
    if req["product_id"].as_str().is_none() {
        return (StatusCode::BAD_REQUEST, "product_id required").into_response();
    }
    
    // Direct DB call bypassing use case — WRONG
    let result = sqlx::query!("INSERT INTO orders...")
        .execute(&state.pool)
        .await;
    ...
}
```

**Fix:**
```rust
// GOOD — handler only: parse → call use case → map response
async fn create_order(
    State(state): State<AppState>,
    Json(req): Json<CreateOrderRequest>,
) -> impl IntoResponse {
    let cmd = CreateOrderCommand::try_from(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()).into_response())?;
    
    match state.create_order.execute(cmd).await {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "id": id }))).into_response(),
        Err(ApplicationError::Domain(DomainError::InvalidQuantity(_))) =>
            (StatusCode::BAD_REQUEST, "Invalid quantity").into_response(),
        Err(_) =>
            StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
```

---

### ❌ DB adapter applying business rules

```rust
// BAD — business rule in repository adapter
impl PostgresOrderRepository {
    pub async fn save(&self, order: &Order) -> Result<(), ApplicationError> {
        // Business rule belongs in domain, not repository
        if order.quantity().value() > 1000 {
            return Err(ApplicationError::Domain(DomainError::QuantityTooLarge));
        }
        sqlx::query!(...).execute(&self.pool).await?;
        Ok(())
    }
}
```

**Fix:** Move the invariant check into the `Order::create()` domain constructor or a domain service. Repository just persists.

---

## 4. Error Mishandling

### ❌ `unwrap()` / `expect()` in production paths

```rust
// BAD — panic at runtime
pub async fn get_order(&self, id: &OrderId) -> Order {
    self.repo.find_by_id(id).await.unwrap()
}
```

**Fix:**
```rust
pub async fn get_order(&self, id: &OrderId) -> Result<Order, ApplicationError> {
    self.repo.find_by_id(id).await?
        .ok_or(ApplicationError::Domain(DomainError::OrderNotFound(id.clone())))
}
```

---

### ❌ `anyhow::Error` in library / domain code

```rust
// BAD — opaque error type in domain
pub fn validate_email(raw: &str) -> Result<Email, anyhow::Error> {
    anyhow::ensure!(raw.contains('@'), "invalid email");
    Ok(Email(raw.to_string()))
}
```

**Fix:** Use `anyhow` only in binary entrypoints (`main.rs`). Domain and application layers use typed error enums.

---

### ❌ Swallowing errors silently

```rust
// BAD — error disappears
if let Err(_) = self.notifier.send(&event).await {
    // silently ignored
}
```

**Fix:**
```rust
// GOOD — at minimum, log it; or propagate
if let Err(e) = self.notifier.send(&event).await {
    tracing::error!("notification failed: {e}");
    // propagate if critical, or handle explicitly
}
```

---

## 5. Dependency Direction Violations

### ❌ Domain importing from application or infrastructure

```rust
// domain/service/order_service.rs — BAD
use crate::infrastructure::db::PostgresOrderRepository; // ❌
use crate::application::CreateOrderUseCase; // ❌
```

---

### ❌ Application importing from infrastructure

```rust
// application/use_case/create_order.rs — BAD
use crate::infrastructure::db::PostgresOrderRepository; // ❌

pub struct CreateOrderUseCase {
    repo: PostgresOrderRepository, // hardcoded concrete type
}
```

**Fix:** Depend on the port trait, not the concrete adapter.

```rust
pub struct CreateOrderUseCase<R: OrderRepository> {
    repo: R, // ✅ any implementation
}
```

---

## 6. Rust-Specific Bad Practices

### ❌ Cloning Arc unnecessarily

```rust
// BAD — clone inside hot loop
for item in items {
    let repo = Arc::clone(&self.repo);
    let result = repo.find_by_id(&item.id).await; // fine, but cloning can be avoided
}
```

**Fix:** Pass `&self.repo` or `Arc::clone` only when moving across thread boundaries.

---

### ❌ Using `String` everywhere instead of `&str` in interfaces

```rust
// BAD — unnecessary allocation
pub fn find_by_email(&self, email: String) -> Option<User>;
```

**Fix:**
```rust
// GOOD — borrow when you don't need ownership
pub fn find_by_email(&self, email: &str) -> Option<User>;
```

---

### ❌ Shared mutable state with `Mutex<HashMap>` everywhere

```rust
// Overuse of interior mutability
pub struct InMemoryRepo {
    data: Mutex<HashMap<Uuid, Order>>,
}
```

This is fine for tests, but flag it in production code. Prefer message-passing or proper DB.

---

### ❌ Ignoring `Send + Sync` bounds on async trait

```rust
// BAD — will fail to compile when used across await points in Tokio
#[async_trait]
pub trait OrderRepository {
    async fn save(&self, order: Order) -> Result<()>;
    // Missing: Send + Sync bounds — won't work with Arc<dyn OrderRepository>
}
```

**Fix:**
```rust
#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn save(&self, order: &Order) -> Result<(), DomainError>;
}
```

---

### ❌ Overusing `Arc<Mutex<T>>` when `Arc<T>` suffices

If a type only needs shared access (read-only or internally manages its state), `Arc<T>` is enough. `Arc<Mutex<T>>` is only needed for exclusive mutable access.

```rust
// Often seen — unnecessary mutex
let repo: Arc<Mutex<PostgresOrderRepository>> = Arc::new(Mutex::new(repo));

// PostgresOrderRepository holds a PgPool which is already Arc<...> internally
// GOOD — no mutex needed
let repo: Arc<PostgresOrderRepository> = Arc::new(repo);
```
