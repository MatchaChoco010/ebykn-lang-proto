//! 字句（仕様 §4）：文脈キーワード、`@` の用法、演算子、E0101、E0102。

mod support;
use ebykn_syntax::diag::Lang;
use support::*;

/// §4 の表の語は、表の位置の外では識別子として使える。`uses` は予約語で、
/// `crates/ebykn_syntax/tests/region_call.rs` が拒むことを確かめる（#19 の既定 P1）。
#[test]
fn contextual_keywords_are_identifiers_elsewhere() {
    let words = [
        "spec",
        "lemma",
        "unchecked",
        "closed",
        "requires",
        "ensures",
        "returns",
        "decreases",
        "no_panic",
        "gives",
        "by",
        "invariant",
        "owns",
        "take",
        "put",
        "borrow",
        "leak",
        "forall",
        "exists",
        "and",
        "or",
        "props",
        "proof",
        "assert",
        "reveal",
        "hide",
        "lemmas",
        "group",
    ];
    for w in words {
        // 引数、let、呼び出しの実引数、フィールド、メソッド名、パスの部分。
        let src = format!(
            "struct W {{ {w}: u64 }}\n\
             fn f({w}: u64) -> u64 {{ let y = {w} + 1; g({w}); m::{w}::x(); W {{ {w}: y }}.{w} }}"
        );
        ok(&src);
    }
    // 関数名・型名としても使える（`requires` は §4 の位置にないので識別子）。
    assert_eq!(
        heads("fn requires(spec: u64) -> u64 { spec }\nstruct owns { spec: u64 }"),
        ["fn requires(spec: u64) -> u64 {..}", "struct owns"]
    );
}

/// 文の先頭でも、直後が §4 の形でなければ識別子。
#[test]
fn statement_keywords_need_their_shape() {
    assert_eq!(e("let take = 1; take + put"), "(let take 1) (+ take put)");
    assert_eq!(e("take[i];"), "(index take i);");
    assert_eq!(e("put(x);"), "(call put x);");
    assert_eq!(e("leak(p);"), "(call leak p);");
    assert_eq!(e("borrow(r);"), "(call borrow r);");
    assert_eq!(e("forall(x)"), "(call forall x)");
    assert_eq!(e("and + or"), "(+ and or)");
    // 同じ語が §4 の形ならキーワード。
    assert_eq!(e("take r[p];"), "(take r[p])");
    assert_eq!(e("leak p@;"), "(leak p)");
    assert_eq!(e("and { a, b }"), "(and a b)");
}

/// `@` の 4 つの用法（Rust の束縛、後置の権限、消去フィールド、前置）を位置で分ける。
#[test]
fn at_sign_is_resolved_by_position() {
    // Rust のパターン束縛。
    assert_eq!(
        e("match v { n @ 1..=5 => n, _ => 0 }"),
        "(match v (n @ 1..5 => n) (_ => 0))"
    );
    // 後置：権限。
    assert_eq!(e("p@"), "p@");
    assert_eq!(e("p@.value()"), "(.value p@)");
    // `self @ Ident`：消去フィールド。
    assert_eq!(e("self@nodes[p].val"), "(. (index (@ self nodes) p) val)");
    // 前置：構造体リテラルとパターン。
    assert_eq!(e("S { a: 1, @r: e }"), "(struct S a:1 @r:e)");
    assert_eq!(e("let S { a, @r } = s;"), "(let S { a, @r } s)");
}

/// 権限 `p@` の直後の契約の句のキーワードは、消去フィールドの名前にしない（`docs/current-spec.md`）。
#[test]
fn clause_keyword_after_permission_starts_a_clause() {
    assert_eq!(
        heads("fn g(p: Ptr<u64>) requires q == p@ ensures r@ == p@ { }"),
        ["fn g(p: Ptr<u64>) (requires (== q p@)) (ensures (== r@ p@)) {..}"]
    );
    assert_eq!(
        e("while i < n invariant x == p@ decreases n { }"),
        "(while (< i n) (invariant (== x p@)) (decreases n) {})"
    );
}

#[test]
fn implies_is_right_associative_and_below_or() {
    assert_eq!(e("a ==> b ==> c"), "(==> a (==> b c))");
    assert_eq!(e("a || b ==> c && d"), "(==> (|| a b) (&& c d))");
    assert_eq!(e("a ==> b || c"), "(==> a (|| b c))");
}

#[test]
fn iff_is_below_implies_and_non_associative() {
    assert_eq!(e("a <==> b ==> c"), "(<==> a (==> b c))");
    assert_eq!(e("a ==> b <==> c"), "(<==> (==> a b) c)");
    rejects(
        "fn f() { a <==> b <==> c }",
        "src/lib.eyk:1:19: E0001: syntax error: non-associative operator requires additional parentheses",
    );
    assert_eq!(e("(a <==> b) <==> c"), "(<==> (paren (<==> a b)) c)");
}

#[test]
fn ext_eq_is_eykp_only() {
    assert_eq!(
        diags("fn f() { a =~= b }"),
        [
            "src/lib.eyk:1:10: E0001: syntax error: `=~=` is proof syntax; it can only be written in `.eykp`, which is not supported yet"
        ]
    );
    assert_eq!(
        diags_in("fn f() { a =~= b }", Lang::Ja),
        [
            "src/lib.eyk:1:10: E0001: 構文エラー: `=~=` は証明構文で、`.eykp` にだけ書けます（`.eykp` はまだ読めません）"
        ]
    );
}

#[test]
fn reserved_prefix_is_e0101_everywhere() {
    let src = "fn __eyk_f<'__eyk_a>(r#__eyk_x: u64) {\n    let y = seq![__eyk_m];\n}\n";
    assert_eq!(
        diags(src),
        [
            "src/lib.eyk:1:4: E0101: identifier `__eyk_f` uses the prefix `__eyk_`, which is reserved for generated names",
            "src/lib.eyk:1:13: E0101: identifier `__eyk_a` uses the prefix `__eyk_`, which is reserved for generated names",
            "src/lib.eyk:1:22: E0101: identifier `__eyk_x` uses the prefix `__eyk_`, which is reserved for generated names",
            "src/lib.eyk:2:18: E0101: identifier `__eyk_m` uses the prefix `__eyk_`, which is reserved for generated names",
        ]
    );
    assert_eq!(
        diags_in("fn __eyk_f() {}", Lang::Ja),
        [
            "src/lib.eyk:1:4: E0101: 識別子 `__eyk_f` は、生成名のために予約された接頭辞 `__eyk_` を使っています"
        ]
    );
    // 接頭辞でなければよい。
    ok("fn f_eyk_() { let x__eyk_ = 1; let _eyk_ = 2; }");
}

#[test]
fn verifier_attribute_is_e0102() {
    let src = "#![verifier::loop_isolation(false)]\n#[verifier::external_body]\nfn f() {\n    #[verifier::truncate] (x as u8);\n}\n#[verifier(external)]\nfn g() {}\n";
    assert_eq!(
        diags(src),
        [
            "src/lib.eyk:1:1: E0102: `#[verifier::..]` attributes cannot be written in Ebykn",
            "src/lib.eyk:2:1: E0102: `#[verifier::..]` attributes cannot be written in Ebykn",
            "src/lib.eyk:4:5: E0102: `#[verifier::..]` attributes cannot be written in Ebykn",
            "src/lib.eyk:6:1: E0102: `#[verifier::..]` attributes cannot be written in Ebykn",
        ]
    );
    assert_eq!(
        diags_in("#[verifier::opaque] fn f() {}", Lang::Ja),
        ["src/lib.eyk:1:1: E0102: `#[verifier::..]` 属性は Ebykn では書けません"]
    );
    // Ebykn が認識する属性と Rust の属性はよい（仕様 §4）。
    ok("#[test] #[inline] #[derive(Clone)] fn f() {}\n#[my::verifier] fn g() {}");
}
