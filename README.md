# sqlparser-canonicalize

[![CI](https://github.com/LucaCappelletti94/sqlparser-canonicalize/actions/workflows/ci.yml/badge.svg)](https://github.com/LucaCappelletti94/sqlparser-canonicalize/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/LucaCappelletti94/sqlparser-canonicalize/branch/main/graph/badge.svg)](https://codecov.io/gh/LucaCappelletti94/sqlparser-canonicalize)
[![Crates.io](https://img.shields.io/crates/v/sqlparser-canonicalize.svg)](https://crates.io/crates/sqlparser-canonicalize)
[![Docs](https://docs.rs/sqlparser-canonicalize/badge.svg)](https://docs.rs/sqlparser-canonicalize)
[![License](https://img.shields.io/crates/l/sqlparser-canonicalize.svg)](LICENSE)

`sqlparser-canonicalize` produces canonical predicate text and stable hashes from `sqlparser` syntax trees. Equivalent predicate spellings produce identical bytes for durable deduplication.

```rust
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser_canonicalize::normalize_sql;

let normalized = normalize_sql(
    "SELECT * FROM orders WHERE age > 18",
    &PostgreSqlDialect {},
)?;
assert_eq!(normalized, "(age > 18)");
# Ok::<(), sqlparser_canonicalize::CanonicalizeError>(())
```

The crate supports `no_std` with allocation. Normalization is O(n log n) for `AND` and `OR` chains because operands are sorted, and O(n) for other syntax tree shapes.
