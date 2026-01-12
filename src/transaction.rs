// MIT License
//
// Copyright (c) 2021 Hajime Nakagami<nakagami@gmail.com>
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

use super::Connection;
use super::error::Error;
use super::params::Params;
use super::statement::Statement;

/// Transaction isolation level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// Read Committed with record versioning (default, optimistic)
    ReadCommitted,
    /// Read Committed with no record versioning (pessimistic locking)
    ReadCommittedNoRecVersion,
    /// Read Committed read-only
    ReadCommittedReadOnly,
    /// Snapshot isolation (concurrency model)
    Snapshot,
    /// Snapshot read-only
    SnapshotReadOnly,
    /// Serializable isolation (consistency model)
    Serializable,
    /// Read Consistency (Firebird 4+, repeatable read)
    ReadConsistency,
}

/// Lock wait behavior for transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockWait {
    /// Wait indefinitely for locks (default)
    Wait,
    /// Fail immediately if lock is unavailable
    NoWait,
    /// Wait for specified number of seconds before failing
    Timeout(u32),
}

impl Default for LockWait {
    fn default() -> Self {
        LockWait::Wait
    }
}

/// Options for configuring a transaction
#[derive(Debug, Clone)]
pub struct TransactionOptions {
    /// Transaction isolation level
    pub isolation_level: IsolationLevel,
    /// Lock wait behavior
    pub lock_wait: LockWait,
    /// Read-only flag (overrides isolation level setting if true)
    pub read_only: bool,
}

impl Default for TransactionOptions {
    fn default() -> Self {
        Self {
            isolation_level: IsolationLevel::ReadCommitted,
            lock_wait: LockWait::Wait,
            read_only: false,
        }
    }
}

impl TransactionOptions {
    /// Create new transaction options with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set isolation level
    pub fn isolation_level(mut self, level: IsolationLevel) -> Self {
        self.isolation_level = level;
        self
    }

    /// Set lock wait behavior
    pub fn lock_wait(mut self, wait: LockWait) -> Self {
        self.lock_wait = wait;
        self
    }

    /// Set read-only mode
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Create options for snapshot isolation
    pub fn snapshot() -> Self {
        Self {
            isolation_level: IsolationLevel::Snapshot,
            lock_wait: LockWait::Wait,
            read_only: false,
        }
    }

    /// Create options for serializable isolation
    pub fn serializable() -> Self {
        Self {
            isolation_level: IsolationLevel::Serializable,
            lock_wait: LockWait::Wait,
            read_only: false,
        }
    }

    /// Create options for read-only snapshot
    pub fn snapshot_read_only() -> Self {
        Self {
            isolation_level: IsolationLevel::SnapshotReadOnly,
            lock_wait: LockWait::Wait,
            read_only: true,
        }
    }
}

pub struct Transaction<'conn> {
    conn: &'conn mut Connection,
    trans_handle: i32,
    finished: bool,  // true if commit() or rollback() was called
}

impl Transaction<'_> {
    pub fn new(conn: &mut Connection) -> Result<Transaction<'_>, Error> {
        let trans_handle = conn._begin_trans()?;
        Ok(Transaction { conn, trans_handle, finished: false })
    }

    /// Create a new transaction with custom options (isolation level, lock wait, etc.)
    pub fn with_options(conn: &mut Connection, options: TransactionOptions) -> Result<Transaction<'_>, Error> {
        let trans_handle = conn._begin_trans_with_options(&options)?;
        Ok(Transaction { conn, trans_handle, finished: false })
    }

    pub fn execute_batch(&mut self, query: &str) -> Result<(), Error> {
        self.conn._execute_batch(query, self.trans_handle)
    }

    pub fn execute<P: Params>(&mut self, query: &str, params: P) -> Result<(), Error> {
        self.conn._execute(query, params, self.trans_handle)
    }

    pub fn commit(&mut self) -> Result<(), Error> {
        let result = self.conn._commit_final(self.trans_handle);
        if result.is_ok() {
            self.finished = true;
        }
        result
    }

    pub fn rollback(&mut self) -> Result<(), Error> {
        let result = self.conn._rollback_final(self.trans_handle);
        if result.is_ok() {
            self.finished = true;
        }
        result
    }

    pub fn prepare(&mut self, query: &str) -> Result<Statement<'_>, Error> {
        self.conn._prepare(query, self.trans_handle, false) // autocommit=false in transaction
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.finished {
            // Only rollback if commit() or rollback() was not called
            self.conn.drop_transaction(self.trans_handle);
        }
    }
}
