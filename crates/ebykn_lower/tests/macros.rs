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

/// `extern` ブロックの中の宣言に付いたマクロも記録する（PR #57 のレビューの N1）。
#[test]
fn macros_on_foreign_items_are_recorded() {
    records(
        "unsafe extern \"C\" {\n    #[a::b]\n    pub safe fn abs(x: i32) -> i32;\n}",
        A,
        "a::b",
        "safe fn abs (x : i32) -> i32 in single",
    );
    assert!(
        macros("unsafe extern \"C\" { #[link_name = \"abs\"] pub safe fn abs(x: i32) -> i32; }")
            .is_empty()
    );
}

/// `Ebykn.toml` の依存クレートの名前が判定に届く（PR #57 のレビューの S2、B3）。`clippy` という名前の
/// 依存クレートがあれば、`#[clippy::x]` はそのクレートのマクロになる。
#[test]
fn dependency_names_reach_the_classification() {
    use ebykn_check::project::TomlValue;
    let src = "#[clippy::zero]\npub fn one() -> u32 { 1 }\n#[derive(::core::clone::Clone)]\npub struct S {}\n";
    let lower_with = |deps: Vec<(String, TomlValue)>, crates: Option<&[String]>| {
        let (mut p, d) = project::single(src);
        assert!(d.is_empty(), "{d:?}");
        p.manifest.dependencies = deps;
        let c = ebykn_check::check(&p);
        ebykn_lower::lower_with_crates(&p, &c, crates)
            .unwrap()
            .macros
            .into_iter()
            .map(|m| m.path)
            .collect::<Vec<_>>()
    };
    // 依存がなければ、どちらもツールの属性と std の derive。
    assert!(lower_with(vec![], None).is_empty());
    // 改名で `clippy` という名前にした依存。
    let renamed = vec![(
        "clippy".to_string(),
        TomlValue::Table(vec![
            ("path".into(), TomlValue::Str("x".into())),
            ("package".into(), TomlValue::Str("x".into())),
        ]),
    )];
    assert_eq!(lower_with(renamed, None), ["clippy::zero"]);
    // registry の依存（`[lib] name` が分からない）なら、どちらも前提として出す。
    let registry = vec![("serde".to_string(), TomlValue::Str("1".into()))];
    assert_eq!(
        lower_with(registry.clone(), None),
        ["clippy::zero", "::core::clone::Clone"]
    );
    // `verify` が `cargo metadata` で名前を確定したら、その名前で決め直す。
    assert!(lower_with(registry.clone(), Some(&["serde".to_string()])).is_empty());
    assert_eq!(
        lower_with(registry, Some(&["serde".to_string(), "core".to_string()])),
        ["::core::clone::Clone"]
    );
}

/// path の依存の `[lib] name` を読む（`innocent` の `[lib] name = "clippy"`）。
#[test]
fn path_dependencies_are_used_by_their_lib_name() {
    let dir = std::env::temp_dir().join(format!("ebykn-libname-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("innocent")).unwrap();
    let pin = ebykn_check::project::single("").0.manifest;
    std::fs::write(
        dir.join("Ebykn.toml"),
        format!(
            "[package]\nname = \"k\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[verus]\nrelease = \"{}\"\ncommit = \"{}\"\n\n[dependencies]\ninnocent = {{ path = \"innocent\" }}\n",
            pin.verus_release, pin.verus_commit
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("innocent/Cargo.toml"),
        "[package]\nname = \"innocent\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"clippy\"\nproc-macro = true\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/lib.eyk"),
        "#[clippy::zero]\npub fn one() -> u32 { 1 }\n",
    )
    .unwrap();
    let (p, d) = project::load(&dir);
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(d.is_empty(), "{d:?}");
    let paths: Vec<String> = out.unwrap().macros.into_iter().map(|m| m.path).collect();
    assert_eq!(paths, ["clippy::zero"]);
}
