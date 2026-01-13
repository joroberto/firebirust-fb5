# History - firebirust-fb5

## v0.6.0 - 2026-01

Fork from [nakagami/firebirust](https://github.com/nakagami/firebirust) with Firebird 4/5 support.

### Bug Fixes

#### utils.rs
- **f32_to_bytes / f64_to_bytes**: Fixed endianness for Firebird wire protocol
  - Before: `to_le_bytes()` (LittleEndian - incorrect)
  - After: `to_be_bytes()` (BigEndian - correct for Firebird)

- **bytes_to_naive_time**: Fixed function call
  - Before: `from_hms_micro_opt()` (incorrect unit)
  - After: `from_hms_nano_opt()` (correct unit)

- **convert_time**: Fixed nanosecond calculation for BLR format
  - Before: `nanosecond * 10` (caused overflow, wrong conversion)
  - After: `nanosecond / 100000` (correct conversion to 1/10000 second units)

#### param.rs
- **Param::Short (SMALLINT)**: Fixed BLR type code
  - Before: BLR code `8` (blr_long/INTEGER)
  - After: BLR code `7` (blr_short/SMALLINT)

#### xsqlvar.rs
- **SQL_TYPE_SHORT**: Fixed value reading
  - Before: `bytes_to_bint16()` (wrong function)
  - After: `bytes_to_bint32() as i16` (correct, Firebird sends 4 bytes)

- **TIMESTAMP_TZ_EX / TIME_TZ_EX**: Added support for extended timezone types (Firebird 4+)
  - Added `SQL_TYPE_TIMESTAMP_TZ_EX` (32748)
  - Added `SQL_TYPE_TIME_TZ_EX` (32750)

#### cellvalue.rs
- **CellValueToVal<i64>**: Extended type conversion support
  - Added: Int128 to i64 conversion
  - Added: Decimal to i64 conversion

- **CellValueToVal<f64>**: Extended type conversion support
  - Added: Float, Decimal, Short, Long, Int64 to f64 conversion

- **CellValueToVal<f32>**: Extended type conversion support
  - Added: Double, Decimal, Short, Long, Int64 to f32 conversion

#### transaction.rs
- **Transaction state tracking**: Fixed commit/rollback flag
  - Transactions now properly track committed state to avoid double commit/rollback

#### wirechannel.rs
- **Connection closed detection**: Added EOF handling
  - Returns proper error when server closes connection unexpectedly

### Performance Improvements

#### wirechannel.rs - TCP/Network Optimizations
- **TCP_NODELAY**: Disabled Nagle's algorithm for low-latency operations
  - Reduces latency for small packets (protocol messages)

- **BufReader/BufWriter**: Added buffered I/O with 32KB buffers
  - Buffer size matches fbclient's MAX_DATA_HW
  - Reduces system call overhead

- **VecDeque**: Changed read buffer from Vec to VecDeque
  - Before: `Vec::remove(0)` - O(n) for each byte
  - After: `VecDeque::drain()` - O(1) amortized

- **Read buffer**: Increased from 4096 to 8192 bytes per read

#### wireprotocol.rs
- **BUFFER_LEN**: Increased from 1024 to 8192 bytes
  - Reduces network round-trips for large result sets

### New Features

#### param.rs
- **From<String> for Param**: Added conversion from owned String
  - Allows using String directly in parameters without borrowing

- **ToSqlParam for NaiveDateTime**: Added direct timestamp parameter support
  - Allows inserting timestamps without manual conversion

#### Transaction Isolation Levels (transaction.rs)
- **IsolationLevel enum**:
  - `ReadCommitted` - Read Committed with record versioning (default)
  - `ReadCommittedNoRecVersion` - Read Committed with pessimistic locking
  - `ReadCommittedReadOnly` - Read Committed read-only
  - `Snapshot` - Snapshot isolation (concurrency)
  - `SnapshotReadOnly` - Snapshot read-only
  - `Serializable` - Serializable isolation (consistency)
  - `ReadConsistency` - Read Consistency (Firebird 4+ only)

- **LockWait enum**:
  - `Wait` - Wait for locks (default)
  - `NoWait` - Fail immediately if lock unavailable
  - `Timeout(u32)` - Wait with timeout in seconds

- **TransactionOptions struct**: Configure transactions with builder pattern
- **Connection::transaction_with_options()**: Start transaction with custom isolation

#### Connection Pooling (pool.rs) - NEW FILE
- **ConnectionPool**: Thread-safe connection pool with Mutex
- **PoolOptions**:
  - `min_size` - Minimum connections to maintain (default: 0)
  - `max_size` - Maximum connections (default: 10)
  - `connection_lifetime` - Max connection age in seconds (0 = infinite)
  - `validate` - Validate connection before use
  - `acquire_timeout` - Timeout to acquire connection (default: 30s)
- **PoolGuard**: RAII guard for automatic connection return

#### Wire Compression (compression.rs) - NEW FILE
- **WireCompressor**: Zlib-based compression using flate2 crate
- Activation via URL: `?compress=true`
- Automatic negotiation during connection handshake
- Order: compress first, then encrypt

#### Event Notifications (alerter.rs) - NEW FILE
- **EventAlerter**: Listen for POST_EVENT notifications
- Supports up to 15 simultaneous events (MAX_EVENTS)
- Dedicated thread with callback function
- Methods: `queue_events()`, `wait_for_event()`, `cancel_events()`

#### Column Metadata (statement.rs)
- **ColumnInfo struct**:
  - `name` - Column alias or field name
  - `type_code` - SQL type code
  - `type_name()` - Human-readable type name
  - `display_size` - Display size for character types
  - `internal_size` - Storage size in bytes
  - `precision` - Numeric precision
  - `scale` - Numeric scale
  - `nullable` - Whether NULL is allowed
  - `field_name` - Original field name
  - `table_name` - Table name
  - `owner_name` - Owner name

- **Statement::description()**: Returns Vec<ColumnInfo> for all columns
- **Statement::rowcount()**: Returns number of rows affected/fetched

#### Protocol Support (wireprotocol.rs)
- **Protocol versions 13-17**: Support for Firebird 3.0 to 5.0
- **PFLAG_COMPRESS**: Compression flag in protocol negotiation
- **op_que_events / op_cancel_events**: Event queue operations
- **wait_for_event()**: Event waiting with timeout
- **op_transaction_with_options()**: Custom transaction parameters
- **build_tpb()**: Transaction Parameter Block builder

### Dependencies
- Added `flate2 = "1.0"` for wire compression

### Compatibility
- Firebird 3.0+ (protocol v13+)
- Firebird 4.0+ (full type support including INT128, TIMESTAMP_TZ, TIME_TZ, DEC64, DEC128)
- Firebird 5.0 (tested, protocol v17)

### Files Changed
| File | Type | Description |
|------|------|-------------|
| src/alerter.rs | NEW | POST_EVENT support |
| src/compression.rs | NEW | Wire compression (zlib) |
| src/pool.rs | NEW | Connection pooling |
| src/cellvalue.rs | FIX | Type conversions |
| src/conn_params.rs | FEAT | compress option |
| src/connection.rs | FEAT | Events, transaction options |
| src/error.rs | FEAT | PoolError type |
| src/lib.rs | FEAT | Exports, TPB constants |
| src/param.rs | FIX | SMALLINT BLR, From<String> |
| src/statement.rs | FEAT | ColumnInfo, description(), rowcount() |
| src/transaction.rs | FEAT | IsolationLevel, LockWait |
| src/utils.rs | FIX | Endianness, time conversion |
| src/wirechannel.rs | PERF | TCP_NODELAY, buffers, compression |
| src/wireprotocol.rs | FEAT | Events, compression, protocol v13-17 |
| src/xsqlvar.rs | FIX | SMALLINT, timezone types |
| Cargo.toml | DEP | flate2 |
