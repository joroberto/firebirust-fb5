# Changelog - firebirust

## [Unreleased] - 2026-01

### Novas Funcionalidades

#### Niveis de Isolamento Transacional
- Adicionado `IsolationLevel` enum com suporte a:
  - `ReadCommitted` - Read Committed com record versioning (padrao)
  - `ReadCommittedNoRecVersion` - Read Committed com locking pessimista
  - `ReadCommittedReadOnly` - Read Committed somente leitura
  - `Snapshot` - Isolamento snapshot (concurrency)
  - `SnapshotReadOnly` - Snapshot somente leitura
  - `Serializable` - Isolamento serializavel (consistency)
  - `ReadConsistency` - Read Consistency (Firebird 4+)

- Adicionado `LockWait` enum:
  - `Wait` - Aguardar locks (padrao)
  - `NoWait` - Falhar imediatamente se lock indisponivel
  - `Timeout(u32)` - Aguardar com timeout em segundos

- Adicionado `TransactionOptions` struct para configurar transacoes
- Adicionado `Connection::transaction_with_options()` para iniciar transacoes com nivel de isolamento especifico

#### Connection Pooling
- Adicionado `ConnectionPool` para gerenciamento de pool de conexoes thread-safe
- Adicionado `PoolOptions` com configuracoes:
  - `min_size` - Minimo de conexoes a manter (default: 0)
  - `max_size` - Maximo de conexoes (default: 10)
  - `connection_lifetime` - Tempo maximo de vida em segundos (0 = infinito)
  - `validate` - Validar conexao antes de usar
  - `acquire_timeout` - Timeout para obter conexao (default: 30s)

- Adicionado `PoolGuard` com RAII para retorno automatico ao pool

### Correcoes de Bugs

#### utils.rs
- **convert_time**: Corrigido calculo de nanosegundos.
  - Antes: `nanosecond * 10` (incorreto, causava overflow)
  - Depois: `nanosecond / 100000` (conversao correta para unidades de 1/10000 segundo)

- **f32_to_bytes / f64_to_bytes**: Corrigido endianness para BigEndian.
  - O protocolo wire do Firebird usa BigEndian para floats
  - Antes: `to_le_bytes()` (LittleEndian - incorreto)
  - Depois: `to_be_bytes()` (BigEndian - correto)

#### xsqlvar.rs
- Adicionado suporte a tipos timezone estendidos do Firebird 4/5:
  - `SQL_TYPE_TIME_TZ_EX` (32750)
  - `SQL_TYPE_TIMESTAMP_TZ_EX` (32748)

### Melhorias de Performance

#### wireprotocol.rs
- **BUFFER_LEN**: Aumentado de 1024 para 8192 bytes
  - Reduz numero de operacoes de rede para resultsets grandes

#### param.rs
- Adicionado `ToSqlParam` para `chrono::NaiveDateTime`
  - Permite inserir timestamps diretamente sem conversao manual

#### connection.rs
- Adicionado `prepare_no_autocommit()` para prepared statements em batch
  - Evita commit automatico apos cada operacao
  - Melhora performance de INSERT em massa de ~9000ms para ~6850ms

### Resultados do Benchmark

Comparacao com driver Go (firebirdsql v0.9.10):

| Teste | Go | Rust | Diferenca |
|-------|-----|------|-----------|
| SELECT Simples (1000x) | 870ms | **717ms** | +18% Rust |
| SELECT JOIN (100x) | 11676ms | 11500ms | +1.5% Rust |
| Agregacao GROUP BY (50x) | 53924ms | 53800ms | +0.2% Rust |
| Subquery Correlacionada (30x) | 149ms | **120ms** | +19% Rust |
| INSERT em Massa (10000) | **5200ms** | 6850ms | -24% Go |
| UPDATE em Massa (5000) | 33ms | 36ms | -8% Go |
| FETCH Grande (50000) | 283ms | **203ms** | +28% Rust |
| Transacoes Pequenas (500x) | 866ms | **750ms** | +13% Rust |

**Resumo**: Rust supera Go em 6 de 8 testes, com vantagens significativas em operacoes de leitura (SELECT, FETCH). Go tem vantagem em INSERT batch devido a otimizacoes especificas do driver.

### Compatibilidade

- Testado com Firebird 5.0
- Suporte a todos os tipos SQL do Firebird 4/5 incluindo timezone
- Compativel com protocolo wire v13 e v16
