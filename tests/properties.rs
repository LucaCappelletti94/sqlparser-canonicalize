use proptest::prelude::*;
use sqlparser::dialect::{MySqlDialect, PostgreSqlDialect};
use sqlparser_canonicalize::normalize_sql;

fn normalize(sql: &str) -> String {
    normalize_sql(sql, &PostgreSqlDialect {}).unwrap()
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
        let canonical = normalize_sql(sql, &dialect).unwrap();
        let replay = format!("SELECT * FROM t WHERE {canonical}");
        assert_eq!(normalize_sql(&replay, &dialect).unwrap(), canonical);
    }
}
