// MIT License
//
// Copyright (c) 2021 Hajime Nakagami<nakagami@gmail.com>
// Copyright (c) 2026 Roberto (Connection Pool implementation)
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Connection Pool for firebirust
//!
//! This module provides a thread-safe connection pool implementation inspired by
//! IBDAC 10.1's TIBCConnectionPool. It supports:
//!
//! - Configurable min/max pool size
//! - Connection lifetime management
//! - Optional connection validation
//! - Version-based invalidation (lock-free design)
//! - RAII-based automatic connection return via PoolGuard
//!
//! # Example
//!
//! ```ignore
//! use firebirust::{ConnectionPool, PoolOptions};
//!
//! let options = PoolOptions {
//!     min_size: 2,
//!     max_size: 10,
//!     connection_lifetime: 3600, // 1 hour
//!     validate: true,
//!     acquire_timeout: 30,
//! };
//!
//! let pool = ConnectionPool::new("firebird://user:pass@host/database", options)?;
//!
//! // Get a connection from the pool
//! let mut guard = pool.get()?;
//! let conn = guard.connection();
//!
//! // Use the connection...
//! let mut stmt = conn.prepare("SELECT * FROM users")?;
//!
//! // Connection is automatically returned to pool when guard is dropped
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::connection::Connection;
use super::error::Error;

/// Options for configuring the connection pool
#[derive(Debug, Clone)]
pub struct PoolOptions {
    /// Minimum number of connections to maintain in the pool (default: 0)
    pub min_size: usize,
    /// Maximum number of connections allowed (default: 10)
    pub max_size: usize,
    /// Maximum lifetime of a connection in seconds (0 = unlimited, default: 0)
    pub connection_lifetime: u64,
    /// Validate connections before returning them from the pool (default: false)
    pub validate: bool,
    /// Timeout in seconds when waiting for a connection (default: 30)
    pub acquire_timeout: u64,
}

impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            min_size: 0,
            max_size: 10,
            connection_lifetime: 0,
            validate: false,
            acquire_timeout: 30,
        }
    }
}

impl PoolOptions {
    /// Create new pool options with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set minimum pool size
    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
    }

    /// Set maximum pool size
    pub fn max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }

    /// Set connection lifetime in seconds (0 = unlimited)
    pub fn connection_lifetime(mut self, seconds: u64) -> Self {
        self.connection_lifetime = seconds;
        self
    }

    /// Enable or disable connection validation
    pub fn validate(mut self, validate: bool) -> Self {
        self.validate = validate;
        self
    }

    /// Set acquire timeout in seconds
    pub fn acquire_timeout(mut self, seconds: u64) -> Self {
        self.acquire_timeout = seconds;
        self
    }
}

/// Internal struct representing a pooled connection with metadata
struct PooledConnection {
    conn: Connection,
    created_at: Instant,
    version: u64,
}

/// Internal state of the connection pool
struct PoolState {
    /// Queue of available connections (FIFO)
    available: VecDeque<PooledConnection>,
    /// Number of connections currently in use
    in_use: usize,
    /// Version counter for invalidation (incremented on invalidate())
    invalidate_version: u64,
    /// Flag indicating if pool is closed
    closed: bool,
}

/// Thread-safe connection pool for Firebird databases
///
/// The pool manages a collection of database connections, providing
/// efficient reuse of connections across multiple threads.
pub struct ConnectionPool {
    conn_string: String,
    options: PoolOptions,
    state: Mutex<PoolState>,
    /// Condition variable signaled when a connection becomes available
    available_cond: Condvar,
}

impl ConnectionPool {
    /// Create a new connection pool
    ///
    /// # Arguments
    ///
    /// * `conn_string` - Firebird connection string (e.g., "firebird://user:pass@host/database")
    /// * `options` - Pool configuration options
    ///
    /// # Returns
    ///
    /// An Arc-wrapped ConnectionPool ready for use across threads
    pub fn new(conn_string: &str, options: PoolOptions) -> Result<Arc<Self>, Error> {
        let pool = Arc::new(Self {
            conn_string: conn_string.to_string(),
            options: options.clone(),
            state: Mutex::new(PoolState {
                available: VecDeque::with_capacity(options.max_size),
                in_use: 0,
                invalidate_version: 0,
                closed: false,
            }),
            available_cond: Condvar::new(),
        });

        // Pre-create minimum connections
        for _ in 0..options.min_size {
            let conn = Connection::connect(&pool.conn_string)?;
            let mut state = pool.state.lock().unwrap();
            state.available.push_back(PooledConnection {
                conn,
                created_at: Instant::now(),
                version: 0,
            });
        }

        Ok(pool)
    }

    /// Get a connection from the pool
    ///
    /// This method will:
    /// 1. Try to return an existing available connection
    /// 2. Create a new connection if under max_size
    /// 3. Wait for a connection to become available (up to acquire_timeout)
    ///
    /// # Returns
    ///
    /// A PoolGuard that automatically returns the connection when dropped
    pub fn get(self: &Arc<Self>) -> Result<PoolGuard, Error> {
        let timeout = Duration::from_secs(self.options.acquire_timeout);
        let deadline = Instant::now() + timeout;

        loop {
            // Check if pool is closed
            {
                let state = self.state.lock().unwrap();
                if state.closed {
                    return Err(Error::PoolError("Pool is closed".to_string()));
                }
            }

            // Try to get an existing connection
            if let Some(conn) = self.try_get_available()? {
                return Ok(PoolGuard {
                    pool: Arc::clone(self),
                    conn: Some(conn),
                });
            }

            // Try to create a new connection
            if let Some(conn) = self.try_create_new()? {
                return Ok(PoolGuard {
                    pool: Arc::clone(self),
                    conn: Some(conn),
                });
            }

            // Wait for a connection to become available
            if Instant::now() >= deadline {
                return Err(Error::PoolTimeout);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let state = self.state.lock().unwrap();
            let _ = self.available_cond.wait_timeout(state, remaining).unwrap();
        }
    }

    /// Try to get an available connection from the pool
    fn try_get_available(&self) -> Result<Option<Connection>, Error> {
        let mut state = self.state.lock().unwrap();

        while let Some(pooled) = state.available.pop_front() {
            // Check if connection is still valid
            if self.is_valid(&pooled, state.invalidate_version) {
                state.in_use += 1;
                return Ok(Some(pooled.conn));
            }
            // Connection invalid, discard and try next
        }

        Ok(None)
    }

    /// Try to create a new connection if under max_size
    fn try_create_new(&self) -> Result<Option<Connection>, Error> {
        let can_create = {
            let state = self.state.lock().unwrap();
            let total = state.available.len() + state.in_use;
            total < self.options.max_size
        };

        if can_create {
            let conn = Connection::connect(&self.conn_string)?;
            let mut state = self.state.lock().unwrap();
            state.in_use += 1;
            return Ok(Some(conn));
        }

        Ok(None)
    }

    /// Check if a pooled connection is still valid
    fn is_valid(&self, pooled: &PooledConnection, current_version: u64) -> bool {
        // Check lifetime
        if self.options.connection_lifetime > 0 {
            let age = pooled.created_at.elapsed().as_secs();
            if age > self.options.connection_lifetime {
                return false;
            }
        }

        // Check invalidation version
        if pooled.version < current_version {
            return false;
        }

        // Optional validation (could be extended to do a ping/SELECT 1)
        if self.options.validate {
            // For now, we just check the version
            // TODO: Implement actual connection validation (e.g., SELECT 1 FROM RDB$DATABASE)
        }

        true
    }

    /// Return a connection to the pool
    fn return_connection(&self, conn: Connection) {
        let mut state = self.state.lock().unwrap();
        state.in_use = state.in_use.saturating_sub(1);

        // Only return to pool if not closed and under max_size
        if !state.closed && state.available.len() < self.options.max_size {
            let version = state.invalidate_version;
            state.available.push_back(PooledConnection {
                conn,
                created_at: Instant::now(),
                version,
            });
        }
        // else: connection is dropped

        // Notify waiting threads
        self.available_cond.notify_one();
    }

    /// Invalidate all pooled connections
    ///
    /// Connections currently in use are not affected, but will be
    /// discarded when returned to the pool.
    pub fn invalidate(&self) {
        let mut state = self.state.lock().unwrap();
        state.invalidate_version += 1;
    }

    /// Clear all available connections from the pool
    ///
    /// Connections currently in use are not affected.
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.invalidate_version += 1;
        state.available.clear();
    }

    /// Close the pool, preventing new connections from being acquired
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        state.available.clear();
        self.available_cond.notify_all();
    }

    /// Get the current number of available connections
    pub fn available_count(&self) -> usize {
        self.state.lock().unwrap().available.len()
    }

    /// Get the current number of connections in use
    pub fn in_use_count(&self) -> usize {
        self.state.lock().unwrap().in_use
    }

    /// Get the total number of connections (available + in use)
    pub fn total_count(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.available.len() + state.in_use
    }
}

/// RAII guard that returns a connection to the pool when dropped
///
/// This struct wraps a connection and ensures it is properly returned
/// to the pool when the guard goes out of scope.
pub struct PoolGuard {
    pool: Arc<ConnectionPool>,
    conn: Option<Connection>,
}

impl PoolGuard {
    /// Get a mutable reference to the underlying connection
    pub fn connection(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("Connection was already taken")
    }

    /// Take ownership of the connection, preventing it from being returned to the pool
    ///
    /// This is useful when you need to keep the connection beyond the guard's lifetime.
    /// Note: The connection will NOT be returned to the pool.
    pub fn take(mut self) -> Connection {
        self.conn.take().expect("Connection was already taken")
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.return_connection(conn);
        }
    }
}

impl std::ops::Deref for PoolGuard {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.conn.as_ref().expect("Connection was already taken")
    }
}

impl std::ops::DerefMut for PoolGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn.as_mut().expect("Connection was already taken")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_options_builder() {
        let options = PoolOptions::new()
            .min_size(5)
            .max_size(20)
            .connection_lifetime(3600)
            .validate(true)
            .acquire_timeout(60);

        assert_eq!(options.min_size, 5);
        assert_eq!(options.max_size, 20);
        assert_eq!(options.connection_lifetime, 3600);
        assert!(options.validate);
        assert_eq!(options.acquire_timeout, 60);
    }

    #[test]
    fn test_pool_options_default() {
        let options = PoolOptions::default();

        assert_eq!(options.min_size, 0);
        assert_eq!(options.max_size, 10);
        assert_eq!(options.connection_lifetime, 0);
        assert!(!options.validate);
        assert_eq!(options.acquire_timeout, 30);
    }
}
