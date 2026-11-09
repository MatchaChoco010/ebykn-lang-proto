//! 局所の名前に Rust の予約語の raw の綴り（`r#type`）を使うと E0103（#119）。
//!
//! Verus は、予約語を `r#` で名前にした局所の名前を SMT の記号に `r#type@` のまま書き、z3 が読めずに
//! `function body check` で失敗する（`tests/verus-probes/raw_keyword_local.rs`）。表
//! （`ebykn_syntax::reserved::RAW_KEYWORDS`）のすべての名前について、束縛の位置の綴りを総当たりで
//! 試す。表に名前を足せば、ここが自動で効く。

mod support;

use ebykn_syntax::reserved::RAW_KEYWORDS;
use support::*;

/// `NAME` を名前に置き換える。
fn fill(t: &str, name: &str) -> String {
    t.replace("NAME", name)
}

/// 局所の名前を束縛する位置。`NAME` が束縛される。
const BINDINGS: &[&str] = &[
    "pub fn f() -> u64 { let NAME = 2u64; NAME + 1 }",
    "pub fn f() { let mut NAME = 1u64; NAME = 2; }",
    "pub fn f(NAME: u64) -> u64 { NAME }",
    "pub fn f(NAME: u64) -> u64 requires NAME < 5, { NAME + 1 }",
    "pub fn f() -> (NAME: u64) ensures NAME == 2, { 2 }",
    "pub fn f(x: Option<u64>) -> u64 { match x { Some(NAME) => NAME, None => 0 } }",
    "pub fn f(x: Option<u64>) -> u64 { if let Some(NAME) = x { NAME } else { 0 } }",
    "pub fn f(x: u64) -> u64 { match x { NAME @ 0..=3 => NAME, _ => 0 } }",
    "pub fn f(x: (u64, u64)) -> u64 { let (NAME, _) = x; NAME }",
    "pub fn f() { for NAME in 0..3u64 { } }",
    "pub fn f() { let g = |NAME: u64| NAME; }",
    "pub fn f() { let g = |x: u64| -> (NAME: u64) ensures NAME == x, { x }; }",
    "pub fn f() ensures forall NAME: int, NAME + 0 == NAME, { }",
    "pub spec fn s(NAME: int) -> int = NAME + 1;",
    "pub fn f<const NAME: usize>() {}",
    "pub struct S { pub x: u64 }\npub fn f(s: S) -> u64 { let S { x: NAME } = s; NAME }",
    "pub struct S { pub NAME: u64 }\npub fn f(s: S) -> u64 { let S { NAME } = s; NAME }",
    "pub struct Node { pub val: u64 }\npub fn f() uses NAME: Region<Ptr<Node>> { }",
    "pub struct S { pub x: u64 } spec { NAME: int }\npub fn f(s: S) { let S { x, @NAME } = s; }",
    // 消去フィールドを含む構造体のパターンの省略形（PR #148 のレビュー）。
    "pub struct S { pub NAME: u64 } spec { g: int }\npub fn f(s: S) -> u64 { let S { NAME, @g } = s; NAME }",
    "pub struct S { pub NAME: u64 } spec { g: int }\npub fn f(s: S) { let S { ref NAME, @.. } = s; }",
];

/// 束縛でない位置（関数・型・モジュール・フィールド・型の引数の名前）。Verus はこれらを通す。
const NOT_BINDINGS: &[&str] = &[
    "pub fn NAME() {}\npub fn f() { crate::NAME(); }",
    "pub struct S { pub NAME: u64 }\npub fn f(s: S) -> u64 { s.NAME }",
    "pub struct S { pub NAME: u64 }\npub fn f() -> S { S { NAME: 1 } }",
    "pub struct S { pub NAME: u64 }\npub fn f(s: S) -> u64 { let S { NAME: y } = s; y }",
    "pub struct S { pub NAME: u64 } spec { g: int }\npub fn f(s: S) -> u64 { let S { NAME: y, @g } = s; y }",
    "pub fn f<NAME>(x: NAME) -> NAME { x }",
    "mod NAME {}",
    "pub struct NAME {}",
];

/// 表のすべての名前について、束縛の位置の raw の綴りが E0103 になる。
#[test]
fn rejects_raw_keywords_at_bindings() {
    for (kw, _) in RAW_KEYWORDS {
        let name = format!("r#{kw}");
        for t in BINDINGS {
            let src = fill(t, &name);
            let c = codes(&src);
            assert!(
                c.iter().any(|c| c == "E0103"),
                "E0103 で拒否されない: {c:?}\n---\n{src}"
            );
        }
    }
}

/// 束縛でない位置の raw の綴りは E0103 にしない。
#[test]
fn keeps_raw_keywords_elsewhere() {
    for (kw, _) in RAW_KEYWORDS {
        let name = format!("r#{kw}");
        for t in NOT_BINDINGS {
            let src = fill(t, &name);
            let c = codes(&src);
            assert!(
                !c.iter().any(|c| c == "E0103"),
                "E0103 になった: {c:?}\n---\n{src}"
            );
        }
    }
    // 予約語の名前でも、束縛でない位置では全体が通る。
    for t in NOT_BINDINGS {
        accepts(&fill(t, "r#type"));
    }
}

/// 予約語でない名前の raw の綴り（`r#foo`、弱いキーワードの `r#union`、Verus の句の `r#requires`）は、
/// rustc が `r#` を外すので通る。
#[test]
fn accepts_raw_non_keywords_at_bindings() {
    for name in ["r#foo", "r#union", "r#raw", "r#requires"] {
        for t in BINDINGS {
            let src = fill(t, name);
            let c = codes(&src);
            assert!(
                !c.iter().any(|c| c == "E0103"),
                "E0103 になった: {c:?}\n---\n{src}"
            );
        }
    }
    accepts("pub fn f() -> u64 { let r#foo = 2u64; r#foo + 1 }");
    // 束縛の位置の例は、E0103 のほかに誤りがない。
    for t in BINDINGS {
        accepts(&fill(t, "r#foo"));
    }
}

/// 束縛の書き方ごとに、そのとおりに書き換えられる直し方を案内する（PR #148 のレビュー）。
#[test]
fn says_what_to_write_for_each_form() {
    let s = "pub struct S { pub r#type: u64 } spec { g: int }\n";
    rejects(
        &format!("{s}pub fn f(s: S) -> u64 {{ let S {{ r#type }} = s; r#type }}"),
        "E0103",
        "such as `r#type: type_`",
    );
    rejects(
        &format!("{s}pub fn f(s: S) -> u64 {{ let S {{ r#type, @g }} = s; r#type }}"),
        "E0103",
        "such as `r#type: type_`",
    );
    // `ref` / `mut` を落とさずに案内する（落とすと借用が移動に変わる）。
    rejects(
        &format!("{s}pub fn f(s: &S) -> u64 {{ let S {{ ref r#type }} = s; *r#type }}"),
        "E0103",
        "such as `r#type: ref type_`",
    );
    rejects(
        &format!("{s}pub fn f(s: S) {{ let S {{ ref r#type, @g }} = s; }}"),
        "E0103",
        "such as `r#type: ref type_`",
    );
    rejects(
        &format!(
            "{s}pub fn f(s: S) -> u64 {{ let S {{ mut r#type, @g }} = s; r#type = 1; r#type }}"
        ),
        "E0103",
        "such as `r#type: mut type_`",
    );
    rejects(
        "pub struct S { pub a: u64 } spec { r#type: int }\npub fn f(s: S) { let S { a, @r#type } = s; }",
        "E0103",
        "for a `spec` field, leave it out of the pattern with `@..` and read it from the struct value, such as `s@r#type`; for an `owns` field, rename the field",
    );
}

#[test]
fn says_what_to_write() {
    rejects(
        "pub fn f() -> u64 { let r#type = 2u64; r#type + 1 }",
        "E0103",
        "`r#type` cannot be used as a local name",
    );
    rejects(
        "pub fn f() -> u64 { let r#type = 2u64; r#type + 1 }",
        "E0103",
        "use another name such as `type_`",
    );
}

/// 版で予約語になった名前（`gen` は 2024）は、その版より前では E0103 にしない（rustc が `r#` を外す）。
#[test]
fn follows_the_edition() {
    let src = "pub fn f() -> u64 { let r#gen = 2u64; r#gen + 1 }\n";
    let e0103 = |d: &[String]| d.iter().any(|l| l.contains(": E0103: "));
    assert!(e0103(&diags_in_edition("kw-gen2024", Some("2024"), src)));
    assert!(e0103(&diags_in_edition("kw-gendefault", None, src)));
    assert!(e0103(&diags_in_edition("kw-genbad", Some("next"), src)));
    let d = diags_in_edition("kw-gen2021", Some("2021"), src);
    assert!(d.is_empty(), "{d:?}");
    // どの版でも予約語の名前は、2021 でも E0103。
    let d = diags_in_edition(
        "kw-type2021",
        Some("2021"),
        "pub fn f() -> u64 { let r#type = 2u64; r#type + 1 }\n",
    );
    assert!(e0103(&d), "{d:?}");
}
