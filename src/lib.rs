#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

use alloc::string::String;

use thiserror::Error;

mod canonicalize;

pub use canonicalize::{hash_canonical, normalize_sql, normalize_where_clause};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum CanonicalizeError {
    #[error("SQL parse error at line {line}, column {column}: {message}")]
    Parse {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("Unsupported SQL: {0}")]
    Unsupported(String),
}
