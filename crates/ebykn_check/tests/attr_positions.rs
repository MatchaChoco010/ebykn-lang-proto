//! 項目の外側の属性のほかの位置（`#![..]`、文・式・`let`・match の腕）には、Rust の組み込みの属性
//! だけを書ける（#49、PR #57 のレビューの B1）。Verus は `custom_inner_attributes` と
//! `stmt_expr_attributes` を有効にして動くので、これらの位置の属性マクロも検証では効き、マクロが作る
//! コードが前提の一覧に出ないまま verified になる。

mod support;
use support::*;

/// `ATTR` を属性の中身に置き換える。どれも E0001（属性の位置）になる。
const POSITIONS: &[&str] = &[
    "#![ATTR]\npub fn f() {}",
    "mod m { #![ATTR] }",
    "pub fn f() -> u32 { #[ATTR] let x: u32 = 1; x }",
    "pub fn f() -> u32 { #[ATTR] 1 }",
    "pub fn f() -> u32 { let g = #[ATTR] || 1; g() }",
    "pub fn f(x: u32) -> u32 { match x { #[ATTR] 0 => 1, _ => 2 } }",
    "pub fn f() { let v: u32 = 1; #[ATTR] { v; } }",
    "pub const C: u32 = #[ATTR] 1;",
];

/// 組み込みでない属性の中身と、診断に出る書き方。
const NON_BUILTIN: &[(&str, &str)] = &[
    ("evil::modimpl", "evil::modimpl"),
    ("::evil::x", "::evil::x"),
    ("main", "main"),
    ("feature(prelude_import)", "feature"),
    ("feature(proc_macro_hygiene)", "feature"),
    ("cfg_attr(all(), evil::x)", "evil::x"),
    ("cfg_attr(test, allow(unused), evil::x)", "evil::x"),
    ("unsafe(evil::x)", "evil::x"),
    ("rustfmt::skip", "rustfmt::skip"),
    ("r#override", "override"),
];

#[test]
fn non_builtin_attributes_are_rejected_in_every_position() {
    let mut tried = 0;
    for pos in POSITIONS {
        for (attr, written) in NON_BUILTIN {
            let src = pos.replace("ATTR", attr);
            let (_, lines) = run(&src);
            assert!(
                lines.iter().any(|l| l.contains(": E0001: ")
                    && l.contains(&format!("the attribute `{written}` cannot be written here"))),
                "拒否されない: {lines:?}\n---\n{src}"
            );
            tried += 1;
        }
    }
    assert_eq!(tried, POSITIONS.len() * NON_BUILTIN.len());
}

/// 組み込みの属性は、どの位置にも書ける。
#[test]
fn builtin_attributes_are_accepted() {
    for src in [
        "#![allow(dead_code)]\npub fn f() {}",
        "#![cfg_attr(test, allow(unused))]\npub fn f() {}",
        "mod m { #![allow(unused)] }",
        "pub fn f() -> u32 { #[allow(unused)] let x: u32 = 1; x }",
        "pub fn f() -> u32 { #[allow(unused_parens)] (1) }",
        "#[inline]\npub fn f() {}",
    ] {
        accepts(src);
    }
}

/// 項目の外側の属性は拒否しない（依存クレートのマクロは前提として一覧に出す）。
#[test]
fn macros_on_items_are_accepted() {
    for src in [
        "#[derive(serde::Serialize)]\npub struct S {}",
        "#[tokio::main]\npub fn main() {}",
        "pub fn f() { #[derive(serde::Serialize)] struct T {} }",
    ] {
        accepts(src);
    }
}

/// 日本語の文言もある。
#[test]
fn the_message_has_japanese_text() {
    let (p, _) = ebykn_check::project::single("#![evil::x]\npub fn f() {}");
    let c = ebykn_check::check(&p);
    let (_, d) = ebykn_check::project::single("#![evil::x]\npub fn f() {}");
    let all: Vec<_> = d.iter().chain(c.diags.iter()).collect();
    assert!(
        all.iter().any(|d| d
            .render(p.file(d.span.file), ebykn_syntax::diag::Lang::Ja)
            .contains("属性 `evil::x` はここに書けません")),
        "{all:?}"
    );
}
