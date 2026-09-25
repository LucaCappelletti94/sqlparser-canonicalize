//! What a predicate means, written independently of the canonicalizer.
//!
//! Two expressions with the same meaning text are the same predicate under every equivalence
//! the canonicalizer promises: redundant parentheses, the dialect's name folding, and the order
//! of operands and items it treats as unordered. Anything else keeps its structure.

use sqlparser::ast::{BinaryOperator, Expr, Ident};
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
            expr,
            pattern,
            escape_char,
            ..
        } => format!(
            "(like {negated} {} {} {escape_char:?})",
            m(expr),
            m(pattern)
        ),
        Expr::ILike {
            negated,
            expr,
            pattern,
            escape_char,
            ..
        } => format!(
            "(ilike {negated} {} {} {escape_char:?})",
            m(expr),
            m(pattern)
        ),
        other => format!("{other}"),
    }
}
