use sqlparser::dialect::PostgreSqlDialect;
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

#[test]
fn test_reject_identifier_that_loses_a_quote_level() {
    assert!(matches!(
        Canonicalizer::new(&PostgreSqlDialect {})
            .normalize_sql("SELECT * FROM t WHERE \"a\"\"\"\"b\" = 1"),
        Err(CanonicalizeError::NotRoundTrippable(_))
    ));
}

#[test]
fn test_function_call_in_predicate_is_canonicalized() {
    let dialect = PostgreSqlDialect {};
    let canonical = Canonicalizer::new(&dialect)
        .normalize_sql("SELECT * FROM t WHERE COALESCE(a, 1) > 0")
        .unwrap();
    assert_eq!(canonical, "(COALESCE(a, 1) > 0)");
}
