//! PR #20 のレビューで見つかった、黙って通っていた入力（B1〜B3、S1〜S5 と細かい指摘）。
//! 各テストは指摘の入力そのものを使う。

mod support;
use ebykn_check::sig::{Subject, Wrap};
use support::*;

const S: &str = "pub struct S { a: u64 }\n";
const INV: &str = "{ closed spec fn invariant(&self) -> bool = self.a > 0; }\n";

/// 関数名の順の、足した `requires` の対象。
fn requires_of(src: &str) -> Vec<String> {
    let (c, lines) = run(src);
    assert!(
        lines.iter().all(|l| l.contains("W0601")),
        "{}",
        lines.join("\n")
    );
    c.facts
        .expansions
        .values()
        .flat_map(|e| e.requires.iter())
        .map(|r| match (&r.subject, r.wrap) {
            (Subject::Var(x), Wrap::None) => format!("{}:{x}", r.ty.join("::")),
            other => format!("{other:?}"),
        })
        .collect()
}

// ---- B1：トレイトと型の別名を索引で解決する ----

/// 再レビュー：`impl Invariant for T` の 1 つの書き方だけを受理し、ほかの綴りは E0001
/// （綴りの総当たりは `tests/reserved.rs`）。
#[test]
fn b1_only_the_bare_spelling_is_the_ebykn_trait() {
    let src = format!("{S}impl ebykn_prelude::Invariant for S {INV}pub fn f(x: S) {{}}\n");
    rejects(&src, "E0001", "written only as `impl Invariant for T`");
    let src = format!(
        "{S}use ebykn_prelude::Invariant as Inv;\nimpl Inv for S {INV}pub fn f(x: S) {{}}\n"
    );
    assert_eq!(codes(&src), ["E0001", "E0001"]);
}

#[test]
fn b1_type_aliases_are_resolved() {
    let src = format!("{S}impl Invariant for S {INV}type A = S;\npub fn f(x: A) {{}}\n");
    assert_eq!(requires_of(&src), ["S:x"]);
    let src = format!("{S}type A = S;\nimpl Invariant for A {INV}pub fn f(x: S) {{}}\n");
    assert_eq!(requires_of(&src), ["S:x"]);
    // 展開しない位置の別名は W0601。
    let src = format!("{S}impl Invariant for S {INV}type A = S;\npub fn f(v: Vec<A>) {{}}\n");
    assert_eq!(codes(&src), ["W0601"]);
}

#[test]
fn b1_invariant_impls_that_cannot_be_tied_to_a_crate_type_are_rejected() {
    // 関数の本体の中の impl は索引に入らない。
    rejects(
        &format!("{S}pub fn f() {{ impl Invariant for S {INV} }}\n"),
        "E0003",
        "inside a function body",
    );
    // クレートの外の型（性質ファイル、M5）。
    rejects(
        "impl Invariant for u64 { spec fn invariant(&self) -> bool = true; }\n",
        "E0003",
        "outside this crate",
    );
    // 利用者が定義した同じ名前のトレイトは Rust のトレイト。
    let src = format!(
        "{S}pub trait Invariant {{ fn ok(&self) -> bool; }}\nimpl Invariant for S {{ fn ok(&self) -> bool {{ true }} }}\npub fn f(x: S) {{}}\n"
    );
    assert!(requires_of(&src).is_empty());
}

// ---- B2：impl の中の部分集合型と impl spec ----

#[test]
fn b2_subset_types_inside_impls_are_rejected() {
    rejects(
        "pub trait Tr { type T; }\nimpl Tr for u64 { type T = u64 where self > 0; }\n",
        "E0003",
        "subset types",
    );
}

// ---- S1：権限を返す式を変数に束縛しない ----

const MK: &str = "pub struct Node { pub val: u64 }\npub fn mk() -> (r: Ptr<Node>) gives r@ { let r = Ptr::new(Node { val: 1 }); r }\npub fn g(p: Ptr<Node>) {}\n";

#[test]
fn s1_permissions_returned_by_self_methods_are_introduced() {
    let src = format!(
        "{MK}pub struct H {{ a: u64 }}\nimpl H {{\n    pub fn mk(&self) -> (r: Ptr<Node>) gives r@ {{ let r = Ptr::new(Node {{ val: 1 }}); r }}\n    pub fn f(&self) {{ let q = self.mk(); }}\n}}\n"
    );
    rejects(&src, "E0505", "`q@`");
    let src = format!(
        "{MK}pub struct H {{ a: u64 }}\nimpl H {{\n    pub fn mk(&self) -> (r: Ptr<Node>) gives r@ {{ let r = Ptr::new(Node {{ val: 1 }}); r }}\n    pub fn f(&self) {{ let q = self.mk(); leak q@; }}\n}}\n"
    );
    accepts(&src);
}

#[test]
fn s1_permissions_not_bound_to_a_variable_are_e0505() {
    for body in [
        "mk();",
        "g(mk());",
        "Ptr::new(Node { val: 1 });",
        "let _ = Ptr::new(Node { val: 1 });",
        "let (a, b) = (Ptr::new(Node { val: 1 }), 1);",
    ] {
        rejects(
            &format!("{MK}pub fn f() {{ {body} }}\n"),
            "E0505",
            "is discarded",
        );
    }
}

// ---- S2：クロージャの中で導入した権限 ----

#[test]
fn s2_permissions_introduced_in_closures_are_e0512() {
    rejects(
        &in_pool("pub fn f(n: u64) { let g = || { let p = Ptr::new(Node { val: n }); }; }"),
        "E0512",
        "`p`",
    );
}

// ---- S3：分解代入 ----

#[test]
fn s3_destructuring_assignment_is_e0506() {
    rejects(
        &in_pool(
            "pub fn f(q: Ptr<Node>) { let mut p = Ptr::new(Node { val: 1 }); let mut x = 0u64; (p, x) = (q, 1); leak p@; }",
        ),
        "E0506",
        "assign to `p`",
    );
}

// ---- S4：borrow ブロックを break / continue で抜ける（#21 の既定 A） ----

#[test]
fn s4_break_or_continue_out_of_borrow_is_e0511() {
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { borrow self@nodes[p] { break; } i = i + 1; } }",
        ),
        "E0511",
        "cannot `break` out of the `borrow` block of `p`",
    );
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { i = i + 1; borrow mut self@nodes[p] { continue; } } }",
        ),
        "E0511",
        "`continue`",
    );
    // `loop` を抜けた後に借用中の状態を残さない（後の take / put は通る）。
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { loop { borrow mut self@nodes[p] { break; } } take self@nodes[p]; put self@nodes[p]; }",
        ),
        "E0511",
        "`break`",
    );
    // ラベル付きのブロックも同じ。
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { 'a: { borrow self@nodes[p] { break 'a; } } take self@nodes[p]; put self@nodes[p]; }",
        ),
        "E0511",
        "`break`",
    );
    // ブロックの中で閉じたループの break は通る。return も通る（#21 の既定 A）。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { borrow self@nodes[p] { loop { break; } } }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { borrow self@nodes[p] { return; } }",
    ));
}

// ---- S5：外してもどのテストも落ちなかったチェック ----

#[test]
fn s5_rebinding_with_a_new_permission_while_held_is_e0507() {
    rejects(
        &in_pool(
            "pub fn f() { let p = Ptr::new(Node { val: 1 }); let p = Ptr::new(Node { val: 2 }); leak p@; }",
        ),
        "E0507",
        "bind `p` again",
    );
}

#[test]
fn s5_breaks_of_labeled_blocks_are_merged() {
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, c: bool) { take self@nodes[p]; 'a: { if c { break 'a; } put self@nodes[p]; } }",
        ),
        "E0510",
        "`p@`",
    );
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, c: bool) { take self@nodes[p]; 'a: { if c { put self@nodes[p]; break 'a; } put self@nodes[p]; } }",
    ));
}

#[test]
fn s5_trait_impl_methods_with_contracts_or_expansions_are_e0003() {
    rejects(
        &format!(
            "{S}pub trait Tr {{ fn g(&self) -> u64; }}\nimpl Tr for S {{ fn g(&self) -> (r: u64) ensures r > 0 {{ 1 }} }}\n"
        ),
        "E0003",
        "traits with contracts",
    );
    rejects(
        &format!(
            "{S}impl Invariant for S {INV}pub trait Tr {{ fn g(&self); }}\nimpl Tr for S {{ fn g(&self) {{ }} }}\n"
        ),
        "E0003",
        "traits with contracts",
    );
}

#[test]
fn s5_impl_spec_is_e0003() {
    rejects(
        "impl spec core::clone::Clone for u64 { spec fn f(&self) -> bool = true; }\n",
        "E0003",
        "`impl spec`",
    );
}

// ---- 細かい指摘 ----

#[test]
fn double_free_across_blocks_is_e0504() {
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { { take self@nodes[p]; let n = p.free(); } let m = p.free(); }",
        ),
        "E0504",
        "not held",
    );
}

#[test]
fn mutable_borrow_through_a_shared_permission_is_e0508() {
    rejects(
        &format!(
            "{PRELUDE}fn set(v: &mut u64) {{ *v = 1; }}\nfn f(p: Ptr<Node>) uses &p@ {{ set(&mut (*p).val); }}\n"
        ),
        "E0508",
        "`p`",
    );
}

#[test]
fn closure_parameters_are_e0507() {
    rejects(
        &in_pool(
            "pub fn f() { let p = Ptr::new(Node { val: 1 }); let g = |p: u64| p + 1; leak p@; }",
        ),
        "E0507",
        "bind `p` again",
    );
}

#[test]
fn permissions_in_unread_macro_arguments_are_e0003() {
    rejects(
        &format!(
            "{PRELUDE}fn eat(p: Ptr<Node>) uses p@ {{ leak p@; }}\nfn f() {{ let p = Ptr::new(Node {{ val: 1 }}); assert!({{ eat(p); true }}); leak p@; }}\n"
        ),
        "E0003",
        "arguments of this macro",
    );
    accepts(&in_pool(
        "pub fn f(x: u64) { let p = Ptr::new(Node { val: 1 }); assert!(x > 0); println!(\"{}\", p.addr()); leak p@; }",
    ));
}

// ---- 再レビュー ----

/// (b) パスでない型の別名も、右辺を §6.4 の位置の判定に代入する。
#[test]
fn rereview_aliases_of_wrappers_and_references_expand() {
    let (c, lines) = run(&format!(
        "{S}impl Invariant for S {INV}type O = Option<S>;\npub fn f(x: O) {{}}\n"
    ));
    assert!(lines.is_empty(), "{lines:?}");
    let e: Vec<_> = c.facts.expansions.values().collect();
    assert_eq!(e[0].requires[0].wrap, Wrap::Option);
    assert_eq!(e[0].requires[0].subject, Subject::Var("x".into()));
    let src = format!("{S}impl Invariant for S {INV}type R<'a> = &'a S;\npub fn f(x: R) {{}}\n");
    assert_eq!(requires_of(&src), ["S:x"]);
    // 型の引数を代入する。
    let src =
        format!("{S}impl Invariant for S {INV}type W<T> = Option<T>;\npub fn f(x: W<S>) {{}}\n");
    let (c, _) = run(&src);
    assert_eq!(c.facts.expansions.len(), 1);
    // 展開しない位置の別名は、使った位置に W0601。
    let src = format!("{S}impl Invariant for S {INV}type V = Vec<S>;\npub fn f(x: V) {{}}\n");
    let (_, lines) = run(&src);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(":4:13: W0601"), "{}", lines[0]);
}

/// 4. グループの境界を保つ（`take(n)` は `take n` ではない）。
#[test]
fn rereview_macro_tokens_keep_group_boundaries() {
    accepts(&in_pool(
        "pub fn f(v: Vec<u64>, n: usize) { println!(\"{}\", v.iter().take(n).count()); }",
    ));
    accepts(&in_pool(
        "pub fn f(x: Option<u64>) -> bool { matches!(x, Some(take) if take > 0) }",
    ));
    accepts(&in_pool(
        "pub fn f(x: u64) { let p = Ptr::new(Node { val: 1 }); println!(\"{}\", 2 * x); leak p@; }",
    ));
    // `take` / `put` は領域添字（`x[..]`、`self@x[..]`）が続くときだけ文のキーワード。
    accepts(&in_pool(
        "pub fn f() { let take = 1u64; println!(\"{}\", take as u64); }",
    ));
    // 領域添字の形は今までどおり拒否する。
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { println!(\"{:?}\", { take self@nodes[p]; 1 }); }",
        ),
        "E0003",
        "arguments of this macro",
    );
}

/// 5. 権限を作る式をマクロの引数と型の別名で隠しても E0003 / E0505。
#[test]
fn rereview_hidden_permission_sources() {
    rejects(
        &in_pool("pub fn f(n: u64) { let v = vec![Ptr::new(Node { val: n })]; }"),
        "E0003",
        "arguments of this macro",
    );
    rejects(
        &format!(
            "{PRELUDE}type P = Ptr<Node>;\npub fn f(n: u64) {{ P::new(Node {{ val: n }}); }}\n"
        ),
        "E0505",
        "is discarded",
    );
    accepts(&format!(
        "{PRELUDE}type P = Ptr<Node>;\npub fn f(n: u64) {{ let p = P::new(Node {{ val: n }}); leak p@; }}\n"
    ));
}

// ---- 3 回目のレビュー：索引は cfg を評価しない ----

#[test]
fn cfg_gated_trait_does_not_hide_the_invariant() {
    let src = "#[cfg(any())] pub trait Invariant { }\npub struct S { a: u64 }\nimpl crate::Invariant for S { closed spec fn invariant(&self) -> bool = self.a > 0; }\npub fn f(x: S) {}\n";
    rejects(src, "E0001", "written only as `impl Invariant for T`");
    let src = "#[cfg(test)] mod m { pub trait Invariant {} }\npub struct S { a: u64 }\nimpl m::Invariant for S { closed spec fn invariant(&self) -> bool = self.a > 0; }\n";
    rejects(src, "E0001", "written only as `impl Invariant for T`");
}

#[test]
fn cfg_gated_functions_do_not_hide_verus_builtins() {
    rejects(
        "#[cfg(any())] fn requires(b: bool) {}\npub fn f(x: u64) { crate::requires(x == 0); }",
        "E0001",
        "`crate::requires(..)` is Verus syntax",
    );
    rejects(
        "#[cfg(test)] fn ensures(b: bool) {}\npub fn f(x: u64) { self::ensures(x == 0); }",
        "E0001",
        "`self::ensures(..)` is Verus syntax",
    );
    rejects(
        "#[cfg(test)] mod m { pub fn requires(b: bool) {} }\npub fn f(x: u64) { m::requires(x == 0); }",
        "E0001",
        "`m::requires(..)` is Verus syntax",
    );
    // cfg_attr は項目を消さない。
    accepts(
        "#[cfg_attr(any(), inline)] fn requires(b: bool) {}\npub fn f(x: u64) { crate::requires(x == 0); }",
    );
}

#[test]
fn duplicate_definitions_are_e0003() {
    rejects(
        "#[cfg(a)] pub fn f() {}\n#[cfg(not(a))] pub fn f() {}\n",
        "E0003",
        "does not evaluate `#[cfg]`",
    );
    rejects(
        &format!(
            "{PRELUDE}#[cfg(a)] fn eat(p: Ptr<Node>) uses p@ {{ leak p@; }}\n#[cfg(not(a))] fn eat(p: Ptr<Node>) {{ }}\n"
        ),
        "E0003",
        "defined more than once",
    );
    rejects(
        "pub struct H {}\nimpl H { #[cfg(a)] pub fn g(&self) {} }\nimpl H { #[cfg(not(a))] pub fn g(&self) {} }\n",
        "E0003",
        "defined more than once",
    );
    // 名前空間が違えば重複ではない（Rust と同じ）。
    accepts("pub mod g {}\npub fn g() {}\n");
}

// ---- PR #26 のレビュー ----

#[test]
fn nested_assumptions_are_e0003() {
    // B2：関数の本体の中の unchecked fn / lemma fn は、verify が一覧に出せない。
    rejects(
        "pub fn outer(x: u32) -> (r: u32) ensures r == 7, { unchecked fn liar(y: u32) -> (z: u32) ensures z == 7, { y } liar(x) }",
        "E0003",
        "inside a function body are not supported yet, because `verify`",
    );
    rejects(
        "pub fn outer() { lemma fn l() ensures false; }",
        "E0003",
        "`unchecked fn` and `lemma fn`",
    );
    accepts("pub fn outer() { fn helper() {} helper(); }");
}

#[test]
fn first_last_in_logical_macro_tokens_are_e0003() {
    // B5：#11 の置き換えはトークンのまま持つマクロの引数に届かない。
    rejects(
        "spec fn nonempty(s: Seq<Option<int>>) -> bool = matches!(s.first(), Some(_));",
        "E0003",
        "`.first()` / `.last()`",
    );
    // 実行文脈の `Vec::first` はそのまま。
    accepts("pub fn f(v: Vec<u64>) -> bool { matches!(v.first(), Some(_)) }");
}

// ---- PR #26 の再レビュー：関数の中の条件付きコンパイル（#27 の既定 B） ----

#[test]
fn cfg_inside_functions_is_e0003() {
    for src in [
        "pub fn f(x: u32) -> (r: u32) ensures r == 0, { if cfg!(verus_keep_ghost) { 0 } else { 1 } }",
        "pub fn g(x: u32) -> (r: u32) ensures r == 0, { #[cfg(not(verus_keep_ghost))] return 1; 0 }",
        "pub fn h() -> u32 { #[cfg_attr(test, cfg(test))] let y = 1; y }",
        "pub fn k(x: u32) -> u32 { match x { #[cfg(test)] 0 => 1, _ => 2 } }",
        "pub struct P { a: u32 }\npub fn m() -> P { P { #[cfg(test)] a: 1 } }",
        "pub fn n(#[cfg(test)] x: u32) {}",
        "pub fn o() { #[cfg(test)] fn inner() {} }",
    ] {
        rejects(src, "E0003", "`cfg!` and `#[cfg(..)]` inside a function");
    }
    // 本体の中で定義した構造体・列挙の欄と列挙子（PR #26 の 3 回目の確認）。
    for src in [
        "pub fn p() { struct T { #[cfg(test)] a: u32, b: u32 } }",
        "pub fn q() { struct T(#[cfg(test)] u32, u32); }",
        "pub fn s() { enum E { #[cfg(test)] A, B } }",
        "pub fn t() { enum E { A { #[cfg(test)] a: u32 }, B } }",
    ] {
        rejects(src, "E0003", "`cfg!` and `#[cfg(..)]` inside a function");
    }
    // 項目に付ける #[cfg] と、cfg を含まない cfg_attr は今のまま。
    accepts("#[cfg(test)]\npub fn f() {}");
    accepts("pub fn f() { #[cfg_attr(test, inline)] let y = 1; }");
}

/// std のマクロの引数はトークンのまま持つので、その中の属性はどれも E0003（PR #26 の 3 回目の確認、#28）。
#[test]
fn attributes_inside_std_macro_arguments_are_e0003() {
    for src in [
        "pub fn f() -> (r: usize) ensures r == 1, { let v: Vec<u32> = vec![#[cfg(not(verus_keep_ghost))] 1u32, 0]; v.len() }",
        "pub fn g(x: u32) -> (r: u32) ensures r == 0, { assert!({ #[cfg(not(verus_keep_ghost))] return 1; true }); 0 }",
        "pub fn h() { assert!({ #[inline] let y = 1; y == 1 }); }",
        "pub fn k() { assert!({ #![allow(unused)] true }); }",
    ] {
        rejects(
            src,
            "E0003",
            "attributes (`#[..]`) inside the arguments of this macro",
        );
    }
    // 属性でない `#` は今のまま（文字列の中や、`[` の続かない `#`）。
    accepts("pub fn f() { assert!(\"#[cfg(test)]\".len() == 12); }");
}
