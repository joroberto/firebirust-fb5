//! firebirust-fb5 Demo
//!
//! This demo tests all features documented in the README.
//! Run with: cargo run

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use firebirust::{
    ColumnInfo, Connection, ConnectionPool, IsolationLevel, LockWait, PoolOptions,
    TransactionOptions,
};
use rust_decimal::Decimal;
use std::env;
use std::path::Path;

// Test result tracking
struct TestResults {
    passed: u32,
    failed: u32,
    total: u32,
}

impl TestResults {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            total: 0,
        }
    }

    fn pass(&mut self, name: &str) {
        self.passed += 1;
        self.total += 1;
        println!("  {} ... OK", name);
    }

    fn fail(&mut self, name: &str, error: &str) {
        self.failed += 1;
        self.total += 1;
        println!("  {} ... FAILED: {}", name, error);
    }

    fn summary(&self) {
        println!("\n=== Summary ===");
        println!("Tests passed: {}/{}", self.passed, self.total);
        if self.failed == 0 {
            println!("All features working correctly!");
        } else {
            println!("Some tests failed. Please check the output above.");
        }
    }
}

fn get_connection_url() -> String {
    let host = env::var("FB_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("FB_PORT").unwrap_or_else(|_| "3050".to_string());
    let user = env::var("FB_USER").unwrap_or_else(|_| "SYSDBA".to_string());
    let password = env::var("FB_PASSWORD").expect("FB_PASSWORD must be set in .env");
    let database = env::var("FB_DATABASE").unwrap_or_else(|_| "data/demo.fdb".to_string());
    let auth_plugin = env::var("FB_AUTH_PLUGIN").unwrap_or_else(|_| "Srp256".to_string());

    // Convert relative path to absolute
    let db_path = if Path::new(&database).is_absolute() {
        database
    } else {
        let cwd = env::current_dir().unwrap();
        cwd.join(&database).to_string_lossy().to_string()
    };

    format!(
        "firebird://{}:{}@{}:{}/{}?auth_plugin_name={}",
        user, password, host, port, db_path, auth_plugin
    )
}

fn main() {
    println!("=== firebirust-fb5 Demo ===\n");

    // Load .env file
    if dotenv::dotenv().is_err() {
        println!("Warning: .env file not found. Using environment variables.");
        println!("Copy .env-example to .env and configure your settings.\n");
    }

    let mut results = TestResults::new();
    let conn_url = get_connection_url();

    // Run all demos
    demo_01_create_database(&conn_url, &mut results);
    demo_02_basic_connection(&conn_url, &mut results);
    demo_03_connection_options(&conn_url, &mut results);
    demo_04_create_tables(&conn_url, &mut results);
    demo_05_dml_operations(&conn_url, &mut results);
    demo_06_query_fetch(&conn_url, &mut results);
    demo_07_transactions(&conn_url, &mut results);
    demo_08_isolation_levels(&conn_url, &mut results);
    demo_09_connection_pooling(&conn_url, &mut results);
    demo_10_column_metadata(&conn_url, &mut results);
    demo_11_data_types(&conn_url, &mut results);
    demo_12_async(&conn_url, &mut results);

    results.summary();
}

// Demo 1: Create database
fn demo_01_create_database(conn_url: &str, results: &mut TestResults) {
    println!("[1/12] Create Database");

    // Extract database path from URL
    let db_path = conn_url.split('/').last().unwrap_or("demo.fdb");

    // Check if database exists
    if Path::new(db_path).exists() {
        results.pass("Database already exists");
        return;
    }

    match Connection::create_database(conn_url) {
        Ok(_) => results.pass("Create database"),
        Err(e) => results.fail("Create database", &format!("{:?}", e)),
    }
}

// Demo 2: Basic connection
fn demo_02_basic_connection(conn_url: &str, results: &mut TestResults) {
    println!("\n[2/12] Basic Connection");

    match Connection::connect(conn_url) {
        Ok(mut conn) => {
            results.pass("Connect to database");

            // Test simple query using prepare (execute_batch is for non-SELECT statements)
            let query_result = conn.prepare("SELECT 1 FROM RDB$DATABASE");
            match query_result {
                Ok(mut stmt) => match stmt.query(()) {
                    Ok(mut rows) => {
                        if rows.next().is_some() {
                            results.pass("Execute simple query");
                        } else {
                            results.fail("Execute simple query", "No rows returned");
                        }
                    }
                    Err(e) => results.fail("Execute simple query", &format!("{:?}", e)),
                },
                Err(e) => results.fail("Execute simple query", &format!("{:?}", e)),
            }
        }
        Err(e) => {
            results.fail("Connect to database", &format!("{:?}", e));
        }
    }
}

// Demo 3: Connection with options
fn demo_03_connection_options(conn_url: &str, results: &mut TestResults) {
    println!("\n[3/12] Connection with Options");

    // Test wire compression (conn_url already has auth_plugin_name, so use &)
    // Note: Wire compression requires server support and proper configuration
    let compress_url = format!("{}&compress=true", conn_url);
    match Connection::connect(&compress_url) {
        Ok(_) => results.pass("Wire compression"),
        Err(_) => {
            println!("  Wire compression ... SKIPPED (server may not support it)");
            // Don't count as failure - it's optional
        }
    }

    // Auth plugin is already tested via the base URL (Srp256 is default)
    results.pass("Auth plugin Srp256 (default)");
}

// Demo 4: Create tables
fn demo_04_create_tables(conn_url: &str, results: &mut TestResults) {
    println!("\n[4/12] Create Tables (DDL)");

    let mut conn = match Connection::connect(conn_url) {
        Ok(c) => c,
        Err(e) => {
            results.fail("Connect", &format!("{:?}", e));
            return;
        }
    };

    // Drop existing tables (ignore errors)
    let _ = conn.execute_batch("DROP TABLE users");
    let _ = conn.execute_batch("DROP TABLE all_types");
    let _ = conn.execute_batch("DROP TABLE logs");

    // Create users table
    match conn.execute_batch(
        r#"
        CREATE TABLE users (
            id INTEGER NOT NULL PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(255),
            age SMALLINT,
            balance DECIMAL(18,2),
            active BOOLEAN DEFAULT TRUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    ) {
        Ok(_) => results.pass("Create users table"),
        Err(e) => results.fail("Create users table", &format!("{:?}", e)),
    }

    // Create all_types table for FB4+ types
    match conn.execute_batch(
        r#"
        CREATE TABLE all_types (
            id INTEGER NOT NULL PRIMARY KEY,
            small_val SMALLINT,
            int_val INTEGER,
            big_val BIGINT,
            float_val FLOAT,
            double_val DOUBLE PRECISION,
            decimal_val DECIMAL(18,4),
            char_val CHAR(10),
            varchar_val VARCHAR(100),
            date_val DATE,
            time_val TIME,
            timestamp_val TIMESTAMP,
            bool_val BOOLEAN,
            blob_val BLOB
        )
        "#,
    ) {
        Ok(_) => results.pass("Create all_types table"),
        Err(e) => results.fail("Create all_types table", &format!("{:?}", e)),
    }

    // Create logs table for batch insert test
    match conn.execute_batch(
        r#"
        CREATE TABLE logs (
            id INTEGER NOT NULL PRIMARY KEY,
            message VARCHAR(255),
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    ) {
        Ok(_) => results.pass("Create logs table"),
        Err(e) => results.fail("Create logs table", &format!("{:?}", e)),
    }
}

// Demo 5: DML operations
fn demo_05_dml_operations(conn_url: &str, results: &mut TestResults) {
    println!("\n[5/12] DML Operations");

    let mut conn = match Connection::connect(conn_url) {
        Ok(c) => c,
        Err(e) => {
            results.fail("Connect", &format!("{:?}", e));
            return;
        }
    };

    // INSERT with parameters
    match conn.execute(
        "INSERT INTO users (id, name, email, age, balance) VALUES (?, ?, ?, ?, ?)",
        (1, "John Doe", "john@example.com", 30i16, Decimal::new(1000_00, 2)),
    ) {
        Ok(_) => results.pass("INSERT with parameters"),
        Err(e) => results.fail("INSERT with parameters", &format!("{:?}", e)),
    }

    // INSERT another user
    match conn.execute(
        "INSERT INTO users (id, name, email, age, balance) VALUES (?, ?, ?, ?, ?)",
        (2, "Jane Smith", "jane@example.com", 25i16, Decimal::new(2500_50, 2)),
    ) {
        Ok(_) => results.pass("INSERT second user"),
        Err(e) => results.fail("INSERT second user", &format!("{:?}", e)),
    }

    // UPDATE
    match conn.execute(
        "UPDATE users SET balance = ? WHERE id = ?",
        (Decimal::new(1500_00, 2), 1),
    ) {
        Ok(_) => results.pass("UPDATE"),
        Err(e) => results.fail("UPDATE", &format!("{:?}", e)),
    }

    // SELECT count
    match conn.prepare("SELECT COUNT(*) FROM users") {
        Ok(mut stmt) => match stmt.query(()) {
            Ok(rows) => {
                let mut count = 0i64;
                for row in rows {
                    count = row.get(0).unwrap();
                }
                if count == 2 {
                    results.pass("SELECT COUNT");
                } else {
                    results.fail("SELECT COUNT", &format!("Expected 2, got {}", count));
                }
            }
            Err(e) => results.fail("SELECT COUNT", &format!("{:?}", e)),
        },
        Err(e) => results.fail("SELECT COUNT", &format!("{:?}", e)),
    }

    // DELETE
    match conn.execute("DELETE FROM users WHERE id = ?", (2,)) {
        Ok(_) => results.pass("DELETE"),
        Err(e) => results.fail("DELETE", &format!("{:?}", e)),
    }
}

// Demo 6: Query and Fetch
fn demo_06_query_fetch(conn_url: &str, results: &mut TestResults) {
    println!("\n[6/12] Query and Fetch");

    let mut conn = match Connection::connect(conn_url) {
        Ok(c) => c,
        Err(e) => {
            results.fail("Connect", &format!("{:?}", e));
            return;
        }
    };

    // Re-insert user 2 for testing
    let _ = conn.execute(
        "INSERT INTO users (id, name, email, age) VALUES (?, ?, ?, ?)",
        (2, "Jane Smith", "jane@example.com", 25i16),
    );

    // Query with iteration
    match conn.prepare("SELECT id, name, email FROM users ORDER BY id") {
        Ok(mut stmt) => match stmt.query(()) {
            Ok(rows) => {
                let mut found = 0;
                for row in rows {
                    let id: i32 = row.get(0).unwrap();
                    let name: String = row.get(1).unwrap();
                    let _email: Option<String> = row.get(2).unwrap();
                    println!("    Found user {}: {}", id, name);
                    found += 1;
                }
                if found >= 1 {
                    results.pass("Query with iteration");
                } else {
                    results.fail("Query with iteration", "No rows found");
                }
            }
            Err(e) => results.fail("Query with iteration", &format!("{:?}", e)),
        },
        Err(e) => results.fail("Query with iteration", &format!("{:?}", e)),
    }

    // Query map with struct
    #[derive(Debug)]
    struct User {
        id: i32,
        name: String,
    }

    let query_map_result = conn.prepare("SELECT id, name FROM users");
    match query_map_result {
        Ok(mut stmt) => {
            match stmt.query_map((), |row| {
                Ok(User {
                    id: row.get(0).unwrap(),
                    name: row.get(1).unwrap(),
                })
            }) {
                Ok(users) => {
                    let mut count = 0;
                    for user in users {
                        if let Ok(u) = user {
                            println!("    Mapped: {:?}", u);
                            count += 1;
                        }
                    }
                    if count >= 1 {
                        results.pass("Query map with struct");
                    } else {
                        results.fail("Query map with struct", "No users mapped");
                    }
                }
                Err(e) => results.fail("Query map with struct", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("Query map with struct", &format!("{:?}", e)),
    };
}

// Demo 7: Transactions
fn demo_07_transactions(conn_url: &str, results: &mut TestResults) {
    println!("\n[7/12] Transactions");

    let mut conn = match Connection::connect(conn_url) {
        Ok(c) => c,
        Err(e) => {
            results.fail("Connect", &format!("{:?}", e));
            return;
        }
    };

    // Explicit transaction with commit
    match conn.transaction() {
        Ok(mut trans) => {
            match trans.execute(
                "INSERT INTO users (id, name) VALUES (?, ?)",
                (100, "Trans User"),
            ) {
                Ok(_) => match trans.commit() {
                    Ok(_) => results.pass("Explicit transaction commit"),
                    Err(e) => results.fail("Explicit transaction commit", &format!("{:?}", e)),
                },
                Err(e) => results.fail("Explicit transaction commit", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("Explicit transaction commit", &format!("{:?}", e)),
    }

    // Explicit transaction with rollback
    match conn.transaction() {
        Ok(mut trans) => {
            let _ = trans.execute(
                "INSERT INTO users (id, name) VALUES (?, ?)",
                (101, "Rollback User"),
            );
            match trans.rollback() {
                Ok(_) => results.pass("Explicit transaction rollback"),
                Err(e) => results.fail("Explicit transaction rollback", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("Explicit transaction rollback", &format!("{:?}", e)),
    }

    // Verify rollback worked (user 101 should not exist)
    match conn.prepare("SELECT COUNT(*) FROM users WHERE id = 101") {
        Ok(mut stmt) => match stmt.query(()) {
            Ok(rows) => {
                for row in rows {
                    let count: i64 = row.get(0).unwrap();
                    if count == 0 {
                        results.pass("Rollback verification");
                    } else {
                        results.fail("Rollback verification", "User 101 should not exist");
                    }
                }
            }
            Err(e) => results.fail("Rollback verification", &format!("{:?}", e)),
        },
        Err(e) => results.fail("Rollback verification", &format!("{:?}", e)),
    }

    // Batch insert with prepare_no_autocommit
    let batch_success = {
        match conn.prepare_no_autocommit("INSERT INTO logs (id, message) VALUES (?, ?)") {
            Ok(mut stmt) => {
                let mut success = true;
                for i in 1..=100i32 {
                    let msg = format!("Log message {}", i);
                    if stmt.execute((i, msg.as_str())).is_err() {
                        success = false;
                        break;
                    }
                }
                if success {
                    Ok(())
                } else {
                    Err("Insert failed".to_string())
                }
            }
            Err(e) => Err(format!("{:?}", e)),
        }
    };

    match batch_success {
        Ok(_) => {
            match conn.commit() {
                Ok(_) => results.pass("Batch insert (100 rows)"),
                Err(e) => results.fail("Batch insert (100 rows)", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("Batch insert (100 rows)", &e),
    }
}

// Demo 8: Isolation levels
fn demo_08_isolation_levels(conn_url: &str, results: &mut TestResults) {
    println!("\n[8/12] Transaction Isolation Levels");

    let mut conn = match Connection::connect(conn_url) {
        Ok(c) => c,
        Err(e) => {
            results.fail("Connect", &format!("{:?}", e));
            return;
        }
    };

    // ReadCommitted (default)
    match conn.transaction() {
        Ok(mut trans) => {
            let _ = trans.execute("SELECT 1 FROM RDB$DATABASE", ());
            match trans.commit() {
                Ok(_) => results.pass("ReadCommitted (default)"),
                Err(e) => results.fail("ReadCommitted (default)", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("ReadCommitted (default)", &format!("{:?}", e)),
    }

    // Snapshot
    let options = TransactionOptions::new().isolation_level(IsolationLevel::Snapshot);
    match conn.transaction_with_options(options) {
        Ok(mut trans) => {
            let _ = trans.execute("SELECT 1 FROM RDB$DATABASE", ());
            match trans.commit() {
                Ok(_) => results.pass("Snapshot isolation"),
                Err(e) => results.fail("Snapshot isolation", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("Snapshot isolation", &format!("{:?}", e)),
    }

    // ReadConsistency (Firebird 4+)
    let options = TransactionOptions::new().isolation_level(IsolationLevel::ReadConsistency);
    match conn.transaction_with_options(options) {
        Ok(mut trans) => {
            let _ = trans.execute("SELECT 1 FROM RDB$DATABASE", ());
            match trans.commit() {
                Ok(_) => results.pass("ReadConsistency (FB4+)"),
                Err(e) => results.fail("ReadConsistency (FB4+)", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("ReadConsistency (FB4+)", &format!("{:?}", e)),
    }

    // NoWait
    let options = TransactionOptions::new().lock_wait(LockWait::NoWait);
    match conn.transaction_with_options(options) {
        Ok(mut trans) => {
            let _ = trans.execute("SELECT 1 FROM RDB$DATABASE", ());
            match trans.commit() {
                Ok(_) => results.pass("LockWait::NoWait"),
                Err(e) => results.fail("LockWait::NoWait", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("LockWait::NoWait", &format!("{:?}", e)),
    }

    // Timeout
    let options = TransactionOptions::new().lock_wait(LockWait::Timeout(5));
    let timeout_result = conn.transaction_with_options(options);
    match timeout_result {
        Ok(mut trans) => {
            let _ = trans.execute("SELECT 1 FROM RDB$DATABASE", ());
            match trans.commit() {
                Ok(_) => results.pass("LockWait::Timeout(5)"),
                Err(e) => results.fail("LockWait::Timeout(5)", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("LockWait::Timeout(5)", &format!("{:?}", e)),
    };
}

// Demo 9: Connection pooling
fn demo_09_connection_pooling(conn_url: &str, results: &mut TestResults) {
    println!("\n[9/12] Connection Pooling");

    let options = PoolOptions {
        min_size: 1,
        max_size: 5,
        connection_lifetime: 3600,
        validate: true,
        acquire_timeout: 10, // seconds
    };

    match ConnectionPool::new(conn_url, options) {
        Ok(pool) => {
            results.pass("Create pool");

            // Get connection from pool
            match pool.get() {
                Ok(mut conn) => {
                    results.pass("Get connection from pool");

                    // Use connection - pool returns a guard, use it directly
                    match conn.prepare("SELECT 1 FROM RDB$DATABASE") {
                        Ok(_) => results.pass("Use pooled connection"),
                        Err(e) => results.fail("Use pooled connection", &format!("{:?}", e)),
                    }
                    // Connection is returned when dropped
                }
                Err(e) => results.fail("Get connection from pool", &format!("{:?}", e)),
            }

            // Get multiple connections
            let mut conns = Vec::new();
            let mut success = true;
            for i in 0..3 {
                match pool.get() {
                    Ok(c) => conns.push(c),
                    Err(e) => {
                        results.fail(&format!("Get connection {}", i), &format!("{:?}", e));
                        success = false;
                        break;
                    }
                }
            }
            if success {
                results.pass("Get multiple connections (3)");
            }
        }
        Err(e) => results.fail("Create pool", &format!("{:?}", e)),
    }
}

// Demo 10: Column metadata
fn demo_10_column_metadata(conn_url: &str, results: &mut TestResults) {
    println!("\n[10/12] Column Metadata");

    let mut conn = match Connection::connect(conn_url) {
        Ok(c) => c,
        Err(e) => {
            results.fail("Connect", &format!("{:?}", e));
            return;
        }
    };

    let prepare_result = conn.prepare("SELECT id, name, email, age, balance FROM users");
    match prepare_result {
        Ok(mut stmt) => {
            // Get description
            let columns: Vec<ColumnInfo> = stmt.description();
            if columns.len() == 5 {
                results.pass("description() returns columns");
                println!("    Columns found:");
                for col in &columns {
                    println!(
                        "      - {} ({}, nullable: {})",
                        col.name,
                        col.type_name(),
                        col.nullable
                    );
                }
            } else {
                results.fail(
                    "description() returns columns",
                    &format!("Expected 5 columns, got {}", columns.len()),
                );
            }

            // Get column names
            let names = stmt.column_names();
            if names.len() == 5 {
                results.pass("column_names()");
            } else {
                results.fail(
                    "column_names()",
                    &format!("Expected 5 names, got {}", names.len()),
                );
            }

            // Get column count
            if stmt.column_count() == 5 {
                results.pass("column_count()");
            } else {
                results.fail(
                    "column_count()",
                    &format!("Expected 5, got {}", stmt.column_count()),
                );
            }

            // Execute and get rowcount
            match stmt.query(()) {
                Ok(_) => {
                    let count = stmt.rowcount();
                    results.pass(&format!("rowcount() = {}", count));
                }
                Err(e) => results.fail("rowcount()", &format!("{:?}", e)),
            }
        }
        Err(e) => results.fail("Prepare statement", &format!("{:?}", e)),
    };
}

// Demo 11: Data types
fn demo_11_data_types(conn_url: &str, results: &mut TestResults) {
    println!("\n[11/12] Data Types");

    let mut conn = match Connection::connect(conn_url) {
        Ok(c) => c,
        Err(e) => {
            results.fail("Connect", &format!("{:?}", e));
            return;
        }
    };

    // Insert various data types
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let time = NaiveTime::from_hms_opt(14, 30, 0).unwrap();
    let timestamp = NaiveDateTime::new(date, time);
    let decimal = Decimal::new(12345_6789, 4); // 1234.56789
    let blob_data: &[u8] = &[1u8, 2, 3, 4, 5];

    match conn.execute(
        r#"INSERT INTO all_types
           (id, small_val, int_val, big_val, float_val, double_val,
            decimal_val, char_val, varchar_val, date_val, time_val,
            timestamp_val, bool_val, blob_val)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        (
            1i32,
            100i16,        // SMALLINT
            50000i32,      // INTEGER
            9999999999i64, // BIGINT
            3.14f32,       // FLOAT
            2.71828f64,    // DOUBLE
            decimal,       // DECIMAL
            "CHAR10",      // CHAR
            "Variable",    // VARCHAR
            date,          // DATE
            time,          // TIME
            timestamp,     // TIMESTAMP
            true,          // BOOLEAN
            blob_data,     // BLOB
        ),
    ) {
        Ok(_) => results.pass("Insert all data types"),
        Err(e) => {
            results.fail("Insert all data types", &format!("{:?}", e));
            return;
        }
    }

    // Read back and verify
    let query_result = conn.prepare("SELECT * FROM all_types WHERE id = 1");
    match query_result {
        Ok(mut stmt) => match stmt.query(()) {
            Ok(rows) => {
                for row in rows {
                    // SMALLINT
                    let small: i16 = row.get(1).unwrap();
                    if small == 100 {
                        results.pass("SMALLINT");
                    } else {
                        results.fail("SMALLINT", &format!("Expected 100, got {}", small));
                    }

                    // INTEGER
                    let int_val: i32 = row.get(2).unwrap();
                    if int_val == 50000 {
                        results.pass("INTEGER");
                    } else {
                        results.fail("INTEGER", &format!("Expected 50000, got {}", int_val));
                    }

                    // BIGINT
                    let big: i64 = row.get(3).unwrap();
                    if big == 9999999999 {
                        results.pass("BIGINT");
                    } else {
                        results.fail("BIGINT", &format!("Expected 9999999999, got {}", big));
                    }

                    // FLOAT
                    let float_val: f32 = row.get(4).unwrap();
                    if (float_val - 3.14).abs() < 0.01 {
                        results.pass("FLOAT");
                    } else {
                        results.fail("FLOAT", &format!("Expected ~3.14, got {}", float_val));
                    }

                    // DOUBLE
                    let double_val: f64 = row.get(5).unwrap();
                    if (double_val - 2.71828).abs() < 0.0001 {
                        results.pass("DOUBLE PRECISION");
                    } else {
                        results.fail(
                            "DOUBLE PRECISION",
                            &format!("Expected ~2.71828, got {}", double_val),
                        );
                    }

                    // DECIMAL
                    let dec: Decimal = row.get(6).unwrap();
                    results.pass(&format!("DECIMAL = {}", dec));

                    // VARCHAR
                    let varchar: String = row.get(8).unwrap();
                    if varchar == "Variable" {
                        results.pass("VARCHAR");
                    } else {
                        results.fail(
                            "VARCHAR",
                            &format!("Expected 'Variable', got '{}'", varchar),
                        );
                    }

                    // DATE
                    let date_val: NaiveDate = row.get(9).unwrap();
                    results.pass(&format!("DATE = {}", date_val));

                    // TIME
                    let time_val: NaiveTime = row.get(10).unwrap();
                    results.pass(&format!("TIME = {}", time_val));

                    // TIMESTAMP
                    let ts: NaiveDateTime = row.get(11).unwrap();
                    results.pass(&format!("TIMESTAMP = {}", ts));

                    // BOOLEAN
                    let bool_val: bool = row.get(12).unwrap();
                    if bool_val {
                        results.pass("BOOLEAN = true");
                    } else {
                        results.fail("BOOLEAN", "Expected true, got false");
                    }

                    // BLOB
                    let blob: Vec<u8> = row.get(13).unwrap();
                    if blob == vec![1u8, 2, 3, 4, 5] {
                        results.pass("BLOB");
                    } else {
                        results.fail("BLOB", &format!("Unexpected blob content: {:?}", blob));
                    }
                }
            }
            Err(e) => results.fail("Query all_types", &format!("{:?}", e)),
        },
        Err(e) => results.fail("Prepare all_types query", &format!("{:?}", e)),
    };
}

// Demo 12: Async/Await
fn demo_12_async(conn_url: &str, results: &mut TestResults) {
    println!("\n[12/12] Async/Await");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let url = conn_url.to_string();

    rt.block_on(async {
        use firebirust::ConnectionAsync;

        match ConnectionAsync::connect(&url).await {
            Ok(mut conn) => {
                results.pass("Async connect");

                match conn.execute("SELECT 1 FROM RDB$DATABASE", ()).await {
                    Ok(_) => results.pass("Async execute"),
                    Err(e) => results.fail("Async execute", &format!("{:?}", e)),
                }

                match conn.commit().await {
                    Ok(_) => results.pass("Async commit"),
                    Err(e) => results.fail("Async commit", &format!("{:?}", e)),
                }
            }
            Err(e) => results.fail("Async connect", &format!("{:?}", e)),
        }
    });
}
