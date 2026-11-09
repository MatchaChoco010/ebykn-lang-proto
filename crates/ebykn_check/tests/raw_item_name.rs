//! Verus が raw の綴り（`r#foo`）を扱えない項目の名前は E0104（#147）。
//!
//! 固定した Verus は、`const`・`static`・`const fn`・本体のない trait の関数の名前の raw の綴りで
//! proc macro が panic し、列挙子と列挙子のフィールドは `no variant` / `no field` で拒否する。予約語で
//! ない `r#foo` でも落ちる（`tests/verus-probes/raw_name_*.rs`）。trait の関連型・関連定数・本体のある
//! 関数は、Rust の予約語の raw の綴りだけが落ちる。ほかの項目の名前は raw の綴りのまま通る
//! （`raw_name_items.rs`）。

mod support;

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

/// Rust の予約語の raw の綴りだけが E0104 になる位置（trait の関連型・関連定数・本体のある関数）。
/// 予約語でない `r#foo` は通る。
const REJECTED_KEYWORDS: &[&str] = &[
    "pub trait T { type NAME; fn f(&self) -> u64 { 1 } }",
    "pub trait T { const NAME: u64; }\npub struct S;\nimpl T for S { const NAME: u64 = 1; }",
    "pub trait T { const NAME: u64 = 1; }",
    "pub trait T { fn NAME(&self) -> u64 { 1 } }\npub struct S { pub x: u64 }\nimpl T for S { fn NAME(&self) -> u64 { 2 } }",
];

/// raw の綴りのまま通る位置。固定した Verus が通すことを `raw_name_items.rs` で確かめた。
const ACCEPTED: &[&str] = &[
    "pub fn NAME() {}\npub fn f() { crate::NAME(); }",
    "pub struct NAME { pub x: u64 }",
    "pub struct S { pub NAME: u64 }\npub fn f(s: S) -> u64 { s.NAME }",
    "pub struct S { pub x: u64 } spec { NAME: int }",
    "pub enum NAME { A, B }",
    "pub trait NAME { fn m(&self) -> u64 { 1 } }",
    "pub struct S;\nimpl S { pub fn NAME(&self) -> u64 { 1 } }",
    "mod NAME {}",
    "pub type NAME = u64;",
    "pub fn f<NAME>(x: NAME) -> NAME { x }",
    "spec fn NAME(x: int) -> int = x;",
    "lemma fn NAME(x: u64) ensures x + 0 == x;",
    "pub fn NAME(x: u32) -> u32 requires x < 100, = x * 2;",
    "unchecked fn NAME(x: u64) -> u64 { x }",
];

/// 予約語でない名前（`r#foo`、弱いキーワードの `r#union`、Verus の句の `r#requires`）。
const NON_KEYWORDS: &[&str] = &["foo", "union", "requires"];

fn has_e0104(src: &str) -> bool {
    codes(src).iter().any(|c| c == "E0104")
}

#[track_caller]
fn assert_e0104(src: &str) {
    assert!(
        has_e0104(src),
        "E0104 で拒否されない: {:?}\n---\n{src}",
        codes(src)
    );
}

#[track_caller]
fn assert_no_e0104(src: &str) {
    assert!(
        !has_e0104(src),
        "E0104 になった: {:?}\n---\n{src}",
        codes(src)
    );
}

/// 予約語でない名前と表のすべての予約語について、拒否する位置の raw の綴りが E0104 になる。
#[test]
fn rejects_raw_names() {
    let names = NON_KEYWORDS
        .iter()
        .copied()
        .chain(RAW_KEYWORDS.iter().map(|(kw, _)| *kw));
    for n in names {
        for t in REJECTED {
            assert_e0104(&fill(t, &format!("r#{n}")));
        }
    }
    // 拒否する位置の例は、`r#` を外せば E0104 にならない。
    for t in REJECTED {
        assert_no_e0104(&fill(t, "foo"));
    }
}

/// 通す位置の raw の綴りは E0104 にしない。
#[test]
fn keeps_raw_names_elsewhere() {
    let names = NON_KEYWORDS
        .iter()
        .copied()
        .chain(RAW_KEYWORDS.iter().map(|(kw, _)| *kw));
    for n in names {
        for t in ACCEPTED {
            assert_no_e0104(&fill(t, &format!("r#{n}")));
        }
    }
    for t in ACCEPTED {
        accepts(&fill(t, "r#foo"));
        accepts(&fill(t, "r#type"));
    }
}

/// trait の関連型・関連定数・本体のある関数は、Rust の予約語の raw の綴りだけが E0104。
#[test]
fn rejects_raw_keyword_trait_members_only() {
    for (kw, _) in RAW_KEYWORDS {
        for t in REJECTED_KEYWORDS {
            assert_e0104(&fill(t, &format!("r#{kw}")));
        }
    }
    for n in NON_KEYWORDS {
        for t in REJECTED_KEYWORDS {
            accepts(&fill(t, &format!("r#{n}")));
        }
    }
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
        "`r#type` cannot be used as the name of an associated type of a trait because Verus cannot handle a Rust keyword as that name; use another name such as `type_`",
    );
    rejects(
        "pub trait T { const r#type: u64; }",
        "E0104",
        "the name of an associated constant of a trait",
    );
    rejects(
        "pub trait T { fn r#dyn(&self) -> u64 { 1 } }",
        "E0104",
        "the name of a trait function with a body",
    );
}

/// 版の予約語を `r#` なしで書いた E0001 は、trait の関数・関連型・関連定数には `gen_` などの別の名前を
/// 案内する（`r#gen` と書くと E0104 になるため）。
#[test]
fn edition_keyword_points_trait_members_to_another_name() {
    rejects(
        "pub trait T { type gen; fn m(&self) -> u64 { 1 } }",
        "E0001",
        "another name such as `gen_` for a local name (variable, parameter, pattern binding) or the name of a `const`, `static`, `const fn`, enum variant, field of an enum variant, or a function, associated type, or associated constant of a trait",
    );
}

/// 列挙子のフィールドの名前に E0104 が出た語には、構造体のパターンの省略形の E0103 を重ねない
/// （E0103 の案内の `r#type: type_` は、そのフィールドの名前を使うので E0104 とぶつかる）。
#[test]
fn does_not_stack_e0103_on_variant_fields() {
    rejects(
        "pub enum E { A { r#type: u64 }, B }\npub fn f(e: E) -> u64 { match e { E::A { r#type } => 1, E::B => 0 } }",
        "E0104",
        "the name of a field of an enum variant",
    );
    // 構造体のフィールドなら E0103 のまま。
    rejects(
        "pub struct S { pub r#type: u64 }\npub fn f(s: S) -> u64 { let S { r#type } = s; 1 }",
        "E0103",
        "such as `r#type: type_`",
    );
}

/// 版で予約語になった名前（`gen` は 2024）は、その版より前では、予約語だけを拒否する位置で E0104 に
/// しない（rustc が `r#` を外す。`tests/verus-probes/raw_name_*_2021.rs`）。予約語でなくても拒否する
/// 位置では E0104 のままで、`gen` と書けば同じ名前なので `r#` を外すよう案内する。
#[test]
fn follows_the_edition() {
    let e0104 = |d: &[String]| d.iter().any(|l| l.contains(": E0104: "));
    for (i, t) in REJECTED_KEYWORDS.iter().enumerate() {
        let src = fill(t, "r#gen");
        let d = diags_in_edition(&format!("item-kw{i}-2024"), Some("2024"), &src);
        assert!(e0104(&d), "{d:?}\n---\n{src}");
        let d = diags_in_edition(&format!("item-kw{i}-2021"), Some("2021"), &src);
        assert!(d.is_empty(), "{d:?}\n---\n{src}");
    }
    let src = "pub const r#gen: u64 = 1;\n";
    let d = diags_in_edition("item-const2021", Some("2021"), src);
    assert!(
        d.len() == 1 && d[0].contains("write it without `r#` as `gen`, which is the same name"),
        "{d:?}"
    );
    let d = diags_in_edition("item-const2024", Some("2024"), src);
    assert!(
        d.len() == 1 && d[0].contains("use another name such as `gen_`"),
        "{d:?}"
    );
}
