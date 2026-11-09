//! Verus が raw の綴り（`r#foo`）を扱えない項目の名前は E0104（#147）。
//!
//! 固定した Verus は、`const`・`static`・`const fn`・本体のない trait の関数の名前の raw の綴りで
//! proc macro が panic し、列挙子と列挙子のフィールドは `no variant` / `no field` で拒否する。予約語で
//! ない `r#foo` でも落ちる（`tests/verus-probes/raw_name_*.rs`）。trait の関連型は、Rust の予約語の
//! raw の綴りだけが落ちる。ほかの項目の名前は raw の綴りのまま通る（`raw_name_items.rs`）。

mod support;

use std::path::PathBuf;

use ebykn_check::project::load;
use ebykn_syntax::diag::Lang;
use ebykn_syntax::reserved::RAW_KEYWORDS;
use support::*;

/// `NAME` を名前に置き換える。
fn fill(t: &str, name: &str) -> String {
    t.replace("NAME", name)
}

/// raw の綴りなら予約語でなくても E0104 になる位置。
const REJECTED: &[&str] = &[
    "pub const NAME: u64 = 3;",
    "pub static NAME: u64 = 3;",
    "pub const fn NAME() -> u64 { 3 }",
    "pub struct S;\nimpl S { pub const NAME: u64 = 1; }",
    "pub struct S;\nimpl S { pub const fn NAME(&self) -> u64 { 1 } }",
    "pub fn f() -> u64 { const NAME: u64 = 1; 1 }",
    "pub trait T { fn NAME(&self) -> u64; }",
    "pub trait T { spec fn NAME(&self) -> int; }",
    "pub trait T { lemma fn NAME(&self); }",
    "pub enum E { NAME, B }",
    "pub enum E { NAME(u64), B }",
    "pub enum E { NAME { x: u64 }, B }",
    "pub enum E { A { NAME: u64 }, B }",
];

/// raw の綴りのまま通る位置。固定した Verus が通すことを `raw_name_items.rs` で確かめた。
const ACCEPTED: &[&str] = &[
    "pub fn NAME() {}\npub fn f() { crate::NAME(); }",
    "pub struct NAME { pub x: u64 }",
    "pub struct S { pub NAME: u64 }\npub fn f(s: S) -> u64 { s.NAME }",
    "pub struct S { pub x: u64 } spec { NAME: int }",
    "pub enum NAME { A, B }",
    "pub trait NAME { fn m(&self) -> u64 { 1 } }",
    "pub trait T { fn NAME(&self) -> u64 { 1 } }",
    "pub trait T { const NAME: u64; }\npub struct S;\nimpl T for S { const NAME: u64 = 1; }",
    "pub struct S;\nimpl S { pub fn NAME(&self) -> u64 { 1 } }",
    "mod NAME {}",
    "pub type NAME = u64;",
    "pub fn f<NAME>(x: NAME) -> NAME { x }",
    "spec fn NAME(x: int) -> int = x;",
    "lemma fn NAME(x: u64) ensures x + 0 == x;",
    "pub fn NAME(x: u32) -> u32 requires x < 100, = x * 2;",
    "unchecked fn NAME(x: u64) -> u64 { x }",
];

fn has_e0104(src: &str) -> bool {
    codes(src).iter().any(|c| c == "E0104")
}

/// 予約語でない名前（`r#foo`、弱いキーワードの `r#union`、Verus の句の `r#requires`）と、表の
/// すべての予約語について、拒否する位置の raw の綴りが E0104 になる。
#[test]
fn rejects_raw_names() {
    let names = ["foo", "union", "requires"]
        .into_iter()
        .chain(RAW_KEYWORDS.iter().map(|(kw, _)| *kw));
    for n in names {
        let name = format!("r#{n}");
        for t in REJECTED {
            let src = fill(t, &name);
            assert!(
                has_e0104(&src),
                "E0104 で拒否されない: {:?}\n---\n{src}",
                codes(&src)
            );
        }
    }
    // 拒否する位置の例は、`r#` を外せば E0104 にならない。
    for t in REJECTED {
        assert!(!has_e0104(&fill(t, "foo")), "{t}");
    }
}

/// 通す位置の raw の綴りは E0104 にしない。
#[test]
fn keeps_raw_names_elsewhere() {
    let names = ["foo", "union", "requires"]
        .into_iter()
        .chain(RAW_KEYWORDS.iter().map(|(kw, _)| *kw));
    for n in names {
        let name = format!("r#{n}");
        for t in ACCEPTED {
            let src = fill(t, &name);
            assert!(
                !has_e0104(&src),
                "E0104 になった: {:?}\n---\n{src}",
                codes(&src)
            );
        }
    }
    for t in ACCEPTED {
        accepts(&fill(t, "r#foo"));
        accepts(&fill(t, "r#type"));
    }
}

/// trait の関連型は、Rust の予約語の raw の綴りだけが E0104。
#[test]
fn rejects_raw_keyword_assoc_types_only() {
    for (kw, _) in RAW_KEYWORDS {
        let src = format!("pub trait T {{ type r#{kw}; fn f(&self) -> u64 {{ 1 }} }}");
        assert!(
            has_e0104(&src),
            "E0104 で拒否されない: {:?}\n---\n{src}",
            codes(&src)
        );
    }
    accepts("pub trait T { type r#foo; fn f(&self) -> u64 { 1 } }");
    accepts("pub trait T { type A; fn f(&self) -> u64 { 1 } }");
    // impl の側は trait の宣言に従うので見ない。
    accepts("pub trait T { type A; }\npub struct S;\nimpl T for S { type A = u64; }");
}

/// 位置ごとに、何の名前かと直し方を案内する。予約語なら別の名前を、そうでなければ `r#` を外すよう
/// 案内する。
#[test]
fn says_what_to_write() {
    rejects(
        "pub const r#foo: u64 = 3;",
        "E0104",
        "`r#foo` cannot be used as the name of a `const` because Verus rejects a raw name (`r#`) there; write it without `r#` as `foo`, which is the same name",
    );
    rejects(
        "pub const r#type: u64 = 3;",
        "E0104",
        "use another name such as `type_`",
    );
    rejects(
        "pub const r#uses: u64 = 3;",
        "E0104",
        "use another name such as `uses_`",
    );
    rejects(
        "pub static r#foo: u64 = 3;",
        "E0104",
        "the name of a `static`",
    );
    rejects(
        "pub const fn r#foo() -> u64 { 3 }",
        "E0104",
        "the name of a `const fn`",
    );
    rejects(
        "pub trait T { fn r#foo(&self) -> u64; }",
        "E0104",
        "the name of a trait function without a body",
    );
    rejects(
        "pub enum E { r#foo, B }",
        "E0104",
        "the name of an enum variant",
    );
    rejects(
        "pub enum E { A { r#foo: u64 }, B }",
        "E0104",
        "the name of a field of an enum variant",
    );
    rejects(
        "pub trait T { type r#type; }",
        "E0104",
        "`r#type` cannot be used as the name of an associated type of a trait because Verus cannot verify a trait whose associated type is named with a Rust keyword; use another name such as `type_`",
    );
}

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `edition` の `Ebykn.toml` と `src/lib.eyk` のプロジェクトの診断。
fn diags_in_edition(name: &str, edition: &str, src: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("ebykn-raw-item-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let t = Tmp(dir);
    std::fs::create_dir_all(t.0.join("src")).unwrap();
    std::fs::write(
        t.0.join("Ebykn.toml"),
        format!(
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n\n[verus]\nrelease = \"release/x\"\ncommit = \"abc\"\n"
        ),
    )
    .unwrap();
    std::fs::write(t.0.join("src/lib.eyk"), src).unwrap();
    let (p, d) = load(&t.0);
    d.iter()
        .map(|d| d.render(p.file(d.span.file), Lang::En))
        .collect()
}

/// 関連型の `r#gen` は、`gen` が予約語でない 2021 では通る（rustc が `r#` を外す。
/// `tests/verus-probes/raw_name_assoc_type_2021.rs`）。ほかの位置の `r#gen` は、どの版でも E0104。
#[test]
fn follows_the_edition_for_assoc_types() {
    let e0104 = |d: &[String]| d.iter().any(|l| l.contains(": E0104: "));
    let assoc = "pub trait T { type r#gen; fn f(&self) -> u64 { 1 } }\n";
    assert!(e0104(&diags_in_edition("assoc2024", "2024", assoc)));
    let d = diags_in_edition("assoc2021", "2021", assoc);
    assert!(d.is_empty(), "{d:?}");
    let d = diags_in_edition("const2021", "2021", "pub const r#gen: u64 = 1;\n");
    assert!(e0104(&d), "{d:?}");
}
