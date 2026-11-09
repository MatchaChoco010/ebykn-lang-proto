//! 名前で判定する検査（`ebykn_syntax::reserved` の表）の綴りの総当たり（PR #20 の再レビュー）。
//!
//! 表のすべての名前について、綴り（裸の名前、`crate::` / `self::` / `super::` / `::`、改名する
//! `use`、ターボフィッシュ、呼び出しでない値としての参照、マクロの引数の中、`mod` 経由）を試し、
//! クレートの中の定義を指すもの以外が拒否されることを確かめる。表に名前を足せば、ここが自動で効く。

mod support;
use ebykn_syntax::reserved::{ReservedKind, all};
use support::*;

/// `NAME` を名前に置き換える。
fn fill(t: &str, name: &str) -> String {
    t.replace("NAME", name)
}

/// 拒否されることを確かめる（ほかのコードが混ざってもよい）。証明の構文の関数は E0303（`.eyk` の
/// 証明構文）。ただし `reveal_with_fuel`（Verus の書き方）と、`reveal::<T>(..)` のように構文解析器
/// （Verus の構文）が読めない綴りは E0001 になる。
#[track_caller]
fn rejected(src: &str, kind: ReservedKind) {
    let want: &[&str] = if kind == ReservedKind::ProofCall {
        &["E0303", "E0001"]
    } else {
        &["E0001"]
    };
    let c = codes(src);
    assert!(
        c.iter().any(|c| want.contains(&c.as_str())),
        "{want:?} で拒否されない: {c:?}\n---\n{src}"
    );
}

/// 関数として呼べる名前（Verus の組み込み関数と、証明の構文の関数）の、拒否する綴り。
const CALL_REJECTED: &[&str] = &[
    "pub fn f(x: bool) { NAME(x); }",
    "pub fn f(x: bool) { crate::NAME(x); }",
    "pub fn f(x: bool) { self::NAME(x); }",
    "mod m { pub fn f(x: bool) { super::NAME(x); } }",
    "pub fn f(x: bool) { ::NAME(x); }",
    "use self::NAME as rq;\npub fn f(x: bool) { rq(x); }",
    "use crate::{NAME as rq};\npub fn f(x: bool) { rq(x); }",
    "use ebykn_prelude::NAME;\npub fn f(x: bool) {}",
    "pub fn f(x: bool) { NAME::<bool>(x); }",
    "pub fn f(x: bool) { let r = NAME; r(x); }",
    "pub fn f(x: bool) { (NAME)(x); }",
    "pub fn f(x: bool) { println!(\"{}\", NAME(x)); }",
    "pub fn f(x: bool) { println!(\"{}\", crate::NAME(x)); }",
    "mod m {}\npub fn f(x: bool) { m::NAME(x); }",
    "pub fn f(x: bool) { fn g() { use self::NAME as rq; } }",
    // cfg で消えうる定義は、クレートの定義として数えない（3 回目のレビュー）。
    "#[cfg(any())]\npub fn NAME(x: bool) {}\npub fn f(x: bool) { crate::NAME(x); }",
    "#[cfg(test)]\nfn NAME(x: bool) {}\npub fn f(x: bool) { self::NAME(x); }",
    // raw の綴りの `cfg` も同じ名前（#122）。
    "#[r#cfg(test)]\nfn NAME(x: bool) {}\npub fn f(x: bool) { self::NAME(x); }",
    "#[r#cfg_attr(any(), cfg(test))]\npub fn NAME(x: bool) {}\npub fn f(x: bool) { crate::NAME(x); }",
    "#[cfg_attr(any(), cfg(test))]\npub fn NAME(x: bool) {}\npub fn f(x: bool) { crate::NAME(x); }",
    "#[cfg(test)]\nmod m { pub fn NAME(x: bool) {} }\npub fn f(x: bool) { m::NAME(x); }",
    "mod m { #![cfg(test)] pub fn NAME(x: bool) {} }\npub fn f(x: bool) { m::NAME(x); }",
    "pub struct H {}\n#[cfg(test)]\nimpl H { pub fn NAME(x: bool) {} }\npub fn f(x: bool) { H::NAME(x); }",
    "pub const C: u64 = { let r = NAME; 0 };",
];

/// クレートの中の定義を指すので通す綴り（`NAME` という名前の自分の関数とローカル変数）。
const CALL_ACCEPTED: &[&str] = &[
    "pub fn NAME(x: bool) {}\npub fn f(x: bool) { crate::NAME(x); }",
    "pub fn NAME(x: bool) {}\npub fn f(x: bool) { self::NAME(x); }",
    "pub fn NAME(x: bool) {}\nmod m { pub fn f(x: bool) { super::NAME(x); } }",
    "pub fn NAME(x: bool) {}\nuse crate::NAME as rq;\npub fn f(x: bool) { rq(x); }",
    "mod m { pub fn NAME(x: bool) {} }\npub fn f(x: bool) { m::NAME(x); }",
    // `cfg_attr` で属性を足すだけの項目は残る。
    "#[cfg_attr(any(), inline)]\npub fn NAME(x: bool) {}\npub fn f(x: bool) { crate::NAME(x); }",
    "pub fn f(NAME: u64) -> u64 { let y = NAME + 1; y }",
    // 腕の本体の先頭の `assume` / `assert` などは、構文解析器（verus_syn）が Verus の式として読む
    // （M1 の制限）。ここでは値として使えることだけを見る。
    "pub fn f(x: Option<u64>) -> u64 { match x { Some(NAME) => 0 + NAME, None => 0 } }",
];

/// Verus のマクロの、拒否する綴り（マクロは許可の一覧の外をすべて拒否する）。
const MACRO_REJECTED: &[&str] = &[
    "pub fn f() { NAME!(); }",
    "pub fn f() { crate::NAME!(); }",
    "pub fn f() { self::NAME!(); }",
    "pub fn f() { ::NAME!(); }",
    "use crate::NAME as mm;\npub fn f() { mm!(); }",
    "use std::NAME;\npub fn f() {}",
];

const S: &str = "pub struct S { a: u64 }\n";

/// `Invariant` / `View` の impl の中身。
fn body(name: &str) -> &'static str {
    match name {
        "Invariant" => "{ spec fn invariant(&self) -> bool = true; }",
        _ => "{ type V = u64; spec fn view(&self) -> u64 = 0; }",
    }
}

/// Ebykn のトレイトの、拒否する綴り（`BODY` は impl の中身）。
const TRAIT_REJECTED: &[&str] = &[
    "impl crate::NAME for S BODY",
    "impl self::NAME for S BODY",
    "mod m { impl super::NAME for super::S BODY }",
    "impl ::ebykn_prelude::NAME for S BODY",
    "impl ebykn_prelude::NAME for S BODY",
    "impl NAME<> for S BODY",
    "mod m { pub use ebykn_prelude::NAME; }\nimpl m::NAME for S BODY",
    "use self::NAME as Inv;\nimpl Inv for S BODY",
    "use ebykn_prelude::NAME as Inv;\nimpl Inv for S BODY",
    "pub fn outer() { use ebykn_prelude::NAME as I2; impl I2 for S BODY }",
    // cfg で消えうる同じ名前のトレイトは、クレートの定義として数えない（3 回目のレビュー）。
    "#[cfg(any())]\npub trait NAME {}\nimpl crate::NAME for S BODY",
    "#[cfg(test)]\nmod m { pub trait NAME {} }\nimpl m::NAME for S BODY",
];

#[test]
fn every_reserved_name_is_rejected_in_every_spelling() {
    let mut tried = 0;
    for (name, kind) in all() {
        let templates = match kind {
            ReservedKind::VerusCall | ReservedKind::ProofCall => CALL_REJECTED,
            ReservedKind::VerusMacro => MACRO_REJECTED,
            ReservedKind::EbyknTrait => TRAIT_REJECTED,
        };
        for t in templates {
            let src = if kind == ReservedKind::EbyknTrait {
                format!("{S}{}", fill(t, name).replace("BODY", body(name)))
            } else {
                fill(t, name)
            };
            rejected(&src, kind);
            tried += 1;
        }
    }
    // 表が空になって何も試さずに通らないよう、数を確かめる。
    assert!(tried > 700, "{tried}");
}

#[test]
fn spellings_that_name_crate_definitions_are_accepted() {
    for (name, kind) in all() {
        match kind {
            ReservedKind::VerusCall | ReservedKind::ProofCall => {
                for t in CALL_ACCEPTED {
                    accepts(&fill(t, name));
                }
            }
            ReservedKind::EbyknTrait => {
                // 裸の名前の 1 つの書き方。
                accepts(&format!("{S}impl {name} for S {}", body(name)));
                // クレートで定義した同じ名前のトレイトは Rust のトレイト（展開しない）。
                let src = format!(
                    "{S}pub trait {name} {{ fn ok(&self); }}\nimpl crate::{name} for S {{ fn ok(&self) {{}} }}\npub fn f(x: S) {{}}\n"
                );
                let (c, lines) = run(&src);
                assert!(lines.is_empty(), "{lines:?}\n---\n{src}");
                assert!(c.facts.expansions.is_empty(), "{src}");
            }
            ReservedKind::VerusMacro => {}
        }
    }
}

/// 拒否の診断は、自分の同じ名前の関数を呼ぶ書き方を案内する（再レビューの 6）。
#[test]
fn the_message_points_to_the_crate_path() {
    let (_, lines) = run("pub fn returns(x: bool) {}\npub fn f(x: bool) { returns(x); }");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("`crate::..::returns`"), "{}", lines[0]);
    accepts("pub fn returns(x: bool) {}\npub fn f(x: bool) { crate::returns(x); }");
}

// ---- 版の予約語（#104） ----

/// 版の予約語（`gen` など）を名前に使う綴り。どれも `r#` がなければ拒否し、`r#` があれば通す。
const EDITION_SPELLINGS: &[&str] = &[
    "pub fn NAME() {}",
    "pub fn f() { let NAME = 1u64; }",
    "pub fn f(NAME: u64) {}",
    "pub struct NAME {}",
    "pub struct S { pub NAME: u64 }",
    "pub struct S { pub NAME: u64 }\npub fn f(s: S) -> u64 { s.NAME }",
    "mod NAME {}",
    "pub fn NAME() {}\npub fn f() { crate::NAME(); }",
    "pub fn NAME() {}\nuse crate::NAME as g;",
    "pub fn f(x: u64) { println!(\"{}\", NAME); }",
    "pub enum E { NAME }",
    "pub const NAME: u64 = 1;",
];

/// 版の予約語を `r#` なしで書いたときの、ちょうど一つの診断。
#[track_caller]
fn rejected_as_edition_keyword(src: &str, name: &str, since: u32) {
    let (_, lines) = run(src);
    let want = format!("`{name}` is a reserved keyword in Rust {since}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("E0001") && l.contains(&want)),
        "版の予約語として拒否されない: {lines:?}\n---\n{src}"
    );
}

#[test]
fn edition_keywords_are_rejected_without_raw_prefix() {
    use ebykn_syntax::reserved::EDITION_KEYWORDS;
    let mut tried = 0;
    for (name, since) in EDITION_KEYWORDS {
        for t in EDITION_SPELLINGS {
            rejected_as_edition_keyword(&fill(t, name), name, *since);
            // `r#` を付ければ通る（`run` は版 2024 のプロジェクト）。
            let raw = fill(t, &format!("r#{name}"));
            let (_, lines) = run(&raw);
            assert!(
                !lines.iter().any(|l| l.contains("reserved keyword")),
                "{lines:?}\n---\n{raw}"
            );
            tried += 1;
        }
    }
    assert!(tried >= EDITION_SPELLINGS.len(), "{tried}");
}

/// 版の予約語は、`Ebykn.toml` の `edition` がその版より前なら名前に使える。
#[test]
fn edition_keywords_are_names_in_earlier_editions() {
    let root = std::env::temp_dir().join(format!("ebykn-edition-{}", std::process::id()));
    let write = |edition: &str| {
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Ebykn.toml"),
            format!(
                "[package]\nname = \"e\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n\n[verus]\nrelease = \"x\"\ncommit = \"y\"\n"
            ),
        )
        .unwrap();
        std::fs::write(root.join("src/lib.eyk"), "pub fn gen() {}\n").unwrap();
        let (p, diags) = ebykn_check::project::load(&root);
        diags
            .iter()
            .map(|d| d.render(p.file(d.span.file), ebykn_syntax::diag::Lang::En))
            .filter(|l| l.contains("reserved keyword"))
            .count()
    };
    let in_2021 = write("2021");
    let in_2024 = write("2024");
    let unknown = write("next");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(in_2021, 0);
    assert_eq!(in_2024, 1);
    // 数として読めない版は、予約語とみなす。
    assert_eq!(unknown, 1);
}
