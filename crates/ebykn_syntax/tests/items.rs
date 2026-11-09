//! 項目（仕様 §5.1）。

mod support;
use ebykn_syntax::ast::*;
use ebykn_syntax::diag::Lang;
use support::*;

#[test]
fn fn_with_block_pure_body_and_named_return() {
    assert_eq!(
        heads(
            "pub fn f(x: u64) -> u64 { x }\n\
             fn add(a: u64, b: u64) -> u64 = a + b;\n\
             fn g(&mut self) -> (h: Ptr<Node<T>>) { h }\n\
             const unsafe fn q() {}"
        ),
        [
            "pub fn f(x: u64) -> u64 {..}",
            "fn add(a: u64, b: u64) -> u64 = (+ a b)",
            "fn g(&mut self) -> (h: Ptr<Node<T>>) {..}",
            "fn q() {..}",
        ]
    );
    // 名前付き戻り値と、戻り値の型としてのタプルを分ける。
    assert_eq!(
        heads("fn t() -> (u64, u64) { (1, 2) }"),
        ["fn t() -> (u64,u64) {..}"]
    );
}

#[test]
fn fn_needs_a_body_outside_traits() {
    assert_eq!(
        diags("fn f();"),
        ["src/lib.eyk:1:7: E0001: syntax error: expected a function body (`{ .. }` or `= expr;`)"]
    );
    rejects("fn f() -> u64", "expected a contract clause");
}

#[test]
fn spec_fn_open_and_closed() {
    assert_eq!(
        heads(
            "spec fn fib(n: nat) -> nat decreases n = if n < 2 { n } else { fib(n - 1) };\n\
             pub closed spec fn has(&self, h: Ptr<Node<T>>) -> bool = self@order.contains(h);\n\
             spec fn b() -> int = { let x = 1; x };"
        ),
        [
            "spec fn fib(n: nat) -> nat (decreases n) = (if (< n 2) {n} {(call fib (- n 1))})",
            "pub closed spec fn has(&self, h: Ptr<Node<T>>) -> bool = (.contains (@ self order) h)",
            "spec fn b() -> int = {(let x 1) x}",
        ]
    );
    // 本体は `= e;`。ブロック本体と、trait の外の `;` は書けない。
    rejects(
        "spec fn s() -> int { 1 }",
        "1:20: E0001: syntax error: expected a contract clause",
    );
    rejects(
        "spec fn s() -> int;",
        "1:19: E0001: syntax error: expected the body of a `spec fn`",
    );
    rejects(
        "spec fn s() = 1;",
        "expected the return type of a `spec fn`",
    );
    rejects(
        "spec fn s() -> (r: int) = 1;",
        "`spec fn .. -> (r: T)` cannot be written here",
    );
    rejects(
        "closed fn x() {}",
        "1:8: E0001: syntax error: expected `spec fn` after `closed`",
    );
}

#[test]
fn lemma_fn_has_no_body_in_eyk() {
    assert_eq!(
        heads("lemma fn l(x: int) requires x > 0 ensures x >= 0;"),
        ["lemma fn l(x: int) (requires (> x 0)) (ensures (>= x 0));"]
    );
    assert_eq!(
        diags("lemma fn l() ensures true { }"),
        [
            "src/lib.eyk:1:27: E0303: `lemma fn .. { .. }` is proof syntax and cannot be written in `.eyk`; write it in the proof file (`proof/*.eykp`)"
        ]
    );
    rejects(
        "lemma fn l() -> int;",
        "`lemma fn .. -> T` cannot be written here",
    );
}

#[test]
fn unchecked_fn_and_lemma() {
    assert_eq!(
        heads("unchecked fn u(p: Ptr<u64>) uses p@ { }\nunchecked lemma fn ul() ensures false;"),
        [
            "unchecked fn u(p: Ptr<u64>) (uses p@) {..}",
            "unchecked lemma fn ul() (ensures false);",
        ]
    );
    // ExternFn は性質ファイルだけ（仕様 §5.1、§8）。
    assert_eq!(
        diags("unchecked fn std::mem::swap(a: u64);"),
        [
            "src/lib.eyk:1:14: E0001: syntax error: `unchecked fn path::f(..);` can only be written in a props file, which is not supported yet"
        ]
    );
    rejects(
        "unchecked spec fn s() -> int = 1;",
        "1:11: E0001: syntax error: expected `fn` or `lemma fn` after `unchecked`",
    );
}

#[test]
fn struct_with_owns_and_spec() {
    let f = ok("pub struct S<T> { a: T } owns { r: Region<Ptr<T>>, } spec { pub q: Seq<Ptr<T>> }");
    let ItemKind::Struct(s) = &f.items[0].kind else {
        panic!("struct");
    };
    let owns = s.owns.as_ref().expect("owns");
    let spec = s.spec.as_ref().expect("spec");
    assert_eq!(owns.fields[0].ident.as_ref().unwrap().name, "r");
    assert_eq!(ty(&owns.fields[0].ty), "Region<Ptr<T>>");
    assert!(matches!(spec.fields[0].vis, Vis::Pub(_)));
    assert!(matches!(s.fields, Fields::Named(ref v) if v.len() == 1));
    // 片方だけでもよい。
    assert_eq!(
        heads(
            "struct A { x: u64 } owns { r: Region<Ptr<u64>> }\nstruct B { x: u64 } spec { s: int }"
        ),
        [
            "struct A owns { r: Region<Ptr<u64>> }",
            "struct B spec { s: int }"
        ]
    );
}

#[test]
fn owns_and_spec_on_non_named_types_are_e0401() {
    assert_eq!(
        diags(
            "struct T(u64) owns { r: Region<Ptr<u64>> }\n\
             struct U(u64); spec { q: Seq<int> }\n\
             struct V; owns { r: Region<Ptr<u64>> }\n\
             enum E { A, B } owns { r: Region<Ptr<u64>> } spec { s: int }"
        ),
        [
            "src/lib.eyk:1:15: E0401: `owns { .. }` can only follow a struct with named fields, but `T` is a tuple struct",
            "src/lib.eyk:2:16: E0401: `spec { .. }` can only follow a struct with named fields, but `U` is a tuple struct",
            "src/lib.eyk:3:11: E0401: `owns { .. }` can only follow a struct with named fields, but `V` is a unit struct",
            "src/lib.eyk:4:17: E0401: `owns { .. }` can only follow a struct with named fields, but `E` is an enum",
            "src/lib.eyk:4:46: E0401: `spec { .. }` can only follow a struct with named fields, but `E` is an enum",
        ]
    );
    assert_eq!(
        diags_in("enum E { A } owns { r: Region<Ptr<u64>> }", Lang::Ja),
        [
            "src/lib.eyk:1:14: E0401: `owns { .. }` は名前付きフィールドを持つ構造体にだけ付けられますが、`E` は列挙型です"
        ]
    );
    // enum にも impl Invariant は付けられる（構文上の制限はない）。
    ok("enum E { A } impl Invariant for E { spec fn invariant(&self) -> bool = true; }");
}

#[test]
fn trait_with_contracts_spec_and_lemma() {
    let f = ok("pub trait T: Sized {\n\
                fn m(&self) -> u64 requires true ensures r > 0;\n\
                fn d(&self) { }\n\
                spec fn s(&self) -> int;\n\
                spec fn t(&self) -> int = 1;\n\
                lemma fn l(&self) ensures self.s() >= 0;\n\
                type X;\n\
                const C: u64;\n\
            }");
    let ItemKind::Trait(t) = &f.items[0].kind else {
        panic!("trait");
    };
    let kinds: Vec<&str> = t
        .items
        .iter()
        .map(|i| match &i.kind {
            TraitItemKind::Fn(f) if f.body.is_some() => "fn{}",
            TraitItemKind::Fn(_) => "fn;",
            TraitItemKind::SpecFn(f) if f.body.is_some() => "spec=",
            TraitItemKind::SpecFn(_) => "spec;",
            TraitItemKind::LemmaFn(_) => "lemma",
            TraitItemKind::Rust(_) => "rust",
        })
        .collect();
    assert_eq!(
        kinds,
        ["fn;", "fn{}", "spec;", "spec=", "lemma", "rust", "rust"]
    );
    assert_eq!(t.supertraits.as_ref().unwrap().tokens.to_string(), "Sized");
    // trait の spec fn は契約と closed を持たない（仕様 §5.1 の TraitItem）。
    rejects(
        "trait T { spec fn s(&self) -> int decreases 1; }",
        "expected the body of a `spec fn`",
    );
    rejects(
        "trait T { closed spec fn s(&self) -> int; }",
        "`closed` cannot be written here",
    );
    rejects(
        "trait T { fn f(&self) -> u64 = 1; }",
        "expected `;` or a block `{ .. }`",
    );
}

#[test]
fn subset_type_and_rust_where_are_told_apart() {
    // `where` の後が `Type :` なら Rust の where 句、そうでなければ部分集合型（仕様 §5.1）。
    assert_eq!(
        heads(
            "type FiniteMesh = Mesh where finite(self);\n\
             type T<X> = U<X> where X: Copy;\n\
             type P = U where self.len() > 0;\n\
             type Q<X> where X: Copy = U<X>;\n\
             type R = Vec<u64>;"
        ),
        [
            "subset FiniteMesh = Mesh where (call finite self)",
            "type T = U<X> where X : Copy",
            "subset P = U where (> (.len self) 0)",
            "type Q = U<X> where X : Copy",
            "type R = Vec<u64>",
        ]
    );
}

#[test]
fn impl_spec_and_impls() {
    assert_eq!(
        heads(
            "impl spec rayon::iter::ParallelIterator for Foo { spec fn s() -> int = 1; lemma fn l(); }\n\
             impl<T> spec Clone { spec fn c() -> bool = true; }\n\
             impl<T> View for DList<T> { type V = Seq<T>; closed spec fn view(&self) -> Seq<T> = self@order; }\n\
             impl<T> DList<T> { pub fn new() -> Self { DList { head: None } } }"
        ),
        [
            "impl spec rayon::iter::ParallelIterator for Foo (2 items)",
            "impl spec Clone (1 items)",
            "impl View for DList<T> (2 items)",
            "impl DList<T> (1 items)",
        ]
    );
    rejects(
        "impl spec Clone { fn f() {} }",
        "expected `spec fn` or `lemma fn` in `impl spec`",
    );
    // `spec` が trait の名前なら Rust の impl。
    assert_eq!(heads("impl spec for X {}"), ["impl spec for X (0 items)"]);
}

#[test]
fn invariant_and_view_in_inherent_impl_are_e0403() {
    assert_eq!(
        diags(
            "impl S {\n    fn invariant(&self) -> bool { true }\n    spec fn view(&self) -> int = 1;\n    const view: u64 = 1;\n}"
        ),
        [
            "src/lib.eyk:2:8: E0403: `invariant` cannot be defined directly in `impl T`; write `impl Invariant for T` instead",
            "src/lib.eyk:3:13: E0403: `view` cannot be defined directly in `impl T`; write `impl View for T` instead",
            "src/lib.eyk:4:11: E0403: `view` cannot be defined directly in `impl T`; write `impl View for T` instead",
        ]
    );
    assert_eq!(
        diags_in("impl S { spec fn view(&self) -> int = 1; }", Lang::Ja),
        [
            "src/lib.eyk:1:18: E0403: `view` は `impl T` の中に直接定義できません。`impl View for T` に書きます"
        ]
    );
    // trait の impl と、ほかの名前はよい。
    ok(
        "impl View for S { type V = int; spec fn view(&self) -> int = 1; }\n\
        impl Invariant for S { spec fn invariant(&self) -> bool = true; }\n\
        impl S { fn invariants(&self) {} fn viewer(&self) {} }\n\
        fn invariant() {} fn view() {}",
    );
}

#[test]
fn proof_items_are_rejected_in_eyk() {
    rejects(
        "lemma group g { a, b }",
        "1:1: E0303: `lemma group` is proof syntax",
    );
    rejects("use lemmas g;", "1:1: E0303: `use lemmas` is proof syntax");
    // `use lemmas::x;` は Rust の use。
    assert_eq!(heads("use lemmas::foo;"), ["use"]);
}

#[test]
fn rust_items_pass_through() {
    assert_eq!(
        heads(
            "mod m { fn f() {} }\nmod n;\nuse std::collections::HashMap;\nconst N: u64 = 3;\n\
             static mut G: u64 = 0;\nextern crate core;\n\
             #[derive(Clone)] pub struct P { pub a: u64 }\npub enum E { A(u64), B { x: u64 } }"
        ),
        [
            "mod m",
            "mod n",
            "use",
            "const N",
            "static G",
            "rust`extern crate core ;`",
            "pub struct P",
            "pub enum E",
        ]
    );
    let f = ok("mod m { #![allow(unused)] fn f() {} fn g() {} }");
    let ItemKind::Mod(m) = &f.items[0].kind else {
        panic!("mod");
    };
    let (attrs, items) = m.content.as_ref().unwrap();
    assert_eq!((attrs.len(), items.len()), (1, 2));
}

#[test]
fn items_inside_blocks_use_the_ebykn_grammar() {
    let f = ok("fn h() {\n\
                fn inner(p: Ptr<u64>) uses &mut p@ { take r[p]; }\n\
                struct I { a: u64 } owns { r: Region<Ptr<u64>> }\n\
                spec fn s() -> int = 1;\n\
            }");
    let ItemKind::Fn(h) = &f.items[0].kind else {
        panic!("fn");
    };
    let Some(FnBody::Block(b)) = &h.body else {
        panic!("body");
    };
    let s: Vec<String> = b.stmts.iter().map(stmt).collect();
    assert_eq!(
        s,
        [
            "(item fn inner(p: Ptr<u64>) (uses &mut p@) {..})",
            "(item struct I owns { r: Region<Ptr<u64>> })",
            "(item spec fn s() -> int = 1)",
        ]
    );
    // ブロックの中の項目の誤りも報告する。
    rejects(
        "fn h() { struct T(u64); owns { r: Region<Ptr<u64>> } }",
        "E0401",
    );
}
