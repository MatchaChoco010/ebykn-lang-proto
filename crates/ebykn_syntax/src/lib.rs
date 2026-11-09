//! 字句・構文・AST（仕様 §4、§5）と、構文の段階の診断（仕様 §10.1）。
//!
//! 入口は [`parse_eyk`]。`.eyk` を読んで [`ast::File`] と診断を返す。

// AST の列挙は節点の大きさがまちまちで、箱に入れると使う側が読みにくくなる。
#![allow(clippy::large_enum_variant)]

pub mod ast;
pub mod diag;
mod parse;
pub mod span;

pub use parse::{Parsed, parse_eyk};
