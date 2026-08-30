#![no_main]

use libfuzzer_sys::fuzz_target;
use sqlparser::ast::{SetExpr, Statement};
use sqlparser::dialect::{
    AnsiDialect, Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;
use sqlparser_canonicalize::Canonicalizer;

fn dialect(selector: u8) -> &'static dyn Dialect {
    match selector % 5 {
        0 => &PostgreSqlDialect {},
        1 => &MySqlDialect {},
        2 => &SQLiteDialect {},
        3 => &GenericDialect {},
        _ => &AnsiDialect {},
    }
}

fuzz_target!(|data: &[u8]| {
    let [parse_selector, verify_selector, sql @ ..] = data else {
        return;
    };
    let Ok(predicate) = core::str::from_utf8(sql) else {
        return;
    };
    let parse_dialect = dialect(*parse_selector);
    // The verification dialect is chosen independently, because the public entry point takes
    // it from the caller and nothing forces it to be the one that parsed the expression.
    let verify_dialect = dialect(*verify_selector);

    let statement = format!("SELECT * FROM t WHERE {predicate}");
    let Ok(mut statements) = Parser::parse_sql(parse_dialect, &statement) else {
        return;
    };
    let Some(Statement::Query(query)) = statements.pop() else {
        return;
    };
    let SetExpr::Select(select) = *query.body else {
        return;
    };

    let Ok(canonical) =
        Canonicalizer::new(verify_dialect).normalize_where_clause(select.selection.as_ref())
    else {
        return;
    };
    let replay = if canonical == "TRUE" {
        "SELECT * FROM t".to_string()
    } else {
        format!("SELECT * FROM t WHERE {canonical}")
    };
    assert_eq!(
        Canonicalizer::new(verify_dialect)
            .normalize_sql(&replay)
            .unwrap(),
        canonical,
        "accepted canonical text must read back as itself"
    );
});
