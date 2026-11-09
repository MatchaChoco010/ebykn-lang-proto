//! `.eykp` の量化子の `#![trigger ..]` の式も検査がたどること（#86 のレビュー）。`.eykp` をプロジェクトで
//! 検査するのは #67 からなので、`parse_eykp` の AST に `ensures` の検査（E0004）を直接かける。

use ebykn_syntax::ast::ItemKind;
use ebykn_syntax::parse_eykp;
use ebykn_syntax::span::{FileId, SourceFile};

fn e0004_count(src: &str) -> usize {
    let f = SourceFile::new(FileId(1), "proof/lib.eykp", src);
    let p = parse_eykp(&f);
    assert!(p.diags.is_empty(), "{:?}", p.diags);
    let ItemKind::Fn(func) = &p.file.items[0].kind else {
        panic!("fn");
    };
    let mut out = Vec::new();
    ebykn_check::ensures::check(&func.sig, &func.contract, &mut out);
    out.iter().filter(|d| d.code().as_str() == "E0004").count()
}

#[test]
fn bare_mut_param_in_a_trigger_is_e0004() {
    // 本体の `final(x)` は正しいが、トリガーの裸の `x` は事後か事前か決まらない。
    assert_eq!(
        e0004_count(
            "fn f(x: &mut u8) ensures forall i: int, #![trigger g(x, i)] g(final(x), i) { }"
        ),
        1
    );
    assert_eq!(
        e0004_count(
            "fn f(x: &mut u8) ensures forall i: int, #![trigger g(final(x), i)] g(final(x), i) { }"
        ),
        0
    );
}
