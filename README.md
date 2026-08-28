# sqlparser-canonicalize

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
