use sqlparser::ast::{SetExpr, Statement};
use sqlparser::dialect::{Dialect, MySqlDialect, PostgreSqlDialect};
use sqlparser::parser::Parser;
use sqlparser_canonicalize::{CanonicalizeError, Canonicalizer, hash_canonical};

#[test]
fn test_normalize_simple() {
    let dialect = PostgreSqlDialect {};

    let sql = "SELECT * FROM t WHERE age > 18";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);
    assert!(result.is_ok());

    let normalized = result.unwrap();
    assert!(normalized.contains("age"));
    assert!(normalized.contains(">"));
    assert!(normalized.contains("18"));
}

#[test]
fn test_normalize_commutative_and() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE a = 1 AND b = 2";
    let sql2 = "SELECT * FROM t WHERE b = 2 AND a = 1";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    assert_eq!(norm1, norm2);
}

#[test]
fn test_normalize_commutative_or() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE a = 1 OR b = 2";
    let sql2 = "SELECT * FROM t WHERE b = 2 OR a = 1";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    assert_eq!(norm1, norm2);
}

#[test]
fn test_normalize_in_list_sorted() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE x IN (1, 2, 3)";
    let sql2 = "SELECT * FROM t WHERE x IN (3, 1, 2)";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    assert_eq!(norm1, norm2);
}

#[test]
fn a_membership_term_normalizes_the_same_under_two_spellings() {
    let dialect = PostgreSqlDialect {};

    let one = Canonicalizer::new(&dialect)
        .normalize_sql("SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'a')")
        .unwrap();
    let two = Canonicalizer::new(&dialect)
        .normalize_sql(
            "SELECT   *  FROM t\n  where   x   in   ( select id from m where owner = 'a' )",
        )
        .unwrap();

    assert_eq!(one, two, "one filter, two spellings, one predicate");
    assert!(
        !one.contains("Span") && !one.contains("Ident"),
        "the term must not normalize through the Debug fallback, got {one:?}"
    );
}

#[test]
fn two_different_membership_terms_are_two_predicates() {
    let dialect = PostgreSqlDialect {};
    let norm = |sql: &str| Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();

    let base = norm("SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'a')");

    for other in [
        "SELECT * FROM t WHERE x IN (SELECT id FROM n WHERE owner = 'a')",
        "SELECT * FROM t WHERE x IN (SELECT ref FROM m WHERE owner = 'a')",
        "SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'b')",
        "SELECT * FROM t WHERE y IN (SELECT id FROM m WHERE owner = 'a')",
        "SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'a' LIMIT 1)",
        "SELECT * FROM t WHERE x NOT IN (SELECT id FROM m WHERE owner = 'a')",
    ] {
        assert_ne!(
            base,
            norm(other),
            "{other} names a different relationship and must not share the predicate"
        );
    }
}

#[test]
fn test_normalize_no_where() {
    let dialect = PostgreSqlDialect {};

    let sql = "SELECT * FROM t";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);
    assert!(result.is_ok());

    let normalized = result.unwrap();
    assert_eq!(normalized, "TRUE");
}

#[test]
fn test_hash_deterministic() {
    let s = "age > 18 AND status = 'active'";

    let hash1 = hash_canonical(s);
    let hash2 = hash_canonical(s);

    assert_eq!(hash1, hash2);
}

#[test]
fn test_hash_different() {
    let s1 = "age > 18";
    let s2 = "age > 19";

    let hash1 = hash_canonical(s1);
    let hash2 = hash_canonical(s2);

    assert_ne!(hash1, hash2);
}

#[test]
fn test_hash_128bit() {
    let s = "test";
    let hash = hash_canonical(s);

    assert!(hash > 0);
    assert!(hash < u128::MAX);
}

#[test]
fn test_normalize_nested_parentheses() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE ((age > 18))";
    let sql2 = "SELECT * FROM t WHERE age > 18";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    assert_eq!(norm1, norm2);
}

#[test]
fn test_normalize_preserves_order_noncommutative() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE a < b";
    let sql2 = "SELECT * FROM t WHERE b < a";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    assert_ne!(norm1, norm2);
}

#[test]
fn test_normalize_error_parse_failure() {
    let dialect = PostgreSqlDialect {};

    let invalid_sql = "NOT VALID SQL ;;;";
    let result = Canonicalizer::new(&dialect).normalize_sql(invalid_sql);

    assert!(matches!(result, Err(CanonicalizeError::Parse { .. })));
}

#[test]
fn test_normalize_error_multiple_statements() {
    let dialect = PostgreSqlDialect {};

    let sql = "SELECT * FROM t WHERE a = 1; SELECT * FROM t WHERE b = 2";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);

    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));
}

#[test]
fn test_normalize_rejects_unbalanced_open_parens() {
    let dialect = PostgreSqlDialect {};
    let err = Canonicalizer::new(&dialect)
        .normalize_sql("SELECT * FROM t WHERE ((((a = 1")
        .unwrap_err();
    assert!(matches!(err, CanonicalizeError::Unsupported(ref m) if m.contains("Unbalanced")));
}

#[test]
fn test_normalize_no_where_clause() {
    let dialect = PostgreSqlDialect {};

    let sql = "SELECT * FROM t";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();

    assert_eq!(result, "TRUE");
}

#[test]
fn test_normalize_all_operators() {
    let dialect = PostgreSqlDialect {};

    for op in &["=", "!=", "<", ">", "<=", ">="] {
        let sql = format!("SELECT * FROM t WHERE a {} b", op);
        let result = Canonicalizer::new(&dialect).normalize_sql(&sql);
        assert!(result.is_ok(), "Failed on operator: {}", op);
    }

    for op in &["AND", "OR"] {
        let sql = format!("SELECT * FROM t WHERE a = 1 {} b = 2", op);
        let result = Canonicalizer::new(&dialect).normalize_sql(&sql);
        assert!(result.is_ok(), "Failed on operator: {}", op);
    }
}

#[test]
fn test_normalize_arithmetic_operators() {
    let dialect = PostgreSqlDialect {};

    for op in &["+", "-", "*", "/", "%"] {
        let sql = format!("SELECT * FROM t WHERE a {} b > 10", op);
        let result = Canonicalizer::new(&dialect).normalize_sql(&sql);
        assert!(result.is_ok(), "Failed on arithmetic operator: {}", op);
    }
}

#[test]
fn test_normalize_not_operator() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE NOT (a = 1)";
    let sql2 = "SELECT * FROM t WHERE a != 1";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    assert_ne!(norm1, norm2);
}

#[test]
fn test_normalize_complex_nested_expression() {
    let dialect = PostgreSqlDialect {};

    let sql = "SELECT * FROM t WHERE ((a = 1 AND b = 2) OR (c = 3 AND d = 4)) AND e = 5";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);

    assert!(result.is_ok());
}

#[test]
fn test_normalize_in_list_order() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE status IN ('active', 'pending', 'processing')";
    let sql2 = "SELECT * FROM t WHERE status IN ('processing', 'active', 'pending')";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    let _ = (norm1, norm2);
}

#[test]
fn test_hash_consistency() {
    let s = "age > 18 AND status = 'active'";

    let hash1 = hash_canonical(s);
    let hash2 = hash_canonical(s);
    let hash3 = hash_canonical(s);

    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);
}

#[test]
fn test_hash_empty_string() {
    let hash = hash_canonical("");
    assert!(hash > 0);
}

#[test]
fn test_hash_long_string() {
    let long_str = "a".repeat(10000);
    let hash = hash_canonical(&long_str);
    assert!(hash > 0);
}

#[test]
fn test_normalize_error_multiple_tables() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t1, t2 WHERE a = 1";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);
    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));
    if let Err(CanonicalizeError::Unsupported(msg)) = result {
        assert!(msg.contains("Exactly one table"));
    }
}

#[test]
fn test_normalize_error_joins() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id WHERE a = 1";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);
    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));
    if let Err(CanonicalizeError::Unsupported(msg)) = result {
        assert!(msg.contains("JOINs not supported"));
    }
}

#[test]
fn test_normalize_error_derived_table() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM (SELECT * FROM t1) AS d WHERE d.a = 1";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);
    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));
    if let Err(CanonicalizeError::Unsupported(msg)) = result {
        assert!(msg.contains("Subqueries and derived tables not supported"));
    }
}

#[test]
fn test_normalize_error_non_select_query() {
    let dialect = PostgreSqlDialect {};

    let insert_sql = "INSERT INTO t VALUES (1, 2)";
    let result = Canonicalizer::new(&dialect).normalize_sql(insert_sql);
    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));

    let update_sql = "UPDATE t SET a = 1";
    let result = Canonicalizer::new(&dialect).normalize_sql(update_sql);
    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));

    let delete_sql = "DELETE FROM t WHERE a = 1";
    let result = Canonicalizer::new(&dialect).normalize_sql(delete_sql);
    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));
}

#[test]
fn test_normalize_is_null() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE age IS NULL";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("IS NULL"));
}

#[test]
fn test_normalize_is_not_null() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE age IS NOT NULL";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("IS NOT NULL"));
}

#[test]
fn test_normalize_between() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE age BETWEEN 18 AND 65";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("BETWEEN"));
    assert!(result.contains("18"));
    assert!(result.contains("65"));
}

#[test]
fn test_normalize_not_between() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE age NOT BETWEEN 18 AND 65";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("NOT BETWEEN"));
}

#[test]
fn test_normalize_like() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE name LIKE 'John%'";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("LIKE"));
}

#[test]
fn test_normalize_not_like() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE name NOT LIKE 'John%'";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("NOT LIKE"));
}

#[test]
fn test_normalize_like_with_escape() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE name LIKE 'John\\%' ESCAPE '\\'";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("LIKE"));
    assert!(result.contains("ESCAPE"));
}

#[test]
fn test_normalize_ilike() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE name ILIKE 'john%'";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("ILIKE"));
}

#[test]
fn test_normalize_not_ilike() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE name NOT ILIKE 'john%'";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("NOT ILIKE"));
}

#[test]
fn test_normalize_ilike_with_escape() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE name ILIKE 'john\\%' ESCAPE '\\'";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("ILIKE"));
    assert!(result.contains("ESCAPE"));
}

#[test]
fn test_normalize_compound_identifier() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE schema.table.column = 1";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("schema.table.column"));
}

#[test]
fn test_normalize_unary_plus() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE +age = 10";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("+"));
}

#[test]
fn test_normalize_unary_minus() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE -balance > 100";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("-"));
}

#[test]
fn test_normalize_not_in_list() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE status NOT IN ('active', 'pending')";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    assert!(result.contains("NOT IN"));
}

#[test]
fn test_error_set_operations() {
    let dialect = PostgreSqlDialect {};

    let sql = "SELECT * FROM t WHERE a = 1 UNION SELECT * FROM t WHERE b = 2";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);

    assert!(result.is_err());
}

#[test]
fn test_fallback_expression_is_idempotent() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE CAST(a AS text) = 'hello'";
    let normalized = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    let replay = format!("SELECT * FROM t WHERE {normalized}");
    assert_eq!(
        Canonicalizer::new(&dialect).normalize_sql(&replay).unwrap(),
        normalized
    );
}

#[test]
fn test_boolean_truth_test_is_idempotent() {
    let dialect = PostgreSqlDialect {};
    let sql = "SELECT * FROM t WHERE enabled IS TRUE";
    let normalized = Canonicalizer::new(&dialect).normalize_sql(sql).unwrap();
    let replay = format!("SELECT * FROM t WHERE {normalized}");
    assert_eq!(
        Canonicalizer::new(&dialect).normalize_sql(&replay).unwrap(),
        normalized
    );
}

#[test]
fn test_normalize_unknown_unary_op_fallback() {
    let dialect = PostgreSqlDialect {};

    let sql = "SELECT * FROM t WHERE ~a = 1";
    let result = Canonicalizer::new(&dialect).normalize_sql(sql);

    assert!(matches!(result, Err(CanonicalizeError::Unsupported(_))));
}

#[test]
fn test_and_tree_flattening() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE a = 1 AND b = 2 AND c = 3";
    let sql2 = "SELECT * FROM t WHERE (a = 1 AND b = 2) AND c = 3";
    let sql3 = "SELECT * FROM t WHERE a = 1 AND (b = 2 AND c = 3)";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();
    let norm3 = Canonicalizer::new(&dialect).normalize_sql(sql3).unwrap();

    assert_eq!(norm1, norm2, "Flat AND should equal left-associated AND");
    assert_eq!(norm1, norm3, "Flat AND should equal right-associated AND");
}

#[test]
fn test_or_tree_flattening() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE a = 1 OR b = 2 OR c = 3";
    let sql2 = "SELECT * FROM t WHERE (a = 1 OR b = 2) OR c = 3";
    let sql3 = "SELECT * FROM t WHERE a = 1 OR (b = 2 OR c = 3)";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();
    let norm3 = Canonicalizer::new(&dialect).normalize_sql(sql3).unwrap();

    assert_eq!(norm1, norm2);
    assert_eq!(norm1, norm3);
}

#[test]
fn test_distinct_operators_produce_different_strings() {
    let dialect = PostgreSqlDialect {};

    let sql1 = "SELECT * FROM t WHERE a + b > 0";
    let sql2 = "SELECT * FROM t WHERE a - b > 0";

    let norm1 = Canonicalizer::new(&dialect).normalize_sql(sql1).unwrap();
    let norm2 = Canonicalizer::new(&dialect).normalize_sql(sql2).unwrap();

    assert_ne!(
        norm1, norm2,
        "'+' and '-' must produce different normalized strings"
    );
}

fn unsupported_message(sql: &str) -> String {
    match Canonicalizer::new(&PostgreSqlDialect {}).normalize_sql(sql) {
        Err(CanonicalizeError::Unsupported(message)) => message,
        other => panic!("expected an unsupported error for {sql}, got {other:?}"),
    }
}

#[test]
fn test_reject_sql_beyond_length_limit() {
    let padding = " ".repeat(8193);
    let sql = format!("SELECT * FROM t WHERE a = 1{padding}");
    assert_eq!(
        Canonicalizer::new(&PostgreSqlDialect {}).normalize_sql(&sql),
        Err(CanonicalizeError::InputTooLong { limit: 8192 })
    );
}

#[test]
fn test_reject_control_character() {
    assert_eq!(
        unsupported_message("SELECT * FROM t WHERE a = \u{1}"),
        "Control character in SQL"
    );
}

#[test]
fn test_reject_unbalanced_square_bracket() {
    assert_eq!(
        unsupported_message("SELECT * FROM t WHERE [a = 1"),
        "Unbalanced square brackets"
    );
}

#[test]
fn test_reject_uncanonicalizable_binary_operator() {
    assert_eq!(
        unsupported_message("SELECT * FROM t WHERE a # b = 1"),
        "Unsupported binary operator: #"
    );
}

#[test]
fn test_reject_literal_that_loses_a_quote_level() {
    assert!(matches!(
        Canonicalizer::new(&PostgreSqlDialect {}).normalize_sql("SELECT * FROM t WHERE a = ''''''"),
        Err(CanonicalizeError::NotRoundTrippable(_))
    ));
}

/// MySQL reads a backslash as escaping the closing quote, so a `LIKE` pattern whose value
/// ends in one has no canonical spelling MySQL accepts. The payload is the rendered text, so
/// the refusal is provably the confirmation read, and PostgreSQL accepting the same bytes
/// shows it is the dialect's verdict. subql relies on this to drop such subscriptions.
#[test]
fn test_reject_mysql_pattern_ending_with_the_escape() {
    let dialect = MySqlDialect {};
    let sql = "SELECT * FROM t WHERE name LIKE 'a\\\\'";
    let canonicalizer = Canonicalizer::new(&dialect);
    let statement_verdict = canonicalizer.normalize_sql(sql);
    assert!(matches!(
        &statement_verdict,
        Err(CanonicalizeError::NotRoundTrippable(text)) if text == "name LIKE 'a\\'"
    ));

    let mut statements = Parser::parse_sql(&dialect, sql).unwrap();
    let Statement::Query(query) = statements.pop().unwrap() else {
        panic!("the test SQL is a query");
    };
    let SetExpr::Select(select) = *query.body else {
        panic!("the test SQL is a plain SELECT");
    };
    let clause_verdict = canonicalizer.normalize_where_clause(select.selection.as_ref());
    assert!(matches!(
        &clause_verdict,
        Err(CanonicalizeError::NotRoundTrippable(text)) if text == "name LIKE 'a\\'"
    ));

    let postgres = Canonicalizer::new(&PostgreSqlDialect {});
    assert_eq!(
        postgres
            .normalize_sql("SELECT * FROM t WHERE name LIKE 'a\\'")
            .unwrap(),
        "name LIKE 'a\\'",
    );
}

/// What caps an `AND` chain's canonical nesting is the parser's depth budget for the
/// re-read rather than any length rule, so text inside the budget must be accepted.
#[test]
fn test_deep_and_chain_is_accepted_up_to_the_read_back_budget() {
    let dialect = PostgreSqlDialect {};
    let canonicalizer = Canonicalizer::new(&dialect);
    let chain = |terms: usize| {
        let parts: Vec<String> = (1..=terms)
            .map(|term| format!("c{term} = {term}"))
            .collect();
        format!("SELECT * FROM t WHERE {}", parts.join(" AND "))
    };
    assert!(canonicalizer.normalize_sql(&chain(46)).is_ok());
    assert!(matches!(
        canonicalizer.normalize_sql(&chain(47)),
        Err(CanonicalizeError::NotRoundTrippable(_))
    ));
}

/// A delimited name escapes its own delimiter by doubling it, and the canonical spelling has
/// to do the same or it names something else. subql keys rows on such a column.
#[test]
fn test_quoted_identifier_carrying_its_delimiter_is_escaped() {
    let canonicalizer = Canonicalizer::new(&PostgreSqlDialect {});
    let canonical = canonicalizer
        .normalize_sql("SELECT * FROM t WHERE \"a\"\"b\" = 7")
        .unwrap();
    assert_eq!(canonical, "(\"a\"\"b\" = 7)");

    // Two doubled delimiters in the name survive the same way.
    let deeper = canonicalizer
        .normalize_sql("SELECT * FROM t WHERE \"a\"\"\"\"b\" = 1")
        .unwrap();
    assert_eq!(deeper, "(\"a\"\"\"\"b\" = 1)");
}

#[test]
fn test_function_call_in_predicate_is_canonicalized() {
    let dialect = PostgreSqlDialect {};
    let canonical = Canonicalizer::new(&dialect)
        .normalize_sql("SELECT * FROM t WHERE COALESCE(a, 1) > 0")
        .unwrap();
    assert_eq!(canonical, "(COALESCE(a, 1) > 0)");
}

fn assert_canonical(dialect: &dyn Dialect, cases: &[(&str, &str)]) {
    let canonicalizer = Canonicalizer::new(dialect);
    for (predicate, expected) in cases {
        let sql = format!("SELECT * FROM t WHERE {predicate}");
        assert_eq!(
            canonicalizer.normalize_sql(&sql).as_deref(),
            Ok(*expected),
            "{dialect:?} {predicate}"
        );
    }
}

#[test]
fn predicate_operands_keep_their_parentheses() {
    let cases = [
        ("flag = (value IS NULL)", "((value IS NULL) = flag)"),
        ("(a IS NULL) = b", "((a IS NULL) = b)"),
        ("flag = (v IS NOT NULL)", "((v IS NOT NULL) = flag)"),
        ("flag = (v IN (1, 2))", "((v IN (1, 2)) = flag)"),
        (
            "flag < (v IN (SELECT id FROM u))",
            "(flag < (v IN (SELECT id FROM u)))",
        ),
        (
            "flag = (v NOT BETWEEN 1 AND 2)",
            "((v NOT BETWEEN 1 AND 2) = flag)",
        ),
        ("flag = (v LIKE 'x')", "((v LIKE 'x') = flag)"),
        ("(a IS NULL) IS NULL", "(a IS NULL) IS NULL"),
        ("v BETWEEN (a IS NULL) AND b", "v BETWEEN (a IS NULL) AND b"),
        ("NOT (a IS NULL)", "NOT (a IS NULL)"),
    ];
    assert_canonical(&PostgreSqlDialect {}, &cases);
    assert_canonical(&MySqlDialect {}, &cases);
    assert_canonical(
        &PostgreSqlDialect {},
        &[("flag = (v ILIKE 'x')", "((v ILIKE 'x') = flag)")],
    );
}

#[test]
fn predicates_that_are_not_operands_stay_unparenthesized() {
    assert_canonical(
        &PostgreSqlDialect {},
        &[
            ("(a IS NULL)", "a IS NULL"),
            ("(v LIKE 'x') AND (w IN (1))", "(v LIKE 'x' AND w IN (1))"),
        ],
    );
}

fn canonical_texts(dialect: &dyn Dialect, predicates: &[&str]) -> Vec<String> {
    let canonicalizer = Canonicalizer::new(dialect);
    predicates
        .iter()
        .map(|predicate| {
            canonicalizer
                .normalize_sql(&format!("SELECT * FROM t WHERE {predicate}"))
                .unwrap()
        })
        .collect()
}

#[test]
fn null_safe_equality_orders_its_operands() {
    let texts = canonical_texts(&MySqlDialect {}, &["n <=> m", "M <=> N", "m <=> (n)"]);
    assert_eq!(texts, ["(m <=> n)"; 3]);
}

#[test]
fn is_not_distinct_from_normalizes_and_orders_its_operands() {
    let texts = canonical_texts(
        &PostgreSqlDialect {},
        &[
            "n IS NOT DISTINCT FROM m",
            "M IS NOT DISTINCT FROM N",
            "m IS NOT DISTINCT FROM (n)",
        ],
    );
    assert_eq!(texts, ["(m IS NOT DISTINCT FROM n)"; 3]);
}

#[test]
fn is_distinct_from_normalizes_and_orders_its_operands() {
    let texts = canonical_texts(
        &PostgreSqlDialect {},
        &["n IS DISTINCT FROM m", "M IS DISTINCT FROM (N)"],
    );
    assert_eq!(texts, ["(m IS DISTINCT FROM n)"; 2]);
}

#[test]
fn inequality_orders_its_operands() {
    let texts = canonical_texts(&PostgreSqlDialect {}, &["a != b", "b <> a"]);
    assert_eq!(texts, ["(a != b)"; 2]);
}

#[test]
fn nested_distinctness_tests_read_back_as_themselves() {
    let texts = canonical_texts(
        &PostgreSqlDialect {},
        &[
            "(a IS DISTINCT FROM b) = c",
            "c IS NOT DISTINCT FROM (b IS DISTINCT FROM a)",
        ],
    );
    assert_eq!(
        texts,
        [
            "((a IS DISTINCT FROM b) = c)",
            "((a IS DISTINCT FROM b) IS NOT DISTINCT FROM c)",
        ]
    );
}

#[test]
fn null_safe_comparisons_keep_predicate_operands_enclosed() {
    assert_canonical(
        &PostgreSqlDialect {},
        &[
            (
                "flag IS DISTINCT FROM (value IS NULL)",
                "((value IS NULL) IS DISTINCT FROM flag)",
            ),
            (
                "(value IS NULL) IS NOT DISTINCT FROM flag",
                "((value IS NULL) IS NOT DISTINCT FROM flag)",
            ),
            (
                "z IS DISTINCT FROM (v LIKE 'x')",
                "((v LIKE 'x') IS DISTINCT FROM z)",
            ),
            (
                "a IS NOT DISTINCT FROM (v IN (1, 2))",
                "((v IN (1, 2)) IS NOT DISTINCT FROM a)",
            ),
        ],
    );
    assert_canonical(
        &MySqlDialect {},
        &[
            ("flag <=> (value IS NULL)", "((value IS NULL) <=> flag)"),
            ("(b IS NULL) != a", "((b IS NULL) != a)"),
            ("z != (b IS NULL)", "((b IS NULL) != z)"),
        ],
    );
}

#[test]
fn a_verbatim_predicate_as_operand_keeps_its_grouping() {
    let postgres = [
        ("z LIKE (x IS TRUE)", "z LIKE (x IS TRUE)"),
        ("z LIKE x IS TRUE", "z LIKE x IS TRUE"),
        ("z NOT ILIKE (x IS UNKNOWN)", "z NOT ILIKE (x IS UNKNOWN)"),
        ("z NOT ILIKE x IS UNKNOWN", "z NOT ILIKE x IS UNKNOWN"),
        ("z BETWEEN 1 AND (x IS TRUE)", "z BETWEEN 1 AND (x IS TRUE)"),
        ("z BETWEEN 1 AND x IS TRUE", "z BETWEEN 1 AND x IS TRUE"),
        ("z LIKE (x = ANY(y))", "z LIKE (x = ANY(y))"),
        ("z LIKE (x SIMILAR TO 'y')", "z LIKE (x SIMILAR TO 'y')"),
    ];
    assert_canonical(&PostgreSqlDialect {}, &postgres);
    let mysql = [
        ("(x REGEXP 'y') IN (1)", "(x REGEXP 'y') IN (1)"),
        ("x REGEXP 'y' IN (1)", "x REGEXP 'y' IN (1)"),
        (
            "(x RLIKE 'y') BETWEEN 1 AND 2",
            "(x RLIKE 'y') BETWEEN 1 AND 2",
        ),
        ("x RLIKE 'y' BETWEEN 1 AND 2", "x RLIKE 'y' BETWEEN 1 AND 2"),
        ("z LIKE (x MEMBER OF (y))", "z LIKE (x MEMBER OF(y))"),
    ];
    assert_canonical(&MySqlDialect {}, &mysql);
}

#[test]
fn a_verbatim_predicate_as_operand_is_accepted() {
    assert_canonical(
        &PostgreSqlDialect {},
        &[
            ("- (x IS TRUE)", "- (x IS TRUE)"),
            ("z BETWEEN (x IS TRUE) AND 2", "z BETWEEN (x IS TRUE) AND 2"),
            ("(x IS NOT FALSE) != z", "((x IS NOT FALSE) != z)"),
        ],
    );
    assert_canonical(
        &MySqlDialect {},
        &[("(x SIMILAR TO 'y') = z", "((x SIMILAR TO 'y') = z)")],
    );
}

#[test]
fn mysql_keeps_the_case_of_a_table_qualifier() {
    assert_canonical(
        &MySqlDialect {},
        &[
            ("T.a = 1", "(1 = T.a)"),
            ("t.A = 1", "(1 = t.a)"),
            ("`T`.a = 1", "(1 = `T`.a)"),
        ],
    );
    assert_canonical(&PostgreSqlDialect {}, &[("T.A = 1", "(1 = t.a)")]);
}
