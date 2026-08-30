use std::collections::BTreeMap;

use sqlparser::dialect::{MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser_canonicalize::{Canonicalizer, hash_canonical};

#[derive(Clone, Copy, Debug)]
enum DialectKind {
    PostgreSql,
    MySql,
    SQLite,
}

#[derive(Debug)]
struct Golden {
    dialect: DialectKind,
    sql: &'static str,
    normalized: &'static str,
    hash: u128,
}

const GOLDENS: &[Golden] = &[
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age > 18",
        normalized: "(age > 18)",
        hash: 292377788459137190795462354130980277275,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age >= 18",
        normalized: "(age >= 18)",
        hash: 96959591457307944904555814042553578216,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age < 65",
        normalized: "(age < 65)",
        hash: 249764920661847514626832755980428109760,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age <= 65",
        normalized: "(age <= 65)",
        hash: 142133684043952530757668767522792284451,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age = 42",
        normalized: "(42 = age)",
        hash: 302037566819660740417919068379990631044,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age != 42",
        normalized: "(age != 42)",
        hash: 12683479535703509878171549340018863210,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a = 1 AND b = 2",
        normalized: "((1 = a) AND (2 = b))",
        hash: 131670881900975508605752108541534450950,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE b = 2 AND a = 1",
        normalized: "((1 = a) AND (2 = b))",
        hash: 131670881900975508605752108541534450950,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a = 1 OR b = 2",
        normalized: "((1 = a) OR (2 = b))",
        hash: 295580346008773016158339669541369738333,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE b = 2 OR a = 1",
        normalized: "((1 = a) OR (2 = b))",
        hash: 295580346008773016158339669541369738333,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE (a = 1 AND b = 2) AND c = 3",
        normalized: "(((1 = a) AND (2 = b)) AND (3 = c))",
        hash: 182759154872355133982334454209519610451,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a = 1 AND (b = 2 AND c = 3)",
        normalized: "(((1 = a) AND (2 = b)) AND (3 = c))",
        hash: 182759154872355133982334454209519610451,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE (a = 1 OR b = 2) OR c = 3",
        normalized: "(((1 = a) OR (2 = b)) OR (3 = c))",
        hash: 95758464856935287648057063807289397135,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a = 1 OR (b = 2 OR c = 3)",
        normalized: "(((1 = a) OR (2 = b)) OR (3 = c))",
        hash: 95758464856935287648057063807289397135,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE x IN (3, 1, 2)",
        normalized: "x IN (1, 2, 3)",
        hash: 235230893105783445659199798622162550252,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE x NOT IN (3, 1, 2)",
        normalized: "x NOT IN (1, 2, 3)",
        hash: 277416314403838883454401070234615665757,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE ((age > 18))",
        normalized: "(age > 18)",
        hash: 292377788459137190795462354130980277275,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a < b",
        normalized: "(a < b)",
        hash: 14746718521301734887795259058823844396,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE b < a",
        normalized: "(b < a)",
        hash: 164195086123485964968356541696287096375,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a + b > 10",
        normalized: "((a + b) > 10)",
        hash: 2811918157887371711831872217975099545,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a - b > 10",
        normalized: "((a - b) > 10)",
        hash: 74443246094558125713561247511123700650,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a * b > 10",
        normalized: "((a * b) > 10)",
        hash: 311378054065053570425432075285704523444,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a / b > 10",
        normalized: "((a / b) > 10)",
        hash: 291742834309287946499546270378476167205,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE a % b > 10",
        normalized: "((a % b) > 10)",
        hash: 82868508035341089427090561311991876240,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE NOT (active = TRUE)",
        normalized: "NOT (active = true)",
        hash: 65851817300140767743482816604299764526,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE deleted_at IS NULL",
        normalized: "deleted_at IS NULL",
        hash: 260105666492253831600832914225129674266,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE deleted_at IS NOT NULL",
        normalized: "deleted_at IS NOT NULL",
        hash: 332224121843328788158539955817329118924,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE name LIKE 'A%'",
        normalized: "name LIKE 'A%'",
        hash: 314974045665179851130112309972384705706,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE name NOT LIKE 'A%'",
        normalized: "name NOT LIKE 'A%'",
        hash: 94303378871260595433799648522321568991,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age BETWEEN 18 AND 65",
        normalized: "age BETWEEN 18 AND 65",
        hash: 226023245311690468322616306060892911446,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE age NOT BETWEEN 18 AND 65",
        normalized: "age NOT BETWEEN 18 AND 65",
        hash: 114231437004172268307254474201086575722,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE status = 'paid'",
        normalized: "('paid' = status)",
        hash: 10488038035579433165683966987072516130,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE status = 'open' AND amount > 100",
        normalized: "(('open' = status) AND (amount > 100))",
        hash: 236186193320798656853084463803930322699,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'a')",
        normalized: "x IN (SELECT id FROM m WHERE owner = 'a')",
        hash: 266372329322768390664236543868795251272,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE x NOT IN (SELECT id FROM m WHERE owner = 'a')",
        normalized: "x NOT IN (SELECT id FROM m WHERE owner = 'a')",
        hash: 47926754135670659209261765469698082688,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT COUNT(*) FROM t",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT SUM(amount) FROM t WHERE amount > 10",
        normalized: "(amount > 10)",
        hash: 268413062225073979841068607631610090110,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT status, COUNT(*) FROM t WHERE active = TRUE GROUP BY status",
        normalized: "(active = true)",
        hash: 61822439498332721400316582865907409867,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT region, SUM(amount) FROM orders GROUP BY region HAVING SUM(amount) > 10",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT region, SUM(amount) FROM orders WHERE active = TRUE GROUP BY region HAVING COUNT(*) > 2",
        normalized: "(active = true)",
        hash: 61822439498332721400316582865907409867,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM t WHERE score = -5",
        normalized: "(- 5 = score)",
        hash: 263991339775984971083937534105788456146,
    },
    Golden {
        dialect: DialectKind::PostgreSql,
        sql: "SELECT * FROM orders WHERE \"Status\" = 'paid'",
        normalized: "(\"Status\" = 'paid')",
        hash: 244578590542428046936922079652582969739,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age > 18",
        normalized: "(age > 18)",
        hash: 292377788459137190795462354130980277275,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age >= 18",
        normalized: "(age >= 18)",
        hash: 96959591457307944904555814042553578216,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age < 65",
        normalized: "(age < 65)",
        hash: 249764920661847514626832755980428109760,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age <= 65",
        normalized: "(age <= 65)",
        hash: 142133684043952530757668767522792284451,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age = 42",
        normalized: "(42 = age)",
        hash: 302037566819660740417919068379990631044,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age != 42",
        normalized: "(age != 42)",
        hash: 12683479535703509878171549340018863210,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a = 1 AND b = 2",
        normalized: "((1 = a) AND (2 = b))",
        hash: 131670881900975508605752108541534450950,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE b = 2 AND a = 1",
        normalized: "((1 = a) AND (2 = b))",
        hash: 131670881900975508605752108541534450950,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a = 1 OR b = 2",
        normalized: "((1 = a) OR (2 = b))",
        hash: 295580346008773016158339669541369738333,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE b = 2 OR a = 1",
        normalized: "((1 = a) OR (2 = b))",
        hash: 295580346008773016158339669541369738333,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE (a = 1 AND b = 2) AND c = 3",
        normalized: "(((1 = a) AND (2 = b)) AND (3 = c))",
        hash: 182759154872355133982334454209519610451,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a = 1 AND (b = 2 AND c = 3)",
        normalized: "(((1 = a) AND (2 = b)) AND (3 = c))",
        hash: 182759154872355133982334454209519610451,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE (a = 1 OR b = 2) OR c = 3",
        normalized: "(((1 = a) OR (2 = b)) OR (3 = c))",
        hash: 95758464856935287648057063807289397135,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a = 1 OR (b = 2 OR c = 3)",
        normalized: "(((1 = a) OR (2 = b)) OR (3 = c))",
        hash: 95758464856935287648057063807289397135,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE x IN (3, 1, 2)",
        normalized: "x IN (1, 2, 3)",
        hash: 235230893105783445659199798622162550252,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE x NOT IN (3, 1, 2)",
        normalized: "x NOT IN (1, 2, 3)",
        hash: 277416314403838883454401070234615665757,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE ((age > 18))",
        normalized: "(age > 18)",
        hash: 292377788459137190795462354130980277275,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a < b",
        normalized: "(a < b)",
        hash: 14746718521301734887795259058823844396,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE b < a",
        normalized: "(b < a)",
        hash: 164195086123485964968356541696287096375,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a + b > 10",
        normalized: "((a + b) > 10)",
        hash: 2811918157887371711831872217975099545,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a - b > 10",
        normalized: "((a - b) > 10)",
        hash: 74443246094558125713561247511123700650,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a * b > 10",
        normalized: "((a * b) > 10)",
        hash: 311378054065053570425432075285704523444,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a / b > 10",
        normalized: "((a / b) > 10)",
        hash: 291742834309287946499546270378476167205,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE a % b > 10",
        normalized: "((a % b) > 10)",
        hash: 82868508035341089427090561311991876240,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE NOT (active = TRUE)",
        normalized: "NOT (active = true)",
        hash: 65851817300140767743482816604299764526,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE deleted_at IS NULL",
        normalized: "deleted_at IS NULL",
        hash: 260105666492253831600832914225129674266,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE deleted_at IS NOT NULL",
        normalized: "deleted_at IS NOT NULL",
        hash: 332224121843328788158539955817329118924,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE name LIKE 'A%'",
        normalized: "name LIKE 'A%'",
        hash: 314974045665179851130112309972384705706,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE name NOT LIKE 'A%'",
        normalized: "name NOT LIKE 'A%'",
        hash: 94303378871260595433799648522321568991,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age BETWEEN 18 AND 65",
        normalized: "age BETWEEN 18 AND 65",
        hash: 226023245311690468322616306060892911446,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE age NOT BETWEEN 18 AND 65",
        normalized: "age NOT BETWEEN 18 AND 65",
        hash: 114231437004172268307254474201086575722,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE status = 'paid'",
        normalized: "('paid' = status)",
        hash: 10488038035579433165683966987072516130,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE status = 'open' AND amount > 100",
        normalized: "(('open' = status) AND (amount > 100))",
        hash: 236186193320798656853084463803930322699,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'a')",
        normalized: "x IN (SELECT id FROM m WHERE owner = 'a')",
        hash: 266372329322768390664236543868795251272,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE x NOT IN (SELECT id FROM m WHERE owner = 'a')",
        normalized: "x NOT IN (SELECT id FROM m WHERE owner = 'a')",
        hash: 47926754135670659209261765469698082688,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT COUNT(*) FROM t",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT SUM(amount) FROM t WHERE amount > 10",
        normalized: "(amount > 10)",
        hash: 268413062225073979841068607631610090110,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT status, COUNT(*) FROM t WHERE active = TRUE GROUP BY status",
        normalized: "(active = true)",
        hash: 61822439498332721400316582865907409867,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT region, SUM(amount) FROM orders GROUP BY region HAVING SUM(amount) > 10",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT region, SUM(amount) FROM orders WHERE active = TRUE GROUP BY region HAVING COUNT(*) > 2",
        normalized: "(active = true)",
        hash: 61822439498332721400316582865907409867,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM t WHERE score = -5",
        normalized: "(- 5 = score)",
        hash: 263991339775984971083937534105788456146,
    },
    Golden {
        dialect: DialectKind::MySql,
        sql: "SELECT * FROM orders WHERE `Status` = 'paid'",
        normalized: "('paid' = `status`)",
        hash: 177394572371791273191020032091017557400,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age > 18",
        normalized: "(age > 18)",
        hash: 292377788459137190795462354130980277275,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age >= 18",
        normalized: "(age >= 18)",
        hash: 96959591457307944904555814042553578216,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age < 65",
        normalized: "(age < 65)",
        hash: 249764920661847514626832755980428109760,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age <= 65",
        normalized: "(age <= 65)",
        hash: 142133684043952530757668767522792284451,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age = 42",
        normalized: "(42 = age)",
        hash: 302037566819660740417919068379990631044,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age != 42",
        normalized: "(age != 42)",
        hash: 12683479535703509878171549340018863210,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a = 1 AND b = 2",
        normalized: "((1 = a) AND (2 = b))",
        hash: 131670881900975508605752108541534450950,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE b = 2 AND a = 1",
        normalized: "((1 = a) AND (2 = b))",
        hash: 131670881900975508605752108541534450950,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a = 1 OR b = 2",
        normalized: "((1 = a) OR (2 = b))",
        hash: 295580346008773016158339669541369738333,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE b = 2 OR a = 1",
        normalized: "((1 = a) OR (2 = b))",
        hash: 295580346008773016158339669541369738333,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE (a = 1 AND b = 2) AND c = 3",
        normalized: "(((1 = a) AND (2 = b)) AND (3 = c))",
        hash: 182759154872355133982334454209519610451,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a = 1 AND (b = 2 AND c = 3)",
        normalized: "(((1 = a) AND (2 = b)) AND (3 = c))",
        hash: 182759154872355133982334454209519610451,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE (a = 1 OR b = 2) OR c = 3",
        normalized: "(((1 = a) OR (2 = b)) OR (3 = c))",
        hash: 95758464856935287648057063807289397135,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a = 1 OR (b = 2 OR c = 3)",
        normalized: "(((1 = a) OR (2 = b)) OR (3 = c))",
        hash: 95758464856935287648057063807289397135,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE x IN (3, 1, 2)",
        normalized: "x IN (1, 2, 3)",
        hash: 235230893105783445659199798622162550252,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE x NOT IN (3, 1, 2)",
        normalized: "x NOT IN (1, 2, 3)",
        hash: 277416314403838883454401070234615665757,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE ((age > 18))",
        normalized: "(age > 18)",
        hash: 292377788459137190795462354130980277275,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a < b",
        normalized: "(a < b)",
        hash: 14746718521301734887795259058823844396,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE b < a",
        normalized: "(b < a)",
        hash: 164195086123485964968356541696287096375,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a + b > 10",
        normalized: "((a + b) > 10)",
        hash: 2811918157887371711831872217975099545,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a - b > 10",
        normalized: "((a - b) > 10)",
        hash: 74443246094558125713561247511123700650,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a * b > 10",
        normalized: "((a * b) > 10)",
        hash: 311378054065053570425432075285704523444,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a / b > 10",
        normalized: "((a / b) > 10)",
        hash: 291742834309287946499546270378476167205,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE a % b > 10",
        normalized: "((a % b) > 10)",
        hash: 82868508035341089427090561311991876240,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE NOT (active = TRUE)",
        normalized: "NOT (active = true)",
        hash: 65851817300140767743482816604299764526,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE deleted_at IS NULL",
        normalized: "deleted_at IS NULL",
        hash: 260105666492253831600832914225129674266,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE deleted_at IS NOT NULL",
        normalized: "deleted_at IS NOT NULL",
        hash: 332224121843328788158539955817329118924,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE name LIKE 'A%'",
        normalized: "name LIKE 'A%'",
        hash: 314974045665179851130112309972384705706,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE name NOT LIKE 'A%'",
        normalized: "name NOT LIKE 'A%'",
        hash: 94303378871260595433799648522321568991,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age BETWEEN 18 AND 65",
        normalized: "age BETWEEN 18 AND 65",
        hash: 226023245311690468322616306060892911446,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE age NOT BETWEEN 18 AND 65",
        normalized: "age NOT BETWEEN 18 AND 65",
        hash: 114231437004172268307254474201086575722,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE status = 'paid'",
        normalized: "('paid' = status)",
        hash: 10488038035579433165683966987072516130,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE status = 'open' AND amount > 100",
        normalized: "(('open' = status) AND (amount > 100))",
        hash: 236186193320798656853084463803930322699,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE x IN (SELECT id FROM m WHERE owner = 'a')",
        normalized: "x IN (SELECT id FROM m WHERE owner = 'a')",
        hash: 266372329322768390664236543868795251272,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE x NOT IN (SELECT id FROM m WHERE owner = 'a')",
        normalized: "x NOT IN (SELECT id FROM m WHERE owner = 'a')",
        hash: 47926754135670659209261765469698082688,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT COUNT(*) FROM t",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT SUM(amount) FROM t WHERE amount > 10",
        normalized: "(amount > 10)",
        hash: 268413062225073979841068607631610090110,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT status, COUNT(*) FROM t WHERE active = TRUE GROUP BY status",
        normalized: "(active = true)",
        hash: 61822439498332721400316582865907409867,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT region, SUM(amount) FROM orders GROUP BY region HAVING SUM(amount) > 10",
        normalized: "TRUE",
        hash: 121277097519756504974338435796877478995,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT region, SUM(amount) FROM orders WHERE active = TRUE GROUP BY region HAVING COUNT(*) > 2",
        normalized: "(active = true)",
        hash: 61822439498332721400316582865907409867,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM t WHERE score = -5",
        normalized: "(- 5 = score)",
        hash: 263991339775984971083937534105788456146,
    },
    Golden {
        dialect: DialectKind::SQLite,
        sql: "SELECT * FROM orders WHERE \"Status\" = 'paid'",
        normalized: "(\"status\" = 'paid')",
        hash: 112717352067792148743778502094369901298,
    },
];

fn normalize_with_dialect(sql: &str, dialect: DialectKind) -> String {
    match dialect {
        DialectKind::PostgreSql => Canonicalizer::new(&PostgreSqlDialect {}).normalize_sql(sql),
        DialectKind::MySql => Canonicalizer::new(&MySqlDialect {}).normalize_sql(sql),
        DialectKind::SQLite => Canonicalizer::new(&SQLiteDialect {}).normalize_sql(sql),
    }
    .unwrap()
}

#[test]
fn matches_subql_compatibility_surface() {
    assert!(GOLDENS.len() >= 100);
    for golden in GOLDENS {
        let normalized = normalize_with_dialect(golden.sql, golden.dialect);
        assert_eq!(normalized, golden.normalized, "SQL: {}", golden.sql);
        assert_eq!(
            hash_canonical(&normalized),
            golden.hash,
            "SQL: {}",
            golden.sql
        );
    }
}

#[test]
fn distinct_golden_text_has_distinct_hashes() {
    let mut normalized_by_hash = BTreeMap::new();
    for golden in GOLDENS {
        if let Some(previous) = normalized_by_hash.insert(golden.hash, golden.normalized) {
            assert_eq!(previous, golden.normalized, "hash: {}", golden.hash);
        }
    }
}

#[test]
fn golden_canonical_forms_are_idempotent() {
    for golden in GOLDENS {
        let sql = if golden.normalized == "TRUE" {
            "SELECT * FROM t".to_string()
        } else {
            format!("SELECT * FROM t WHERE {}", golden.normalized)
        };
        let second = normalize_with_dialect(&sql, golden.dialect);
        assert_eq!(second, golden.normalized, "SQL: {}", golden.sql);
    }
}
