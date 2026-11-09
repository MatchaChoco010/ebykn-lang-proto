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

/// 線形性などの本体の検査（`body.rs`）も、量化子と `assert forall` のトリガーの式をたどる。`self` のない
/// 関数で `self@x` をトリガーにだけ書くと E0515 になる。
fn body_codes(src: &str) -> Vec<String> {
    let (mut project, diags) = ebykn_check::project::single("");
    assert!(diags.is_empty());
    let f = SourceFile::new(FileId(1), "proof/lib.eykp", src);
    let p = parse_eykp(&f);
    assert!(p.diags.is_empty(), "{:?}", p.diags);
    project.files[1] = f;
    project.asts[1] = Some(p.file);
    ebykn_check::check(&project)
        .diags
        .iter()
        .map(|d| d.code().as_str().to_string())
        .collect()
}

#[test]
fn body_checks_walk_triggers() {
    for src in [
        "fn f() ensures forall i: int, #![trigger self@order.len()] true { }",
        "fn f() { assert forall i: int, #![trigger self@order.len()] true by { }; }",
    ] {
        assert!(body_codes(src).contains(&"E0515".to_string()), "{src}");
    }
    assert!(
        !body_codes("fn f() { assert forall i: int, #![auto] true by { }; }")
            .contains(&"E0515".to_string())
    );
}
