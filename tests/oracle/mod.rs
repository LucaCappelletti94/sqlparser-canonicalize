//! What a predicate means, written independently of the canonicalizer.
//!
//! Two expressions with the same meaning text are the same predicate under every equivalence
//! the canonicalizer promises: redundant parentheses, the dialect's name folding, the order of
//! operands and items it treats as unordered, and keyword synonyms such as `SOME` for `ANY`.
//! Anything else keeps its structure.

use sqlparser::ast::{BinaryOperator, Expr, Ident, ObjectName};
use sqlparser::dialect::{AnsiDialect, Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    Lower,
    Upper,
    Insensitive,
    Exact,
}

/// How a dialect folds column names and table qualifiers.
#[derive(Clone, Copy, Debug)]
pub struct Folding {
    pub column: Fold,
    pub qualifier: Fold,
}

impl Folding {
    pub fn of(dialect: &dyn Dialect) -> Self {
        let (column, qualifier) = if dialect.is::<PostgreSqlDialect>() {
            (Fold::Lower, Fold::Lower)
        } else if dialect.is::<AnsiDialect>() {
            (Fold::Upper, Fold::Upper)
        } else if dialect.is::<MySqlDialect>() {
            (Fold::Insensitive, Fold::Exact)
        } else if dialect.is::<SQLiteDialect>() {
            (Fold::Insensitive, Fold::Insensitive)
        } else {
            (Fold::Exact, Fold::Exact)
        };
        Self { column, qualifier }
    }
}

fn name(ident: &Ident, fold: Fold) -> String {
    let quoted = ident.quote_style.is_some();
    match fold {
        Fold::Lower if !quoted => ident.value.to_ascii_lowercase(),
        Fold::Upper if !quoted => ident.value.to_ascii_uppercase(),
        Fold::Insensitive => ident.value.to_ascii_lowercase(),
        _ => ident.value.clone(),
    }
}

fn flatten(expr: &Expr, operator: &BinaryOperator, folding: Folding, out: &mut Vec<String>) {
    match expr {
        Expr::Nested(inner) => flatten(inner, operator, folding, out),
        Expr::BinaryOp { left, op, right } if op == operator => {
            flatten(left, operator, folding, out);
            flatten(right, operator, folding, out);
        }
        _ => out.push(meaning(expr, folding)),
    }
}

fn unordered(left: &Expr, right: &Expr, folding: Folding) -> (String, String) {
    let (left, right) = (meaning(left, folding), meaning(right, folding));
    if left > right {
        (right, left)
    } else {
        (left, right)
    }
}

/// Returns the meaning text of `expr`.
pub fn meaning(expr: &Expr, folding: Folding) -> String {
    let m = |expr: &Expr| meaning(expr, folding);
    match expr {
        Expr::Nested(inner) => m(inner),
        Expr::Identifier(ident) => name(ident, folding.column),
        Expr::CompoundIdentifier(parts) => {
            let last = parts.len().saturating_sub(1);
            parts
                .iter()
                .enumerate()
                .map(|(index, part)| {
                    let fold = if index == last {
                        folding.column
                    } else {
                        folding.qualifier
                    };
                    name(part, fold)
                })
                .collect::<Vec<_>>()
                .join(".")
        }
        Expr::BinaryOp { op, .. } if matches!(op, BinaryOperator::And | BinaryOperator::Or) => {
            let mut children = Vec::new();
            flatten(expr, op, folding, &mut children);
            children.sort();
            format!("({op} {})", children.join(" "))
        }
        Expr::BinaryOp { left, op, right }
            if matches!(
                op,
                BinaryOperator::Eq | BinaryOperator::NotEq | BinaryOperator::Spaceship
            ) =>
        {
            let (left, right) = unordered(left, right, folding);
            format!("({op} {left} {right})")
        }
        Expr::BinaryOp { left, op, right } => format!("({op} {} {})", m(left), m(right)),
        Expr::IsDistinctFrom(left, right) => {
            let (left, right) = unordered(left, right, folding);
            format!("(distinct {left} {right})")
        }
        Expr::IsNotDistinctFrom(left, right) => {
            let (left, right) = unordered(left, right, folding);
            format!("(not-distinct {left} {right})")
        }
        Expr::UnaryOp { op, expr } => format!("({op} {})", m(expr)),
        Expr::IsNull(expr) => format!("(is-null {})", m(expr)),
        Expr::IsNotNull(expr) => format!("(is-not-null {})", m(expr)),
        Expr::IsTrue(expr) => format!("(is-true {})", m(expr)),
        Expr::IsNotTrue(expr) => format!("(is-not-true {})", m(expr)),
        Expr::IsFalse(expr) => format!("(is-false {})", m(expr)),
        Expr::IsNotFalse(expr) => format!("(is-not-false {})", m(expr)),
        Expr::IsUnknown(expr) => format!("(is-unknown {})", m(expr)),
        Expr::IsNotUnknown(expr) => format!("(is-not-unknown {})", m(expr)),
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            let mut items: Vec<String> = list.iter().map(m).collect();
            items.sort();
            format!("(in {negated} {} [{}])", m(expr), items.join(" "))
        }
        Expr::InSubquery {
            expr,
            subquery,
            negated,
        } => format!("(in-subquery {negated} {} {subquery})", m(expr)),
        Expr::Between {
            expr,
            negated,
            low,
            high,
        } => format!("(between {negated} {} {} {})", m(expr), m(low), m(high)),
        Expr::Like {
            negated,
            any,
            expr,
            pattern,
            escape_char,
        } => format!(
            "(like {negated} {any} {} {} {:?})",
            m(expr),
            m(pattern),
            escape_char.as_deref().map(m)
        ),
        Expr::ILike {
            negated,
            any,
            expr,
            pattern,
            escape_char,
        } => format!(
            "(ilike {negated} {any} {} {} {:?})",
            m(expr),
            m(pattern),
            escape_char.as_deref().map(m)
        ),
        Expr::SimilarTo {
            negated,
            expr,
            pattern,
            escape_char,
        } => {
            let escape = escape_char.as_deref().map(m);
            format!("(similar {negated} {} {} {escape:?})", m(expr), m(pattern))
        }
        // `REGEXP` and `RLIKE` are one operator.
        Expr::RLike {
            negated,
            expr,
            pattern,
            ..
        } => format!("(rlike {negated} {} {})", m(expr), m(pattern)),
        // `SOME` and `ANY` are one quantifier.
        Expr::AnyOp {
            left,
            compare_op,
            right,
            ..
        } => format!("(any {compare_op} {} {})", m(left), m(right)),
        Expr::AllOp {
            left,
            compare_op,
            right,
        } => format!("(all {compare_op} {} {})", m(left), m(right)),
        Expr::IsJson {
            expr,
            kind,
            unique_keys,
            negated,
        } => format!("(is-json {negated} {} {kind:?} {unique_keys:?})", m(expr)),
        Expr::IsNormalized {
            expr,
            form,
            negated,
        } => format!("(is-normalized {negated} {} {form:?})", m(expr)),
        Expr::MemberOf(member) => {
            format!("(member-of {} {})", m(&member.value), m(&member.array))
        }
        Expr::AtTimeZone {
            timestamp,
            time_zone,
        } => format!("(at-time-zone {} {})", m(timestamp), m(time_zone)),
        Expr::Collate { expr, collation } => format!("(collate {} {collation})", m(expr)),
        Expr::MatchAgainst {
            columns,
            match_value,
            opt_search_modifier,
        } => {
            let columns: Vec<String> = columns
                .iter()
                .map(|column| object_name(column, folding))
                .collect();
            format!(
                "(match [{}] {match_value} {opt_search_modifier:?})",
                columns.join(" ")
            )
        }
        other => format!("{other}"),
    }
}

fn object_name(name: &ObjectName, folding: Folding) -> String {
    let last = name.0.len().saturating_sub(1);
    name.0
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let fold = if index == last {
                folding.column
            } else {
                folding.qualifier
            };
            part.as_ident()
                .map_or_else(|| part.to_string(), |ident| self::name(ident, fold))
        })
        .collect::<Vec<_>>()
        .join(".")
}
