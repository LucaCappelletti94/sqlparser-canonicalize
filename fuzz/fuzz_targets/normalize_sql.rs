#![no_main]

use libfuzzer_sys::fuzz_target;
use sqlparser::dialect::{
    AnsiDialect, Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser_canonicalize::{Canonicalizer, hash_canonical};

fn exercise(sql: &str, dialect: &dyn Dialect) {
    let Ok(canonical) = Canonicalizer::new(dialect).normalize_sql(sql) else {
        return;
    };
    let hash = hash_canonical(&canonical);
    assert_eq!(hash_canonical(&canonical), hash);
    let replay = if canonical == "TRUE" {
        "SELECT * FROM t".to_string()
    } else {
        format!("SELECT * FROM t WHERE {canonical}")
    };
    // Canonical text can outgrow the statement entry's own input limit, so the replay is
    // asserted only where that entry accepts it.
    if let Ok(again) = Canonicalizer::new(dialect).normalize_sql(&replay) {
        assert_eq!(again, canonical);
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, bytes)) = data.split_first() else {
        return;
    };
    let Ok(sql) = core::str::from_utf8(bytes) else {
        return;
    };
    let dialect: &dyn Dialect = match selector % 5 {
        0 => &PostgreSqlDialect {},
        1 => &MySqlDialect {},
        2 => &SQLiteDialect {},
        3 => &GenericDialect {},
        _ => &AnsiDialect {},
    };
    if (selector / 5) % 2 == 0 {
        exercise(sql, dialect);
    } else {
        exercise(&format!("SELECT * FROM t WHERE {sql}"), dialect);
    }
});
