//! #37 の再現（#16 の調査）：`.eyk` の検査を通らない経路から入った `assume(false)` を、`ebykn verify` の
//! `--no-cheating` の照合が捕まえる。
//!
//! 固定した Verus が要るので `#[ignore]` にし、`cargo xtask golden-verify`（CI の Verus のジョブ）が
//! `--ignored` で走らせる。ケースは `tests/golden/smuggled/`（差し込む前は `golden-verify` が普通の golden と
//! して verify し、成功することを確かめる）。
//!
//! 今の `.eyk` の検査は、依存クレートのマクロ・`include!`・`verus!` を拒否するので、`.eyk` からはこの形を
//! 書けない。そこで、書き下した Verus に直接差し込み、`ebykn verify` と同じ関数（`verify_build`）にかける。

use std::path::{Path, PathBuf};

use ebykn::no_cheating::Cheating;
use ebykn::verify::verify_build;
use ebykn::verus::{self, Summary};

fn case() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/smuggled")
}

/// 書き下した `src/lib.rs` への差し込み（前、後）。
const INJECT: [(&str, &str); 3] = [
    // 依存クレートの `macro_rules!` が `::vstd::prelude::proof!{ assume(false); }` を出す。
    (
        "pub fn a() -> (r: u32)\nensures r == 1, { 1 }",
        "pub fn a() -> (r: u32)\nensures r == 1, { dep::sneaky_assume!(); 0 }",
    ),
    // `include!` で `assume` を持ち込む（`build/verus/src/` から見たパス）。
    (
        "pub fn b() -> (r: u32)\nensures r == 1, { 1 }",
        "pub fn b() -> (r: u32)\nensures r == 1, { include!(\"../../../smuggled.in\"); 0 }",
    ),
    // マクロが入れ子の `verus!` を出す（`assume(false)` を持つ補題。どの項目の外にもある）。
    (
        "pub fn c() -> (r: u32)\nensures r == 1, { 1 }",
        "pub fn c() -> (r: u32)\nensures r == 1, { 1 }\ndep::nested!{}",
    ),
];

#[test]
#[ignore = "固定した Verus が要る。cargo xtask golden-verify が --ignored で走らせる"]
fn smuggled_assumptions_fail_verify() {
    let dir = case();
    let report = ebykn::check_project(&dir);
    assert!(
        !report.has_errors(),
        "{:?}",
        report.render(ebykn_syntax::diag::Lang::En)
    );
    let mut output = ebykn::lower_report(&report)
        .expect("書き下す")
        .expect("書き下せる");
    let lib = output
        .files
        .iter_mut()
        .find(|f| f.rel == "src/lib.rs")
        .expect("src/lib.rs");
    for (from, to) in INJECT {
        // 差し込みが実際に当たったことを先に確かめる（当たらなければ、何も証明しない）。
        assert!(
            lib.text.contains(from),
            "差し込む場所がない: {from}\n{}",
            lib.text
        );
        lib.text = lib.text.replacen(from, to, 1);
    }
    let build = verus::write_build(&report.project, &output).expect("build/verus/ に書く");
    std::fs::copy(
        dir.join("expected/verus/Cargo.lock"),
        build.join("Cargo.lock"),
    )
    .expect("Cargo.lock を写す");
    let v = verus::find(ebykn_syntax::diag::Lang::En).expect("固定した Verus");
    let got = verify_build(&v, &build, &output).expect("Verus を走らせる");

    // 照合がなければ、a と b は verified になる（穴が実在する）。
    assert!(got.run.success, "{}", got.run.text);
    let Summary::Ran { errors: 0, .. } = got.summary else {
        panic!("検証が通っていない: {:?}", got.summary);
    };
    let bare = verus::statuses(
        &output.files,
        &output,
        &got.run.locations(),
        got.run.checked.as_ref(),
        got.summary,
        false,
    );
    for item in ["smuggled::a", "smuggled::b"] {
        assert!(
            bare.iter()
                .any(|s| s.item == item && s.status == "verified"),
            "{item}: {bare:?}"
        );
    }

    // 照合は、差し込んだ 3 か所だけを紛れ込んだ前提として出し、verify を失敗にする。
    assert!(
        matches!(&got.cheating, Cheating::Smuggled(v) if v.len() == 3),
        "{:?}\n{}",
        got.cheating,
        got.cheating_text
    );
    let expected =
        std::fs::read_to_string(dir.join("expected/smuggled.txt")).expect("expected/smuggled.txt");
    assert_eq!(
        verus::status_text(&got.all, got.summary, &got.cheating),
        expected
    );
    assert!(!got.success(&got.all));
    // 標準エラーに出す診断（紛れ込んだものだけ）は、許した前提（liar の external_body、helper の admit）を含まない。
    let Cheating::Smuggled(found) = &got.cheating else {
        unreachable!()
    };
    for e in found {
        assert!(
            !e.block.is_empty() && !e.block.contains("fn liar") && !e.block.contains("admit()"),
            "{}",
            e.block
        );
    }
}
