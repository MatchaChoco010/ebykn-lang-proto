//! 受理した cfg の位置ごとに、書き下した Verus にも属性が残ることを確かめる（PR #59 のレビュー）。
//!
//! 書き下しが属性を落とすと、Verus は cfg を外したコードを検証し、配るコードは cfg のとおりにビルドされて、
//! 書き下しで意味が変わる（P0）。ここでは、述語の目印（`"zz"`）が、ソースと書き下しで同じ数だけ現れる
//! ことを見る。代入の文の属性は検査が E0003 にする（`crates/ebykn_check/tests/cfg_stmt.rs`）。

use ebykn_check::{check, project};

const PRELUDE: &str = "pub struct Node { pub val: u64 }\npub struct Pool { len: u64 } owns { nodes: Region<Ptr<Node>> }\n";

/// 受理する位置。`$P` を述語に、`PRELUDE ` を前置きに置き換える。
const POSITIONS: &[&str] = &[
    "#[cfg($P)]\npub fn f() {}",
    "pub mod m { #![cfg($P)] pub fn f() {} }",
    "pub struct S { #[cfg($P)] pub a: u32, pub b: u32 }",
    "#[cfg_attr($P, inline)]\npub fn f() {}",
    "pub fn f() -> bool { cfg!($P) }",
    "pub const C: bool = cfg!($P);",
    "pub fn f() -> u32 { #[cfg($P)] let y = 1u32; 0 }",
    "pub fn f(x: u32) -> u32 { match x { #[cfg($P)] 0 => 1, _ => 2 } }",
    "pub fn f(#[cfg($P)] x: u32) {}",
    "pub fn f() { struct T { #[cfg($P)] a: u32, b: u32 } }",
    "pub fn f() -> usize { let v: Vec<u32> = vec![#[cfg($P)] 1u32, 0]; v.len() }",
    "pub struct Q { pub a: u32, #[cfg($P)] pub b: u32 }\npub fn m() -> Q { Q { a: 1, #[cfg($P)] b: 2 } }",
    "fn side(x: &mut u32) { *x = 7; }\npub fn f() -> u32 { let mut x = 0u32; #[cfg($P)] side(&mut x); x }",
    "pub fn f() -> u32 { let mut x = 0u32; #[cfg($P)] { x = 1; } x }",
    "pub fn f(v: u32) -> u32 { #[cfg($P)] if v > 0 { return 1; } 0 }",
    "PRELUDE impl Pool { pub fn f(&mut self, p: Ptr<Node>) requires old(self)@nodes.contains(p), { #[cfg($P)] borrow mut self@nodes[p] { (*p).val = 0; } } }",
    // ジェネリクスの引数、クロージャの引数、構造体パターンの欄（#95）。`#[cfg]` は E0003 なので、cfg を含まない
    // `cfg_attr` の条件で書く。
    "pub fn f<#[cfg_attr($P, allow(unused))] T>(a: u32) -> u32 { a }",
    "pub struct T<#[cfg_attr($P, allow(unused))] A> { pub a: A }",
    "pub fn f() -> u32 { let c = |#[cfg_attr($P, allow(unused))] a: u32| a; c(1) }",
    "pub struct S { pub x: u32 }\npub fn f(s: S) -> u32 { match s { S { #[cfg_attr($P, allow(unused))] x: 0 } => 1, _ => 2 } }",
];

const MARK: &str = "\"zz\"";

fn lowered(src: &str) -> String {
    let (p, diags) = project::single(src);
    assert!(diags.is_empty(), "{diags:?}\n---\n{src}");
    let checked = check(&p);
    let errors: Vec<String> = checked
        .diags
        .iter()
        .map(|d| d.render(p.file(d.span.file), ebykn_syntax::diag::Lang::En))
        .collect();
    assert!(errors.is_empty(), "{}\n---\n{src}", errors.join("\n"));
    let out = ebykn_lower::lower(&p, &checked).unwrap_or_else(|e| panic!("{e}\n---\n{src}"));
    out.files.iter().map(|f| f.text.as_str()).collect()
}

#[test]
fn accepted_cfg_attributes_survive_lowering() {
    for pos in POSITIONS {
        let src = pos
            .replace("$P", "feature = \"zz\"")
            .replace("PRELUDE ", PRELUDE);
        let want = src.matches(MARK).count();
        assert!(want > 0, "{src}");
        let out = lowered(&src);
        assert_eq!(
            out.matches(MARK).count(),
            want,
            "属性が落ちた:\n{src}\n---\n{out}"
        );
    }
}
