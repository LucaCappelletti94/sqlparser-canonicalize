use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

use seahash::SeaHasher;
use sqlparser::ast::{
    BinaryOperator, Distinct, Expr, Ident, LimitClause, Query, Select, SelectModifiers, SetExpr,
    Statement, TableFactor, Value,
};
use sqlparser::dialect::{AnsiDialect, Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::keywords::ALL_KEYWORDS;
use sqlparser::parser::Parser;

use crate::CanonicalizeError;

const MAX_EXPR_DEPTH: usize = 128;
const MAX_SQL_LEN: usize = 8192;
/// Canonical text for a `SELECT` with no `WHERE` clause.
const NO_FILTER: &str = "TRUE";

impl<'a> Canonicalizer<'a> {
    /// Parses one `SELECT` and returns canonical text for its `WHERE` clause.
    ///
    /// The canonical text is verified to survive a parse of itself, so a predicate whose
    /// canonical form would read back as something else is rejected instead of hashed.
    pub fn normalize_sql(&self, sql: &str) -> Result<String, CanonicalizeError> {
        let canonical = normalize_sql_inner(sql, self)?;
        confirm_reads_back_as_itself(canonical, self)
    }

    /// Canonicalizes a parsed `WHERE` clause in O(n log n) time for boolean chains and O(n)
    /// otherwise.
    ///
    /// `where_expr` MUST have been parsed with this canonicalizer's dialect, which decides
    /// how names fold and which reads the canonical text back to check it.
    pub fn normalize_where_clause(
        &self,
        where_expr: Option<&Expr>,
    ) -> Result<String, CanonicalizeError> {
        let canonical = normalize_where_clause_inner(where_expr, self)?;
        confirm_reads_back_as_itself(canonical, self)
    }
}

fn normalize_sql_inner(
    sql: &str,
    context: &Canonicalizer<'_>,
) -> Result<String, CanonicalizeError> {
    if sql.len() > MAX_SQL_LEN {
        return Err(CanonicalizeError::InputTooLong { limit: MAX_SQL_LEN });
    }
    check_sql_sanity(sql)?;

    let statements = Parser::parse_sql(context.dialect, sql)
        .map_err(|error| CanonicalizeError::Parse(error.to_string()))?;
    let [statement] = statements.as_slice() else {
        return Err(CanonicalizeError::Unsupported(
            "Expected exactly one SELECT statement".to_string(),
        ));
    };
    let where_expr = extract_where(statement)?;
    normalize_where_clause_inner(where_expr, context)
}

fn normalize_where_clause_inner(
    where_expr: Option<&Expr>,
    context: &Canonicalizer<'_>,
) -> Result<String, CanonicalizeError> {
    where_expr.map_or_else(
        || Ok(NO_FILTER.to_string()),
        |expr| normalize_expr_inner(expr, 0, false, context),
    )
}

/// Returns the canonical text only if a second pass reproduces it byte for byte.
///
/// Canonical text is a hash key, so text that reads back as a different predicate would
/// give two distinct predicates one hash.
fn confirm_reads_back_as_itself(
    canonical: String,
    context: &Canonicalizer<'_>,
) -> Result<String, CanonicalizeError> {
    let replay = if canonical == NO_FILTER {
        "SELECT * FROM t".to_string()
    } else {
        format!("SELECT * FROM t WHERE {canonical}")
    };
    match normalize_sql_inner(&replay, context) {
        Ok(again) if again == canonical => Ok(canonical),
        _ => Err(CanonicalizeError::NotRoundTrippable(canonical)),
    }
}

/// Returns the stable 128-bit SeaHash value for canonical text.
#[must_use]
pub fn hash_canonical(normalized: &str) -> u128 {
    let mut first = SeaHasher::new();
    normalized.hash(&mut first);
    let first = first.finish();

    let mut second = SeaHasher::with_seeds(
        first,
        first.wrapping_add(1),
        first.wrapping_add(2),
        first.wrapping_add(3),
    );
    normalized.hash(&mut second);
    (u128::from(first) << 64) | u128::from(second.finish())
}

fn check_sql_sanity(sql: &str) -> Result<(), CanonicalizeError> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut consecutive_ops = 0usize;

    for byte in sql.bytes() {
        match byte {
            b'(' => {
                paren_depth += 1;
                consecutive_ops += 1;
            }
            b')' => {
                paren_depth = paren_depth.saturating_sub(1);
                consecutive_ops = 0;
            }
            b'[' => {
                bracket_depth += 1;
                consecutive_ops = 0;
            }
            b']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                consecutive_ops = 0;
            }
            b'+' | b'-' | b'*' | b'/' | b'=' | b'<' | b'>' | b'!' | b'~' => {
                consecutive_ops += 1;
            }
            b' ' | b'\t' | b'\n' | b'\r' => {}
            0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F | 0x7F => {
                return Err(CanonicalizeError::Unsupported(
                    "Control character in SQL".to_string(),
                ));
            }
            _ => consecutive_ops = 0,
        }

        if paren_depth > MAX_EXPR_DEPTH
            || bracket_depth > MAX_EXPR_DEPTH
            || consecutive_ops > MAX_EXPR_DEPTH
        {
            return Err(CanonicalizeError::TooDeep {
                limit: MAX_EXPR_DEPTH,
            });
        }
    }

    if paren_depth != 0 {
        return Err(CanonicalizeError::Unsupported(
            "Unbalanced parentheses".to_string(),
        ));
    }
    if bracket_depth != 0 {
        return Err(CanonicalizeError::Unsupported(
            "Unbalanced square brackets".to_string(),
        ));
    }
    Ok(())
}

fn extract_where(statement: &Statement) -> Result<Option<&Expr>, CanonicalizeError> {
    let Statement::Query(query) = statement else {
        return Err(CanonicalizeError::Unsupported(
            "Only SELECT statements are supported".to_string(),
        ));
    };
    Ok(single_table_select(query)?.selection.as_ref())
}

fn single_table_select(query: &Query) -> Result<&Select, CanonicalizeError> {
    let SetExpr::Select(select) = query.body.as_ref() else {
        return Err(CanonicalizeError::Unsupported(
            "Set operations are not supported".to_string(),
        ));
    };
    if select.from.len() != 1 {
        return Err(CanonicalizeError::Unsupported(
            "Exactly one table is required".to_string(),
        ));
    }
    if !select.from[0].joins.is_empty() {
        return Err(CanonicalizeError::Unsupported(
            "JOINs not supported".to_string(),
        ));
    }
    if !matches!(select.from[0].relation, TableFactor::Table { .. }) {
        return Err(CanonicalizeError::Unsupported(
            "Subqueries and derived tables not supported".to_string(),
        ));
    }
    check_served_clauses(query, select)?;
    Ok(select)
}

fn check_served_clauses(query: &Query, select: &Select) -> Result<(), CanonicalizeError> {
    let Query {
        with,
        body: _,
        order_by: _,
        limit_clause,
        fetch,
        locks,
        for_clause,
        settings,
        format_clause,
        pipe_operators,
    } = query;
    let Select {
        select_token: _,
        optimizer_hints,
        distinct,
        select_modifiers,
        top,
        top_before_distinct: _,
        projection: _,
        exclude,
        into,
        from: _,
        lateral_views,
        prewhere,
        selection: _,
        connect_by,
        group_by: _,
        cluster_by,
        distribute_by,
        sort_by,
        having: _,
        named_window,
        qualify,
        window_before_qualify: _,
        value_table_mode,
        flavor: _,
    } = select;

    let deduplicating = !matches!(distinct, None | Some(Distinct::All));
    for (present, clause) in [
        (with.is_some(), "WITH"),
        (deduplicating, "DISTINCT"),
        (
            limit_clause.is_some(),
            limit_clause_name(limit_clause.as_ref()),
        ),
        (fetch.is_some(), "FETCH"),
        (!locks.is_empty(), "FOR UPDATE or FOR SHARE"),
        (for_clause.is_some(), "FOR XML or FOR JSON"),
        (settings.is_some(), "SETTINGS"),
        (format_clause.is_some(), "FORMAT"),
        (!pipe_operators.is_empty(), "pipe operators"),
        (!optimizer_hints.is_empty(), "optimizer hints"),
        (
            select_modifiers
                .as_ref()
                .is_some_and(SelectModifiers::is_any_set),
            "SELECT modifiers",
        ),
        (top.is_some(), "TOP"),
        (exclude.is_some(), "EXCLUDE"),
        (into.is_some(), "INTO"),
        (!lateral_views.is_empty(), "LATERAL VIEW"),
        (prewhere.is_some(), "PREWHERE"),
        (!connect_by.is_empty(), "CONNECT BY"),
        (!cluster_by.is_empty(), "CLUSTER BY"),
        (!distribute_by.is_empty(), "DISTRIBUTE BY"),
        (!sort_by.is_empty(), "SORT BY"),
        (!named_window.is_empty(), "WINDOW"),
        (qualify.is_some(), "QUALIFY"),
        (
            value_table_mode.is_some(),
            "SELECT AS VALUE or SELECT AS STRUCT",
        ),
    ] {
        if present {
            return Err(CanonicalizeError::Unsupported(format!(
                "{clause} is not supported"
            )));
        }
    }
    Ok(())
}

const fn limit_clause_name(limit: Option<&LimitClause>) -> &'static str {
    match limit {
        Some(LimitClause::LimitOffset {
            limit: None,
            offset,
            limit_by,
        }) => {
            if offset.is_some() {
                "OFFSET"
            } else if limit_by.is_empty() {
                "LIMIT"
            } else {
                "LIMIT BY"
            }
        }
        None | Some(LimitClause::LimitOffset { .. } | LimitClause::OffsetCommaLimit { .. }) => {
            "LIMIT"
        }
    }
}

fn normalize_expr_inner(
    expr: &Expr,
    depth: usize,
    tight_parent: bool,
    context: &Canonicalizer<'_>,
) -> Result<String, CanonicalizeError> {
    if depth > MAX_EXPR_DEPTH {
        return Err(CanonicalizeError::TooDeep {
            limit: MAX_EXPR_DEPTH,
        });
    }

    Ok(match expr {
        Expr::BinaryOp { left, op, right } => {
            if matches!(op, BinaryOperator::And | BinaryOperator::Or) {
                let mut children = collect_flat_children(left, op);
                children.extend(collect_flat_children(right, op));
                let mut child_text: Vec<String> = children
                    .iter()
                    .map(|child| normalize_expr_inner(child, depth + 1, false, context))
                    .collect::<Result<_, _>>()?;
                child_text.sort();
                let operator = operator_text(op)?;
                child_text
                    .into_iter()
                    .reduce(|left, right| format!("({left} {operator} {right})"))
                    .unwrap_or_default()
            } else {
                let left = normalize_expr_inner(left, depth + 1, true, context)?;
                let right = normalize_expr_inner(right, depth + 1, true, context)?;
                let (left, right) = if is_commutative(op) && left > right {
                    (right, left)
                } else {
                    (left, right)
                };
                format!("({left} {} {right})", operator_text(op)?)
            }
        }
        Expr::UnaryOp { op, expr } => {
            let operator = unary_operator_text(op)?;
            let operand = normalize_expr_inner(expr, depth + 1, true, context)?;
            // NOT binds looser than any operator that can enclose it, so a nested NOT
            // reparses against the wrong operand without parentheses.
            if tight_parent && matches!(op, sqlparser::ast::UnaryOperator::Not) {
                format!("({operator} {operand})")
            } else {
                format!("{operator} {operand}")
            }
        }
        Expr::IsNull(expr) => format!(
            "{} IS NULL",
            normalize_expr_inner(expr, depth + 1, true, context)?
        ),
        Expr::IsNotNull(expr) => {
            format!(
                "{} IS NOT NULL",
                normalize_expr_inner(expr, depth + 1, true, context)?
            )
        }
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            let mut items: Vec<String> = list
                .iter()
                .map(|item| normalize_expr_inner(item, depth + 1, true, context))
                .collect::<Result<_, _>>()?;
            items.sort();
            let not = if *negated { "NOT " } else { "" };
            format!(
                "{} {not}IN ({})",
                normalize_expr_inner(expr, depth + 1, true, context)?,
                items.join(", ")
            )
        }
        Expr::InSubquery {
            expr,
            subquery,
            negated,
        } => {
            let not = if *negated { "NOT " } else { "" };
            format!(
                "{} {not}IN ({subquery})",
                normalize_expr_inner(expr, depth + 1, true, context)?
            )
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let not = if *negated { "NOT " } else { "" };
            format!(
                "{} {not}BETWEEN {} AND {}",
                normalize_expr_inner(expr, depth + 1, true, context)?,
                normalize_expr_inner(low, depth + 1, true, context)?,
                normalize_expr_inner(high, depth + 1, true, context)?
            )
        }
        Expr::Like {
            expr,
            pattern,
            negated,
            escape_char,
            ..
        } => {
            let not = if *negated { "NOT " } else { "" };
            let escape = escape_char
                .as_ref()
                .map_or_else(String::new, |value| format!(" ESCAPE {value}"));
            format!(
                "{} {not}LIKE {}{escape}",
                normalize_expr_inner(expr, depth + 1, true, context)?,
                normalize_expr_inner(pattern, depth + 1, true, context)?
            )
        }
        Expr::ILike {
            expr,
            pattern,
            negated,
            escape_char,
            ..
        } => {
            let not = if *negated { "NOT " } else { "" };
            let escape = escape_char
                .as_ref()
                .map_or_else(String::new, |value| format!(" ESCAPE {value}"));
            format!(
                "{} {not}ILIKE {}{escape}",
                normalize_expr_inner(expr, depth + 1, true, context)?,
                normalize_expr_inner(pattern, depth + 1, true, context)?
            )
        }
        Expr::Nested(inner) => normalize_expr_inner(inner, depth + 1, tight_parent, context)?,
        Expr::Identifier(identifier) => identifier_text(identifier, context)?,
        Expr::CompoundIdentifier(parts) => parts
            .iter()
            .map(|part| identifier_text(part, context))
            .collect::<Result<Vec<_>, _>>()?
            .join("."),
        Expr::Value(value) => {
            reject_lossy_quoting(&value.value)?;
            format!("{}", value.value)
        }
        Expr::Function(function) => format!("{function}"),
        _ => format!("{expr}"),
    })
}

/// How a dialect decides whether two spellings of a name are the same column.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Folding {
    /// An unquoted name folds to lower case, a quoted one keeps its spelling.
    LowerUnquoted,
    /// An unquoted name folds to upper case, a quoted one keeps its spelling.
    UpperUnquoted,
    /// Every spelling of the same letters is one name.
    CaseInsensitive,
    /// Unknown rule, so keep the name exactly as written and never merge two spellings.
    Exact,
}

/// Canonicalizes predicates under one SQL dialect.
///
/// The dialect decides how identifiers fold, so it decides which two spellings mean one
/// column. Build this from the dialect that parsed whatever you hand it.
pub struct Canonicalizer<'a> {
    dialect: &'a dyn Dialect,
    folding: Folding,
}

impl<'a> Canonicalizer<'a> {
    /// Reads the folding rule for `dialect`.
    #[must_use]
    pub fn new(dialect: &'a dyn Dialect) -> Self {
        let folding = if dialect.is::<PostgreSqlDialect>() {
            Folding::LowerUnquoted
        } else if dialect.is::<AnsiDialect>() {
            Folding::UpperUnquoted
        } else if dialect.is::<MySqlDialect>() || dialect.is::<SQLiteDialect>() {
            Folding::CaseInsensitive
        } else {
            Folding::Exact
        };
        Self { dialect, folding }
    }
}

/// Resolves an identifier to the name the database would see, then spells it the one way
/// that reads back as that same name.
fn identifier_text(
    identifier: &Ident,
    context: &Canonicalizer<'_>,
) -> Result<String, CanonicalizeError> {
    let quoted = identifier.quote_style.is_some();
    // Case folding rules are stated for ASCII. Anything else keeps its exact spelling,
    // because merging two names the database might separate is the unsafe direction.
    let folding = if identifier.value.is_ascii() {
        context.folding
    } else {
        Folding::Exact
    };
    let resolved = match folding {
        Folding::LowerUnquoted if !quoted => identifier.value.to_ascii_lowercase(),
        Folding::UpperUnquoted if !quoted => identifier.value.to_ascii_uppercase(),
        Folding::CaseInsensitive => identifier.value.to_ascii_lowercase(),
        _ => identifier.value.clone(),
    };

    if bare_spelling_is_faithful(&resolved, quoted, folding) {
        return Ok(resolved);
    }
    // A delimited name escapes its own delimiter by doubling it, so `a"b` is written
    // `"a""b"`. Emitting the delimiter raw produces a name that reads back as something else.
    let quote = identifier.quote_style.unwrap_or('"');
    let mut delimited = String::with_capacity(resolved.len() + 2);
    delimited.push(quote);
    for character in resolved.chars() {
        if character == quote {
            delimited.push(quote);
        }
        delimited.push(character);
    }
    delimited.push(quote);
    Ok(delimited)
}

/// Reports whether writing `name` without quotes reads back as `name` itself.
///
/// The answer must depend only on the name, never on how canonicalization is going, or one
/// predicate could end up with two spellings and so two keys.
fn bare_spelling_is_faithful(name: &str, quoted: bool, folding: Folding) -> bool {
    if !quoted {
        // This dialect already accepted the name without quotes, and folding only applied the
        // change the dialect makes itself, so writing it bare reads back as the same name.
        return true;
    }
    if !is_plain_non_keyword(name) {
        return false;
    }
    match folding {
        // Dropping the quotes hands the name to the dialect's folding, so it may only go bare
        // when it is already in the folded form.
        Folding::LowerUnquoted => !name.bytes().any(|byte| byte.is_ascii_uppercase()),
        Folding::UpperUnquoted => !name.bytes().any(|byte| byte.is_ascii_lowercase()),
        Folding::CaseInsensitive => true,
        Folding::Exact => false,
    }
}

/// Reports whether `name` is a bare word no dialect reads as a keyword.
///
/// Whether a given keyword may still name a column is dialect and position specific, so this
/// errs towards keeping the quotes. The cost is that a quoted keyword does not share a key
/// with its unquoted spelling, which splits one column into two entries rather than merging
/// two columns into one.
fn is_plain_non_keyword(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return false;
    }
    ALL_KEYWORDS
        .binary_search(&name.to_ascii_uppercase().as_str())
        .is_err()
}

/// Rejects a literal the parser cannot print without changing its value.
fn reject_lossy_quoting(value: &Value) -> Result<(), CanonicalizeError> {
    let intact = match value {
        Value::SingleQuotedString(text) | Value::NationalStringLiteral(text) => {
            quoting_survives_printing(text, '\'')
        }
        Value::DoubleQuotedString(text) => quoting_survives_printing(text, '"'),
        _ => true,
    };
    if intact {
        Ok(())
    } else {
        Err(CanonicalizeError::NotRoundTrippable(
            "string literal".to_string(),
        ))
    }
}

/// Reports whether printing `text` inside `quote` delimiters preserves it.
///
/// `sqlparser` leaves a quote alone when it already looks escaped, either doubled or preceded
/// by a backslash, so such text reads back one escape level shorter than it went in.
fn quoting_survives_printing(text: &str, quote: char) -> bool {
    let mut characters = text.chars().peekable();
    let mut previous = char::default();
    while let Some(character) = characters.next() {
        if character == quote
            && (previous == '\\' || characters.peek().is_some_and(|next| *next == quote))
        {
            return false;
        }
        previous = character;
    }
    true
}

fn collect_flat_children<'a>(expr: &'a Expr, operator: &BinaryOperator) -> Vec<&'a Expr> {
    match expr {
        Expr::Nested(inner) => collect_flat_children(inner, operator),
        Expr::BinaryOp { left, op, right } if op == operator => {
            let mut children = collect_flat_children(left, operator);
            children.extend(collect_flat_children(right, operator));
            children
        }
        _ => vec![expr],
    }
}

const fn is_commutative(operator: &BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Eq
    )
}

fn operator_text(operator: &BinaryOperator) -> Result<&'static str, CanonicalizeError> {
    match operator {
        BinaryOperator::And => Ok("AND"),
        BinaryOperator::Or => Ok("OR"),
        BinaryOperator::Eq => Ok("="),
        BinaryOperator::NotEq => Ok("!="),
        BinaryOperator::Lt => Ok("<"),
        BinaryOperator::LtEq => Ok("<="),
        BinaryOperator::Gt => Ok(">"),
        BinaryOperator::GtEq => Ok(">="),
        BinaryOperator::Plus => Ok("+"),
        BinaryOperator::Minus => Ok("-"),
        BinaryOperator::Multiply => Ok("*"),
        BinaryOperator::Divide => Ok("/"),
        BinaryOperator::Modulo => Ok("%"),
        other => Err(CanonicalizeError::Unsupported(format!(
            "Unsupported binary operator: {other}"
        ))),
    }
}

fn unary_operator_text(
    operator: &sqlparser::ast::UnaryOperator,
) -> Result<&'static str, CanonicalizeError> {
    match operator {
        sqlparser::ast::UnaryOperator::Not => Ok("NOT"),
        sqlparser::ast::UnaryOperator::Plus => Ok("+"),
        sqlparser::ast::UnaryOperator::Minus => Ok("-"),
        other => Err(CanonicalizeError::Unsupported(format!(
            "Unsupported unary operator: {other}"
        ))),
    }
}
