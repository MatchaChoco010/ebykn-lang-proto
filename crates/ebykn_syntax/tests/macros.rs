//! 手続きマクロの判定（`ebykn_syntax::macros` の表。#49）の綴りの総当たり。
//!
//! 表のすべての名前について、std や組み込みとして数える綴りと、前提として出す綴り（パス付き、
//! `cfg_attr` の中、名前を覆い隠す `use` / `mod` / `extern crate` / 依存クレート）を試す。
//! 表に名前を足せば、ここが自動で効く。

use std::str::FromStr as _;

use ebykn_syntax::macros::{
    BUILTIN_ATTRS, Bindings, ExternalMacro, MacroKind, PRELUDE_ATTR_MACROS, STD_DERIVES,
    TOOL_NAMESPACES, external_macros_in_tokens,
};
use proc_macro2::TokenStream;

/// `src` 全体から束縛を集め（`deps` は依存クレートがコードで使われる名前。`*` は「すべては分からない」）、
/// `attrs`（項目の前に並ぶ属性）を判定する。
fn found(src: &str, attrs: &str, deps: &[&str]) -> Vec<ExternalMacro> {
    let mut b = Bindings::default();
    for d in deps {
        if *d == "*" {
            b.crate_names_unknown();
        } else {
            b.add_crate(d);
        }
    }
    b.scan(&TokenStream::from_str(src).unwrap_or_else(|e| panic!("{e}: {src}")));
    let ts = TokenStream::from_str(&format!("{attrs} pub struct S {{}}")).unwrap();
    external_macros_in_tokens(&ts, &b)
}

#[track_caller]
fn none(src: &str, attrs: &str, deps: &[&str]) {
    let f = found(src, attrs, deps);
    assert!(
        f.is_empty(),
        "前提として出た: {f:?}\n--- {src}\n--- {attrs}"
    );
}

#[track_caller]
fn listed(src: &str, attrs: &str, deps: &[&str], kind: MacroKind, path: &str) {
    let f = found(src, attrs, deps);
    assert!(
        f.iter().any(|m| m.kind == kind && m.path == path),
        "{kind:?} {path} が出ない: {f:?}\n--- {src}\n--- {attrs}"
    );
}

/// std の derive として数える綴り（`NAME` を名前に置き換える）。
const STD_DERIVE_OK: &[(&str, &str)] = &[
    ("", "#[derive(NAME)]"),
    ("", "#[derive(Clone, NAME)]"),
    ("", "#[derive(std::NAME)]"),
    ("", "#[derive(core::m::NAME)]"),
    ("", "#[derive(::core::m::NAME)]"),
    ("", "#[derive(::std::NAME)]"),
    ("", "#[cfg_attr(test, derive(NAME))]"),
    (
        "",
        "#[cfg_attr(all(), cfg_attr(test, derive(core::m::NAME)))]",
    ),
    ("use std::m::NAME;", "#[derive(NAME)]"),
    ("use core::{m::NAME};", "#[derive(NAME)]"),
    ("use foo::NAME as Other;", "#[derive(NAME)]"),
];

/// 前提として出す綴り（パスは `PATH`）。
const STD_DERIVE_LISTED: &[(&str, &str, &str)] = &[
    ("", "#[derive(evil::NAME)]", "evil::NAME"),
    ("", "#[derive(::evil::NAME)]", "::evil::NAME"),
    ("", "#[derive(crate::NAME)]", "crate::NAME"),
    ("", "#[cfg_attr(test, derive(evil::NAME))]", "evil::NAME"),
    ("use evil::NAME;", "#[derive(NAME)]", "NAME"),
    ("use evil::{a, NAME};", "#[derive(NAME)]", "NAME"),
    ("use evil::x as NAME;", "#[derive(NAME)]", "NAME"),
    ("use evil::{x as NAME};", "#[derive(NAME)]", "NAME"),
    ("pub use evil::NAME;", "#[derive(NAME)]", "NAME"),
    ("use std::m::Other as NAME;", "#[derive(NAME)]", "NAME"),
    ("fn f() { use evil::NAME; }", "#[derive(NAME)]", "NAME"),
    ("mod m { pub use evil::NAME; }", "#[derive(NAME)]", "NAME"),
    (
        "mod core { pub use evil::*; }",
        "#[derive(core::NAME)]",
        "core::NAME",
    ),
    ("mod m { mod std {} }", "#[derive(std::NAME)]", "std::NAME"),
    // 先頭の `::` でも、同じ名前を束縛していれば std を指すと言えない（PR #57 のレビューの B3）。
    (
        "mod core {}",
        "#[derive(::core::m::NAME)]",
        "::core::m::NAME",
    ),
];

/// 依存クレートの名前で std の derive と数えなくなる綴り（`deps`、`src`、属性、パス）。`*` は名前が
/// 分からない。
type ByCrate = (
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
);
const STD_DERIVE_BY_CRATE: &[ByCrate] = &[
    // `core` という名前の依存クレート（`[lib] name = "core"` など）。
    (
        &["core"],
        "",
        "#[derive(::core::clone::NAME)]",
        "::core::clone::NAME",
    ),
    (&["core"], "", "#[derive(core::NAME)]", "core::NAME"),
    (
        &["core"],
        "use core::clone::NAME;",
        "#[derive(NAME)]",
        "NAME",
    ),
    (
        &["core"],
        "use core::{clone::NAME};",
        "#[derive(NAME)]",
        "NAME",
    ),
    (&["std"], "use std::m::NAME;", "#[derive(NAME)]", "NAME"),
    // 名前が分からなければ、std のパスも std から取り込んだ名前も数えない。
    (&["*"], "", "#[derive(::core::m::NAME)]", "::core::m::NAME"),
    (&["*"], "", "#[derive(std::NAME)]", "std::NAME"),
    (&["*"], "use std::m::NAME;", "#[derive(NAME)]", "NAME"),
];

#[test]
fn every_std_derive_is_classified_in_every_spelling() {
    let mut tried = 0;
    for name in STD_DERIVES {
        let fill = |s: &str| s.replace("NAME", name);
        for (src, attrs) in STD_DERIVE_OK {
            none(&fill(src), &fill(attrs), &[]);
            tried += 1;
        }
        for (src, attrs, path) in STD_DERIVE_LISTED {
            listed(
                &fill(src),
                &fill(attrs),
                &[],
                MacroKind::Derive,
                &fill(path),
            );
            tried += 1;
        }
        for (deps, src, attrs, path) in STD_DERIVE_BY_CRATE {
            listed(
                &fill(src),
                &fill(attrs),
                deps,
                MacroKind::Derive,
                &fill(path),
            );
            tried += 1;
        }
        // 裸の名前の derive は、依存クレートの名前では覆い隠せない（名前空間が違う）。
        none("", &fill("#[derive(NAME)]"), &["*", "core"]);
    }
    assert!(tried >= 9 * 33, "{tried}");
}

#[test]
fn other_derives_are_listed() {
    for attrs in [
        "#[derive(serde::Serialize)]",
        "#[derive(Clone, serde::Serialize)]",
        "#[cfg_attr(feature = \"x\", derive(serde::Serialize))]",
        "#[cfg_attr(all(), derive(Clone), derive(serde::Serialize))]",
    ] {
        listed("", attrs, &[], MacroKind::Derive, "serde::Serialize");
    }
    listed(
        "use serde::Serialize;",
        "#[derive(Serialize)]",
        &[],
        MacroKind::Derive,
        "Serialize",
    );
    // `derive` そのものを覆い隠すと、`#[derive(..)]` は属性マクロ。
    listed(
        "use evil::x as derive;",
        "#[derive(Clone)]",
        &[],
        MacroKind::Attribute,
        "derive",
    );
}

#[test]
fn builtin_attributes_are_not_listed() {
    for name in BUILTIN_ATTRS.iter().chain(PRELUDE_ATTR_MACROS) {
        none("", &format!("#[{name}]"), &[]);
        // `derive` の引数は derive のパス（std の derive を入れる）。
        let arg = if *name == "derive" { "Clone" } else { "x" };
        none("", &format!("#[{name}({arg})]"), &[]);
        none("", &format!("#[{name} = \"x\"]"), &[]);
        none("", &format!("#[cfg_attr(test, {name})]"), &[]);
        none("", &format!("#[unsafe({name})]"), &[]);
    }
    for ns in TOOL_NAMESPACES {
        none("", &format!("#[{ns}::skip]"), &[]);
        none("", &format!("#[{ns}::a::b(c)]"), &[]);
    }
    // std のパスの derive も derive（PR #57 のレビューの N2）。
    none("", "#[core::prelude::v1::derive(Clone)]", &[]);
    listed(
        "",
        "#[core::prelude::v1::derive(evil::X)]",
        &[],
        MacroKind::Derive,
        "evil::X",
    );
}

/// `#[r#override]` は利用者が束縛した名前（素の `#[override]` は構文解析が読めない）。
/// `#[feature]` は組み込みとして数えない（PR #57 のレビューの B2、B1）。
#[test]
fn override_and_feature_are_listed() {
    for src in ["use evil::zero as r#override;", ""] {
        listed(
            src,
            "#[r#override]",
            &[],
            MacroKind::Attribute,
            "r#override",
        );
    }
    listed("", "#[feature(x)]", &[], MacroKind::Attribute, "feature");
}

/// 名前を覆い隠せる組み込み（std の prelude のマクロ）を、覆い隠す綴り。
const SHADOW: &[&str] = &[
    "use evil::NAME;",
    "use evil::x as NAME;",
    "use evil::{x as NAME};",
    "use evil::{a, b::{NAME}};",
    "pub(crate) use evil::NAME;",
    "fn f() { use evil::x as NAME; }",
];

#[test]
fn shadowed_prelude_attribute_macros_are_listed() {
    for name in PRELUDE_ATTR_MACROS {
        for src in SHADOW {
            let src = src.replace("NAME", name);
            // `derive` は `(..)` の中ではなく属性そのものが外のマクロになる。
            listed(&src, &format!("#[{name}]"), &[], MacroKind::Attribute, name);
            listed(
                &src,
                &format!("#[cfg_attr(test, {name})]"),
                &[],
                MacroKind::Attribute,
                name,
            );
        }
        // std から改名せずに取り込むのは std のもの。
        none(
            &format!("use core::prelude::v1::{name};"),
            &format!("#[{name}]"),
            &[],
        );
    }
}

#[test]
fn tool_namespaces_shadowed_by_a_crate_name_are_listed() {
    for ns in TOOL_NAMESPACES {
        let path = format!("{ns}::skip");
        let attrs = format!("#[{path}]");
        listed("", &attrs, &[ns], MacroKind::Attribute, &path);
        listed(
            &format!("extern crate foo as {ns};"),
            &attrs,
            &[],
            MacroKind::Attribute,
            &path,
        );
        listed(
            &format!("extern crate {ns};"),
            &attrs,
            &[],
            MacroKind::Attribute,
            &path,
        );
        listed(
            &format!("use foo as {ns};"),
            &attrs,
            &[],
            MacroKind::Attribute,
            &path,
        );
        listed(
            &format!("use foo::{ns}::{{self}};"),
            &attrs,
            &[],
            MacroKind::Attribute,
            &path,
        );
        listed(
            &format!("mod {ns} {{}}"),
            &attrs,
            &[],
            MacroKind::Attribute,
            &path,
        );
        // 依存クレートの名前が分からなければ、ツールの属性として数えない。
        listed("", &attrs, &["*"], MacroKind::Attribute, &path);
        // 先頭の `::` はクレートの名前。
        listed(
            "",
            &format!("#[::{path}]"),
            &[],
            MacroKind::Attribute,
            &format!("::{path}"),
        );
    }
    // 依存クレートの名前の `-` は `_`。
    listed(
        "",
        "#[rust_analyzer::x]",
        &["rust-analyzer"],
        MacroKind::Attribute,
        "rust_analyzer::x",
    );
}

#[test]
fn attribute_macros_are_listed() {
    for (attrs, path) in [
        ("#[tokio::main]", "tokio::main"),
        ("#[::tokio::main]", "::tokio::main"),
        ("#[some_crate::a::attr(x = 1)]", "some_crate::a::attr"),
        ("#[main]", "main"),
        ("#[cfg_attr(test, tokio::main)]", "tokio::main"),
        ("#[cfg_attr(all(), inline, tokio::main)]", "tokio::main"),
        ("#[cfg_attr(a, cfg_attr(b, tokio::main))]", "tokio::main"),
        ("#[unsafe(tokio::main)]", "tokio::main"),
        ("#[inline] #[tokio::main]", "tokio::main"),
    ] {
        listed("", attrs, &[], MacroKind::Attribute, path);
    }
    // 予約語の raw の綴りは予約語とは別の名前で、利用者が束縛した名前（PR #57 の再レビューの B1）。
    // `r#unsafe(..)` を組み込みの `unsafe(..)` の包みとして読まない。
    for src in ["use evil::zero as r#unsafe;", ""] {
        for attrs in [
            "#[r#unsafe(allow(dead_code))]",
            "#[r#unsafe]",
            "#[cfg_attr(test, r#unsafe(allow(dead_code)))]",
            "#[unsafe(r#unsafe(allow(dead_code)))]",
        ] {
            listed(src, attrs, &[], MacroKind::Attribute, "r#unsafe");
        }
    }
}

/// 予約語でない名前の raw の綴り（`r#derive`、`r#inline`）は、同じ名前の組み込みの属性（rustc も同じ
/// 名前として読む）。
#[test]
fn raw_spellings_of_builtin_names_are_the_same_names() {
    for name in BUILTIN_ATTRS.iter().chain(PRELUDE_ATTR_MACROS) {
        let arg = if *name == "derive" { "Clone" } else { "x" };
        none("", &format!("#[r#{name}({arg})]"), &[]);
    }
    none("", "#[derive(r#Clone)]", &[]);
    listed(
        "",
        "#[r#derive(evil::X)]",
        &[],
        MacroKind::Derive,
        "evil::X",
    );
    listed(
        "use evil::x as r#derive;",
        "#[derive(Clone)]",
        &[],
        MacroKind::Attribute,
        "derive",
    );
    // 組み込みの名前でも、別の名前空間のパスは外のマクロ。
    listed(
        "",
        "#[evil::inline]",
        &[],
        MacroKind::Attribute,
        "evil::inline",
    );
    listed(
        "",
        "#[evil::derive(Clone)]",
        &[],
        MacroKind::Attribute,
        "evil::derive",
    );
}
