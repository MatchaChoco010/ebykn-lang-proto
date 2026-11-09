//! 論理式（仕様 §5.3）。

mod support;
use ebykn_syntax::diag::Lang;
use support::*;

#[test]
fn quantifier_binders() {
    assert_eq!(e("forall x: int, x > 0"), "(forall [x:int] (> x 0))");
    assert_eq!(
        e("exists x: Map<int, int>, f(x)"),
        "(exists [x:Map<int,int>] (call f x))"
    );
    assert_eq!(
        e("forall p in self@order, ok(p)"),
        "(forall [p in (@ self order)] (call ok p))"
    );
    // 型も `in` もない束縛子は OPEN-8 の既定で許す。
    assert_eq!(e("forall x, P(x)"), "(forall [x] (call P x))");
    // 混ぜてよい。
    assert_eq!(
        e("forall x: int, y in s, z, x < y"),
        "(forall [x:int, y in s, z] (< x y))"
    );
}

/// 仕様 §5.3：`,` の直後が `Ident :`、`Ident in`、`Ident ,` なら束縛子、そうでなければ本体。
#[test]
fn binder_reading_rule() {
    assert_eq!(e("forall x, y, P"), "(forall [x, y] P)");
    assert_eq!(e("forall x, y + 1 > 0"), "(forall [x] (> (+ y 1) 0))");
    assert_eq!(e("forall x, y"), "(forall [x] y)");
    assert_eq!(e("forall x, f(y)"), "(forall [x] (call f y))");
    // `::` は型注釈ではない。
    assert_eq!(e("forall x, y::Z"), "(forall [x] y::Z)");
}

#[test]
fn in_right_hand_sides() {
    assert_eq!(
        e("forall i in 0..n, i < n"),
        "(forall [i in (.. 0 n)] (< i n))"
    );
    assert_eq!(
        e("forall i in 0..=n, i <= n"),
        "(forall [i in (..= 0 n)] (<= i n))"
    );
    assert_eq!(
        e("forall i in 0..self@order.len(), ok(i)"),
        "(forall [i in (.. 0 (.len (@ self order)))] (call ok i))"
    );
    assert_eq!(e("exists k in m, k > 0"), "(exists [k in m] (> k 0))");
}

#[test]
fn quantifier_body_extends_to_the_end() {
    assert_eq!(
        e("forall x: int, x > 0 ==> y"),
        "(forall [x:int] (==> (> x 0) y))"
    );
    assert_eq!(
        e("forall x: int, y: int, x < y ==> exists z: int, x < z && z < y"),
        "(forall [x:int, y:int] (==> (< x y) (exists [z:int] (&& (< x z) (< z y)))))"
    );
    assert_eq!(
        e("(forall x: int, x > 0) && y"),
        "(&& (paren (forall [x:int] (> x 0))) y)"
    );
    // 構造体リテラルを読まない位置（`if` の条件、契約の句）では、本体の後の `{` は本体ではない。
    assert_eq!(
        e("if forall x: int, x == y { 1 } else { 2 }"),
        "(if (forall [x:int] (== x y)) {1} {2})"
    );
    assert_eq!(
        heads("fn f() ensures forall x: int, x == y { }"),
        ["fn f() (ensures (forall [x:int] (== x y))) {..}"]
    );
}

#[test]
fn quantifier_needs_a_comma() {
    assert_eq!(
        diags("fn f() { forall x: int x }"),
        ["src/lib.eyk:1:24: E0001: syntax error: expected `,`"]
    );
}

#[test]
fn and_or_blocks() {
    assert_eq!(e("and { a, b, or { c, d }, }"), "(and a b (or c d))");
    assert_eq!(
        e("and { forall p in self@order, self@nodes.contains(p), x }"),
        "(and (forall [p in (@ self order)] (.contains (@ self nodes) p)) x)"
    );
    assert_eq!(
        diags("fn f() { and { } }"),
        [
            "src/lib.eyk:1:14: E0001: syntax error: expected at least one expression in `and { .. }` / `or { .. }`"
        ]
    );
    assert_eq!(
        diags_in("fn f() { or { } }", Lang::Ja),
        [
            "src/lib.eyk:1:13: E0001: 構文エラー: `and { .. }` / `or { .. }` の中に一つ以上の式が必要です"
        ]
    );
}

#[test]
fn at_forms() {
    assert_eq!(e("p@"), "p@");
    assert_eq!(
        e("p@.is_init() && p@.value() == 1"),
        "(&& (.is_init p@) (== (.value p@) 1))"
    );
    assert_eq!(
        e("self@nodes[self@order[i]].prev"),
        "(. (index (@ self nodes) (index (@ self order) i)) prev)"
    );
    // 付録 A は `self` 以外にも `@name` を付ける（`OPEN.md`）。
    assert_eq!(
        e("old(self)@order.index_of(h)"),
        "(.index_of (@ (call old self) order) h)"
    );
    assert_eq!(
        e("b@order == a@order.push(n)"),
        "(== (@ b order) (.push (@ a order) n))"
    );
    assert_eq!(e("*p == old(*p)"), "(== (* p) (call old (* p)))");
    assert_eq!(e("final(x)"), "(call final x)");
}

#[test]
fn view_is_written_as_a_method() {
    assert_eq!(e("v.view()"), "(.view v)");
    assert_eq!(
        diags("fn f() { v.foo()@ }"),
        ["src/lib.eyk:1:10: E0001: syntax error: `e@` is Verus syntax; in Ebykn write `e.view()`"]
    );
}

/// Verus にあって Ebykn にない式の書き方（設計ノート §3.2「同じ概念を複数の形で書けるようにしない」）。
#[test]
fn verus_expression_forms_are_rejected() {
    let cases = [
        (
            "forall|x: int| x > 0",
            "`forall|x| ..` is Verus syntax; in Ebykn write `forall x: T, P`",
        ),
        (
            "exists|x: int| x > 0",
            "`exists|x| ..` is Verus syntax; in Ebykn write `exists x: T, P`",
        ),
        (
            "choose|x: int| x > 0",
            "`choose|x| ..` is Verus syntax and cannot be written in `.eyk`",
        ),
        (
            "&&& a &&& b",
            "`&&&` is Verus syntax; in Ebykn write `and { .. }`",
        ),
        (
            "||| a ||| b",
            "`|||` is Verus syntax; in Ebykn write `or { .. }`",
        ),
        (
            "a <== b",
            "`a <== b` is Verus syntax; in Ebykn write `b ==> a`",
        ),
        ("a === b", "`===` is Verus syntax; in Ebykn write `==`"),
        ("a !== b", "`!==` is Verus syntax; in Ebykn write `!=`"),
        (
            "a =~~= b",
            "`=~~=` is Verus syntax and cannot be written in `.eyk`",
        ),
        (
            "x is Some",
            "`e is V` is Verus syntax; in Ebykn write `matches!(e, V { .. })`",
        ),
        ("x isnt Some", "`e isnt V` is Verus syntax"),
        (
            "s has x",
            "`s has x` is Verus syntax; in Ebykn write `s.contains(x)`",
        ),
        ("s hasnt x", "`s hasnt x` is Verus syntax"),
        (
            "x matches Some(y)",
            "`e matches p` is Verus syntax; in Ebykn write `matches!(e, p)`",
        ),
        (
            "x->f",
            "`e->f` is Verus syntax and cannot be written in `.eyk`",
        ),
        (
            "a < b < c",
            "comparison operators cannot be chained; write `a < b && b < c`",
        ),
    ];
    for (src, want) in cases {
        rejects(&format!("fn f() {{ {src} }}"), want);
    }
    // 括弧で囲めば比較を重ねられる（Rust と同じ）。
    assert_eq!(e("(a < b) == c"), "(== (paren (< a b)) c)");
}

#[test]
fn proof_expressions_are_rejected() {
    for (src, want) in [
        ("assert(x);", "`assert` is proof syntax"),
        ("assert x;", "`assert` is proof syntax"),
        ("proof { }", "`proof { .. }` is proof syntax"),
        ("reveal(f);", "`reveal` is proof syntax"),
        ("hide(f);", "`hide` is proof syntax"),
        ("spec let x = 1;", "`spec let` is proof syntax"),
        ("#[trigger] x", "`#[trigger]` is proof syntax"),
        ("let y = { assert(x); 1 };", "`assert` is proof syntax"),
        ("f(proof { 1 })", "`proof { .. }` is proof syntax"),
    ] {
        rejects(&format!("fn f() {{ {src} }}"), want);
    }
    rejects(
        "fn f() { assume(x); }",
        "`assume(..)` is Verus syntax and cannot be written in `.eyk`",
    );
    // Rust の `assert!` はよい。
    ok("fn f() { assert!(x); assert_eq!(a, b); }");
}
