//! PR #15 のレビューで見つかった穴の再発防止。入力はレビューで示されたもの。

mod support;
use support::*;

/// B1：属性の検査は、`#[..]` の最初の識別子だけでなく、`r#`、先頭の `::`、`cfg_attr` の中、
/// `verus::` の名前空間も見る。
#[test]
fn hidden_verifier_and_proof_attributes_are_rejected() {
    let e0102 = "E0102: `#[verifier::..]` attributes cannot be written in Ebykn";
    for src in [
        "#[cfg_attr(verus_keep_ghost, verifier::external_body)] fn f() ensures false {}",
        "#![cfg_attr(verus_keep_ghost, verifier::exec_allows_no_decreases_clause)]\nfn f() {}",
        "#[r#verifier::external_body] fn f() {}",
        "#[::verifier::external_body] fn f() {}",
        "#[cfg_attr(all(), cfg_attr(any(), verifier::opaque))] fn f() {}",
    ] {
        rejects(src, e0102);
    }
    rejects("fn f() { #[r#trigger] x }", "`#[trigger]` is proof syntax");
    rejects(
        "fn f() { #[cfg_attr(all(), trigger)] x }",
        "`#[trigger]` is proof syntax",
    );
    rejects(
        "#[verus::internal(spec)] fn f() {}",
        "`#[verus::..]` is Verus syntax and cannot be written in `.eyk`",
    );
    // 条件の中の語は属性ではない。
    ok("#[cfg_attr(feature = \"verifier\", derive(Clone))] struct S { a: u64 }");
}

/// B2-1：`verus!` と Verus のマクロは、項目・文・式のどの位置でも拒否する。
#[test]
fn verus_macros_are_rejected() {
    let want = "`verus!` is Verus syntax and cannot be written in `.eyk`";
    rejects(
        "verus! { proof fn bad() ensures false { assume(false); } }",
        want,
    );
    rejects(
        "fn f() { verus! { proof fn bad() ensures false { assume(false); } } }",
        want,
    );
    rejects("fn f() { let x = verus!(1); }", want);
    rejects(
        "fn f() { proof_decl! { } }",
        "`proof_decl!` is Verus syntax",
    );
    rejects(
        "fn f() { calc! { (==) a; { } b } }",
        "`calc!` is Verus syntax",
    );
}

/// B2-2：vstd の仕様用のマクロの引数は Ebykn の式として読む。
#[test]
fn spec_macro_arguments_are_ebykn_expressions() {
    assert_eq!(e("seq![n] + self@order"), "(+ seq![n] (@ self order))");
    assert_eq!(e("set![a, b@x]"), "set![a, (@ b x)]");
    assert_eq!(e("map![1 => a, 2 => b]"), "map![1 => a, 2 => b]");
    rejects(
        "fn f() { seq![forall|i: int| i > 0] }",
        "`forall|x| ..` is Verus syntax; in Ebykn write `forall x: T, P`",
    );
    rejects("fn f() { set![a &&& b] }", "E0001");
}

/// B2-3：ほかのマクロの引数は読まないが、証明の構文と Verus の構文を字句的に探す。
#[test]
fn proof_syntax_inside_other_macros_is_rejected() {
    for (src, want) in [
        (
            "vec![{ proof { assume(false); } 1u64 }]",
            "`proof { .. }` is proof syntax",
        ),
        (
            "println!(\"{}\", assume(false))",
            "`assume(..)` is Verus syntax",
        ),
        ("vec![admit()]", "`admit(..)` is Verus syntax"),
        ("dbg!(assert(x))", "`assert(..)` is proof syntax"),
        (
            "vec![forall|i: int| i > 0]",
            "`forall|x| ..` is Verus syntax",
        ),
        (
            "vec![exists|i: int| i > 0]",
            "`exists|x| ..` is Verus syntax",
        ),
        (
            "vec![choose|i: int| i > 0]",
            "`choose|x| ..` is Verus syntax",
        ),
        (
            "vec![{ let ghost g = 1; g }]",
            "`let ghost / let tracked` is Verus syntax",
        ),
        ("vec![a &&& b]", "`&&&` is Verus syntax"),
        ("vec![#[verifier::truncate] (x as u8)]", "E0102"),
        ("vec![#[trigger] x]", "`#[trigger]` is proof syntax"),
        ("vec![verus!{}]", "`verus!` is Verus syntax"),
    ] {
        rejects(&format!("fn f() {{ {src}; }}"), want);
    }
    rejects(
        "macro_rules! m { () => { assume(false) } }",
        "`assume(..)` is Verus syntax",
    );
    // Rust のマクロはそのまま。
    ok("fn f() { println!(\"{} {}\", a, b); assert!(x); assert_eq!(a, b); let v = vec![1, 2]; }");
}

/// S1：`assert` などの呼び出しは、式のどの位置でも拒否する。
#[test]
fn proof_calls_are_rejected_in_any_expression_position() {
    for src in [
        "fn f() { foo(assert(x)) }",
        "fn f() { x + assert(y) }",
        "fn f() ensures assert(false) { }",
        "fn f() { let y = reveal(g); }",
        "fn f() { let y = hide(g); }",
        "fn f() { let y = reveal_with_fuel(g, 2); }",
    ] {
        rejects(src, "is proof syntax");
    }
    for src in [
        "fn f() { let y = assume(false); }",
        "fn f() -> bool = assume(false);",
        "fn f() { let y = admit(); }",
    ] {
        rejects(src, "is Verus syntax and cannot be written in `.eyk`");
    }
    // パス付きの呼び出しやメソッドは利用者の関数。
    ok("fn f() { m::assume(x); s.assert(y); }");
}

/// S2：差し込み口は `parse_eyk` の外では何もしない（上流と同じに動く）。
#[test]
fn hook_is_inert_outside_parse_eyk() {
    ok("fn f() { forall x: int, x > 0 }");
    let f: verus_syn::File = verus_syn::parse_str("fn f() { fn g() {} }").expect("parse");
    assert_eq!(f.items.len(), 1);
    let f: verus_syn::File =
        verus_syn::parse_str("fn f() { while x invariant y ensures z { } let a = x@; }")
            .expect("parse");
    assert_eq!(f.items.len(), 1);
    // ほかのスレッドでも。
    std::thread::spawn(|| {
        let f: verus_syn::File = verus_syn::parse_str("fn f() { fn g() {} }").expect("parse");
        assert_eq!(f.items.len(), 1);
    })
    .join()
    .unwrap();
}

/// S3：`self@` などは権限ではない（文法は `Ident "@"`）。
#[test]
fn keyword_paths_are_not_permissions() {
    // `crate@` と `super@` はフォークがパスとして読まない（構文エラー）。
    rejects("fn f() { crate@ == x }", "E0001");
    for src in ["self@", "Self@"] {
        rejects(
            &format!("fn f() {{ {src} == x }}"),
            "`e@` is Verus syntax; in Ebykn write `e.view()`",
        );
    }
    assert_eq!(e("p@"), "p@");
    assert_eq!(e("self@x"), "(@ self x)");
}

/// S4：`impl spec::Foo for T` は Rust の trait の impl（`impl spec` ではない）。
#[test]
fn impl_of_a_path_starting_with_spec_is_rust() {
    assert_eq!(
        heads("impl spec::Foo for T { spec fn f() -> int = 1; }"),
        ["impl spec::Foo for T (1 items)"]
    );
}

/// S5：`broadcast` などは、後ろに Verus の形が続くときだけ Verus の構文とする。
#[test]
fn verus_item_words_are_identifiers_otherwise() {
    ok("fn f() { broadcast(tx, 1); uninterp.run(); assume_specification(); }");
    rejects("broadcast use foo;", "`broadcast` is Verus syntax");
    rejects("broadcast group g { a }", "`broadcast` is Verus syntax");
    rejects("broadcast proof fn p() {}", "`broadcast` is Verus syntax");
    rejects(
        "uninterp spec fn u() -> int;",
        "`uninterp spec fn` is Verus syntax",
    );
    rejects(
        "assume_specification [ std::mem::swap ] (a: u64);",
        "`assume_specification` is Verus syntax",
    );
}

/// 入れ子の項目は深さについて線形に読む（指数的に読み直すと終わらない深さ）。
#[test]
fn nested_items_parse_in_linear_time() {
    let depth = 40;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&format!("fn f{i}() {{ "));
    }
    src.push_str("let __eyk_x = 1;");
    for _ in 0..depth {
        src.push_str(" }");
    }
    // 深い入れ子は debug ビルドの既定のスタックに収まらないので、大きいスタックで読む。
    let d = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let t = std::time::Instant::now();
            let d = diags(&src);
            (d, t.elapsed())
        })
        .unwrap()
        .join()
        .unwrap();
    // 一番内側の診断は一度だけ出る。指数的なら深さ 40 は終わらない（深さ 16 で約 1 秒だった）。
    assert_eq!(d.0.len(), 1, "{:?}", d.0);
    assert!(d.0[0].contains("E0101"));
    assert!(d.1 < std::time::Duration::from_secs(10), "{:?}", d.1);
}

/// パターンの `@name` / `@..` に付いた属性は残す。
#[test]
fn attributes_on_erased_field_patterns_are_kept() {
    assert_eq!(
        e("let S { a, #[cfg(x)] @r, #[cfg(y)] @.. } = s;"),
        "(let S { a, #[cfg] @r, #[cfg] @.. } s)"
    );
}

/// `where` の後の `for<'a>` も Rust の where 句。
#[test]
fn higher_ranked_where_is_rust() {
    assert_eq!(
        heads("type T = u64 where for<'a> &'a u64: Copy;"),
        ["type T = u64 where for <'a > &'a u64 : Copy"]
    );
}

/// `async` などの中も、証明の構文を字句的に探す。
#[test]
fn proof_syntax_inside_opaque_rust_expressions_is_rejected() {
    rejects(
        "fn f() { foo(forall|x: int| x > 0).await }",
        "`forall|x| ..` is Verus syntax",
    );
    rejects(
        "fn f() { async { assume(false) } }",
        "`assume(..)` is Verus syntax",
    );
}
