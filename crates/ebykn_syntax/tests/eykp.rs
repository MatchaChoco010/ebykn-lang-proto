//! `.eykp` の構文（仕様 §5.7）。`.eykp` は `.eyk` の文法に証明構文を足したもの。`.eyk` では同じ構文が
//! E0303 になる（`docs/current-spec.md` §4）。

mod support;
use ebykn_syntax::ast::*;
use support::*;

const DLIST_P: &str = include_str!("fixtures/dlist.eykp");

#[test]
fn proof_block_and_assert_forms() {
    assert_eq!(ep("proof { l(x); }"), "(proof {(call l x);})");
    assert_eq!(ep("assert x == 1;"), "(assert (== x 1))");
    assert_eq!(ep("assert (x == 1);"), "(assert (paren (== x 1)))");
    assert_eq!(
        ep("assert x == 1 by { l(x); };"),
        "(assert (== x 1) by {(call l x);})"
    );
    assert_eq!(
        ep("assert x * y >= 0 by(nonlinear_arith) requires x >= 0, y >= 0;"),
        "(assert (>= (* x y) 0) by(nonlinear_arith) requires (>= x 0) (>= y 0))"
    );
    assert_eq!(
        ep("assert x & 1 == 0 by(bit_vector);"),
        "(assert (== (& x 1) 0) by(bit_vector))"
    );
}

/// `assert forall .. by { }` は束縛子を `by` の中で使える形。`by { }` がなければ量化子の式の `assert`。
#[test]
fn assert_forall_with_and_without_by() {
    assert_eq!(
        ep("assert forall i in 0..n, f(i) > 0 by { l(i); };"),
        "(assert-forall [i in (.. 0 n)] (> (call f i) 0) by {(call l i);})"
    );
    assert_eq!(
        ep("assert forall i: int, j: int, f(i, j) by { };"),
        "(assert-forall [i:int, j:int] (call f i j) by {})"
    );
    assert_eq!(
        ep("assert forall i: int, f(i);"),
        "(assert (forall [i:int] (call f i)))"
    );
    assert_eq!(
        ep("assert forall i: int, f(i) by(nonlinear_arith);"),
        "(assert (forall [i:int] (call f i)) by(nonlinear_arith))"
    );
    assert_eq!(
        ep("assert exists i: int, f(i) by { };"),
        "(assert (exists [i:int] (call f i)) by {})"
    );
}

#[test]
fn spec_let_reveal_hide() {
    assert_eq!(ep("spec let before = *self;"), "(spec-let before (* self))");
    assert_eq!(ep("spec let n: int = 3;"), "(spec-let n: int 3)");
    assert_eq!(ep("reveal(f);"), "(reveal f)");
    assert_eq!(ep("reveal(m::f, 3);"), "(reveal m::f 3)");
    assert_eq!(ep("hide(Self::f);"), "(hide Self::f)");
}

#[test]
fn ext_eq_and_triggers() {
    assert_eq!(ep("assert s =~= t;"), "(assert (=~= s t))");
    assert_eq!(
        ep("assert forall i: int, #![trigger f(i), g(i)] f(i) == g(i);"),
        "(assert (forall [i:int] #![trigger (call f i) (call g i)] (== (call f i) (call g i))))"
    );
    assert_eq!(
        ep("assert forall i: int, #![auto] f(i) by { };"),
        "(assert-forall [i:int] #![auto] (call f i) by {})"
    );
    assert_eq!(
        ep("assert forall i: int, #[trigger] f(i) > 0;"),
        "(assert (forall [i:int] (> #[trigger] (call f i) 0)))"
    );
}

#[test]
fn lemma_items() {
    assert_eq!(
        heads_p("lemma fn l(x: int) requires x > 0 ensures x >= 1 { assert x > 0; }"),
        ["lemma fn l(x: int) (requires (> x 0)) (ensures (>= x 1)) {(assert (> x 0))}"]
    );
    assert_eq!(
        heads_p("lemma fn l(x: u8) by(bit_vector) ensures x & 0 == 0 { }"),
        ["lemma fn l(x: u8) (by bit_vector) (ensures (== (& x 0) 0)) {}"]
    );
    // 本体のない補題も書ける（`.eyk` の写し）。
    assert_eq!(
        heads_p("lemma fn l() ensures true;"),
        ["lemma fn l() (ensures true);"]
    );
    assert_eq!(
        heads_p("pub lemma group g { a, m::b, }\nuse lemmas g;"),
        ["pub lemma group g { a, m::b }", "use lemmas g"]
    );
    assert_eq!(ep("use lemmas m::g;"), "(item use lemmas m::g)");
    // `use lemmas::x;` は Rust の `use`。
    assert_eq!(heads_p("use lemmas::x;"), ["use"]);
    assert_eq!(
        heads_p(
            "#[opaque] spec fn f(x: int) -> int = x;\n#[rlimit(20)] #[spinoff_prover] lemma fn l() { }"
        ),
        ["spec fn f(x: int) -> int = x", "lemma fn l() {}"]
    );
}

#[test]
fn misplaced_proof_syntax_in_eykp() {
    rejects_p(
        "fn f() { g(proof { 1 }) }",
        "`proof { .. }` can only be written as a statement",
    );
    // マクロの引数の中は Ebykn が読まないので、`.eykp` でも証明構文を書けない。
    for (src, written) in [
        ("vec![reveal(g)]", "reveal(..)"),
        ("vec![{ proof { } 1 }]", "proof { .. }"),
        ("vec![{ assert x; 1 }]", "assert"),
        ("vec![{ spec let y = 1; y }]", "spec let"),
    ] {
        rejects_p(
            &format!("fn f() {{ {src}; }}"),
            &format!("`{written}` cannot be written in code that Ebykn does not read"),
        );
    }
    rejects_p(
        "fn f() by(bit_vector) { }",
        "`by(..)` cannot be written here",
    );
    rejects_p(
        "spec fn f() -> int by(bit_vector) = 1;",
        "`by(..)` cannot be written here",
    );
    rejects_p("lemma fn l() by(nonsense) { }", "expected a prover mode");
    rejects_p(
        "unchecked lemma fn l() { }",
        "`unchecked lemma fn .. { .. }` cannot be written here",
    );
    rejects_p(
        "pub use lemmas g;",
        "`pub use lemmas` cannot be written here",
    );
    rejects_p(
        "fn f() { lemma group g { a } }",
        "`lemma group` cannot be written here",
    );
    rejects_p(
        "trait T { lemma fn l() { } }",
        "`lemma fn .. { .. } in a trait` cannot be written here",
    );
    rejects_p("#[auto] fn f() { }", "`#[auto]` cannot be written here");
    rejects_p(
        "fn f() { #[cfg(x)] assert true; }",
        "`#[..]` cannot be written here",
    );
    rejects_p(
        "fn f() { assert x by g; }",
        "expected `{` or `(` after `by`",
    );
}

/// `.eykp` でも Verus の書き方は拒否する（同じ概念を二つの形で書けるようにしない。`admit` / `assume` は
/// 文法上書けない。設計ノート §3.2、§9）。文言は `.eykp` を名指す。
#[test]
fn verus_syntax_is_still_rejected_in_eykp() {
    rejects_p(
        "fn f() { assert(forall|x: int| x == x); }",
        "is Verus syntax; in Ebykn write `forall x: T, P`",
    );
    rejects_p(
        "proof fn l() { }",
        "`proof fn` is Verus syntax; in Ebykn write `lemma fn`",
    );
    rejects_p(
        "fn f() { assume(false); }",
        "is Verus syntax and cannot be written in `.eykp`",
    );
    rejects_p(
        "fn f() { proof { admit(); } }",
        "cannot be written in `.eykp`",
    );
    rejects_p(
        "fn f() { proof { let ghost x = 1; } }",
        "cannot be written in `.eykp`",
    );
    rejects_p(
        "broadcast use g;",
        "in Ebykn write `lemma group / use lemmas`",
    );
    rejects_p(
        "fn f() { reveal_with_fuel(g, 2); }",
        "in Ebykn write `reveal(f, n)`",
    );
    rejects_p(
        "fn f() { g(reveal_with_fuel(g, 2)); }",
        "in Ebykn write `reveal(f, n)`",
    );
    rejects_p("#[verifier::opaque] spec fn f() -> int = 1;", "E0102");
    rejects_p(
        "fn f() { 1 !~= 2; }",
        "`!~=` is Verus syntax and cannot be written in `.eykp`",
    );
}

/// 同じ構文は `.eyk` では E0303。
#[test]
fn proof_syntax_is_e0303_in_eyk() {
    for (src, written) in [
        ("fn f() { proof { } }", "proof { .. }"),
        ("fn f() { assert x; }", "assert"),
        ("fn f() { spec let x = 1; }", "spec let"),
        ("fn f() { reveal(g); }", "reveal"),
        ("fn f() { hide(g); }", "hide"),
        ("fn f() { a =~= b; }", "=~="),
        ("lemma fn l() { }", "lemma fn .. { .. }"),
        ("lemma fn l() by(bit_vector);", "by(..)"),
        ("lemma group g { a }", "lemma group"),
        ("use lemmas g;", "use lemmas"),
        ("#[opaque] spec fn f() -> int = 1;", "#[opaque]"),
        (
            "fn f() -> bool { forall x: int, #![auto] x == x }",
            "#![auto]",
        ),
    ] {
        let d = diags(src);
        assert!(
            d.iter()
                .any(|d| d.contains(&format!("E0303: `{written}` is proof syntax"))),
            "{src}: {d:?}"
        );
        // 証明構文は AST に入らない（書き下しは `.eyk` の AST に証明構文が来ないことに頼る）。
        let (_, p) = parse(src);
        assert!(!format!("{:?}", p.file).contains("Proof("), "{src}");
    }
    // 量化子の属性は読み飛ばしたうえで E0303 だけを出す（構文エラーを重ねない）。
    assert_eq!(
        diags("fn f() -> bool { forall x: int, #![trigger g(x)] g(x) }"),
        [
            "src/lib.eyk:1:33: E0303: `#![trigger]` is proof syntax and cannot be written in `.eyk`; write it in the proof file (`proof/*.eykp`)"
        ]
    );
}

/// 付録 A.2 の `.eykp` を、現行仕様の形（`docs/current-spec.md` の「付録の例との食い違い」）に直し、
/// 省略（`/* .. */`、`{ .. }`）を埋めたもの。
#[test]
fn appendix_a2_reads() {
    let f = ok_p(DLIST_P);
    let heads: Vec<String> = f.items.iter().map(item_head).collect();
    assert_eq!(heads.len(), 6);
    assert!(
        heads[5].starts_with(
            "lemma fn push_keeps_invariant(a: DList<T>, b: DList<T>, n: Ptr<Node<T>>)"
        )
    );
    let ItemKind::Impl(i) = &f.items[4].kind else {
        panic!("impl");
    };
    let push = i
        .items
        .iter()
        .find_map(|it| match &it.kind {
            ItemKind::Fn(m) if m.sig.ident.name == "push_back" => Some(m),
            _ => None,
        })
        .expect("push_back");
    let Some(FnBody::Block(b)) = &push.body else {
        panic!("body");
    };
    let stmts: Vec<String> = b.stmts.iter().map(stmt).collect();
    assert_eq!(
        stmts.first().map(String::as_str),
        Some("(spec-let before (* self))")
    );
    assert_eq!(
        stmts[stmts.len() - 2],
        "(proof {(call push_keeps_invariant before (* self) n);})"
    );
    let ItemKind::LemmaFn(l) = &f.items[5].kind else {
        panic!("lemma");
    };
    let body = l.body.as_ref().expect("lemma の本体");
    assert_eq!(
        body.stmts.iter().map(stmt).collect::<Vec<_>>(),
        [
            "(assert (.no_duplicates (@ b order)))",
            "(assert-forall [i in (.. 0 (.len (@ b order)))] (== (. (index (@ b nodes) (index (@ b order) i)) prev) (.get (@ b order) (- i 1))) by {(if (< i (.len (@ a order))) {(assert (== (index (@ b order) i) (index (@ a order) i)))})})",
        ]
    );
}

/// `.eykp` でも書けないものは、`.eyk` でも証明ファイルへ案内しない（E0303 にしない。#86 のレビュー）。
#[test]
fn eyk_does_not_point_to_eykp_for_what_eykp_rejects_too() {
    for (src, want) in [
        (
            "fn f() { reveal_with_fuel(g, 2); }",
            "in Ebykn write `reveal(f, n)`",
        ),
        (
            "fn f() { let y = reveal_with_fuel(g, 2); }",
            "in Ebykn write `reveal(f, n)`",
        ),
        (
            "fn f() by(bit_vector) { }",
            "`by(..)` cannot be written here",
        ),
        (
            "spec fn f() -> int by(bit_vector) = 1;",
            "`by(..)` cannot be written here",
        ),
        ("lemma fn l() by(nonsense);", "expected a prover mode"),
        (
            "unchecked lemma fn l() { }",
            "`unchecked lemma fn .. { .. }` cannot be written here",
        ),
        (
            "trait T { lemma fn l() { } }",
            "`lemma fn .. { .. } in a trait` cannot be written here",
        ),
        ("#[auto] fn f() { }", "`#[auto]` cannot be written here"),
        (
            "#[memoize] fn f() { }",
            "`#[memoize]` cannot be written here",
        ),
        ("#[rlimit(x)] fn f() { }", "write `#[rlimit(20)]`"),
    ] {
        for text in [diags(src), diags_p(src)] {
            assert!(text.iter().any(|d| d.contains(want)), "{src}: {text:?}");
            assert!(!text.iter().any(|d| d.contains("E0303")), "{src}: {text:?}");
        }
    }
    // `.eykp` に書ける形は、`.eyk` では E0303 のまま。
    for src in [
        "lemma fn l() by(bit_vector);",
        "#[opaque] spec fn f() -> int = 1;",
        "#[rlimit(20)] fn f() { }",
    ] {
        assert!(diags(src).iter().any(|d| d.contains("E0303")), "{src}");
        assert!(diags_p(src).is_empty(), "{src}: {:?}", diags_p(src));
    }
}

/// 仕様 §5.7：`"spec" "let" Pat "=" Expr ";"`（#86 のレビュー）。
#[test]
fn spec_let_needs_an_initializer_and_no_else() {
    rejects_p("fn f() { spec let x; }", "expected `= expr` in `spec let`");
    rejects_p(
        "fn f() { spec let Some(x) = o else { return; }; }",
        "`spec let .. else { .. }` cannot be written here",
    );
}

/// 証明の属性は、書ける位置（式の `#[trigger]`、量化子の本体の前の `#![trigger ..]` / `#![auto]`、
/// `spec fn` の `#[opaque]` / `#[memoize]`、`fn` / `lemma fn` の `#[rlimit(n)]` / `#[spinoff_prover]`）と
/// 正しい引数でだけ書ける（#86 のレビュー）。
#[test]
fn proof_attributes_are_checked_for_position_and_arguments() {
    for (src, written) in [
        ("#![auto]\nfn f() { }", "#![auto]"),
        ("#![trigger x]\nfn f() { }", "#![trigger]"),
        ("mod m { #![auto] }", "#![auto]"),
        ("#[trigger] struct S { a: u8 }", "#[trigger]"),
        ("struct S { #[trigger] a: u8 }", "#[trigger]"),
        ("fn f() { #[trigger] let x = 1; }", "#[trigger]"),
        ("#[memoize] fn f() { }", "#[memoize]"),
        ("#[opaque] fn f() { }", "#[opaque]"),
        (
            "#[spinoff_prover] spec fn f() -> int = 1;",
            "#[spinoff_prover]",
        ),
        (
            "#[cfg_attr(all(), opaque)] spec fn f() -> int = 1;",
            "#[opaque]",
        ),
    ] {
        rejects_p(src, &format!("`{written}` cannot be written here"));
    }
    for (src, form) in [
        ("#[rlimit(x)] fn f() { }", "#[rlimit(20)]"),
        ("#[rlimit] fn f() { }", "#[rlimit(20)]"),
        ("#[opaque(1)] spec fn f() -> int = 1;", "#[opaque]"),
        ("fn f() { assert #[trigger(x)] g(x); }", "#[trigger]"),
    ] {
        rejects_p(src, &format!("write `{form}`"));
    }
    ok_p(
        "#[memoize] #[opaque] spec fn f() -> int = 1;\n#[rlimit(20)] #[spinoff_prover] fn g() { }",
    );
}

/// 「一つ以上」「一つだけ」の文法（#86 のレビュー）。
#[test]
fn at_least_one_and_at_most_one() {
    rejects_p(
        "lemma group g { }",
        "expected at least one lemma in `lemma group g { .. }`",
    );
    rejects_p(
        "fn f() { assert forall i: int, #![trigger] i == i; }",
        "expected at least one expression in `#![trigger ..]`",
    );
    rejects_p(
        "lemma fn l() by(bit_vector) by(nonlinear_arith) { }",
        "`a second by(..)` cannot be written here",
    );
}

/// `.eyk` が E0303 でまるごと拒否する証明のコードの中の属性には、「ここには書けない」を重ねない
/// （#86 の再確認 SF1）。読めない項目の中も同じ。
#[test]
fn no_extra_attribute_errors_inside_rejected_code() {
    for src in [
        "fn f() { proof { #[trigger] g(x); } }",
        "lemma fn l() ensures true { assert forall x: int, #![auto] g(x) by { }; }",
        "lemma fn l() ensures true { #[trigger] g(x) }",
        "fn f() { assert forall x: int, #![trigger g(x)] g(x); }",
    ] {
        let d = diags(src);
        assert!(!d.is_empty(), "{src}");
        assert!(d.iter().all(|d| d.contains("E0303")), "{src}: {d:?}");
        // `.eykp` では通る。
        assert!(diags_p(src).is_empty(), "{src}: {:?}", diags_p(src));
    }
    // 読めない項目の中の属性にも重ねない。
    let d = diags_p("#[opaque] spec fn f() -> int = ;");
    assert!(
        !d.iter().any(|d| d.contains("cannot be written here")),
        "{d:?}"
    );
}

/// 関数の本体の中の項目に付けた証明の属性（#86 の再確認 SF2）。
#[test]
fn proof_attributes_on_items_inside_bodies() {
    for src in [
        "fn f() { #[opaque] spec fn g() -> int = 1; }",
        "fn f() { #[rlimit(3)] fn g() { } }",
        "lemma fn l() { #[rlimit(2)] lemma fn m() { } }",
    ] {
        assert!(diags_p(src).is_empty(), "{src}: {:?}", diags_p(src));
    }
    for src in [
        "fn f() { #[opaque] spec fn g() -> int = 1; }",
        "fn f() { #[rlimit(3)] fn g() { } }",
    ] {
        let d = diags(src);
        assert!(
            !d.is_empty() && d.iter().all(|d| d.contains("E0303")),
            "{src}: {d:?}"
        );
    }
    // 検証しない `unchecked` の関数には付かない。
    rejects_p(
        "#[rlimit(3)] unchecked fn g() { }",
        "`#[rlimit]` cannot be written here",
    );
    rejects_p(
        "#[spinoff_prover] unchecked lemma fn g();",
        "`#[spinoff_prover]` cannot be written here",
    );
}

/// `.eykp` でも通らない `lemma group` / `use lemmas` の形は、`.eyk` でも E0001（#86 の再確認 SF3）。
#[test]
fn lemma_group_and_use_lemmas_shapes_in_eyk() {
    for (src, want) in [
        (
            "fn f() { lemma group g { a } }",
            "`lemma group` cannot be written here",
        ),
        (
            "impl S { lemma group g { a } }",
            "`lemma group` cannot be written here",
        ),
        ("lemma group g { }", "expected at least one lemma"),
        (
            "pub use lemmas g;",
            "`pub use lemmas` cannot be written here",
        ),
    ] {
        for d in [diags(src), diags_p(src)] {
            assert!(d.iter().any(|x| x.contains(want)), "{src}: {d:?}");
            assert!(!d.iter().any(|x| x.contains("E0303")), "{src}: {d:?}");
        }
    }
}

/// 量化子の属性の誤りは、その誤りだけを出す（#86 の再確認の nit）。
#[test]
fn quantifier_attribute_errors_are_specific() {
    let d = diags_p("fn f() { assert forall i: int, #![trigger] i == i; }");
    assert!(
        d.iter()
            .any(|x| x.contains("expected at least one expression")),
        "{d:?}"
    );
    assert!(
        !d.iter().any(|x| x.contains("cannot be written here")),
        "{d:?}"
    );
    let d = diags_p("fn f() { assert forall i: int, #![auto(x)] i == i; }");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].contains("write `#![auto]`"), "{d:?}");
    // 証明の文の証明の属性は、名前つきの 1 つだけ。
    let d = diags_p("fn f() { #[trigger] proof { } }");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].contains("`#[trigger]` cannot be written here"),
        "{d:?}"
    );
}
