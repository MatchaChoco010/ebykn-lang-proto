//! 字句・構文・AST（仕様 §4、§5）と、構文の段階の診断（仕様 §10.1）。
//!
//! 入口は [`parse_eyk`]。`.eyk` を読んで [`ast::File`] と診断を返す。

// AST の列挙は節点の大きさがまちまちで、箱に入れると使う側が読みにくくなる。
#![allow(clippy::large_enum_variant)]

pub mod ast;
pub mod diag;
pub mod macros;
mod parse;
pub mod reserved;
pub mod span;

pub use parse::{Parsed, parse_eyk};
pub use reserved::VERUS_BUILTIN_CALLS;

/// `.eyk` で呼べるマクロ（#16 の既定）。std のマクロと、vstd の仕様用のマクロ。
/// 裸の名前か、std / core / alloc（vstd のマクロは vstd）から始まるパスで呼べる。
pub const ALLOWED_MACROS: &[&str] = &[
    // std
    "vec",
    "println",
    "print",
    "eprintln",
    "eprint",
    "format",
    "format_args",
    "write",
    "writeln",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "unreachable",
    "unimplemented",
    "todo",
    "matches",
    "concat",
    "stringify",
    "line",
    "column",
    "module_path",
    "include_str",
    "include_bytes",
    "compile_error",
    // vstd
    "seq",
    "set",
    "map",
];

/// [`ALLOWED_MACROS`] のうち vstd のもの。引数を Ebykn の式として読む。
pub const VSTD_MACROS: &[&str] = &["seq", "set", "map"];
