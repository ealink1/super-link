# dameng

[![Crates.io](https://img.shields.io/crates/v/dameng.svg)](https://crates.io/crates/dameng)
[![Docs.rs](https://docs.rs/dameng/badge.svg)](https://docs.rs/dameng)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A pure-Rust driver for [Dameng Database](https://www.dameng.com/) (DM8).

> [中文文档 (Chinese)](README_CN.md)

## Features

- **Pure-Rust protocol** — full DM8 binary wire protocol (STARTUP, LOGIN, EXEC, OPE, FETCH, COMMIT, ROLLBACK)
- **SQLx-style parameter binding** — `&[&id, &name]` with automatic type conversion
- **Transaction support** — `Transaction` API with automatic rollback on `Drop`
- **Rich type system** — INT, BIGINT, VARCHAR, FLOAT, DOUBLE, DECIMAL, DATE, TIME, TIMESTAMP, BLOB, CLOB, and more
- **Safe** — parameterized queries prevent SQL injection
- **TLS support** — optional SSL/TLS encrypted connections

## Installation

```toml
[dependencies]
dameng = "0.1"
```

## Quick Start

### Connect

```rust
use dameng::Client;

let mut client = Client::new("127.0.0.1", 5236);
client.connect("SYSDBA", "SYSDBA")?;
```

### Query

```rust
let rs = client.query("SELECT ID, NAME FROM PERSON")?;
for row in rs.iter() {
    let id: i32 = row.get(0).unwrap_or_default();
    let name = row.get_str(1).unwrap_or("<NULL>");
    println!("ID={}, NAME={}", id, name);
}
```

### Parameterized Queries (SQLx style)

```rust
let id: i32 = 1;
let name: &str = "Alice";
let rs = client.query_with_params(
    "SELECT * FROM PERSON WHERE ID = ? AND NAME = ?",
    &[&id, &name],
)?;
```

### DML (INSERT / UPDATE / DELETE)

```rust
let affected = client.execute_with_params(
    "INSERT INTO PERSON (ID, NAME, AGE) VALUES (?, ?, ?)",
    &[&1, &"Alice", &25],
)?;
println!("Inserted {} rows", affected);
```

### Transactions

```rust
let mut tx = client.transaction()?;

tx.execute_with_params("INSERT INTO PERSON VALUES (?, ?)", &[&1, &"Alice"])?;
tx.execute_with_params("INSERT INTO PERSON VALUES (?, ?)", &[&2, &"Bob"])?;

// Commit — tx is consumed, Client borrow is released
tx.commit()?;

// Client is immediately available for reuse
client.close()?;
```

All operations within a transaction execute as an atomic unit. If a `Transaction` is dropped without an explicit `commit()` or `rollback()`, a `ROLLBACK` is sent automatically.

### Full CRUD Example

```rust
use dameng::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = Client::new("127.0.0.1", 5236);
    client.connect("SYSDBA", "SYSDBA")?;

    // CREATE
    client.execute("CREATE TABLE IF NOT EXISTS users (
        id INT PRIMARY KEY,
        name VARCHAR(100),
        age INT
    )")?;

    // INSERT — batch insert in a transaction
    let mut tx = client.transaction()?;
    let users = [(1, "Alice", 25i32), (2, "Bob", 30), (3, "Carol", 28)];
    for (id, name, age) in &users {
        tx.execute_with_params(
            "INSERT INTO users VALUES (?, ?, ?)",
            &[&id, &name, &age],
        )?;
    }
    tx.commit()?;

    // SELECT
    let rs = client.query_with_params(
        "SELECT name, age FROM users WHERE age > ? ORDER BY age",
        &[&26i32],
    )?;
    for row in rs.iter() {
        let name = row.get_str(0).unwrap_or("<NULL>");
        let age: i32 = row.get(1).unwrap_or_default();
        println!("{name}: {age}");
    }

    // UPDATE
    client.execute_with_params(
        "UPDATE users SET age = ? WHERE id = ?",
        &[&31i32, &1i32],
    )?;

    // DELETE
    client.execute_with_params("DELETE FROM users WHERE id = ?", &[&3i32])?;

    client.close()?;
    Ok(())
}
```

## API Reference

### Client

| Method | Returns | Description |
|--------|---------|-------------|
| `Client::new(host, port)` | `Client` | Create a new client |
| `connect(username, password)` | `Result<()>` | Connect to the database |
| `close()` | `Result<()>` | Close the connection |
| `transaction()` | `Result<Transaction>` | Begin a new transaction |
| `execute(sql)` | `Result<u64>` | Execute DML, returns affected rows |
| `execute_with_params(sql, params)` | `Result<u64>` | Execute DML with parameters |
| `query(sql)` | `Result<ResultSet>` | Execute a SELECT query |
| `query_with_params(sql, params)` | `Result<ResultSet>` | Execute a SELECT query with parameters |
| `begin()` | `Result<()>` | Disable auto-commit (low-level) |

### Transaction

| Method | Returns | Description |
|--------|---------|-------------|
| `commit(self)` | `Result<()>` | Commit and consume the transaction, releasing the Client |
| `rollback(self)` | `Result<()>` | Rollback and consume the transaction, releasing the Client |
| `execute(sql)` | `Result<u64>` | DML within the transaction |
| `execute_with_params(sql, params)` | `Result<u64>` | DML with parameters within the transaction |
| `query(sql)` | `Result<ResultSet>` | SELECT within the transaction |
| `query_with_params(sql, params)` | `Result<ResultSet>` | SELECT with parameters within the transaction |

### ResultSet

| Method / Field | Description |
|----------------|-------------|
| `iter()` | Returns an iterator over the rows |
| `columns` | Column metadata: `Vec<Column>` |
| `rows` | Row data: `Vec<Row>` |
| `total_row_count` | Total row count reported by the server |

### QueryRow

| Method | Description |
|--------|-------------|
| `get::<T>(idx)` | Get a column value by type (recommended) |
| `get_i32(idx)` / `get_i64(idx)` | Get an integer value |
| `get_str(idx)` | Get a string value |
| `get_f64(idx)` | Get a float value |
| `get_opt_str(idx)` | Get an optional string (NULL-safe) |

## Type Mapping

| Rust Type | DM Type | `ToDmValue` |
|-----------|---------|-------------|
| `i8` | TINYINT | `DmValue::TinyInt` |
| `i16` | SMALLINT | `DmValue::SmallInt` |
| `i32` | INT | `DmValue::Int` |
| `i64` | BIGINT | `DmValue::BigInt` |
| `f32` | FLOAT | `DmValue::Float` |
| `f64` | DOUBLE | `DmValue::Double` |
| `bool` | BIT | `DmValue::Boolean` |
| `&str` / `String` | VARCHAR | `DmValue::Text` |
| `Vec<u8>` | VARBINARY | `DmValue::Bytea` |
| `rust_decimal::Decimal` | DECIMAL | — |
| `chrono::NaiveDate` | DATE | — |
| `chrono::NaiveDateTime` | TIMESTAMP | — |

## Connection Configuration

```rust
use dameng::{Client, ConnectOptions, IsolationLevel};

// Via DSN
let opts = ConnectOptions::from_dsn(
    "dm://SYSDBA:SYSDBA@127.0.0.1:5236/?auto_commit=false&isolation_level=serializable"
)?;
let mut client = Client::connect_with(&opts)?;

// Or manually
let mut client = Client::new("127.0.0.1", 5236);
client.auto_commit = false;
client.isolation_level = IsolationLevel::Serializable;
client.connect("SYSDBA", "SYSDBA")?;
```

## Project Structure

```
rust-dameng/
├── dameng/             # Sync driver (main crate)
├── dameng-protocol/    # Wire protocol (message encode/decode + frame format)
├── dameng-types/       # Type system (DmValue + ToDmValue + encoding)
├── tokio-dameng/       # Async driver (in development)
├── dameng-macros/      # Procedural macros (in development)
├── integration-test/   # Integration tests
└── examples/           # Usage examples
```

## License

MIT

---

## Publishing to crates.io

This is a workspace with multiple sub-crates. Publish them in dependency order:

### 1. Update Cargo.toml metadata

Each sub-crate needs `repository`, `keywords`, `categories`, `readme`, and dependencies should use version numbers instead of paths:

```toml
[package]
repository = "https://github.com/yourname/rust-dameng-ex"
documentation = "https://docs.rs/dameng"
readme = "../README.md"
keywords = ["dameng", "database", "sql", "dm8"]
categories = ["database"]

[dependencies]
# Use version instead of path for publishing
dameng-protocol = "0.1"
dameng-types = "0.1"
```

Files to update: `dameng-types/Cargo.toml`, `dameng-protocol/Cargo.toml`, `dameng/Cargo.toml`.

### 2. Publish in order

```bash
# Login
cargo login <your-api-token>

# Dry-run checks
cargo publish --dry-run -p dameng-types
cargo publish --dry-run -p dameng-protocol
cargo publish --dry-run -p dameng

# Publish (no external deps first)
cargo publish -p dameng-types
cargo publish -p dameng-protocol
cargo publish -p dameng
```

### 3. Version management

- Use `0.1.0` as the initial version; bump to `1.0.0` once the API stabilizes
- Published versions cannot be deleted — use `cargo yank` to deprecate
- Always run `cargo publish --dry-run` before publishing

### 4. Get your API token

1. Sign in to [crates.io](https://crates.io) with GitHub
2. Go to Account Settings → API Tokens → New Token
3. Run `cargo login` and paste the token
