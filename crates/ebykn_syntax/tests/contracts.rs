//! 契約（仕様 §5.2）。

mod support;
use ebykn_syntax::ast::*;
use support::*;

fn contract_of(src: &str) -> Contract {
    match &ok(src).items[0].kind {
        ItemKind::Fn(f) => f.contract.clone(),
        ItemKind::SpecFn(f) => f.contract.clone(),
        ItemKind::LemmaFn(f) => f.contract.clone(),
        _ => panic!("fn"),
    }
}

#[test]
fn all_clauses_in_any_order() {
    let src = "fn f(p: Ptr<u64>, c: &Cell<u64>) -> (q: Ptr<u64>)\n\
        ensures q != p\n\
        no_panic\n\
        uses &mut p@, &c@, r: &mut Region<Ptr<u64>>, s: Region<Cell<u64>>\n\
        returns q\n\
        decreases 3, 4\n\
        requires p@.is_init(),\n\
        gives q@\n\
        { q }";
    assert_eq!(
        support::contract(&contract_of(src)),
        "(ensures (!= q p)) (no_panic) (uses &mut p@, &c@, r: &mut Region<Ptr<u64>>, s: Region<Cell<u64>>) \
         (returns q) (decreases 3 4) (requires (.is_init p@)) (gives q@)"
    );
}

#[test]
fn repeated_clauses_are_concatenated() {
    let c = contract_of(
        "fn f(p: Ptr<u64>) uses p@ requires a ensures b requires c, d uses &q@ ensures e { }",
    );
    let req: Vec<String> = c.requires().map(expr).collect();
    let ens: Vec<String> = c.ensures().map(expr).collect();
    let uses: Vec<String> = c
        .uses()
        .map(|u| match u {
            UsesItem::Perm { ident, .. } => ident.name.clone(),
            UsesItem::Named { ident, .. } => ident.name.clone(),
        })
        .collect();
    assert_eq!(req, ["a", "c", "d"]);
    assert_eq!(ens, ["b", "e"]);
    assert_eq!(uses, ["p", "q"]);
    // 書かれた順も残る（M3 の剥がし一致に使う）。
    assert_eq!(c.clauses.len(), 6);
}

#[test]
fn uses_modes() {
    let c = contract_of("fn f(p: Ptr<u64>) uses p@, &q@, &mut r@ { }");
    let modes: Vec<PermMode> = c
        .uses()
        .map(|u| match u {
            UsesItem::Perm { mode, .. } => *mode,
            UsesItem::Named { .. } => panic!("named"),
        })
        .collect();
    assert_eq!(modes, [PermMode::Move, PermMode::Shared, PermMode::Mut]);
    rejects(
        "fn f() uses p { }",
        "1:13: E0001: syntax error: expected a permission (`p@`, `&p@`, `&mut p@`) or a region (`r: Region<..>`)",
    );
    rejects(
        "fn f() gives q { }",
        "1:14: E0001: syntax error: expected a permission of a named return value (`q@`)",
    );
}

#[test]
fn where_clause_then_contract() {
    assert_eq!(
        heads("fn g<T>(x: T) -> T where T: Copy, requires true { x }"),
        ["fn g(x: T) -> T where T : Copy (requires true) {..}"]
    );
    assert_eq!(
        heads("fn g<T>(x: T) -> T where T: Copy + Clone uses p@ { x }"),
        ["fn g(x: T) -> T where T : Copy + Clone (uses p@) {..}"]
    );
}

#[test]
fn clause_lists_end_at_the_body() {
    // 最後の式の直後の `{` は本体（構造体リテラルにしない）。
    assert_eq!(
        heads("fn f() -> S ensures r == x { S { a: 1 } }"),
        ["fn f() -> S (ensures (== r x)) {..}"]
    );
    // 最上位の `=` は代入ではなく純粋関数の本体（`OPEN.md`）。
    assert_eq!(
        heads("fn f(n: u64) -> u64 requires n > 0 = n - 1;"),
        ["fn f(n: u64) -> u64 (requires (> n 0)) = (- n 1)"]
    );
    assert_eq!(
        heads("spec fn s(n: nat) -> S decreases n = S { a: n };"),
        ["spec fn s(n: nat) -> S (decreases n) = (struct S a:n)"]
    );
    // 末尾のカンマ。
    assert_eq!(
        heads("fn f() requires a, b, ensures c, { }"),
        ["fn f() (requires a b) (ensures c) {..}"]
    );
}

#[test]
fn by_clause_is_eykp_only() {
    assert_eq!(
        diags("lemma fn l() by(nonlinear_arith) ensures true;"),
        [
            "src/lib.eyk:1:14: E0001: syntax error: `by(..)` is proof syntax; it can only be written in `.eykp`, which is not supported yet"
        ]
    );
}

#[test]
fn verus_clauses_are_rejected() {
    rejects(
        "fn f() no_unwind { }",
        "1:8: E0001: syntax error: `no_unwind` is Verus syntax; in Ebykn write `no_panic`",
    );
    for kw in [
        "recommends x",
        "opens_invariants any",
        "default_ensures x",
        "invariant x",
    ] {
        rejects(
            &format!("fn f() {kw} {{ }}"),
            "is Verus syntax and cannot be written in `.eyk`",
        );
    }
}
