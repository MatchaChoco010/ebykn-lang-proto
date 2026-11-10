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

/// #95：書き下しは、属性を付けられるどの位置の属性も落とさない。位置ごとに別の印の属性を付け、
/// すべての印が書き下しに残ることを確かめる（位置を足したら、ここにも足す）。`..` と `@..` の前の
/// 属性は、rustc と同じく構文解析が拒否する（`AttrOnRest`）。
#[test]
fn attributes_survive_lowering_in_every_position() {
    let src = "#[allow(m01)] pub struct S<#[allow(m02)] 'a, #[allow(m03)] T, #[allow(m04)] const N: usize> { #[allow(m05)] pub x: u32, pub t: &'a T }
#[allow(m06)] pub enum E { #[allow(m07)] A { #[allow(m08)] f: u32 }, B(#[allow(m09)] u32) }
pub struct B { } owns { #[allow(m10)] items: Region<Ptr<u64>> }
#[allow(m11)] pub fn f<#[allow(m12)] U>(#[allow(m13)] s: S<u8, 1>, b: B) -> u32 {
    #[allow(m14)] let c = |#[allow(m15)] a: u32| a;
    let S { #[allow(m16)] x, .. } = s;
    let B { #[allow(m17)] @items } = b;
    let v = match x { #[allow(m18)] 0 => 1, _ => 2 };
    let w = S { #[allow(m19)] x: 1, t: &0u8 };
    #[allow(m20)] c(v)
}
pub struct R { a: u32 }
impl R { pub fn m(#[allow(m21)] &self) -> u32 { self.a } }";
    let got = lowered_ignoring_errors(src);
    for i in 1..=21 {
        let m = format!("m{i:02}");
        assert!(
            got.contains(&format!("allow ( {m} )")),
            "{m} が書き下しにない:\n{got}"
        );
    }
}

/// 検査の誤りを見ずに書き下す（検査が拒む入力で、書き下しが何をするかを確かめる）。
fn lowered_ignoring_errors(src: &str) -> String {
    let (p, d) = project::single(src);
    assert!(d.is_empty(), "{d:?}");
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c).unwrap();
    squash(&out.files[0].text)
}

#[test]
fn ensures_of_mut_parameters_are_written_as_is() {
    // #10 の決定（案 B）：`ensures` の `&mut` 引数は利用者が `final(..)` / `old(..)` を書き、
    // 書き下しはそのまま写す。
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self, x: &mut u64) ensures final(self).a == old(self).a, *final(x) == 1, {{ }} }}"
    );
    has(
        &src,
        "ensures final(self).a == old(self).a, *final(x) == 1,",
    );
    // `requires` の裸の `x` は入口の値（Verus と同じ）。
    let src = format!("{PRE}impl P {{ fn f(&mut self) requires self.a > 0, {{ }} }}");
    has(&src, "requires self.a > 0,");
    // `uses &mut p@` の権限：`final(p@)` と `final(*p)`。Ebykn が足す `pptr` の `ensures` は `final`。
    let src = format!(
        "{PRE}fn f(p: Ptr<Node>) uses &mut p@ ensures final(p@).is_init(), final(*p).val == 1, old(*p).val == 0, {{ }}"
    );
    has(&src, "Tracked(__eyk_perm_p): Tracked<&mut PointsTo<Node>>");
    has(
        &src,
        "requires __eyk_perm_p.pptr() == p, ensures final(__eyk_perm_p).is_init(), final(__eyk_perm_p).value().val == 1, old(__eyk_perm_p).value().val == 0, final(__eyk_perm_p).pptr() == p,",
    );
    // 領域：`final(self)@r[p]`、`old(self@r)[p]`、`final(r)[p]`。
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self, p: Ptr<Node>) ensures final(self)@r[p].val == 1, old(self@r)[p].val == 0, final(self)@q.len() == 0, {{ }} }}"
    );
    has(
        &src,
        "ensures final(self).__eyk_erased_r@.index(p).val == 1, old(self).__eyk_erased_r@.index(p).val == 0, final(self).__eyk_erased_q@.len() == 0,",
    );
    // `old(x@f)` / `final(x@f)` は `old(x).f@` / `final(x).f@`（Verus の `old` は `&mut` の引数そのもの
    // にしか付けられない）。
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self) ensures old(self@q).len() <= final(self@q).len(), {{ }} }}"
    );
    has(
        &src,
        "ensures old(self).__eyk_erased_q@.len() <= final(self).__eyk_erased_q@.len(),",
    );
    let src = format!(
        "{PRE}fn g(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> ensures final(r)[p].val == old(r)[p].val, {{ }}"
    );
    has(
        &src,
        "ensures final(r).index(p).val == old(r).index(p).val,",
    );
}

#[test]
fn lowering_does_not_add_final_to_bare_mut_parameters() {
    // 裸の `&mut` 引数は検査が E0004 にする。書き下しは `final(..)` を足さない（M2 で入れた #10 の
    // 旧既定 A の暗黙の変換を戻すと、ここで落ちる）。
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self, x: &mut u64, p: Ptr<Node>) uses &mut p@ ensures self.a == 1, *x == 1, p@.is_init(), self@r[p].val == 1, {{ }} }}"
    );
    let got = lowered_ignoring_errors(&src);
    let want = squash(
        "ensures self.a == 1, *x == 1, __eyk_perm_p.is_init(), self.__eyk_erased_r@.index(p).val == 1, final(__eyk_perm_p).pptr() == p,",
    );
    assert!(got.contains(&want), "{want:?} がない:\n{got}");
    let src = format!(
        "{PRE}fn g(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> ensures r.contains(p), {{ }}"
    );
    assert!(lowered_ignoring_errors(&src).contains(&squash("ensures r.contains(p),")));
}

#[test]
fn first_and_last_are_written_as_is() {
    // #11 の決定（案 B）：論理文脈の `.first()` / `.last()` も vstd の意味（要素を返す）のまま写す。
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
    has(&src, "requires old(self).__eyk_erased_r@.contains(p),");
    has(
        &src,
        "let tracked __eyk_perm_p = self.__eyk_erased_r.borrow_mut().remove(p);",
    );
    has(
        &src,
        "proof { self.__eyk_erased_r.borrow_mut().insert(__eyk_perm_p); }",
    );
    has(
        &src,
        "proof { self.__eyk_erased_q = Ghost(self.__eyk_erased_q@.push(1)); }",
    );
    // 構造体：owns は Tracked、spec は Ghost。
    has(
        &src,
        "__eyk_erased_r: Tracked<Region<Ptr<Node>>>, __eyk_erased_q: Ghost<Seq<u64>>,",
    );
    // `borrow mut` の末尾の `insert` も `proof { }`。
    let src = format!(
        "{PRE}impl P {{ fn f(&mut self, p: Ptr<Node>) requires old(self)@r.contains(p), {{ borrow mut self@r[p] {{ (*p).val = 1; }} }} }}"
    );
    has(
        &src,
        "let tracked __eyk_perm_p = self.__eyk_erased_r.borrow_mut().remove(p); let __eyk_res = { p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 1; }; #[allow(unreachable_code)] let __eyk_res = { proof { self.__eyk_erased_r.borrow_mut().insert(__eyk_perm_p); } __eyk_res }; __eyk_res",
    );
}

/// #21 の既定 D：`borrow mut` のブロックの外へ抜ける文の前で、権限を領域に戻す。
#[test]
fn jumps_out_of_borrow_mut_put_the_permission_back() {
    const INS: &str = "proof { self.__eyk_erased_r.borrow_mut().insert(__eyk_perm_p); }";
    let f = |body: &str| {
        format!(
            "{PRE}impl P {{ fn f(&mut self, p: Ptr<Node>, q: Ptr<Node>, n: u64) -> Option<u64> requires old(self)@r.contains(p), {{ {body} Some(0) }} }}"
        )
    };
    // break / continue / return：抜ける文の直前。
    has(
        &f("loop { borrow mut self@r[p] { if (*p).val == 0 { break; } } }"),
        &format!("{{ {INS} break }}"),
    );
    has(
        &f("loop { borrow mut self@r[p] { if (*p).val == 0 { continue; } } }"),
        &format!("{{ {INS} continue }}"),
    );
    has(
        &f("borrow mut self@r[p] { return None; }"),
        &format!("{{ let __eyk_exit = None; {INS} return __eyk_exit }}"),
    );
    // 値を持って抜けるなら、値を先に評価する（値の式が権限を読む）。
    has(
        &f("borrow mut self@r[p] { if (*p).val > 1 { return Some((*p).val); } }"),
        &format!(
            "{{ let __eyk_exit = Some((*p.read(Tracked(&__eyk_perm_p))).val); {INS} return __eyk_exit }}"
        ),
    );
    // `?` は明示の match に展開する（戻り値の型が Option）。
    has(
        &f("let o: Option<u64> = None; borrow mut self@r[p] { (*p).val = o?; }"),
        &format!("match o {{ Some(__eyk_ok) => __eyk_ok, None => {{ {INS} return None }} }}"),
    );
    // 入れ子なら内側から戻す。
    has(
        &f("loop { borrow mut self@r[p] { borrow mut self@r[q] { break; } } }"),
        "{ proof { self.__eyk_erased_r.borrow_mut().insert(__eyk_perm_q); self.__eyk_erased_r.borrow_mut().insert(__eyk_perm_p); } break }",
    );
    // ブロックの中で閉じる break と、共有の borrow は戻さない。
    lacks(
        &f("borrow mut self@r[p] { loop { break; } }"),
        &format!("{INS} break"),
    );
    lacks(
        &f("'a: { borrow mut self@r[p] { 'b: { break 'b; } } }"),
        &format!("{INS} break"),
    );
    has(
        &f("'a: { borrow mut self@r[p] { 'b: { break 'a; } } }"),
        &format!("{{ {INS} break 'a }}"),
    );
    lacks(&f("loop { borrow self@r[p] { break; } }"), "insert");
    // `borrow mut` の外の `?` と、クロージャの中の `return` はそのまま。
    has(&f("let o: Option<u64> = None; let v = o?;"), "let v = o?;");
    has(
        &f("borrow mut self@r[p] { let g = || { return 1u64; }; }"),
        "let g = | | { return 1u64; };",
    );
}

/// 本体の後の、権限を戻す文は、発散の解析に頼らず常に置く（届かないときの rustc の警告は
/// `#[allow(unreachable_code)]` で止める。PR #51 のレビューの B1）。
#[test]
fn borrow_mut_bodies_always_have_the_trailing_insert() {
    const TAIL: &str = "#[allow(unreachable_code)] let __eyk_res = { proof { self.__eyk_erased_r.borrow_mut().insert(__eyk_perm_p); } __eyk_res }; __eyk_res";
    for body in [
        "loop { borrow mut self@r[p] { break; } }",
        // 右辺だけが抜ける `&&` / `||`：通常の経路で戻す文が要る。
        "borrow mut self@r[p] { let _b = c && { return; }; }",
        "borrow mut self@r[p] { let _b = c || { return; }; }",
    ] {
        let src = format!(
            "{PRE}impl P {{ fn f(&mut self, p: Ptr<Node>, c: bool) requires old(self)@r.contains(p), {{ {body} }} }}"
        );
        has(&src, TAIL);
    }
}

/// 消去フィールドは `.eyk` の名前のフィールドとしては書き下さない（#99）。`.eyk` の `x.r` は、検査が
/// 見逃しても `Tracked` / `Ghost` のフィールドに届かず、Verus のビルドで止まる。
#[test]
fn erased_fields_are_lowered_under_reserved_names() {
    let src = format!(
        "{PRE}impl P {{ pub fn new() -> Self {{ P {{ a: 1, @r: Region::empty(), @q: Seq::empty() }} }}\n\
         pub fn f(x: P) -> u64 {{ let P {{ a, @r, @q }} = x; a }} }}"
    );
    has(
        &src,
        "a: u64, __eyk_erased_r: Tracked<Region<Ptr<Node>>>, __eyk_erased_q: Ghost<Seq<u64>>,",
    );
    has(
        &src,
        "P { a: 1, __eyk_erased_r: Tracked(Region::empty()), __eyk_erased_q: Ghost(Seq::empty()) }",
    );
    has(
        &src,
        "let P { a, __eyk_erased_r: Tracked(r), __eyk_erased_q: Ghost(q) } = x;",
    );
    // 書き下しのどこにも、`.eyk` の名前のままの消去フィールド（`r:` / `q:` / `.r` / `.q`）がない。
    let got = lowered(&src);
    let toks: Vec<&str> = got.split(' ').collect();
    for w in toks.windows(2) {
        let erased = matches!(w[0], "r" | "q") && w[1] == ":";
        let dotted = w[0] == "." && matches!(w[1], "r" | "q");
        assert!(
            !erased && !dotted,
            "`.eyk` の名前の消去フィールド {w:?}:\n{got}"
        );
    }
}

#[test]
fn erased_fields_of_other_values() {
    // #14 の既定：`x@field` を `self` 以外にも書き下す。
    let src = format!("{PRE}pub spec fn f(x: P) -> Seq<u64> = x@q;");
    has(&src, "{ x.__eyk_erased_q@ }");
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
        "P { a: 1, __eyk_erased_r: Tracked(Region::empty()), __eyk_erased_q: Ghost(Seq::empty()) }",
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
fn statics_are_exec_statics() {
    // #163：Verus は `verus!` の中の `exec` のない `static` を拒否する（`tests/verus-probes/static_without_exec.rs`、
    // `static_pub_without_exec.rs`）。可視性と型と初期化の式はそのまま写す。
    let src = "pub static A: u64 = 1;\nstatic B: &'static u64 = &A;\npub(crate) static S: &'static str = \"hi\";\nmod m { pub static C: u64 = 2; }\n";
    for want in [
        "pub exec static A: u64 = 1;",
        "exec static B: &'static u64 = &A;",
        "pub(crate) exec static S: &'static str = \"hi\";",
        "pub exec static C: u64 = 2;",
    ] {
        has(src, want);
    }
    // `exec` のない `static` は残らない（`'static` の寿命は除く）。
    let got = lowered(src);
    assert_eq!(got.matches("exec static").count(), 4, "{got}");
    assert_eq!(
        got.matches(" static ").count(),
        4 + got.matches("' static ").count(),
        "{got}"
    );
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
    // 上のディレクトリの Cargo のワークスペースに入らないよう、空の `[workspace]` を持つ（#78）。
    assert!(t.starts_with("[workspace]\n\n[package]\n"), "{t}");
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
        "(forall|k| self.__eyk_erased_m@.dom().contains(k) ==> k > 0)",
    );
    has(
        "pub spec fn f(s: Seq<int>) -> bool = forall k in s, k > 0;",
        "(forall|k| s.contains(k) ==> k > 0)",
    );
}

#[test]
fn old_of_a_permission_read() {
    let src = format!(
        "{PRE}fn f(p: Ptr<Node>) uses &mut p@ ensures final(*p).val == old(*p).val + 1, {{ }}"
    );
    has(
        &src,
        "ensures final(__eyk_perm_p).value().val == old(__eyk_perm_p).value().val + 1,",
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
    // raw の綴りも同じ名前（#122）。
    let src = "#[r#test]\nfn t() {}";
    has(src, "#[verifier::external] #[r#test] fn t() {}");
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

/// 本体を検証する実行関数の一覧（`Output::exec`）。`verify` の停止の注記と、`--json` の
/// `"termination": "not_proven"` がこの一覧で決まる（#81）。関数・impl のメソッド・trait の既定のメソッド・
/// 外の型への impl のメソッド・`Drop::drop` を含み、`spec fn`・純粋関数・`lemma fn`・`unchecked`・`#[test]`・
/// 本体のない trait のメソッドを含まない。一覧に入る項目には、書き下しで属性が付く。
#[test]
fn exec_functions_are_listed() {
    let src = "\
pub fn f(x: u64) -> u64 { x }
pub struct S { pub a: u64 }
impl S { pub fn m(&self) -> u64 { self.a } }
pub trait Tr {
    fn e(&self) -> u64;
    fn d(&self) -> u64 { self.e() }
}
impl Tr for u32 { fn e(&self) -> u64 { 1 } }
pub struct G { pub a: u64 }
impl Drop for G { fn drop(&mut self) { self.a = 0; } }
pub spec fn sp(n: nat) -> nat decreases n, = if n == 0 { 0 } else { sp((n - 1) as nat) };
pub fn pf(x: u32) -> u32 requires x < 100, = x * 2;
lemma fn l(x: u64) ensures x + 0 == x;
unchecked fn u() {}
#[test]
fn t() {}
";
    let (p, _) = project::single(src);
    let c = ebykn_check::check(&p);
    let out = lower(&p, &c).unwrap();
    let mut exec = out.exec.clone();
    exec.sort();
    assert_eq!(
        exec,
        [
            "single::<u32 as Tr>::e",
            "single::G::drop",
            "single::S::m",
            "single::Tr::d",
            "single::f",
        ]
    );
    // 一覧に入った数だけ、書き下しに属性が付く（一覧と属性が食い違わない）。
    let text = &out.files[0].text;
    for attr in [
        "#[verifier::exec_allows_no_decreases_clause]",
        "#[verifier::allow(decreases_when_exec_allows_no_decreases_clause)]",
    ] {
        assert_eq!(text.matches(attr).count(), exec.len(), "{attr}:\n{text}");
    }
}

#[test]
fn owned_regions_are_rebound_as_mut() {
    // #90：値で受けた領域は本体の先頭で `mut` に束縛し直す（Verus は引数のパターン
    // `Tracked(mut r)` を受け付けない）。参照で受けた領域は束縛し直さない。
    let src = format!(
        "{PRE}fn f(v: u64) uses r: Region<Ptr<Node>>, s: &mut Region<Ptr<Node>> {{ let p = Ptr::new(Node {{ val: v }}); put r[p]; }}"
    );
    has(
        &src,
        "Tracked(r): Tracked<Region<Ptr<Node>>>, Tracked(s): Tracked<&mut Region<Ptr<Node>>>",
    );
    has(
        &src,
        "let tracked mut r = r; let (p, Tracked(__eyk_perm_p)) = Ptr::new(Node { val: v });",
    );
    lacks(&src, "let tracked mut s");
    // #102：移動で受けた権限も同じ。借りて受けた権限は束縛し直さない。
    let src = format!(
        "{PRE}fn g(p: Ptr<Node>, q: Ptr<Node>) uses p@, &mut q@ requires p@.is_init() {{ (*p).val = 1; (*q).val = 1; leak p@; }}"
    );
    has(
        &src,
        "let tracked mut __eyk_perm_p = __eyk_perm_p; p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 1;",
    );
    lacks(&src, "let tracked mut __eyk_perm_q");
}

#[test]
fn region_arguments_are_appended_as_tracked() {
    // #19 の既定 P1：`uses` の後の領域を、呼ばれる側の `uses` の宣言順で引数の末尾に足す。
    let helpers = "fn mv(n: u64) uses r: Region<Ptr<Node>> { }\nfn rd(n: u64) uses r: &Region<Ptr<Node>> { }\nfn wr(n: u64) uses r: &mut Region<Ptr<Node>> { }\nfn mixed(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>>, &mut p@ { }\n";
    let local = "let mut r: Region<Ptr<Node>> = Region::empty();";
    let src = format!(
        "{PRE}{helpers}fn f() {{ {local} rd(1, uses &r); wr(2, uses &mut r); mv(3, uses r); }}"
    );
    has(
        &src,
        "rd(1, Tracked(&r)); wr(2, Tracked(&mut r)); mv(3, Tracked(r));",
    );
    // `uses` で受けた参照は借り直す。
    let src = format!(
        "{PRE}{helpers}fn g() uses r: &mut Region<Ptr<Node>> {{ rd(1, uses &r); wr(2, uses &mut r); }}"
    );
    has(&src, "rd(1, Tracked(&*r)); wr(2, Tracked(&mut *r));");
    // `owns` フィールド。
    let src = format!(
        "{PRE}{helpers}impl P {{ fn h(&mut self) {{ rd(1, uses &self@r); wr(2, uses &mut self@r); }} }}"
    );
    has(
        &src,
        "rd(1, Tracked(self.__eyk_erased_r.borrow())); wr(2, Tracked(self.__eyk_erased_r.borrow_mut()));",
    );
    // 権限と領域が混ざった宣言は、宣言の順に並べる。
    let src = format!(
        "{PRE}{helpers}fn k() {{ {local} let p = Ptr::new(Node {{ val: 1 }}); mixed(p, uses &mut r); let _ = p.free(); }}"
    );
    has(
        &src,
        "mixed(p, Tracked(&mut r), Tracked(&mut __eyk_perm_p));",
    );
}

#[test]
fn region_arguments_are_never_dropped() {
    // 呼ばれる側を引けない呼び出しや、検査が拒む呼び出しでも、書いた領域は末尾に残す（受け取らない
    // 関数なら Verus が型の誤りとして拒む）。
    let src = format!(
        "{PRE}fn plain(n: u64) {{ }}\nfn f(g: fn(u64)) {{ let mut r: Region<Ptr<Node>> = Region::empty(); g(1, uses &mut r); plain(2, uses &r); }}"
    );
    let got = lowered_ignoring_errors(&src);
    for want in ["g(1, Tracked(&mut r))", "plain(2, Tracked(&r))"] {
        assert!(got.contains(&squash(want)), "{want:?} がない:\n{got}");
    }
}

#[test]
fn region_arguments_survive_missing_facts() {
    // 検査の事実（`facts.calls`）に並びがない呼び出しでも、書き下しは書いた領域を末尾に足す
    // （`ebykn_lower` の予備の経路）。
    let src = format!(
        "{PRE}fn rd(n: u64) uses r: &Region<Ptr<Node>> {{ }}\nfn f() {{ let mut r: Region<Ptr<Node>> = Region::empty(); rd(1, uses &r); rd(2, uses &mut r); }}"
    );
    let (p, d) = project::single(&src);
    assert!(d.is_empty(), "{d:?}");
    let mut c = ebykn_check::check(&p);
    assert!(!c.facts.calls.is_empty());
    c.facts.calls.clear();
    let got = squash(&lower(&p, &c).unwrap().files[0].text);
    for want in ["rd(1, Tracked(&r))", "rd(2, Tracked(&mut r))"] {
        assert!(got.contains(&squash(want)), "{want:?} がない:\n{got}");
    }
}

/// 利用者の `impl Drop`（#61、現行仕様 §7.2）：std の `Drop` の `fn drop` に `opens_invariants none` と
/// `no_unwind` を足す。Verus はどちらもない `Drop` の実装を拒否し、利用者は `.eyk` にどちらも書けない。
#[test]
fn std_drop_gets_opens_invariants_none_and_no_unwind() {
    let marked = "fn drop(&mut self) opens_invariants none no_unwind {}";
    for (src, why) in [
        (
            "pub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "裸の Drop",
        ),
        (
            "pub struct S { a: u64 }\nimpl std::ops::Drop for S { fn drop(&mut self) {} }",
            "std::ops::Drop",
        ),
        (
            "pub struct S { a: u64 }\nimpl ::core::ops::Drop for S { fn drop(&mut self) {} }",
            "::core::ops::Drop",
        ),
        (
            "use std::ops::Drop as D;\npub struct S { a: u64 }\nimpl D for S { fn drop(&mut self) {} }",
            "use の別名",
        ),
        (
            "use std::ops;\npub struct S { a: u64 }\nimpl ops::Drop for S { fn drop(&mut self) {} }",
            "use したモジュールからのパス",
        ),
        (
            "pub fn f() {}\nmod m { pub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} } }",
            "mod の中",
        ),
        (
            "mod r { pub use std::ops::Drop; }\npub struct S { a: u64 }\nimpl r::Drop for S { fn drop(&mut self) {} }",
            "pub use で再公開した std の Drop",
        ),
        (
            "mod r { pub use std::ops::Drop; }\nmod m { pub struct S { a: u64 }\nimpl super::r::Drop for S { fn drop(&mut self) {} } }",
            "super からの再公開",
        ),
        (
            "mod r { pub use std::ops::Drop; }\nuse crate::r::Drop;\npub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "crate からの再公開の use",
        ),
        (
            "pub struct S { a: u64 }\nimpl std::prelude::rust_2024::Drop for S { fn drop(&mut self) {} }",
            "std の prelude のパス",
        ),
        (
            "use std::ops::*;\npub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "std の glob",
        ),
    ] {
        let got = lowered(src);
        assert!(
            got.contains(&squash(marked)),
            "{why}：{marked:?} がない:\n{got}"
        );
    }
    // 本体を検証しない `unchecked` でも、Verus は `Drop` の実装に両方を要求する。
    has(
        "pub struct S { a: u64 }\nimpl Drop for S { unchecked fn drop(&mut self) {} }",
        "#[verifier::external_body] fn drop(&mut self) opens_invariants none no_unwind {}",
    );
}

/// std の `Drop` でないトレイトの `drop` には何も足さない（クレートで定義した同じ名前のトレイト、
/// `Drop` の impl の `drop` 以外のメソッド、固有の impl の `drop`）。
#[test]
fn other_drops_are_not_marked() {
    for (src, why) in [
        (
            "pub trait Drop { fn drop(&mut self); }\npub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "クレートの Drop",
        ),
        (
            "mod t { pub trait Drop { fn drop(&mut self); } }\nuse t::Drop;\npub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "use したクレートの Drop",
        ),
        (
            "mod t { pub trait Drop { fn drop(&mut self); } }\nuse t::*;\npub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "glob の use で見えるクレートの Drop",
        ),
        (
            "mod a { pub trait Drop { fn drop(&mut self); } }\nmod b { pub use super::a::*; }\nuse b::*;\npub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "glob の use の鎖で見えるクレートの Drop",
        ),
        (
            "mod a { pub trait Drop { fn drop(&mut self); } }\nmod b { pub use super::a::Drop; }\npub struct S { a: u64 }\nimpl b::Drop for S { fn drop(&mut self) {} }",
            "再公開したクレートの Drop",
        ),
        (
            "use other::Drop;\npub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) {} }",
            "外のクレートの Drop",
        ),
        (
            "pub struct S { a: u64 }\nimpl other::Drop for S { fn drop(&mut self) {} }",
            "外のクレートのパス",
        ),
        (
            "pub struct S { a: u64 }\nimpl ::Drop for S { fn drop(&mut self) {} }",
            "先頭の :: の Drop（外のクレート）",
        ),
        (
            "pub struct S { a: u64 }\nimpl S { pub fn drop(&mut self) {} }",
            "固有の impl",
        ),
    ] {
        let got = lowered(src);
        assert!(
            !got.contains("opens_invariants"),
            "{why}：opens_invariants がある:\n{got}"
        );
    }
}

#[test]
fn raw_identifiers_keep_their_prefix() {
    // 版の予約語は `r#` を付けて名前にする（#104）。書き下しは `r#` を残す（edition 2024 の構文）。
    has(
        "pub fn r#gen(x: u64) -> u64 { x }\npub fn f() -> u64 { crate::r#gen(1) }",
        "pub fn r#gen(x: u64) -> u64",
    );
    has(
        "pub fn r#gen(x: u64) -> u64 { x }\npub fn f() -> u64 { crate::r#gen(1) }",
        "crate::r#gen(1)",
    );
}
