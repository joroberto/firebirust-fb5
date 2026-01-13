# firebirust-fb5

Firebird database driver for Rust with support for Firebird 3.0 to 5.0.

This is a fork of [firebirust](https://github.com/nakagami/firebirust) with additional features for Firebird 4/5 and performance improvements.

## Features

- Wire protocol versions 13-17 (Firebird 3.0 to 5.0)
- Firebird 4+ types: INT128, TIMESTAMP_TZ, TIME_TZ, DEC64, DEC128
- Wire compression (zlib)
- POST_EVENT notifications
- Connection pooling
- Transaction isolation levels including ReadConsistency (Firebird 4+)
- Async/await support

## Supported Firebird Versions

- Firebird 3.0+
- Firebird 4.0+ (full type support)
- Firebird 5.0 (tested)

## Installation

Add to your `Cargo.toml`:
```toml
[dependencies]
firebirust = { git = "https://github.com/joroberto/firebirust-fb5" }
```

## Basic Usage

### Connection

```rust
use firebirust::Connection;

let mut conn = Connection::connect(
    "firebird://SYSDBA:masterkey@localhost/path/to/database.fdb"
).unwrap();
```

### Connection with Options

```rust
// With wire compression
let mut conn = Connection::connect(
    "firebird://SYSDBA:masterkey@localhost/database.fdb?compress=true"
).unwrap();

// With specific auth plugin
let mut conn = Connection::connect(
    "firebird://SYSDBA:masterkey@localhost/database.fdb?auth_plugin_name=Srp256"
).unwrap();
```

### Execute SQL

```rust
conn.execute_batch(
    r#"
    CREATE TABLE users (
        id INTEGER NOT NULL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        email VARCHAR(255),
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )
    "#,
).unwrap();
```

### Execute with Parameters

```rust
conn.execute(
    "INSERT INTO users (id, name, email) VALUES (?, ?, ?)",
    (1, "John Doe", "john@example.com"),
).unwrap();

conn.commit().unwrap();
```

### Query and Fetch Results

```rust
let mut stmt = conn.prepare("SELECT * FROM users WHERE id = ?").unwrap();
for row in stmt.query((1,)).unwrap() {
    let id: i32 = row.get(0).unwrap();
    let name: String = row.get(1).unwrap();
    println!("User {}: {}", id, name);
}
```

### Query Map

```rust
#[derive(Debug)]
struct User {
    id: i32,
    name: String,
    email: Option<String>,
}

let mut stmt = conn.prepare("SELECT id, name, email FROM users").unwrap();
let users = stmt.query_map((), |row| {
    Ok(User {
        id: row.get(0).unwrap(),
        name: row.get(1).unwrap(),
        email: row.get(2).unwrap(),
    })
}).unwrap();

for user in users {
    println!("{:?}", user.unwrap());
}
```

## Transaction Behavior

### Autocommit Mode (Default)

By default, each INSERT/UPDATE/DELETE statement is automatically committed:

```rust
let mut conn = Connection::connect("firebird://...").unwrap();

// Each statement commits automatically
conn.execute("INSERT INTO users (id, name) VALUES (?, ?)", (1, "John")).unwrap();
conn.execute("UPDATE users SET name = ? WHERE id = ?", ("Jane", 1)).unwrap();
// No explicit commit needed - already committed
```

### Explicit Transaction

For multiple operations that should be atomic:

```rust
let mut conn = Connection::connect("firebird://...").unwrap();

let mut trans = conn.transaction().unwrap();

trans.execute("INSERT INTO users VALUES (?, ?)", (1, "John")).unwrap();
trans.execute("INSERT INTO users VALUES (?, ?)", (2, "Jane")).unwrap();

// Must explicitly commit or rollback
trans.commit().unwrap();
// or: trans.rollback().unwrap();
```

### Batch Insert (No Autocommit)

For bulk inserts, use `prepare_no_autocommit()` to avoid commit overhead on each row:

```rust
let mut conn = Connection::connect("firebird://...").unwrap();

// Prepare without autocommit for better performance
let mut stmt = conn.prepare_no_autocommit("INSERT INTO logs (id, msg) VALUES (?, ?)").unwrap();

for i in 0..10000 {
    stmt.execute((i, format!("Message {}", i))).unwrap();
}

// Single commit at the end
conn.commit().unwrap();
```

This reduces insert time significantly (e.g., from ~9000ms to ~6850ms for 10000 rows).

## Transaction Isolation Levels

```rust
use firebirust::{Connection, IsolationLevel, LockWait, TransactionOptions};

let mut conn = Connection::connect("firebird://...").unwrap();

// Read Committed (default)
let mut trans = conn.transaction().unwrap();

// Snapshot isolation
let options = TransactionOptions::new()
    .isolation_level(IsolationLevel::Snapshot);
let mut trans = conn.transaction_with_options(options).unwrap();

// Read Consistency (Firebird 4+)
let options = TransactionOptions::new()
    .isolation_level(IsolationLevel::ReadConsistency);
let mut trans = conn.transaction_with_options(options).unwrap();

// With lock timeout
let options = TransactionOptions::new()
    .isolation_level(IsolationLevel::ReadCommitted)
    .lock_wait(LockWait::Timeout(5)); // 5 seconds timeout
let mut trans = conn.transaction_with_options(options).unwrap();

// No wait (fail immediately if lock unavailable)
let options = TransactionOptions::new()
    .lock_wait(LockWait::NoWait);
let mut trans = conn.transaction_with_options(options).unwrap();

trans.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1", ()).unwrap();
trans.commit().unwrap();
```

### Available Isolation Levels

| Level | Description |
|-------|-------------|
| `ReadCommitted` | Read Committed with record versioning (default) |
| `ReadCommittedNoRecVersion` | Read Committed with pessimistic locking |
| `ReadCommittedReadOnly` | Read Committed read-only |
| `Snapshot` | Snapshot isolation (concurrency) |
| `SnapshotReadOnly` | Snapshot read-only |
| `Serializable` | Serializable isolation (consistency) |
| `ReadConsistency` | Read Consistency (Firebird 4+ only) |

## Connection Pooling

```rust
use firebirust::{ConnectionPool, PoolOptions};
use std::time::Duration;

let options = PoolOptions {
    min_size: 2,
    max_size: 10,
    connection_lifetime: 3600, // 1 hour (0 = infinite)
    validate: true,
    acquire_timeout: Duration::from_secs(30),
};

let pool = ConnectionPool::new(
    "firebird://SYSDBA:masterkey@localhost/database.fdb",
    options,
).unwrap();

// Get connection from pool
let mut conn = pool.get().unwrap();

// Use connection
conn.execute("INSERT INTO logs (message) VALUES (?)", ("Hello",)).unwrap();
conn.commit().unwrap();

// Connection is automatically returned to pool when dropped
```

## Event Notifications (POST_EVENT)

```rust
use firebirust::{Connection, EventAlerter};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

let conn = Connection::connect("firebird://...").unwrap();

// Create event alerter
let alerter = EventAlerter::new(&conn, vec!["my_event", "other_event"]).unwrap();

let running = Arc::new(AtomicBool::new(true));
let running_clone = running.clone();

// Start listening with callback
alerter.start(move |event_name, count| {
    println!("Event '{}' fired {} times", event_name, count);
});

// In another connection, post an event
// POST_EVENT 'my_event';

// Stop listening
running.store(false, Ordering::SeqCst);
alerter.stop();
```

## Wire Compression

Wire compression reduces network bandwidth using zlib. Enable it via URL parameter:

```rust
let mut conn = Connection::connect(
    "firebird://SYSDBA:masterkey@localhost/database.fdb?compress=true"
).unwrap();
```

Compression is automatically negotiated during connection handshake. It's especially useful for:
- Large result sets
- Slow network connections
- Blob transfers

## Async/Await

```rust
use firebirust::ConnectionAsync;

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = ConnectionAsync::connect(
        "firebird://SYSDBA:masterkey@localhost/database.fdb"
    ).await?;

    conn.execute("INSERT INTO test (id) VALUES (?)", (1,)).await?;
    conn.commit().await?;

    Ok(())
}
```

## Column Metadata (Statement Description)

Get detailed metadata about query result columns:

```rust
use firebirust::{Connection, ColumnInfo};

let mut conn = Connection::connect("firebird://...").unwrap();
let mut stmt = conn.prepare("SELECT id, name, salary FROM employees").unwrap();

// Get column metadata (DB-API 2.0 style description)
let columns: Vec<ColumnInfo> = stmt.description();

for col in &columns {
    println!("Column: {}", col.name);
    println!("  Type: {} (code: {})", col.type_name(), col.type_code);
    println!("  Nullable: {}", col.nullable);
    println!("  Table: {}", col.table_name);
    if let Some(precision) = col.precision {
        println!("  Precision: {}, Scale: {}", precision, col.scale);
    }
}

// Execute query and get row count
let rows = stmt.query(()).unwrap();
println!("Rows affected/fetched: {}", stmt.rowcount());

// Simple column info
println!("Column count: {}", stmt.column_count());
println!("Column names: {:?}", stmt.column_names());
```

### ColumnInfo Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | String | Column alias or field name |
| `type_code` | u32 | SQL type code |
| `display_size` | Option\<i32\> | Display size (character types) |
| `internal_size` | i32 | Storage size in bytes |
| `precision` | Option\<i32\> | Numeric precision |
| `scale` | i32 | Numeric scale |
| `nullable` | bool | Whether NULL is allowed |
| `field_name` | String | Original field name |
| `table_name` | String | Table name |
| `owner_name` | String | Owner name |

## Supported Data Types

| Firebird Type | Rust Type |
|---------------|-----------|
| SMALLINT | i16 |
| INTEGER | i32 |
| BIGINT | i64 |
| INT128 | i128 (Firebird 4+) |
| FLOAT | f32 |
| DOUBLE PRECISION | f64 |
| DECIMAL/NUMERIC | rust_decimal::Decimal |
| DEC64/DEC128 | rust_decimal::Decimal (Firebird 4+) |
| CHAR/VARCHAR | String |
| DATE | chrono::NaiveDate |
| TIME | chrono::NaiveTime |
| TIMESTAMP | chrono::NaiveDateTime |
| TIME WITH TIME ZONE | chrono::NaiveTime (Firebird 4+) |
| TIMESTAMP WITH TIME ZONE | chrono::NaiveDateTime (Firebird 4+) |
| BLOB | Vec\<u8\> |
| BOOLEAN | bool |

## URL Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `role` | "" | SQL role name |
| `timezone` | "" | Session timezone |
| `wire_crypt` | "true" | Enable wire encryption |
| `auth_plugin_name` | "Srp256" | Authentication plugin |
| `page_size` | "4096" | Database page size (for create) |
| `compress` | "false" | Enable wire compression |

## Performance

Benchmark comparison with Go driver (firebirdsql v0.9.10):

| Test | Go | Rust | Difference |
|------|-----|------|------------|
| Simple SELECT (1000x) | 870ms | **717ms** | +18% Rust |
| SELECT with JOIN (100x) | 11676ms | 11500ms | +1.5% Rust |
| GROUP BY aggregation (50x) | 53924ms | 53800ms | +0.2% Rust |
| Correlated subquery (30x) | 149ms | **120ms** | +19% Rust |
| Bulk INSERT (10000) | **5200ms** | 6850ms | -24% Go |
| Bulk UPDATE (5000) | 33ms | 36ms | -8% Go |
| Large FETCH (50000) | 283ms | **203ms** | +28% Rust |
| Small transactions (500x) | 866ms | **750ms** | +13% Rust |

Rust outperforms Go in 6 of 8 tests, with significant advantages in read operations.

## License

MIT License - See LICENSE file for details.

## Credits

- Original project: [nakagami/firebirust](https://github.com/nakagami/firebirust)
- Firebird Foundation: [firebirdsql.org](https://firebirdsql.org/)
