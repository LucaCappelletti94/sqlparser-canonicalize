use proptest::prelude::*;
use sqlparser::ast::{SetExpr, Statement};
use sqlparser::dialect::{
    AnsiDialect, Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;
use sqlparser_canonicalize::Canonicalizer;

fn normalize(sql: &str) -> String {
    Canonicalizer::new(&PostgreSqlDialect {})
        .normalize_sql(sql)
        .unwrap()
}

fn assert_idempotent(canonical: &str) {
    let sql = if canonical == "TRUE" {
        "SELECT * FROM t".to_string()
    } else {
        format!("SELECT * FROM t WHERE {canonical}")
    };
    assert_eq!(normalize(&sql), canonical);
}

fn term(identifier: &str, value: u16) -> String {
    format!("{identifier} = {value}")
}

proptest! {
    #[test]
    fn commuted_and_or_chains(
        identifiers in prop::collection::btree_set("col_[a-z]{1,7}", 3),
        values in prop::array::uniform3(any::<u16>()),
        operator in prop_oneof![Just("AND"), Just("OR")],
    ) {
        let identifiers: Vec<_> = identifiers.into_iter().collect();
        let terms = [
            term(&identifiers[0], values[0]),
            term(&identifiers[1], values[1]),
            term(&identifiers[2], values[2]),
        ];
        let first = normalize(&format!(
            "SELECT * FROM t WHERE {} {operator} {} {operator} {}",
            terms[0], terms[1], terms[2]
        ));
        let second = normalize(&format!(
            "SELECT * FROM t WHERE {} {operator} {} {operator} {}",
            terms[2], terms[0], terms[1]
        ));
        prop_assert_eq!(&first, &second);
        assert_idempotent(&first);
    }

    #[test]
    fn reassociated_and_or_chains(
        identifiers in prop::collection::btree_set("col_[a-z]{1,7}", 3),
        values in prop::array::uniform3(any::<u16>()),
        operator in prop_oneof![Just("AND"), Just("OR")],
    ) {
        let identifiers: Vec<_> = identifiers.into_iter().collect();
        let terms = [
            term(&identifiers[0], values[0]),
            term(&identifiers[1], values[1]),
            term(&identifiers[2], values[2]),
        ];
        let left = normalize(&format!(
            "SELECT * FROM t WHERE ({} {operator} {}) {operator} {}",
            terms[0], terms[1], terms[2]
        ));
        let right = normalize(&format!(
            "SELECT * FROM t WHERE {} {operator} ({} {operator} {})",
            terms[0], terms[1], terms[2]
        ));
        prop_assert_eq!(&left, &right);
        assert_idempotent(&left);
    }

    #[test]
    fn keyword_case_and_whitespace_are_equivalent(
        identifier in "col_[a-z]{1,7}",
        value in any::<u16>(),
        spaces in 1usize..8,
        select in prop_oneof![Just("SELECT"), Just("select"), Just("SeLeCt")],
        from in prop_oneof![Just("FROM"), Just("from"), Just("FrOm")],
        where_keyword in prop_oneof![Just("WHERE"), Just("where"), Just("WhErE")],
    ) {
        let gap = " ".repeat(spaces);
        let varied = format!(
            "{select}{gap}*{gap}{from}{gap}t{gap}{where_keyword}{gap}{identifier}{gap}={gap}{value}"
        );
        let standard = format!("SELECT * FROM t WHERE {identifier} = {value}");
        let first = normalize(&varied);
        let second = normalize(&standard);
        prop_assert_eq!(&first, &second);
        assert_idempotent(&first);
    }

    #[test]
    fn distinct_literals_remain_distinct(
        identifier in "col_[a-z]{1,7}",
        first in 0u16..32_768,
        increment in 1u16..32_768,
    ) {
        let second = first + increment;
        let first = normalize(&format!("SELECT * FROM t WHERE {identifier} = {first}"));
        let second = normalize(&format!("SELECT * FROM t WHERE {identifier} = {second}"));
        prop_assert_ne!(first, second);
    }

    #[test]
    fn distinct_quoted_identifiers_remain_distinct(
        suffix in "[a-z]{1,7}",
        value in any::<u16>(),
    ) {
        let first = normalize(&format!(
            "SELECT * FROM t WHERE \"A{suffix}\" = {value}"
        ));
        let second = normalize(&format!(
            "SELECT * FROM t WHERE \"a{suffix}\" = {value}"
        ));
        prop_assert_ne!(first, second);
    }

    #[test]
    fn generated_canonical_forms_are_idempotent(
        identifier in "col_[a-z]{1,7}",
        first in any::<u16>(),
        second in any::<u16>(),
    ) {
        let canonical = normalize(&format!(
            "SELECT * FROM t WHERE {identifier} = {first} AND score > {second}"
        ));
        assert_idempotent(&canonical);
    }
}

#[test]
fn distinct_operators_remain_distinct() {
    let less = normalize("SELECT * FROM t WHERE a < b");
    let greater = normalize("SELECT * FROM t WHERE a > b");
    assert_ne!(less, greater);
}

#[test]
fn unsafe_quoted_identifiers_are_idempotent() {
    let dialect = MySqlDialect {};
    for sql in [
        "SELECT * FROM orders WHERE `-tatus` = 'paid'",
        "SELECT * FROM orders WHERE `any` = 'paid'",
    ] {
        let canonical = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
        let replay = format!("SELECT * FROM t WHERE {canonical}");
        assert_eq!(
            Canonicalizer::new(&dialect).normalize_sql(&replay).unwrap(),
            canonical
        );
    }
}

#[test]
fn nested_negation_is_idempotent() {
    let dialect = PostgreSqlDialect {};
    for sql in [
        "SELECT * FROM t WHERE (NOT a) = b",
        "SELECT * FROM t WHERE (NOT a) IS NULL",
        "SELECT * FROM t WHERE (NOT a) IN (b, c)",
        "SELECT * FROM t WHERE (NOT a) BETWEEN b AND c",
    ] {
        let canonical = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
        let replay = format!("SELECT * FROM t WHERE {canonical}");
        assert_eq!(
            Canonicalizer::new(&dialect).normalize_sql(&replay).unwrap(),
            canonical,
            "{sql}"
        );
    }
}

#[test]
fn predicates_whose_canonical_text_would_not_read_back_are_rejected() {
    let dialect = PostgreSqlDialect {};
    for sql in [
        // The parser drops one level of quote doubling every time it prints this literal.
        "SELECT * FROM t WHERE ''''''",
        // The parser prints this field access without the space it needs to be read back.
        "SELECT * FROM t WHERE CASE WHEN a = 1 THEN b ELSE c END . 2",
    ] {
        assert!(
            Canonicalizer::new(&dialect).normalize_sql(sql).is_err(),
            "{sql}"
        );
    }
}

/// Without the parser's stack protection this input aborts the process rather than returning.
#[cfg(feature = "std")]
#[test]
fn deeply_nested_negation_is_refused_rather_than_fatal() {
    let sql = format!("SELECT * FROM t WHERE {}a", "NOT ".repeat(1024));
    assert!(sql.len() < 8192);
    assert!(
        Canonicalizer::new(&PostgreSqlDialect {})
            .normalize_sql(&sql)
            .is_err()
    );
}

/// PostgreSQL folds an unquoted name to lower case and leaves a quoted one alone, so these
/// two spell different columns and must not share a key.
#[test]
fn postgres_keeps_a_quoted_name_distinct_from_its_unquoted_spelling() {
    let dialect = PostgreSqlDialect {};
    let quoted = Canonicalizer::new(&dialect)
        .normalize_sql("SELECT * FROM t WHERE \"Status\" = 'paid'")
        .unwrap();
    let bare = Canonicalizer::new(&dialect)
        .normalize_sql("SELECT * FROM t WHERE Status = 'paid'")
        .unwrap();
    assert_ne!(quoted, bare);
}

/// The same two spellings do mean one column where the dialect ignores case, so there they
/// must share a key rather than register twice.
#[test]
fn case_insensitive_dialects_merge_the_two_spellings() {
    for sql in [
        "SELECT * FROM t WHERE \"MyCol\" = 'paid'",
        "SELECT * FROM t WHERE MyCoL = 'paid'",
    ] {
        assert_eq!(
            Canonicalizer::new(&SQLiteDialect {})
                .normalize_sql(sql)
                .unwrap(),
            Canonicalizer::new(&SQLiteDialect {})
                .normalize_sql("SELECT * FROM t WHERE mycol = 'paid'")
                .unwrap(),
            "{sql}"
        );
    }
}

/// A quoted name that is also a SQL keyword keeps its quotes, so it does not share a key with
/// its unquoted spelling. That splits one column into two entries, which is the safe way to
/// be wrong, and it avoids emitting a bare word the parser may read as a keyword.
#[test]
fn a_quoted_keyword_name_keeps_its_quotes() {
    let dialect = SQLiteDialect {};
    assert_eq!(
        Canonicalizer::new(&dialect)
            .normalize_sql("SELECT * FROM t WHERE \"status\" = 1")
            .unwrap(),
        "(\"status\" = 1)"
    );
    assert_eq!(
        Canonicalizer::new(&dialect)
            .normalize_sql("SELECT * FROM t WHERE status = 1")
            .unwrap(),
        "(1 = status)"
    );
}

/// A name PostgreSQL would fold keeps its quotes, and one it would leave alone loses them,
/// so every accepted spelling is the one the database resolves to.
#[test]
fn postgres_quotes_only_names_that_folding_would_change() {
    let dialect = PostgreSqlDialect {};
    assert_eq!(
        Canonicalizer::new(&dialect)
            .normalize_sql("SELECT * FROM t WHERE \"Status\" = 1")
            .unwrap(),
        "(\"Status\" = 1)"
    );
    // `status` is a SQL keyword, so the quoted spelling keeps its quotes even though folding
    // alone would not have needed them.
    assert_eq!(
        Canonicalizer::new(&dialect)
            .normalize_sql("SELECT * FROM t WHERE \"status\" = 1")
            .unwrap(),
        "(\"status\" = 1)"
    );
    // A name that is not a keyword does lose them, and so shares a key with the bare spelling.
    assert_eq!(
        Canonicalizer::new(&dialect)
            .normalize_sql("SELECT * FROM t WHERE \"mycol\" = 1")
            .unwrap(),
        "(1 = mycol)"
    );
    assert_eq!(
        Canonicalizer::new(&dialect)
            .normalize_sql("SELECT * FROM t WHERE Status = 1")
            .unwrap(),
        "(1 = status)"
    );
}

/// The standard folds an unquoted name upward, so the quoted upper-case spelling is the one
/// that needs no quotes, the mirror image of PostgreSQL.
#[test]
fn ansi_folds_unquoted_names_upward() {
    let canonicalizer = Canonicalizer::new(&AnsiDialect {});
    assert_eq!(
        canonicalizer
            .normalize_sql("SELECT * FROM t WHERE mycol = 1")
            .unwrap(),
        "(1 = MYCOL)"
    );
    assert_eq!(
        canonicalizer
            .normalize_sql("SELECT * FROM t WHERE \"MYCOL\" = 1")
            .unwrap(),
        "(1 = MYCOL)"
    );
    // Lower case survived quoting, so it is a different name and keeps its quotes.
    assert_eq!(
        canonicalizer
            .normalize_sql("SELECT * FROM t WHERE \"mycol\" = 1")
            .unwrap(),
        "(\"mycol\" = 1)"
    );
}

/// A dialect whose folding rule the crate does not know keeps every name exactly as written,
/// which never merges two spellings and so never merges two columns.
#[test]
fn an_unknown_dialect_keeps_names_exactly() {
    let canonicalizer = Canonicalizer::new(&GenericDialect {});
    assert_eq!(
        canonicalizer
            .normalize_sql("SELECT * FROM t WHERE \"MyCol\" = 1")
            .unwrap(),
        "(\"MyCol\" = 1)"
    );
    assert_eq!(
        canonicalizer
            .normalize_sql("SELECT * FROM t WHERE MyCol = 1")
            .unwrap(),
        "(1 = MyCol)"
    );
}

/// MySQL delimits with a backtick, and a name carrying one doubles it the same way.
#[test]
fn mysql_escapes_a_backtick_inside_a_name() {
    assert_eq!(
        Canonicalizer::new(&MySqlDialect {})
            .normalize_sql("SELECT * FROM t WHERE `a``b` = 1")
            .unwrap(),
        "(1 = `a``b`)"
    );
}

/// The clause entry point answers exactly as the statement one, since subql reaches the crate
/// through it after resolving its own placeholders.
#[test]
fn the_clause_entry_point_agrees_with_the_statement_one() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE b = 2 AND a = 1";
    let statement = Parser::parse_sql(&dialect, sql).unwrap().pop().unwrap();
    let Statement::Query(query) = statement else {
        panic!("the test SQL is a query");
    };
    let SetExpr::Select(select) = *query.body else {
        panic!("the test SQL is a plain SELECT");
    };

    let canonicalizer = Canonicalizer::new(&dialect as &dyn Dialect);
    assert_eq!(
        canonicalizer
            .normalize_where_clause(select.selection.as_ref())
            .unwrap(),
        canonicalizer.normalize_sql(sql).unwrap()
    );
    // No clause at all is the same sentinel a filterless statement produces.
    assert_eq!(
        canonicalizer.normalize_where_clause(None).unwrap(),
        canonicalizer.normalize_sql("SELECT * FROM t").unwrap()
    );
}
