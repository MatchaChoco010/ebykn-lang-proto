//! 書き下しの規則（仕様 §7 と、判断待ちの Issue・M0 の注意の既定）。生成した Verus の文字列で確かめる。
//! 生成したものが Verus を通ることは、golden（`tests/golden/stack`、`tests/golden/lowering`）を
//! `cargo xtask golden-verify` が確かめる。

use ebykn_check::project::{self, TomlValue};
use ebykn_lower::{Deps, cargo_toml, lower};

const PRE: &str = "pub struct Node { pub val: u64 }\npub struct P { a: u64 } owns { r: Region<Ptr<Node>> } spec { q: Seq<u64> }\n";

/// 書き下した `src/lib.rs` を、空白を詰めて返す（比べやすくするため）。
fn lowered(src: &str) -> String {
    let (p, d) = project::single(src);
    assert!(d.is_empty(), "{d:?}");
    let c = ebykn_check::check(&p);
    assert!(
        c.diags.iter().all(|d| d.code().is_warning()),
        "{:?}",
        c.diags
    );
    let out = lower(&p, &c).unwrap();
    squash(&out.files[0].text)
}

/// トークンを一つずつ空白で区切った形（整形の空白の違いを見ない）。
fn squash(s: &str) -> String {
    use proc_macro2::{Delimiter, TokenStream, TokenTree};
    fn flat(ts: TokenStream, out: &mut Vec<String>) {
        for t in ts {
            match t {
                TokenTree::Group(g) => {
                    let (o, c) = match g.delimiter() {
                        Delimiter::Parenthesis => ("(", ")"),
                        Delimiter::Bracket => ("[", "]"),
                        Delimiter::Brace => ("{", "}"),
                        Delimiter::None => ("", ""),
                    };
                    out.push(o.into());
                    flat(g.stream(), out);
                    out.push(c.into());
                }
                t => out.push(t.to_string()),
            }
        }
    }
    let ts: TokenStream = s.parse().unwrap_or_else(|e| panic!("{e}: {s}"));
    let mut out = Vec::new();
    flat(ts, &mut out);
    out.join(" ")
}

#[track_caller]
fn has(src: &str, want: &str) {
    let got = lowered(src);
    assert!(got.contains(&squash(want)), "{want:?} がない:\n{got}");
}

#[track_caller]
fn lacks(src: &str, bad: &str) {
    let got = lowered(src);
    assert!(!got.contains(&squash(bad)), "{bad:?} がある:\n{got}");
}

#[test]
fn header_imports_names_explicitly() {
    // #16 の既定：glob の `use vstd::prelude::*;` を書かない。
    has(
        "pub fn f() {}",
        "Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus,",
    );
    lacks("pub fn f() {}", "use vstd::prelude::*;");
    has("pub fn f() {}", "use ebykn_prelude::*; verus!");
}

#[test]
fn bare_mut_parameters_in_ensures_are_final() {
    // #10 の既定：`ensures` の `old` / `final` の下にない `&mut` 引数を `final(..)` で包む。
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self, x: &mut u64) ensures self.a == old(self).a, *x == 1, {{ }} }}"
    );
    has(
        &src,
        "ensures final(self).a == old(self).a, *final(x) == 1,",
    );
    // `requires` では包まない。
    let src = format!("{PRE}impl P {{ fn f(&mut self) requires self.a > 0, {{ }} }}");
    has(&src, "requires self.a > 0,");
    // `uses &mut p@` の権限も。
    let src = format!("{PRE}fn f(p: Ptr<Node>) uses &mut p@ ensures p@.is_init(), {{ }}");
    has(&src, "Tracked(__eyk_perm_p): Tracked<&mut PointsTo<Node>>");
    has(
        &src,
        "requires __eyk_perm_p.pptr() == p, ensures final(__eyk_perm_p).is_init(), final(__eyk_perm_p).pptr() == p,",
    );
}

#[test]
fn first_and_last_are_written_as_is() {
    // #11 の既定 B：論理文脈の `.first()` / `.last()` も vstd の意味（要素を返す）のまま写す。
    // M2 で入れた旧既定 A（`first_opt` / `last_opt` への置き換え）を戻すと、ここで落ちる。
    let src = "pub spec fn f(s: Seq<u64>) -> u64 = s.first();\npub fn g(v: Vec<u64>) -> Option<&u64> { v.first() }";
    has(src, "{ s.first() }");
    has(src, "{ v.first() }");
    lacks(src, "first_opt");
    let src = "pub spec fn f(s: Seq<u64>) -> u64 = s.last();";
    has(src, "{ s.last() }");
    lacks(src, "last_opt");
    // `Option` が要るときは `get(i)`。
    has(
        "pub spec fn f(s: Seq<u64>) -> Option<u64> = s.get(0);",
        "{ s.get(0) }",
    );
}

#[test]
fn regions_and_spec_fields() {
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self, p: Ptr<Node>) requires old(self)@r.contains(p), {{ take self@r[p]; put self@r[p]; self@q = self@q.push(1); }} }}"
    );
    // M0 の注意：`take` は `let tracked`、`put` と spec フィールドの代入は `proof { }`、論理の `<r>` は `self.r@`。
    has(&src, "requires old(self).r@.contains(p),");
    has(
        &src,
        "let tracked __eyk_perm_p = self.r.borrow_mut().remove(p);",
    );
    has(&src, "proof { self.r.borrow_mut().insert(__eyk_perm_p); }");
    has(&src, "proof { self.q = Ghost(self.q@.push(1)); }");
    // 構造体：owns は Tracked、spec は Ghost。
    has(&src, "r: Tracked<Region<Ptr<Node>>>, q: Ghost<Seq<u64>>,");
    // `borrow mut` の末尾の `insert` も `proof { }`。
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self, p: Ptr<Node>) requires old(self)@r.contains(p), {{ borrow mut self@r[p] {{ (*p).val = 1; }} }} }}"
    );
    has(
        &src,
        "let tracked __eyk_perm_p = self.r.borrow_mut().remove(p); let __eyk_res = { p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 1; }; proof { self.r.borrow_mut().insert(__eyk_perm_p); } __eyk_res",
    );
}

#[test]
fn erased_fields_of_other_values() {
    // #14 の既定：`x@field` を `self` 以外にも書き下す。
    let src = format!("{PRE}pub spec fn f(x: P) -> Seq<u64> = x@q;");
    has(&src, "{ x.q@ }");
}

#[test]
fn pure_functions_cast_integer_results() {
    // M0 の注意：純粋関数の spec 側は `(e) as T`。
    let src = "pub fn d(x: u32) -> u32 requires x < 10, = x * 2;";
    has(
        src,
        "#[verifier::when_used_as_spec(__eyk_spec_d)] pub fn d(x: u32) -> (__eyk_r: u32) requires x < 10, ensures __eyk_r == __eyk_spec_d(x),",
    );
    has(
        src,
        "pub open spec fn __eyk_spec_d(x: u32) -> u32 { (x * 2) as u32 }",
    );
}

#[test]
fn self_results_are_named() {
    let src = format!(
        "{PRE}impl Invariant for P {{ spec fn invariant(&self) -> bool = self.a > 0; }}\nimpl P {{ pub fn new() -> Self {{ P {{ a: 1, @r: Region::empty(), @q: Seq::empty() }} }} }}"
    );
    // M0 の注意：`-> Self` の名前は `__eyk_r`。
    has(
        &src,
        "pub fn new() -> (__eyk_r: Self) ensures __eyk_r.invariant(),",
    );
    has(
        &src,
        "P { a: 1, r: Tracked(Region::empty()), q: Ghost(Seq::empty()) }",
    );
    // impl Invariant は T の固有の impl の `pub spec fn invariant`（closed がなければ open）。
    has(
        &src,
        "impl P { pub open spec fn invariant(&self) -> bool { self.a > 0 } }",
    );
}

#[test]
fn spec_fn_visibility() {
    // `pub` でない `open spec fn` は Verus のエラー。
    has(
        "spec fn f(x: u64) -> bool = x > 0;",
        "spec fn f(x: u64) -> bool { x > 0 }",
    );
    lacks("spec fn f(x: u64) -> bool = x > 0;", "open spec fn f");
    has(
        "pub closed spec fn f(x: u64) -> bool = x > 0;",
        "pub closed spec fn f",
    );
    has(
        "pub spec fn f(x: u64) -> bool = x > 0;",
        "pub open spec fn f",
    );
}

#[test]
fn lemmas_without_bodies_are_admitted_and_listed() {
    let src = "lemma fn l(x: u64) ensures x + 0 == x;\nunchecked fn u() {}\nunchecked lemma fn ul() ensures true;";
    let (p, _) = project::single(src);
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c).unwrap();
    let text = squash(&out.files[0].text);
    for want in [
        "proof fn l(x: u64) ensures x + 0 == x, { vstd::prelude::admit() }",
        "#[verifier::external_body] fn u() { }",
        "#[verifier::external_body] proof fn ul() ensures true, { }",
    ] {
        assert!(text.contains(&squash(want)), "{want:?} がない:\n{text}");
    }
    // §7.2：本体のない補題と unchecked は、verify が必ず一覧に出す。
    assert_eq!(out.admitted, ["single::l"]);
    assert_eq!(out.unchecked, ["single::u", "single::ul"]);
}

#[test]
fn quantifiers_and_and_or() {
    has(
        "pub spec fn f(s: Seq<int>) -> bool = forall i in 0..s.len(), s[i] > 0;",
        "(forall|i: int| 0 <= i < s.len() ==> s[i] > 0)",
    );
    has(
        "pub spec fn f(s: Seq<int>) -> bool = exists x in s, x > 0 || x < -5;",
        "(exists|x| s.contains(x) && (x > 0 || x < -5))",
    );
    has(
        "pub spec fn f(a: bool, b: bool) -> bool = and { a, b };",
        "((a) && (b))",
    );
}

#[test]
fn cargo_toml_has_panic_abort_and_rewrites_paths() {
    let (mut p, _) = project::single("pub fn f() {}");
    p.manifest.dependencies = vec![
        ("rayon".into(), TomlValue::Str("1.10".into())),
        (
            "mylib".into(),
            TomlValue::Table(vec![("path".into(), TomlValue::Str("../mylib".into()))]),
        ),
    ];
    let t = cargo_toml(
        &p,
        &Deps {
            vstd: "=0.0.0-x",
            prelude_path: "../p",
        },
    );
    for want in [
        "vstd = \"=0.0.0-x\"",
        "ebykn_prelude = { path = \"../p\" }",
        "rayon = \"1.10\"",
        "mylib = { path = \"../../../mylib\" }",
        "[profile.dev]\npanic = \"abort\"",
        "[profile.release]\npanic = \"abort\"",
    ] {
        assert!(t.contains(want), "{want:?} がない:\n{t}");
    }
}

// ---- PR #26 のレビュー ----

const INV: &str = "pub struct S { a: u64 }\nimpl Invariant for S { spec fn invariant(&self) -> bool = self.a > 0; }\n";

#[test]
fn option_positions_are_implications() {
    // §6.4：`Option<T>` は `x is Some ==> P[x.unwrap()]`（`&&` ではない）。
    let src = format!("{INV}pub fn f(x: Option<S>) {{}}");
    has(&src, "requires (x is Some ==> x.unwrap().invariant()),");
    let src = format!(
        "{INV}pub struct T {{ o: Option<S> }}\nimpl Invariant for T {{ spec fn invariant(&self) -> bool = true; }}"
    );
    // 入れ子の `Option<U>` のフィールド。
    has(
        &src,
        "pub open spec fn invariant(&self) -> bool { (true) && ((self.o is Some ==> self.o.unwrap().invariant())) }",
    );
}

#[test]
fn extra_arguments_follow_the_callee_order() {
    let src = format!(
        "{PRE}fn g(p: Ptr<Node>, q: Ptr<Node>) uses &mut p@, &q@ {{ }}\nfn f(p: Ptr<Node>, q: Ptr<Node>) uses &mut p@, &q@ {{ g(p, q); }}"
    );
    has(
        &src,
        "g(p, q, Tracked(&mut *__eyk_perm_p), Tracked(&*__eyk_perm_q));",
    );
}

#[test]
fn map_membership_uses_dom() {
    // §7.4：`Map` は `r.dom().contains(x)`。型を書いた引数と消去フィールド。
    has(
        "pub spec fn f(m: Map<int, int>) -> bool = forall k in m, m[k] > 0;",
        "(forall|k| m.dom().contains(k) ==> m[k] > 0)",
    );
    has(
        "pub struct W { a: u64 } spec { m: Map<int, int> }\nimpl W { pub closed spec fn f(&self) -> bool = forall k in self@m, k > 0; }",
        "(forall|k| self.m@.dom().contains(k) ==> k > 0)",
    );
    has(
        "pub spec fn f(s: Seq<int>) -> bool = forall k in s, k > 0;",
        "(forall|k| s.contains(k) ==> k > 0)",
    );
}

#[test]
fn old_of_a_permission_read() {
    let src =
        format!("{PRE}fn f(p: Ptr<Node>) uses &mut p@ ensures (*p).val == old(*p).val + 1, {{ }}");
    has(
        &src,
        "ensures (final(__eyk_perm_p).value()).val == old(__eyk_perm_p).value().val + 1,",
    );
}

#[test]
fn tests_are_external_and_listed() {
    let src = "#[test]\nfn t() {}";
    has(src, "#[verifier::external] #[test] fn t() {}");
    let (p, _) = project::single(src);
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c).unwrap();
    assert_eq!(out.external, ["single::t"]);
}

#[test]
fn gives_returns_carry_the_permission() {
    let src = format!(
        "{PRE}fn mk(c: bool) -> (q: Ptr<Node>) gives q@ ensures q@.is_init(), {{ let q = Ptr::new(Node {{ val: 1 }}); if c {{ return q; }} q }}"
    );
    has(&src, "return (q, Tracked(__eyk_perm_q))");
    // 本体の末尾も（`let` のパターン、`return`、末尾の 3 か所）。
    let got = lowered(&src);
    assert_eq!(
        got.matches(&squash("(q, Tracked(__eyk_perm_q))")).count(),
        3,
        "{got}"
    );
    has(
        &src,
        "-> (__eyk_ret: (Ptr<Node>, Tracked<PointsTo<Node>>)) ensures __eyk_ret.1@.is_init(), __eyk_ret.1@.pptr() == __eyk_ret.0,",
    );
    // 束縛子は gives の付け替えを隠す（シャドウ）。
    let src = format!(
        "{PRE}fn mk(o: Ptr<Node>) -> (q: Ptr<Node>) gives q@ ensures forall q: Ptr<Node>, q == o, {{ let q = Ptr::new(Node {{ val: 1 }}); q }}"
    );
    has(&src, "(forall|q: Ptr<Node>| q == o)");
}

#[test]
fn typed_lets_keep_their_annotation() {
    // B4：`let p: Ptr<u8> = Ptr::new(..)` の型注釈を残す。
    let src = "pub fn f() -> u8 { let p: Ptr<u8> = Ptr::new(255); let v = p.free(); v }";
    has(
        src,
        "let (p, Tracked(__eyk_perm_p)): (Ptr<u8>, Tracked<_>) = Ptr::new(255);",
    );
    let src = format!(
        "{PRE}fn mk() -> (q: Ptr<Node>) gives q@ {{ let q = Ptr::new(Node {{ val: 1 }}); q }}\nfn f() {{ let q: Ptr<Node> = mk(); leak q@; }}"
    );
    has(
        &src,
        "let (q, Tracked(__eyk_perm_q)): (Ptr<Node>, Tracked<_>) = mk();",
    );
    let src = "pub fn f() { let c: Cell<u8> = Cell::new(1); leak c@; }";
    has(
        src,
        "let (c, Tracked(__eyk_perm_c)): (Cell<u8>, Tracked<_>) = Cell::new(1);",
    );
}

#[test]
fn named_mut_regions_are_final_in_ensures() {
    // S3：`uses r: &mut Region<..>` も #10 の既定で `final(r)`。
    let src = format!(
        "{PRE}fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> ensures r.contains(p), {{ }}"
    );
    has(&src, "ensures final(r).contains(p),");
}

#[test]
fn range_binders_are_typed() {
    // S4：範囲の束縛子に型を付ける（端の接尾辞、なければ int）。
    has(
        "pub spec fn f() -> bool = forall i in 0..3u64, i < 2;",
        "(forall|i: u64| 0 <= i < 3u64 ==> i < 2)",
    );
    has(
        "pub spec fn f(n: int) -> bool = exists i in 0..=n, i > 0;",
        "(exists|i: int| 0 <= i <= n && i > 0)",
    );
}

#[test]
fn trait_default_methods_are_items() {
    // S1：trait の既定のメソッドも、検証の結果を出す項目。
    let (p, _) = project::single("pub trait Tr { fn f(&self) -> u64 { 1 } fn g(&self); }");
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c).unwrap();
    let names: Vec<&str> = out.files[0]
        .items
        .iter()
        .map(|(_, _, n)| n.as_str())
        .collect();
    assert_eq!(names, ["single::Tr::f"]);
}

/// 外の型への impl の `unchecked fn` と `#[test]` を書き下しても panic しない（`report` の二重の借用。
/// PR #26 の 5 回目の確認）。一覧には `<u32 as Tr>::e` の形で出る。
#[test]
fn foreign_impl_unchecked_methods_are_listed() {
    let src = "pub trait Tr { fn e(&self) -> u32; }\nimpl Tr for u32 { unchecked fn e(&self) -> u32 { 1 } }";
    let (p, _) = project::single(src);
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c).unwrap();
    assert_eq!(out.unchecked, ["single::<u32 as Tr>::e"]);
}

#[test]
fn foreign_impl_test_methods_are_listed() {
    let src = "pub trait Tr { fn e(&self) -> u32; }\nimpl Tr for u64 { #[test] fn e(&self) -> u32 { 1 } }";
    let (p, _) = project::single(src);
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c).unwrap();
    assert_eq!(out.external, ["single::<u64 as Tr>::e"]);
}
