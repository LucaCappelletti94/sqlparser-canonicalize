use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser_canonicalize::{hash_canonical, normalize_sql};

const CORPUS: &[&str] = &[
    "SELECT * FROM t",
    "SELECT * FROM t WHERE age > 18",
    "SELECT * FROM t WHERE a = 1 AND b = 2",
    "SELECT * FROM t WHERE a = 1 OR b = 2 OR c = 3",
    "SELECT * FROM t WHERE x IN (3, 1, 2)",
    "SELECT * FROM t WHERE age BETWEEN 18 AND 65",
    "SELECT * FROM t WHERE name LIKE 'A%'",
    "SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'a')",
    "SELECT SUM(amount) FROM t WHERE amount > 10",
    "SELECT region, SUM(amount) FROM orders WHERE active = TRUE GROUP BY region HAVING COUNT(*) > 2",
];

fn normalize_and_hash(c: &mut Criterion) {
    let dialect = PostgreSqlDialect {};
    c.bench_function("normalize_and_hash_corpus", |b| {
        b.iter(|| {
            for sql in CORPUS {
                let normalized = normalize_sql(sql, &dialect).unwrap();
                black_box(hash_canonical(&normalized));
            }
        });
    });
}

criterion_group!(benches, normalize_and_hash);
criterion_main!(benches);
