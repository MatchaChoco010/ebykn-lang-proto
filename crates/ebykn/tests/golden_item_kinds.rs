//! 書き下す項目の種類が、Verus にかける golden のどれかに必ず出ることを確かめる（#163）。
//!
//! `static` は構文として受け付けて書き下していたが、どの golden にも出ておらず、生成した Verus を
//! Verus が拒否することに気づかなかった。golden の `expected/status.txt` は `cargo xtask golden-verify` が
//! Verus にかけて確かめるので、ここでは各種類がそのどれかの `.eyk` に出ることだけを見る。
//!
//! 項目の種類（`ItemKind` の列挙子）を足すと、[`kind`] の `match` が網羅でなくなってコンパイルが止まる。
//! 書き下すなら名前を返して [`REQUIRED`] に足し、その種類を使う golden を足す。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ebykn_check::project;
use ebykn_syntax::ast::ItemKind;

/// Verus にかける golden のどれかに出なければならない種類。
const REQUIRED: &[&str] = &[
    "fn", "spec fn", "lemma fn", "struct", "enum", "trait", "impl", "type", "mod", "const",
    "static", "use",
];

/// 項目の種類の名前。書き下さない種類（検査が拒否するか、`.eykp` だけのもの）は `None`。
fn kind(k: &ItemKind) -> Option<&'static str> {
    match k {
        ItemKind::Fn(_) => Some("fn"),
        ItemKind::SpecFn(_) => Some("spec fn"),
        ItemKind::LemmaFn(_) => Some("lemma fn"),
        ItemKind::Struct(_) => Some("struct"),
        ItemKind::Enum(_) => Some("enum"),
        ItemKind::Trait(_) => Some("trait"),
        ItemKind::Impl(_) => Some("impl"),
        ItemKind::TypeAlias(_) => Some("type"),
        ItemKind::Mod(_) => Some("mod"),
        ItemKind::Const(_) => Some("const"),
        ItemKind::Static(_) => Some("static"),
        ItemKind::Use(_) => Some("use"),
        // `union`・`extern` のブロック・`extern crate`・項目の位置のマクロをトークンのまま写すもの。
        // 中身の形が多く、種類の名前一つでは何を確かめたかを表せないので数えない（`extern` のブロックの
        // 関数を Verus が拒否するのは #169）。
        ItemKind::Rust(_) => None,
        // 検査が E0003 にする（部分集合型と `impl spec`。M4・M5）。
        ItemKind::Subset(_) | ItemKind::ImplSpec(_) => None,
        // `.eykp` だけ。
        ItemKind::LemmaGroup(_) | ItemKind::UseLemmas(_) => None,
    }
}

fn golden_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// `golden-verify` が Verus にかけて検証まで進むケース（`expected/status.txt` があり、Verus が検証の前に
/// 止まることを期待しないもの）。読めなければ失敗にする。
fn verified_cases() -> Vec<PathBuf> {
    let root = golden_root();
    let mut out = Vec::new();
    for e in
        std::fs::read_dir(&root).unwrap_or_else(|e| panic!("{} を読めません: {e}", root.display()))
    {
        let dir = e.expect("ディレクトリの項目").path();
        let status = dir.join("expected/status.txt");
        if !status.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&status)
            .unwrap_or_else(|e| panic!("{} を読めません: {e}", status.display()));
        if !text.contains("results not verified") {
            out.push(dir);
        }
    }
    out.sort();
    out
}

/// ケースのモジュール（インラインの `mod` を含む）の直下に出る種類。
fn kinds_in(dir: &Path) -> BTreeSet<&'static str> {
    let (p, d) = project::load(dir);
    assert!(d.is_empty(), "{}: {d:?}", dir.display());
    let mut out = BTreeSet::new();
    p.for_each_module(|_, _, items| {
        out.extend(items.iter().filter_map(|i| kind(&i.kind)));
    });
    out
}

/// [`REQUIRED`] のうち `found` にないもの、`found` のうち [`REQUIRED`] にないもの。
fn mismatch(found: &BTreeSet<&str>) -> Vec<String> {
    let mut out: Vec<String> = REQUIRED
        .iter()
        .filter(|k| !found.contains(*k))
        .map(|k| format!("`{k}` を使う golden（expected/status.txt のあるもの）がありません"))
        .collect();
    out.extend(
        found
            .iter()
            .filter(|k| !REQUIRED.contains(k))
            .map(|k| format!("`{k}` が REQUIRED にありません")),
    );
    out
}

#[test]
fn every_lowered_item_kind_is_in_a_verified_golden() {
    let cases = verified_cases();
    assert!(
        cases.iter().any(|c| c.ends_with("lowering")),
        "lowering がない: {cases:?}"
    );
    let mut found = BTreeSet::new();
    for c in &cases {
        found.extend(kinds_in(c));
    }
    let m = mismatch(&found);
    assert!(m.is_empty(), "{}", m.join("\n"));
}

#[test]
fn fails_when_a_kind_is_missing_from_the_goldens() {
    let mut found = BTreeSet::new();
    for c in &verified_cases() {
        found.extend(kinds_in(c));
    }
    // `static` を使う golden がなくなると失敗する。
    assert!(found.remove("static"), "static がない: {found:?}");
    assert_eq!(
        mismatch(&found),
        ["`static` を使う golden（expected/status.txt のあるもの）がありません"]
    );
    // REQUIRED にない種類が出ても失敗する。
    found.insert("static");
    found.insert("union");
    assert_eq!(mismatch(&found), ["`union` が REQUIRED にありません"]);
    // 数えている `static` は `lowering` のもの（#163 で足した）。
    let lowering = golden_root().join("lowering");
    assert!(kinds_in(&lowering).contains("static"));
}
