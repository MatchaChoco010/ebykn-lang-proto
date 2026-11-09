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
    ("r#override", "r#override"),
    // 予約語の raw の綴りは、組み込みの `unsafe(..)` の包みではない（PR #57 の再レビューの B1）。
    ("r#unsafe(allow(unused))", "r#unsafe"),
    ("cfg_attr(all(), r#unsafe(allow(unused)))", "r#unsafe"),
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

/// Verus の属性として別の文言で拒否するかを、属性のパスだけで決める（値の `verus_spec` は見ない。
/// #57 の再確認の N4）。
#[test]
fn the_value_of_an_inner_attribute_does_not_skip_the_check() {
    let (_, lines) = run("#![evil::x = verus_spec]\npub fn f() {}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("the attribute `evil::x` cannot be written here")),
        "{lines:?}"
    );
    // パスの最後が `verus_spec` なら Verus の属性として拒否する（位置の文言は重ねない）。
    let (_, lines) = run("#![a::verus_spec]\npub fn f() {}");
    assert!(
        lines.iter().any(|l| l.contains("`#[verus_spec]`")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("cannot be written here")),
        "{lines:?}"
    );
}

/// 組み込みの属性は、どの位置にも書ける。
/// ツールの属性は、内側の位置では書けず、文・式の位置では書ける（何もしない属性で、安定版の Rust も
/// 許す。PR #57 の再レビューの nit）。
#[test]
fn tool_attributes_are_rejected_only_as_inner_attributes() {
    for src in [
        "#![rustfmt::skip]\npub fn f() {}",
        "mod m { #![rustfmt::skip] }",
    ] {
        let (_, lines) = run(src);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("the attribute `rustfmt::skip` cannot be written here")),
            "{lines:?}\n---\n{src}"
        );
    }
    accepts("pub fn f() -> u32 { #[rustfmt::skip] let x: u32 = 1; x }");
    accepts("pub fn f() -> u32 { #[clippy::x] 1 }");
    // 先頭の `::` はクレートの名前で、ツールではない。
    let (_, lines) = run("pub fn f() -> u32 { #[::rustfmt::skip] 1 }");
    assert!(
        lines.iter().any(|l| l.contains("`::rustfmt::skip`")),
        "{lines:?}"
    );
}

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
