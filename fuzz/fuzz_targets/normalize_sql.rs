#![no_main]

use libfuzzer_sys::fuzz_target;
use sqlparser::dialect::{Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser_canonicalize::normalize_sql;

fn exercise(sql: &str, dialect: &dyn Dialect) {
    let Ok(canonical) = normalize_sql(sql, dialect) else {
        return;
    };
    let replay = if canonical == "TRUE" {
        "SELECT * FROM t".to_string()
    } else {
        format!("SELECT * FROM t WHERE {canonical}")
    };
    assert_eq!(normalize_sql(&replay, dialect).unwrap(), canonical);
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, bytes)) = data.split_first() else {
        return;
    };
    let Ok(sql) = core::str::from_utf8(bytes) else {
        return;
    };
    match selector % 3 {
        0 => exercise(sql, &PostgreSqlDialect {}),
        1 => exercise(sql, &MySqlDialect {}),
        _ => exercise(sql, &SQLiteDialect {}),
    }
});
