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
fn first_and_last_in_logic_are_opt() {
    // #11 の既定：論理文脈の引数なしの `.first()` / `.last()`。実行文脈はそのまま。
    let src = "pub spec fn f(s: Seq<u64>) -> Option<u64> = s.first();\npub fn g(v: Vec<u64>) -> Option<&u64> { v.first() }";
    has(src, "{ s.first_opt() }");
    has(src, "{ v.first() }");
    has(
        "pub spec fn f(s: Seq<u64>) -> Option<u64> = s.last();",
        "s.last_opt()",
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
        "(forall|i| 0 <= i < s.len() ==> s[i] > 0)",
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
