# Rust Patterns in Hexagonal Architecture

## Table of Contents
1. [Newtype Pattern for Value Objects](#1-newtype-pattern-for-value-objects)
2. [Trait-Based Ports](#2-trait-based-ports)
3. [Constructor Injection](#3-constructor-injection)
4. [Error Layering with thiserror](#4-error-layering-with-thiserror)
5. [Generic vs Trait Object Trade-offs](#5-generic-vs-trait-object-trade-offs)
6. [Builder Pattern for Entities](#6-builder-pattern-for-entities)
7. [Conversion Between Layers](#7-conversion-between-layers)
8. [Testing Patterns](#8-testing-patterns)

---

## 1. Newtype Pattern for Value Objects

Wrap primitives to enforce invariants at the type level.

```rust
// domain/model/value_objects.rs

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderId(Uuid);

impl OrderId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(id: Uuid) -> Self { Self(id) }
    pub fn value(&self) -> Uuid { self.0 }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Quantity(u32);

impl Quantity {
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::InvalidQuantity(value));
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> u32 { self.0 }
}
```

---

## 2. Trait-Based Ports

### Outbound Port (Repository)
```rust
// domain/port/outbound/order_repository.rs
use async_trait::async_trait;

#[async_trait]
pub trait OrderRepository: Send + Sync + 'static {
    async fn find_by_id(&self, id: &OrderId) -> Result<Option<Order>, DomainError>;
    async fn find_all_by_customer(&self, customer_id: &CustomerId) -> Result<Vec<Order>, DomainError>;
    async fn save(&self, order: &Order) -> Result<(), DomainError>;
    async fn delete(&self, id: &OrderId) -> Result<(), DomainError>;
}
```

### Inbound Port (Use Case Interface)
```rust
// domain/port/inbound/order_service.rs
#[async_trait]
pub trait CreateOrder: Send + Sync {
    async fn execute(&self, cmd: CreateOrderCommand) -> Result<OrderId, ApplicationError>;
}
```

---

## 3. Constructor Injection

Prefer constructor injection over service locator or global state.

```rust
// application/use_case/create_order.rs

pub struct CreateOrderUseCase<R: OrderRepository, N: NotificationPort> {
    order_repo: R,
    notifier: N,
}

impl<R: OrderRepository, N: NotificationPort> CreateOrderUseCase<R, N> {
    pub fn new(order_repo: R, notifier: N) -> Self {
        Self { order_repo, notifier }
    }
}

#[async_trait]
impl<R: OrderRepository, N: NotificationPort> CreateOrder for CreateOrderUseCase<R, N> {
    async fn execute(&self, cmd: CreateOrderCommand) -> Result<OrderId, ApplicationError> {
        let order = Order::create(cmd.product_id, cmd.quantity)?;
        self.order_repo.save(&order).await?;
        self.notifier.notify_created(&order).await?;
        Ok(order.id().clone())
    }
}
```

### Wiring in main.rs
```rust
// main.rs
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = PgPoolOptions::new().connect(&config.db_url).await?;
    
    let order_repo = PostgresOrderRepository::new(pool.clone());
    let notifier = EmailNotifier::new(config.smtp.clone());
    let create_order_uc = Arc::new(CreateOrderUseCase::new(order_repo, notifier));
    
    let app = Router::new()
        .route("/orders", post(create_order_handler))
        .with_state(AppState { create_order: create_order_uc });
    
    axum::serve(listener, app).await?;
    Ok(())
}
```

---

## 4. Error Layering with thiserror

```rust
// domain/error.rs
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    #[error("order {0} not found")]
    OrderNotFound(OrderId),
    #[error("invalid quantity: {0}")]
    InvalidQuantity(u32),
    #[error("order already cancelled")]
    AlreadyCancelled,
}

// application/error.rs
#[derive(thiserror::Error, Debug)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("repository error")]
    Repository,
    #[error("notification failed")]
    Notification,
}

// infrastructure/error.rs (stays in infra, doesn't leak up)
#[derive(thiserror::Error, Debug)]
pub enum InfraError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

impl From<InfraError> for ApplicationError {
    fn from(e: InfraError) -> Self {
        tracing::error!("infra error: {e}");
        ApplicationError::Repository
    }
}
```

---

## 5. Generic vs Trait Object Trade-offs

| | Generics (`T: Trait`) | Trait Objects (`dyn Trait`) |
|---|---|---|
| Performance | Zero-cost (monomorphized) | Vtable overhead |
| Flexibility | Compile-time only | Runtime polymorphism |
| Compile time | Slower | Faster |
| `Clone` | Works | Requires `Arc` |
| Multiple implementations | One per call site | Many at runtime |

**Use generics** when: use case has one concrete impl per binary, or in library code.
**Use `Arc<dyn>`** when: you need to swap implementations at runtime (feature flags, testing strategies, plugin systems).

```rust
// Generic — preferred for use cases
pub struct CreateOrderUseCase<R: OrderRepository> { repo: R }

// Trait object — preferred for HTTP state shared across handlers
pub struct AppState {
    create_order: Arc<dyn CreateOrder>,
}
```

---

## 6. Builder Pattern for Entities

Use for entities with many optional fields:

```rust
pub struct OrderBuilder {
    product_id: Option<ProductId>,
    customer_id: Option<CustomerId>,
    quantity: Option<Quantity>,
}

impl OrderBuilder {
    pub fn new() -> Self { Self { product_id: None, customer_id: None, quantity: None } }
    pub fn product(mut self, id: ProductId) -> Self { self.product_id = Some(id); self }
    pub fn customer(mut self, id: CustomerId) -> Self { self.customer_id = Some(id); self }
    pub fn quantity(mut self, qty: Quantity) -> Self { self.quantity = Some(qty); self }
    
    pub fn build(self) -> Result<Order, DomainError> {
        Ok(Order {
            id: OrderId::new(),
            product_id: self.product_id.ok_or(DomainError::MissingProduct)?,
            customer_id: self.customer_id.ok_or(DomainError::MissingCustomer)?,
            quantity: self.quantity.ok_or(DomainError::MissingQuantity)?,
            status: OrderStatus::Pending,
        })
    }
}
```

---

## 7. Conversion Between Layers

### DB Row → Domain Entity (in infrastructure adapter)
```rust
// infrastructure/db/order_row.rs

#[derive(sqlx::FromRow)]
struct OrderRow {
    id: Uuid,
    product_id: Uuid,
    customer_id: Uuid,
    quantity: i32,
    status: String,
}

impl TryFrom<OrderRow> for Order {
    type Error = InfraError;
    
    fn try_from(row: OrderRow) -> Result<Self, Self::Error> {
        Ok(Order {
            id: OrderId::from_uuid(row.id),
            product_id: ProductId::from_uuid(row.product_id),
            customer_id: CustomerId::from_uuid(row.customer_id),
            quantity: Quantity::new(row.quantity as u32)
                .map_err(|_| InfraError::InvalidData("quantity"))?,
            status: OrderStatus::try_from(row.status.as_str())
                .map_err(|_| InfraError::InvalidData("status"))?,
        })
    }
}
```

### HTTP Request → Command (in HTTP adapter)
```rust
// infrastructure/http/order_handler.rs

#[derive(Deserialize)]
pub struct CreateOrderRequest {
    pub product_id: String,
    pub quantity: u32,
}

impl TryFrom<CreateOrderRequest> for CreateOrderCommand {
    type Error = ValidationError;
    
    fn try_from(req: CreateOrderRequest) -> Result<Self, Self::Error> {
        Ok(CreateOrderCommand {
            product_id: ProductId::from_str(&req.product_id)
                .map_err(|_| ValidationError::InvalidUuid("product_id"))?,
            quantity: Quantity::new(req.quantity)
                .map_err(|_| ValidationError::InvalidQuantity)?,
        })
    }
}
```

---

## 8. Testing Patterns

### Unit Test — Domain (no mocks needed)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn order_cannot_have_zero_quantity() {
        let result = Quantity::new(0);
        assert!(matches!(result, Err(DomainError::InvalidQuantity(0))));
    }
    
    #[test]
    fn cancelled_order_cannot_be_cancelled_again() {
        let mut order = Order::create(ProductId::new(), Quantity::new(1).unwrap()).unwrap();
        order.cancel().unwrap();
        assert!(matches!(order.cancel(), Err(DomainError::AlreadyCancelled)));
    }
}
```

### Use Case Test — Mock Port
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockOrderRepo {
        saved: Mutex<Vec<Order>>,
    }
    
    #[async_trait]
    impl OrderRepository for MockOrderRepo {
        async fn find_by_id(&self, id: &OrderId) -> Result<Option<Order>, DomainError> {
            let saved = self.saved.lock().unwrap();
            Ok(saved.iter().find(|o| o.id() == id).cloned())
        }
        
        async fn save(&self, order: &Order) -> Result<(), DomainError> {
            self.saved.lock().unwrap().push(order.clone());
            Ok(())
        }
        
        async fn delete(&self, _id: &OrderId) -> Result<(), DomainError> { Ok(()) }
        async fn find_all_by_customer(&self, _: &CustomerId) -> Result<Vec<Order>, DomainError> { Ok(vec![]) }
    }
    
    #[tokio::test]
    async fn creates_order_and_saves_to_repo() {
        let repo = MockOrderRepo { saved: Mutex::new(vec![]) };
        let uc = CreateOrderUseCase::new(repo, MockNotifier);
        
        let cmd = CreateOrderCommand {
            product_id: ProductId::new(),
            quantity: Quantity::new(3).unwrap(),
        };
        
        let order_id = uc.execute(cmd).await.unwrap();
        // verify via repo or returned id
        assert!(!order_id.value().is_nil());
    }
}
```

### Integration Test — Real Adapter
```rust
// tests/order_repository_test.rs
use sqlx::PgPool;
use testcontainers::*;

#[tokio::test]
async fn saves_and_retrieves_order() {
    let container = postgres_container().await;
    let pool = PgPool::connect(&container.connection_string()).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    
    let repo = PostgresOrderRepository::new(pool);
    let order = Order::create(ProductId::new(), Quantity::new(2).unwrap()).unwrap();
    
    repo.save(&order).await.unwrap();
    let found = repo.find_by_id(order.id()).await.unwrap();
    
    assert!(found.is_some());
    assert_eq!(found.unwrap().id(), order.id());
}
```
