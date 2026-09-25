//! Generated predicates, checked against the meaning oracle in every dialect.

mod oracle;

use oracle::{Folding, meaning};
use sqlparser::ast::Expr;
use sqlparser::dialect::{
    AnsiDialect, Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::Token;
use sqlparser_canonicalize::{CanonicalizeError, Canonicalizer};

const SEEDS: u64 = 3000;

/// Xorshift, so every run generates the same predicates.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).expect("bound fits in u64");
        usize::try_from(self.next() % bound).expect("value below a usize bound fits in usize")
    }

    fn percent(&mut self, chance: usize) -> bool {
        self.below(100) < chance
    }
}

enum Tree {
    Atom(&'static str, bool),
    Binary(&'static str, Box<Tree>, Box<Tree>),
    Not(Box<Tree>),
    Negate(Box<Tree>),
    Postfix(&'static str, Box<Tree>),
    In(Box<Tree>, Vec<Tree>, bool),
    Between(Box<Tree>, Box<Tree>, Box<Tree>, bool),
    /// Operator, subject, pattern, negated, and `ANY` for `LIKE ANY` and `ILIKE ANY`.
    Like(&'static str, Box<Tree>, Box<Tree>, bool, bool),
    Quantified(&'static str, &'static str, Box<Tree>, Box<Tree>),
    AtTimeZone(Box<Tree>, Box<Tree>),
    Coalesce(Box<Tree>, Box<Tree>),
    Cast(Box<Tree>),
    Case(Box<Tree>, Box<Tree>, Box<Tree>),
}

const BINARY: &[&str] = &[
    "=",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "+",
    "-",
    "*",
    "/",
    "%",
    "AND",
    "OR",
    "<=>",
    "IS DISTINCT FROM",
    "IS NOT DISTINCT FROM",
];
const SYMMETRIC: &[&str] = &[
    "=",
    "!=",
    "<=>",
    "AND",
    "OR",
    "IS DISTINCT FROM",
    "IS NOT DISTINCT FROM",
];
const POSTFIX: &[&str] = &[
    "IS NULL",
    "IS NOT NULL",
    "IS TRUE",
    "IS NOT TRUE",
    "IS FALSE",
    "IS NOT FALSE",
    "IS UNKNOWN",
    "IS NOT UNKNOWN",
];
const PATTERN_MATCH: &[&str] = &["LIKE", "ILIKE", "SIMILAR TO", "RLIKE", "REGEXP"];
const COMPARISON: &[&str] = &["=", "!=", "<", ">="];
const QUANTIFIER: &[&str] = &["ANY", "SOME", "ALL"];
const ATOMS: &[(&str, bool)] = &[
    ("a", true),
    ("b", true),
    ("c", true),
    ("1", false),
    ("2", false),
    ("'x'", false),
    ("NULL", false),
    ("TRUE", false),
];

/// Builds a random predicate tree. `normalized_only` keeps to forms the canonicalizer
/// normalizes itself, leaving out every form it prints verbatim.
fn tree(rng: &mut Rng, depth: u32, quoted: &'static str, normalized_only: bool) -> Tree {
    if depth == 0 || rng.percent(25) {
        if rng.percent(8) {
            return Tree::Atom(quoted, false);
        }
        let (text, identifier) = ATOMS[rng.below(ATOMS.len())];
        return Tree::Atom(text, identifier);
    }
    let child = |rng: &mut Rng| Box::new(tree(rng, depth - 1, quoted, normalized_only));
    match rng.below(if normalized_only { 13 } else { 16 }) {
        0..=4 => Tree::Binary(BINARY[rng.below(BINARY.len())], child(rng), child(rng)),
        5 => Tree::Not(child(rng)),
        6 => Tree::Negate(child(rng)),
        7 => Tree::Postfix(POSTFIX[rng.below(POSTFIX.len())], child(rng)),
        8 => {
            let items = (0..=rng.below(3))
                .map(|_| tree(rng, depth - 1, quoted, normalized_only))
                .collect();
            Tree::In(child(rng), items, rng.percent(50))
        }
        9 => Tree::Between(child(rng), child(rng), child(rng), rng.percent(50)),
        10 => {
            let operator = PATTERN_MATCH[rng.below(PATTERN_MATCH.len())];
            let any = matches!(operator, "LIKE" | "ILIKE") && rng.percent(20);
            Tree::Like(operator, child(rng), child(rng), rng.percent(50), any)
        }
        11 => {
            let comparison = COMPARISON[rng.below(COMPARISON.len())];
            let quantifier = QUANTIFIER[rng.below(QUANTIFIER.len())];
            Tree::Quantified(comparison, quantifier, child(rng), child(rng))
        }
        12 => Tree::AtTimeZone(child(rng), child(rng)),
        13 => Tree::Coalesce(child(rng), child(rng)),
        14 => Tree::Cast(child(rng)),
        _ => Tree::Case(child(rng), child(rng), child(rng)),
    }
}

/// How to spell a tree. The default spelling leaves each operand bare or parenthesized at
/// random, so the dialect's precedence decides its grouping.
#[derive(Clone, Copy, Default)]
struct Spelling {
    parenthesize_every_operand: bool,
    flip_name_case: bool,
    flip_keyword_case: bool,
    swap_symmetric_operands: bool,
}

fn keyword(text: &str, spelling: Spelling, rng: &mut Rng) -> String {
    if spelling.flip_keyword_case && rng.percent(50) {
        text.to_lowercase()
    } else {
        text.to_string()
    }
}

fn spell(tree: &Tree, spelling: Spelling, rng: &mut Rng) -> String {
    let operand = |tree: &Tree, rng: &mut Rng| {
        let text = spell(tree, spelling, rng);
        if spelling.parenthesize_every_operand {
            if spelling.flip_keyword_case && rng.percent(20) {
                format!("(({text}))")
            } else {
                format!("({text})")
            }
        } else if rng.percent(40) {
            format!("({text})")
        } else {
            text
        }
    };
    match tree {
        Tree::Atom(text, identifier) => {
            if *identifier && spelling.flip_name_case && rng.percent(50) {
                text.to_uppercase()
            } else if !*identifier && text.chars().all(|c| c.is_ascii_uppercase()) {
                keyword(text, spelling, rng)
            } else {
                (*text).to_string()
            }
        }
        Tree::Binary(operator, left, right) => {
            let (mut first, mut second) = (left, right);
            if spelling.swap_symmetric_operands && SYMMETRIC.contains(operator) && rng.percent(50) {
                (first, second) = (second, first);
            }
            let operator = if *operator == "!=" && spelling.flip_keyword_case && rng.percent(50) {
                "<>".to_string()
            } else {
                keyword(operator, spelling, rng)
            };
            let (first, second) = (operand(first, rng), operand(second, rng));
            format!("{first} {operator} {second}")
        }
        Tree::Not(inner) => format!("{} {}", keyword("NOT", spelling, rng), operand(inner, rng)),
        Tree::Negate(inner) => format!("- {}", operand(inner, rng)),
        Tree::Postfix(operator, inner) => {
            format!(
                "{} {}",
                operand(inner, rng),
                keyword(operator, spelling, rng)
            )
        }
        Tree::In(subject, items, negated) => {
            let subject = operand(subject, rng);
            let items: Vec<String> = items.iter().map(|item| operand(item, rng)).collect();
            let not = if *negated { "NOT " } else { "" };
            let not = keyword(not, spelling, rng);
            format!("{subject} {not}IN ({})", items.join(", "))
        }
        Tree::Between(subject, low, high, negated) => {
            let (subject, low, high) =
                (operand(subject, rng), operand(low, rng), operand(high, rng));
            let not = if *negated { "NOT " } else { "" };
            format!(
                "{subject} {}BETWEEN {low} AND {high}",
                keyword(not, spelling, rng)
            )
        }
        Tree::Like(operator, subject, pattern, negated, any) => {
            let (subject, pattern) = (operand(subject, rng), operand(pattern, rng));
            let not = if *negated { "NOT " } else { "" };
            let not = keyword(not, spelling, rng);
            let any = if *any {
                keyword(" ANY", spelling, rng)
            } else {
                String::new()
            };
            format!(
                "{subject} {not}{}{any} {pattern}",
                keyword(operator, spelling, rng)
            )
        }
        Tree::Quantified(comparison, quantifier, left, right) => {
            let (left, right) = (operand(left, rng), spell(right, spelling, rng));
            let quantifier = match *quantifier {
                "ANY" | "SOME" if spelling.flip_keyword_case && rng.percent(50) => {
                    if *quantifier == "ANY" { "SOME" } else { "ANY" }
                }
                other => other,
            };
            let quantifier = keyword(quantifier, spelling, rng);
            format!("{left} {comparison} {quantifier}({right})")
        }
        Tree::AtTimeZone(timestamp, zone) => {
            let (timestamp, zone) = (operand(timestamp, rng), operand(zone, rng));
            format!(
                "{timestamp} {} {zone}",
                keyword("AT TIME ZONE", spelling, rng)
            )
        }
        Tree::Coalesce(first, second) => {
            let name = keyword("COALESCE", spelling, rng);
            format!("{name}({}, {})", operand(first, rng), operand(second, rng))
        }
        Tree::Cast(inner) => {
            format!(
                "{}({} AS INTEGER)",
                keyword("CAST", spelling, rng),
                operand(inner, rng)
            )
        }
        Tree::Case(condition, then, otherwise) => {
            let (condition, then, otherwise) = (
                operand(condition, rng),
                operand(then, rng),
                operand(otherwise, rng),
            );
            let case = keyword("CASE", spelling, rng);
            format!("{case} WHEN {condition} THEN {then} ELSE {otherwise} END")
        }
    }
}

fn parse(dialect: &dyn Dialect, predicate: &str) -> Option<Expr> {
    let mut parser = Parser::new(dialect).try_with_sql(predicate).ok()?;
    let expr = parser.parse_expr().ok()?;
    matches!(parser.peek_token_ref().token, Token::EOF).then_some(expr)
}

fn dialects() -> [(&'static str, Box<dyn Dialect>, &'static str); 5] {
    [
        ("PostgreSQL", Box::new(PostgreSqlDialect {}), "\"Q\""),
        ("MySQL", Box::new(MySqlDialect {}), "`Q`"),
        ("SQLite", Box::new(SQLiteDialect {}), "\"Q\""),
        ("ANSI", Box::new(AnsiDialect {}), "\"Q\""),
        ("Generic", Box::new(GenericDialect {}), "\"Q\""),
    ]
}

fn canonicalize(
    canonicalizer: &Canonicalizer<'_>,
    predicate: &str,
) -> Result<String, CanonicalizeError> {
    canonicalizer.normalize_sql(&format!("SELECT * FROM t WHERE {predicate}"))
}

#[test]
fn canonical_text_means_what_the_input_means() {
    for (name, dialect, quoted) in dialects() {
        let dialect = dialect.as_ref();
        let canonicalizer = Canonicalizer::new(dialect);
        let folding = Folding::of(dialect);
        for seed in 0..SEEDS {
            let mut rng = Rng::new(seed);
            let predicate = spell(
                &tree(&mut rng, 3, quoted, false),
                Spelling::default(),
                &mut rng,
            );
            let Some(input) = parse(dialect, &predicate) else {
                continue;
            };
            let Ok(canonical) = canonicalize(&canonicalizer, &predicate) else {
                continue;
            };
            let read_back = parse(dialect, &canonical)
                .unwrap_or_else(|| panic!("{name}: {canonical} from {predicate} does not parse"));
            assert_eq!(
                meaning(&read_back, folding),
                meaning(&input, folding),
                "{name}: {predicate} canonicalized to {canonical}, which means something else"
            );
        }
    }
}

#[test]
fn equivalent_spellings_of_normalized_forms_agree() {
    let reference = Spelling {
        parenthesize_every_operand: true,
        ..Spelling::default()
    };
    for (name, dialect, quoted) in dialects() {
        let dialect = dialect.as_ref();
        let canonicalizer = Canonicalizer::new(dialect);
        let folding = Folding::of(dialect);
        let variant = Spelling {
            parenthesize_every_operand: true,
            flip_name_case: !dialect.is::<GenericDialect>(),
            flip_keyword_case: true,
            swap_symmetric_operands: true,
        };
        for seed in 0..SEEDS {
            let mut rng = Rng::new(seed);
            let generated = tree(&mut rng, 3, quoted, true);
            let predicate = spell(&generated, Spelling::default(), &mut rng);
            let (Some(input), Some(intended)) = (
                parse(dialect, &predicate),
                parse(dialect, &spell(&generated, reference, &mut rng)),
            ) else {
                continue;
            };
            // Only a spelling the dialect groups the way the tree does is equivalent to the
            // fully parenthesized variant.
            if meaning(&input, folding) != meaning(&intended, folding) {
                continue;
            }
            let respelled = spell(&generated, variant, &mut rng);
            assert_eq!(
                canonicalize(&canonicalizer, &predicate),
                canonicalize(&canonicalizer, &respelled),
                "{name}: {predicate} and {respelled} are one predicate"
            );
        }
    }
}
