//! 依存クレートの derive と属性マクロが付いた項目を、書き下しが前提として記録する（#49）。
//! 項目の種類（本体の中の項目、impl・trait の中の項目、トークン列のまま持つ項目を含む）ごとに
//! 確かめる。`verify` がこの記録を前提の一覧に出すことは、golden（`tests/golden/external_macros`）を
//! `cargo xtask golden-verify` が確かめる。

use ebykn_check::project;
use ebykn_lower::{MacroUse, lower};
use ebykn_syntax::macros::MacroKind;

fn macros(src: &str) -> Vec<MacroUse> {
    let (p, d) = project::single(src);
    assert!(d.is_empty(), "{d:?}");
    let c = ebykn_check::check(&p);
    assert!(
        c.diags.iter().all(|d| d.code().is_warning()),
        "{:?}\n---\n{src}",
        c.diags
    );
    lower(&p, &c).unwrap().macros
}

#[track_caller]
fn records(src: &str, kind: MacroKind, path: &str, item: &str) {
    let got = macros(src);
    assert!(
        got.iter()
            .any(|m| m.kind == kind && m.path == path && m.item == item && m.rel == "src/lib.rs"),
        "{kind:?} {path} on {item} がない: {got:?}\n---\n{src}"
    );
}

const D: MacroKind = MacroKind::Derive;
const A: MacroKind = MacroKind::Attribute;

#[test]
fn derives_on_types_are_recorded() {
    records(
        "#[derive(Clone, serde::Serialize)]\npub struct Foo { a: u32 }",
        D,
        "serde::Serialize",
        "single::Foo",
    );
    records(
        "#[derive(serde::Serialize)]\npub enum E { A, B }",
        D,
        "serde::Serialize",
        "single::E",
    );
    records(
        "mod m { #[derive(serde::Serialize)] pub struct Foo {} }",
        D,
        "serde::Serialize",
        "single::m::Foo",
    );
    records(
        "#[cfg_attr(feature = \"s\", derive(serde::Serialize))]\npub struct Foo {}",
        D,
        "serde::Serialize",
        "single::Foo",
    );
    records(
        "use serde::Serialize as Clone;\n#[derive(Clone)]\npub struct Foo {}",
        D,
        "Clone",
        "single::Foo",
    );
    records(
        "#[derive(serde::Serialize)]\nunion U { a: u32 }",
        D,
        "serde::Serialize",
        "union U in single",
    );
    records(
        "pub fn f() { #[derive(serde::Serialize)] struct T {} }",
        D,
        "serde::Serialize",
        "single::T",
    );
}

#[test]
fn attribute_macros_on_items_are_recorded() {
    records(
        "#[tokio::main]\npub fn main() {}",
        A,
        "tokio::main",
        "single::main",
    );
    records(
        "pub struct S {}\n#[a::b]\nimpl S { pub fn f(&self) {} }",
        A,
        "a::b",
        "impl S",
    );
    records(
        "pub struct S {}\nimpl S { #[a::b] pub fn f(&self) {} }",
        A,
        "a::b",
        "single::S::f",
    );
    records(
        "pub struct S {}\npub trait Tr { fn f(&self); }\n#[a::b]\nimpl Tr for S { fn f(&self) {} }",
        A,
        "a::b",
        "impl Tr for S",
    );
    records(
        "#[a::b]\npub trait Tr { fn f(&self); }",
        A,
        "a::b",
        "single::Tr",
    );
    records(
        "pub trait Tr { #[a::b] fn f(&self); }",
        A,
        "a::b",
        "single::Tr::f",
    );
    records("#[a::b]\nmod m {}", A, "a::b", "single::m");
    records("#[a::b]\npub const C: u32 = 1;", A, "a::b", "single::C");
    records("#[a::b]\npub static C: u32 = 1;", A, "a::b", "single::C");
    records("#[a::b]\npub type T = u32;", A, "a::b", "single::T");
    records(
        "#[a::b]\nuse std::vec::Vec;",
        A,
        "a::b",
        "use std :: vec :: Vec in single",
    );
    records("pub fn f() { #[a::b] fn g() {} }", A, "a::b", "single::g");
    records(
        "#[cfg_attr(test, a::b)]\npub fn f() {}",
        A,
        "a::b",
        "single::f",
    );
    records(
        "use evil::x as test;\n#[test]\nfn t() {}",
        A,
        "test",
        "single::t",
    );
}

#[test]
fn std_derives_and_builtin_attributes_are_not_recorded() {
    let src = "#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]\npub struct Foo {}\n\
#[derive(core::clone::Clone, ::std::fmt::Debug)]\npub struct Bar {}\n\
#[inline]\n#[must_use]\n#[allow(dead_code)]\n#[doc = \"x\"]\n#[cfg_attr(test, inline)]\n#[rustfmt::skip]\npub fn f() -> u32 { 1 }\n\
#[test]\nfn t() {}\n";
    let got = macros(src);
    assert!(got.is_empty(), "{got:?}");
}

/// 一つのマクロは一度だけ記録する（属性をトークン列のまま持つ項目を含む）。
#[test]
fn each_macro_is_recorded_once() {
    for src in [
        "#[derive(serde::Serialize)]\nunion U { a: u32 }",
        "#[a::b]\nuse std::vec::Vec;",
        "#[a::b]\npub fn f() {}",
        "pub struct S {}\nimpl S { #[a::b] pub fn f(&self) {} }",
        "pub trait Tr { #[a::b] fn f(&self); }",
    ] {
        assert_eq!(macros(src).len(), 1, "{:?}\n---\n{src}", macros(src));
    }
}
