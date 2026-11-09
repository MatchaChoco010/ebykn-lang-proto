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

// ---- S4：borrow ブロックを break / continue / return / ? で抜ける（#21 の既定 D） ----

/// 抜ける点で `borrow` ブロックが終わる。抜けた先の Γ には借りた権限が残らない。
#[test]
fn s4_jumping_out_of_borrow_ends_the_block() {
    // #21 のコメントの例。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { borrow mut self@nodes[p] { if (*p).val == 0 { break; } } i = i + 1; } }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { borrow self@nodes[p] { break; } i = i + 1; } }",
    ));
    // `continue` も（抜けた先の Γ はループの入口と同じ）。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { i = i + 1; borrow mut self@nodes[p] { continue; } } }",
    ));
    // `loop` を抜けた後に借用中の状態を残さない（後の take / put は通る）。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { loop { borrow mut self@nodes[p] { break; } } take self@nodes[p]; put self@nodes[p]; }",
    ));
    // ラベル付きのブロックも同じ。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { 'a: { borrow self@nodes[p] { break 'a; } } take self@nodes[p]; put self@nodes[p]; }",
    ));
    // 入れ子の borrow を一度に抜ける。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, q: Ptr<Node>) { loop { borrow mut self@nodes[p] { borrow mut self@nodes[q] { break; } } } take self@nodes[p]; put self@nodes[p]; }",
    ));
    // ブロックの中で閉じたループの break、return。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { borrow self@nodes[p] { loop { break; } } }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { borrow mut self@nodes[p] { return; } }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, o: Option<u64>) -> Option<u64> { borrow mut self@nodes[p] { let v = o?; (*p).val = v; } Some(0) }",
    ));
}

/// 抜けた先の比較は、借りた権限を消したあとの Γ で行う（ほかの権限の食い違いは今までどおり）。
#[test]
fn s4_permissions_other_than_the_borrow_are_still_compared() {
    // `take` した権限を持ったまま `break` すると、ほかの `break` と食い違う。食い違いに `p@` は出ない。
    let src = in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, q: Ptr<Node>, c: bool) { loop { if c { break; } borrow mut self@nodes[p] { take self@nodes[q]; if c { break; } put self@nodes[q]; } } }",
    );
    rejects(
        &src,
        "E0511",
        "`q@` is not held after the loop but owned at this `break`",
    );
    let (_, lines) = run(&src);
    assert!(!lines[0].contains("`p@`"), "{}", lines[0]);
    // `continue` で、本体の始めにない権限を持ち越す。
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, q: Ptr<Node>, n: u64) { let mut i = 0; while i < n { i = i + 1; borrow mut self@nodes[p] { take self@nodes[q]; continue; } } }",
        ),
        "E0511",
        "`q@`",
    );
    // `return` で、所有した権限を残す（E0505）。
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, q: Ptr<Node>) { borrow mut self@nodes[p] { take self@nodes[q]; return; } }",
        ),
        "E0505",
        "`q@`",
    );
}

/// `borrow mut` の中の `?` は、戻り値の型を `Option` / `Result` と書いた関数だけ（書き下しで展開する）。
#[test]
fn s4_try_inside_borrow_mut_needs_option_or_result() {
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, o: Result<u64, u8>) -> Result<u64, u8> { borrow mut self@nodes[p] { (*p).val = o?; } Ok(0) }",
    ));
    rejects(
        &format!(
            "{PRELUDE}type R = Option<u64>;\nimpl Pool {{ pub fn f(&mut self, p: Ptr<Node>, o: Option<u64>) -> R {{ borrow mut self@nodes[p] {{ (*p).val = o?; }} Some(0) }} }}\n"
        ),
        "E0003",
        "`?` inside `borrow mut`",
    );
    // 共有の borrow は戻すものがないので、そのまま。
    accepts(&format!(
        "{PRELUDE}type R = Option<u64>;\nimpl Pool {{ pub fn f(&mut self, p: Ptr<Node>, o: Option<u64>) -> R {{ borrow self@nodes[p] {{ let v = o?; }} Some(0) }} }}\n"
    ));
}

/// `&&` / `||` の右辺は評価しないことがある。右辺だけが抜けても、式は必ず抜けるとは数えない
/// （PR #51 のレビューの B1）。
#[test]
fn s4_short_circuit_rhs_does_not_always_diverge() {
    // 右辺の `return` を必ず実行されると数えると、続く文が届かないとみなされ、本体の末尾の E0505 を見ない。
    rejects(
        &in_pool(
            "pub fn f(c: bool) { let _b = c && { return; }; let p = Ptr::new(Node { val: 1 }); }",
        ),
        "E0505",
        "`p@`",
    );
    rejects(
        &in_pool(
            "pub fn f(c: bool) { let _b = c || { return; }; let p = Ptr::new(Node { val: 1 }); }",
        ),
        "E0505",
        "`p@`",
    );
    // 右辺で権限が変われば、評価したときとしないときで食い違う。
    rejects(
        &in_pool(
            "pub fn f(&mut self, q: Ptr<Node>, c: bool) { let _b = c && { take self@nodes[q]; true }; }",
        ),
        "E0510",
        "`q@`",
    );
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, c: bool) { borrow mut self@nodes[p] { let _b = c && { return; }; } }",
    ));
}

/// マクロの引数の中の `return` / `break` / `continue` / `?` は E0003（PR #51 のレビューの B2）。
#[test]
fn s4_jumps_inside_macro_arguments_are_e0003() {
    for body in [
        "let _v = vec![{ if c { return; } 1u8 }];",
        "loop { let _v = vec![{ if c { break; } 1u8 }]; }",
        "loop { let _v = vec![{ if c { continue; } 1u8 }]; break; }",
        "let _v = vec![o?];",
    ] {
        rejects(
            &in_pool(&format!(
                "pub fn f(&mut self, p: Ptr<Node>, c: bool, o: Option<u8>) -> Option<u8> {{ borrow mut self@nodes[p] {{ {body} }} None }}"
            )),
            "E0003",
            "inside the arguments of a macro",
        );
    }
    // `borrow` の外でも拒む。
    rejects(
        &in_pool("pub fn f(c: bool) { let _v = vec![{ if c { return; } 1u8 }]; }"),
        "E0003",
        "inside the arguments of a macro",
    );
    // 式として読むマクロの引数も同じ。
    rejects(
        &in_pool("pub fn f(c: bool) { assert!({ if c { return; } true }); }"),
        "E0003",
        "inside the arguments of a macro",
    );
    // 文字列の中の `?` は抜ける式ではない。
    accepts(&in_pool(
        "pub fn f(x: u8) { let _s = format!(\"{:?}\", x); }",
    ));
}

/// Rust のトークンのまま持つ式（`.await`）の中の脱出は E0003（PR #51 の再レビューの B3）。検査は中を
/// 読まないので、`?` の経路で権限を捨てても E0505 にならない。
#[test]
fn s4_jumps_inside_raw_rust_expressions_are_e0003() {
    const ID: &str = "async fn id(x: u64) -> (r: u64) ensures r == x, { x }\n";
    for body in [
        "borrow mut self@nodes[p] { (*p).val = id(o?).await; } None",
        "borrow mut self@nodes[p] { (*p).val = id({ return None; }).await; } None",
        // `borrow` の外でも：`?` の経路で `q@` を捨てる。
        "let q = Ptr::new(Node { val: 1 }); let _v = id(o?).await; q.free(); None",
    ] {
        rejects(
            &format!(
                "{PRELUDE}{ID}impl Pool {{ async fn f(&mut self, p: Ptr<Node>, o: Option<u64>) -> Option<u64> {{ {body} }} }}\n"
            ),
            "E0003",
            "such as `.await`",
        );
    }
    // 脱出のない `.await` は今のまま。
    accepts(&format!(
        "{PRELUDE}{ID}impl Pool {{ async fn f(&mut self, p: Ptr<Node>) -> u64 {{ let v = id(1).await; borrow mut self@nodes[p] {{ (*p).val = v; }} v }} }}\n"
    ));
}

/// Rust のトークンのまま持つ式（`.await`、async ブロック）の中の権限の操作は E0003（#74）。検査は中を
/// 読まないので、そこで作った権限を捨てても解放し忘れにならない。
#[test]
fn perms_inside_raw_rust_expressions_are_e0003() {
    const ID: &str = "async fn id(x: u64) -> (r: u64) ensures r == x, { x }\n";
    for body in [
        // #74 の例：`.await` の中で作った `q@` を捨てる。
        "let v = id({ let q = Ptr::new(Node { val: 1 }); 1 }).await; v",
        // async ブロックの中の領域の操作。
        "let _f = async { take self@nodes[p]; }; 0",
    ] {
        rejects(
            &format!(
                "{PRELUDE}{ID}impl Pool {{ async fn f(&mut self, p: Ptr<Node>) -> u64 {{ {body} }} }}\n"
            ),
            "E0003",
            "permissions cannot be used inside this expression",
        );
    }
    // 持っている権限を `.await` の中で解放する（検査は解放を見ないので、末尾の E0505 も出る）。
    let (_, lines) = run(&format!(
        "{PRELUDE}{ID}impl Pool {{ async fn f(&mut self) -> u64 {{ let q = Ptr::new(Node {{ val: 1 }}); let v = id({{ let n = q.free(); n.val }}).await; v }} }}\n"
    ));
    assert!(
        lines.iter().any(|l| l.contains("E0003")
            && l.contains("permissions cannot be used inside this expression")),
        "{}",
        lines.join("\n")
    );
    // 権限の操作のない `.await` と、ポインタの値を使うだけの式は今のまま。
    accepts(&format!(
        "{PRELUDE}{ID}impl Pool {{ async fn f(&mut self, p: Ptr<Node>) -> u64 {{ let v = id(1).await; let w = id(p.addr() as u64).await; v + w }} }}\n"
    ));
}

/// 権限を作る・受け渡す呼び出しの綴りの表（#74 のレビューの B1、#98）。読まないトークンの中では、
/// ターボフィッシュ・修飾パス・値としての関数・局所の `use` のどれでも E0003。
const SPELL_PRE: &str = "async fn id(x: u64) -> (r: u64) ensures r == x, { x }
fn mk(v: u64) -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: v }); q }
fn mkg<T>(v: T) -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: 1 }); q }
type P = Ptr<Node>;
type C = Cell<u64>;
type PP<T> = Ptr<T>;
use ebykn_prelude::Ptr as PU;
pub trait Tr { type Out; }
impl Tr for Node { type Out = Ptr<Node>; }
pub mod m { pub type Q = Ptr<super::Node>; }
type X = <Node as Tr>::Out;
type PX<T> = T;
";
const SPELL_IMPL: &str =
    "fn mks() -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: 1 }); q }
fn mkm(&self) -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: 1 }); q }
";
const SPELLINGS: &[&str] = &[
    "let q = Ptr::<Node>::new(Node { val: 1 })",
    "let q = Ptr::<Node>::empty()",
    "let q = Cell::<u64>::new(1)",
    "let q = ebykn_prelude::Ptr::<Node>::new(Node { val: 1 })",
    "let q = mk::<>(1)",
    "let q = mkg::<u8>(0)",
    "let q = Self::mks::<>()",
    "let q = self.mkm::<>()",
    "let q = <Ptr<Node>>::new(Node { val: 1 })",
    "let q = <Self>::mks()",
    "let q = (Ptr::new)(Node { val: 1 })",
    "let g = Ptr::new; let q = g(Node { val: 1 })",
    "let g = mk; let q = g(1)",
    "use crate::mk as m2; let q = m2(1)",
    // モジュールの直下の別名（`type`、`use .. as`、型の引数を持つもの）。
    "let q = P::new(Node { val: 1 })",
    "let q = <P>::new(Node { val: 1 })",
    "let q = C::new(1)",
    "let q = PP::<Node>::new(Node { val: 1 })",
    "let q = PU::new(Node { val: 1 })",
    // 別のモジュールの別名（パスを読まないので、名前で数える）。
    "let q = m::Q::new(Node { val: 1 })",
    // raw の識別子。
    "let q = r#Ptr::new(Node { val: 1 })",
    "let q = r#mk(1)",
    "let q = self.r#mkm()",
    // 関連型を通した修飾パス。
    "let q = <Node as Tr>::Out::new(Node { val: 1 })",
    // 行き先を引けない別名（射影、型の引数）と、型に相対的なパス（#75 の再確認）。
    "let q = X::new(Node { val: 1 })",
    "let q = PX::<Ptr<Node>>::new(Node { val: 1 })",
    "let q = Self::Out2::new(Node { val: 1 })",
];

#[test]
fn perm_spellings_inside_unread_tokens_are_e0003() {
    for x in SPELLINGS {
        for (body, needle) in [
            (
                format!("let v = id({{ {x}; 1 }}).await; v"),
                "permissions cannot be used inside this expression",
            ),
            (
                format!("let _b = matches!({{ {x}; 1u64 }}, 1); 0"),
                "arguments of this macro",
            ),
        ] {
            let src = format!(
                "{PRELUDE}{SPELL_PRE}impl Pool {{ {SPELL_IMPL}async fn f(&self) -> u64 {{ {body} }} }}\n"
            );
            let (_, lines) = run(&src);
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains("E0003") && l.contains(needle)),
                "{x}:\n{}",
                lines.join("\n")
            );
        }
    }
    // 誤検出しない：名前の一部が重なるメソッド、文字列と文字、権限に関わらない名前。
    for x in [
        "let n = v.freeze()",
        "let n = \"Ptr::new free take\".len()",
        "let n = 'q'",
        "let n = v.len()",
    ] {
        accepts(&format!(
            "{PRELUDE}{SPELL_PRE}pub struct V {{ a: u64 }}\nimpl V {{ fn freeze(&self) -> u64 {{ 1 }} fn len(&self) -> u64 {{ 1 }} }}\nasync fn f(v: V) -> u64 {{ let w = id({{ {x}; 1 }}).await; let _b = matches!({{ {x}; 1u64 }}, 1); w }}\n"
        ));
    }
}

/// 領域を渡す呼び出し（`f(x, uses &mut r)`。#19）をマクロの引数の中に書くと E0003。
#[test]
fn region_passing_calls_inside_macro_arguments_are_e0003() {
    rejects(
        &format!(
            "{PRELUDE}fn g(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> requires old(r).contains(p), {{ take r[p]; put r[p]; }}\nfn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> requires old(r).contains(p), {{ let _b = matches!(g(p, uses &mut r), ()); }}\n"
        ),
        "E0003",
        "arguments of this macro",
    );
}

/// 権限を持つ関数を値として使うか修飾パスで呼ぶと、読むトークンの中でも E0003（#98）。
#[test]
fn perm_functions_used_as_values_are_e0003() {
    for x in [
        "let g = Ptr::new; let q = g(Node { val: 1 });",
        "let q = (Ptr::new)(Node { val: 1 });",
        "let g = mk; let q = g(1);",
        "let g = Self::mks; let q = g();",
        "let q = <Ptr<Node>>::new(Node { val: 1 });",
        "let q = <Self>::mks();",
        // 引けない修飾パス（関連型を通したもの）は安全側に倒す（#75 の再確認の B3）。
        "let q = <Node as Tr>::Out::new(Node { val: 1 });",
    ] {
        let src = format!(
            "{PRELUDE}{SPELL_PRE}impl Pool {{ {SPELL_IMPL}fn f(&self) -> u64 {{ {x} 1 }} }}\n"
        );
        let (_, lines) = run(&src);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") && l.contains("can only be called directly")),
            "{x}:\n{}",
            lines.join("\n")
        );
    }
    // 呼び出しの頭に書くのは今のまま。同じ名前のローカル変数も通す。
    accepts(&format!(
        "{PRELUDE}{SPELL_PRE}impl Pool {{ {SPELL_IMPL}fn f(&self) -> u64 {{ let q = Ptr::<Node>::new(Node {{ val: 1 }}); let r = mk(2); let s = Self::mks(); leak q@; leak r@; leak s@; let mk = 3u64; mk }} }}\n"
    ));
}

/// 関連型・型の引数・行き先を引けない別名を通るパスは、トークンの中でも外でも E0003（#75 の再確認）。
/// 索引は関連型を解決しないので、`Self::Out2::new` が `Ptr::new` かを引けない。
#[test]
fn type_relative_paths_are_e0003() {
    const PRE: &str = "pub trait Tr2 { type Out2; fn go(&self) -> u64; fn go2(&self) -> u64; }\n";
    const TOKENS: &str = "permissions cannot be used inside this expression";
    const MACRO: &str = "arguments of this macro";
    const CALL: &str = "through an associated type";
    for (item, needle) in [
        (
            "impl Tr2 for Node { type Out2 = Ptr<Node>; fn go(&self) -> u64 { let _b = matches!({ let q = Self::Out2::new(Node { val: 1 }); 1u64 }, 1); 0 } fn go2(&self) -> u64 { 0 } }",
            MACRO,
        ),
        (
            "impl Tr2 for Node { type Out2 = Ptr<Node>; fn go(&self) -> u64 { 0 } fn go2(&self) -> u64 { let q = Self::Out2::new(Node { val: 1 }); 0 } }",
            CALL,
        ),
        (
            "pub fn g1<T: Tr<Out = Ptr<Node>>>() -> u64 { let _b = matches!({ let q = T::Out::new(Node { val: 1 }); 1u64 }, 1); 0 }",
            MACRO,
        ),
        (
            "pub fn g2<T: Tr<Out = Ptr<Node>>>() -> u64 { let q = T::Out::new(Node { val: 1 }); 0 }",
            CALL,
        ),
        (
            "pub async fn h4<T: Tr<Out = Ptr<Node>>>() -> u64 { let v = id({ let q = T::Out::new(Node { val: 1 }); 1 }).await; v }",
            TOKENS,
        ),
        (
            "pub fn h1() -> u64 { let _b = matches!({ let q = X::new(Node { val: 1 }); 1u64 }, 1); 0 }",
            MACRO,
        ),
        (
            "pub fn h2() -> u64 { let q = X::new(Node { val: 1 }); 0 }",
            CALL,
        ),
        (
            "pub fn h3() -> u64 { let q = PX::<Ptr<Node>>::new(Node { val: 1 }); 0 }",
            CALL,
        ),
        // 別名の鎖と、impl の型の引数。
        (
            "type X2 = X; pub fn h5() -> u64 { let q = X2::new(Node { val: 1 }); 0 }",
            CALL,
        ),
        (
            "pub struct S<T> { t: T }\nimpl<T: Tr<Out = Ptr<Node>>> S<T> { pub fn h6(&self) -> u64 { let q = T::Out::new(Node { val: 1 }); 0 } }",
            CALL,
        ),
        // 値として使う形は #98 の診断。
        (
            "pub fn h7<T: Tr<Out = Ptr<Node>>>() -> u64 { let g = T::Out::new; let q = g(Node { val: 1 }); 0 }",
            "can only be called directly",
        ),
    ] {
        let src = format!("{PRELUDE}{SPELL_PRE}{PRE}{item}\n");
        let (_, lines) = run(&src);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") && l.contains(needle)),
            "{item}:\n{}",
            lines.join("\n")
        );
    }
    // 行き先を引ける別名、2 つの区切りの `T::f`、クレートの外の型のパスは今のまま。
    accepts(&format!(
        "{PRELUDE}{SPELL_PRE}type V = Vec<u64>;\npub fn k<T: Default>() -> u64 {{ let _t = T::default(); let _v = V::new(); let _w = Vec::<u64>::new(); let _x = std::vec::Vec::<u64>::new(); let _b = matches!(T::default(), _); 0 }}\n"
    ));
}

/// 行き先を引けない別名を `use` で取り込んでも、別名として引く（#75 の再確認の B1）。これまでは
/// `use` の先で別名を `Unknown` までたどってしまい、`Y::new(..)` を見逃していた。
#[test]
fn unresolved_aliases_imported_by_use_are_e0003() {
    const PRE: &str = "pub trait Tr { type Out; }
impl Tr for Node { type Out = Ptr<Node>; }
async fn id(x: u64) -> (r: u64) ensures r == x, { x }
pub mod mm { pub type X = <super::Node as super::Tr>::Out; pub type PX<T> = T; }
";
    for (items, body, needle) in [
        (
            "use mm::X as Y;",
            "let q = Y::new(Node { val: 1 }); 0",
            "through an associated type",
        ),
        (
            "use mm::X;",
            "let q = X::new(Node { val: 1 }); 0",
            "through an associated type",
        ),
        (
            "use mm::PX as PY;",
            "let q = PY::<Ptr<Node>>::new(Node { val: 1 }); 0",
            "through an associated type",
        ),
        (
            "use mm::X as Y;",
            "let _b = matches!({ let q = Y::new(Node { val: 1 }); 1u64 }, 1); 0",
            "arguments of this macro",
        ),
        (
            "use mm::X as Y;",
            "let v = id({ let q = Y::new(Node { val: 1 }); 1 }).await; v",
            "permissions cannot be used inside this expression",
        ),
    ] {
        let src = format!("{PRELUDE}{PRE}{items}\npub async fn f() -> u64 {{ {body} }}\n");
        let (_, lines) = run(&src);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") && l.contains(needle)),
            "{items} {body}:\n{}",
            lines.join("\n")
        );
    }
}

/// 解決できない頭で、最後の名前が権限を作る・受け渡す関数の名前（`new`、`empty`、`uses` / `gives` を
/// 持つ関数）なら E0003（#75 の再確認）。解決できない綴りが新しく見つかっても黙って通さない。
#[test]
fn unresolved_heads_with_perm_function_names_are_e0003() {
    const PRE: &str =
        "fn mk(v: u64) -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: v }); q }
use ebykn_prelude::Ptr as PU;
use ebykn_prelude as ep;
use ep::Ptr as P3;
pub mod pm { pub use super::mk; }
";
    const CALL: &str = "a path that cannot be resolved";
    for (body, needle) in [
        (
            "let q = ::ebykn_prelude::Ptr::new(Node { val: 1 }); 0",
            CALL,
        ),
        ("let q = pm::mk(1); 0", CALL),
        ("let q = ep::Ptr::new(Node { val: 1 }); 0", CALL),
        ("let q = ep::Cell::new(1u64); 0", CALL),
        ("let q = P3::new(Node { val: 1 }); 0", CALL),
        ("let q = P3::<Node>::empty(); 0", CALL),
        // 値として使う形も同じ。
        ("let g = P3::new; let q = g(Node { val: 1 }); 0", CALL),
        // 読まないトークンの中。
        (
            "let _b = matches!({ let q = P3::new(Node { val: 1 }); 1u64 }, 1); 0",
            "arguments of this macro",
        ),
    ] {
        let src = format!("{PRELUDE}{PRE}pub fn f() -> u64 {{ {body} }}\n");
        let (_, lines) = run(&src);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") && l.contains(needle)),
            "{body}:\n{}",
            lines.join("\n")
        );
    }
    // サブモジュールから、モジュールの直下の別名を通す形。
    let src = format!(
        "{PRELUDE}{PRE}pub mod sm {{ pub fn g() -> u64 {{ let q = crate::PU::new(crate::Node {{ val: 1 }}); 0 }} }}\n"
    );
    let (_, lines) = run(&src);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("E0003") && l.contains(CALL)),
        "{}",
        lines.join("\n")
    );
    // std と依存クレートのパス、外への別名、クレートの型の上のトレイトのメソッドは今のまま。
    accepts(&format!(
        "{PRELUDE}use std::collections::BTreeMap;\ntype V = Vec<u64>;\nuse crate::V as W;\npub trait Mk {{ fn new() -> Self; }}\npub struct S5 {{ x: u64 }}\nimpl Mk for S5 {{ fn new() -> Self {{ S5 {{ x: 0 }} }} }}\npub fn k() -> u64 {{ let _a = Vec::<u64>::new(); let _b = String::new(); let _c = std::vec::Vec::<u64>::new(); let _d = V::new(); let _e = W::new(); let _f = BTreeMap::<u64, u64>::new(); let _g = S5::new(); let _h = matches!(Vec::<u64>::new().len(), 0); let new = 1u64; new }}\n"
    ));
}

/// 解決できないパスは、根が外（std・core・alloc・依存クレート・std の prelude の名前）と言えなければ、最後の
/// 名前によらず E0003（#75 の 4 回目の再確認。綴りを一つずつ追わず、外の許可リストで決める）。
#[test]
fn unresolved_roots_outside_the_allow_list_are_e0003() {
    const CALL: &str = "a path that cannot be resolved";
    for body in [
        "let x = foo::bar(1); 0",
        "let x = baz(1); 0",
        "let g = foo::bar; 0",
    ] {
        rejects(
            &format!("{PRELUDE}use foo::bar as baz;\npub fn f() -> u64 {{ {body} }}\n"),
            "E0003",
            CALL,
        );
    }
    // self 型を引けない impl（`impl Mk for Ptr<Node>`）の中の `Self::new(..)`（develop では黙って通った）。
    let (_, lines) = run(&format!(
        "{PRELUDE}pub trait Mk {{ fn mk() -> Self; }}\nimpl Mk for Ptr<Node> {{ fn mk() -> Self {{ Self::new(Node {{ val: 1 }}) }} }}\n"
    ));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("E0003") && l.contains(CALL)),
        "{}",
        lines.join("\n")
    );
    // 読まないトークンの中も同じ。
    rejects(
        &format!("{PRELUDE}pub fn f() -> u64 {{ let _b = matches!(foo::bar(1), 1); 0 }}\n"),
        "E0003",
        "arguments of this macro",
    );
    // 本体の中の項目（`use` の別名、型と impl、glob の `use`）を根に持つパスは今のまま。権限に行き着く項目は
    // 本体の中の項目の検査が拒む。
    accepts(&format!(
        "{PRELUDE}pub mod m5 {{ pub fn id5(x: u64) -> u64 {{ x }} }}\npub fn f() -> u64 {{ let a = m6::id5(1); use crate::m5 as m6; struct L5; impl L5 {{ fn new() -> L5 {{ L5 }} }} let _l = L5::new(); {{ use crate::m5::*; let _b = id5(2); }} a }}\n"
    ));
    // 外の glob があるモジュールの知らない根、定数、列挙子、ローカルのクロージャ、本体の中の関数、型の引数は
    // 今のまま。
    accepts(&format!(
        "{PRELUDE}pub mod sx {{ use std::collections::*; pub fn g() -> u64 {{ let _m = BTreeMap::<u64, u64>::new(); 0 }} }}\npub const MAX: u64 = 3;\npub enum Color {{ Red, Blue }}\nuse Color::Blue;\npub fn f<T: Default>() -> u64 {{ let _c = Color::Red; let _d = Blue; let g = |x: u64| x; fn h() -> u64 {{ 1 }} let _t = T::default(); let _b = matches!(Color::Red, Color::Red); g(MAX) + h() }}\n"
    ));
}

/// 2 つ目以降の区切りの `super`（`super::super::mk`）を解決し、§6.2 の検査に通す（#75 の 4 回目の再確認）。
/// これまでは解決できず、別名（`zz`、`yy`）で呼ぶと安全側の規則にも掛からずに抜けていた。別名はモジュールの
/// 中の `use` を読まないと引けないので、ここでは安全側の規則で E0003 になる。
#[test]
fn super_after_the_first_segment_is_resolved() {
    const PRE: &str = "fn mk(v: u64) -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: v }); q }\nuse crate::mk as zz;\n";
    let src = |uses: &str, body: &str| {
        format!(
            "{PRELUDE}{PRE}pub mod a {{ pub mod b {{ {uses} use super::super::Node; pub fn f() -> u64 {{ {body} }} }} }}\n"
        )
    };
    for (uses, call) in [
        ("", "super::super::mk(1)"),
        ("use super::super::mk;", "mk(1)"),
    ] {
        rejects(
            &src(uses, &format!("let q = {call}; 0")),
            "E0505",
            "still owned",
        );
        accepts(&src(uses, &format!("let q = {call}; leak q@; 0")));
    }
    for (uses, body, needle) in [
        (
            "",
            "let q = super::super::zz(1); 0",
            "a path that cannot be resolved",
        ),
        (
            "use super::super::zz as yy;",
            "let q = yy(1); 0",
            "a path that cannot be resolved",
        ),
        (
            "use super::super::zz as yy;",
            "let _b = matches!({ let q = yy(1); 1u64 }, 1); 0",
            "arguments of this macro",
        ),
    ] {
        let (_, lines) = run(&src(uses, body));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") && l.contains(needle)),
            "{uses} {body}:\n{}",
            lines.join("\n")
        );
    }
}

/// 5 回目の再確認：安全側の規則の例外は、肯定の事実（ローカル変数、本体の中の項目、許可リストの根、
/// 行き先が外と分かった glob）があるときだけ通す。索引が名前を取りこぼす綴り（`r#` の別名、本体の中の
/// glob、深さの打ち切り）で黙って抜けない。
#[test]
fn allow_list_exceptions_need_positive_facts() {
    const MK: &str =
        "pub fn mk(v: u64) -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: v }); q }\n";
    const CALL: &str = "a path that cannot be resolved";
    // blocker 1：`use .. as r#k;` の別名は `k` と同じ（`r#` を外して比べる）。
    for (pre, body) in [
        ("use crate::mk as r#k;", "let q = k(1); 0"),
        ("use crate::mk as r#k;", "let q = r#k(1); 0"),
        (
            "pub struct W;\nimpl W { pub fn mkw() -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: 1 }); q } }\nuse crate::W as r#WW;",
            "let q = WW::mkw(); 0",
        ),
    ] {
        rejects(
            &format!("{PRELUDE}{MK}{pre}\npub fn f() -> u64 {{ {body} }}\n"),
            "E0505",
            "still owned",
        );
    }
    rejects(
        &format!(
            "{PRELUDE}{MK}use crate::mk as r#k;\npub fn f() -> u64 {{ let _b = matches!({{ let q = k(1); 1u64 }}, 1); 0 }}\n"
        ),
        "E0003",
        "arguments of this macro",
    );
    // blocker 2：本体の中の glob は、行き先が外と分かるときだけ、知らない根を通す。クレートの中の glob も、
    // 内側のブロックの外の glob も、self 型を引けない impl の `Self::new` を通さない。
    for body in [
        "use crate::Color::*; let q = Self::new(Node { val: 1 }); 0",
        "{ use std::collections::*; } let q = Self::new(Node { val: 1 }); 0",
        "use std::collections::*; let q = Self::new(Node { val: 1 }); 0",
    ] {
        let (_, lines) = run(&format!(
            "{PRELUDE}pub enum Color {{ Red }}\npub trait Mk {{ fn mkt() -> u64; }}\nimpl Mk for Ptr<Node> {{ fn mkt() -> u64 {{ {body} }} }}\n"
        ));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") && l.contains(CALL)),
            "{body}:\n{}",
            lines.join("\n")
        );
    }
    // 本体の中の外の glob は、クレートの名前（24 段の別名の鎖の `e24`）を外にしない。
    let mut chain = String::from("use ebykn_prelude as e0;\n");
    for k in 1..=24 {
        chain.push_str(&format!("use e{} as e{k};\n", k - 1));
    }
    rejects(
        &format!(
            "{PRELUDE}{chain}pub fn f() -> u64 {{ use std::collections::*; let q = e24::Ptr::new(Node {{ val: 1 }}); 0 }}\n"
        ),
        "E0003",
        CALL,
    );
    // 内側のブロックの本体の項目は、ブロックの外に効かない（`mm` はモジュールの直下の別名に戻る）。
    rejects(
        &format!(
            "{PRELUDE}pub mod m2 {{ use super::*; pub fn mk2(v: u64) -> (q: Ptr<Node>) gives q@ {{ let q = Ptr::new(Node {{ val: v }}); q }} }}\npub mod util {{ pub fn mk2(v: u64) -> u64 {{ v }} }}\nuse crate::m2 as r#mm;\npub fn f() -> u64 {{ {{ use crate::util as mm; }} let q = mm::mk2(1); 0 }}\n"
        ),
        "E0505",
        "still owned",
    );
    rejects(
        &format!(
            "{PRELUDE}pub mod util {{ pub fn id(v: u64) -> u64 {{ v }} }}\npub fn f() -> u64 {{ {{ use crate::util as zz; }} let x = zz::id(1); x }}\n"
        ),
        "E0003",
        CALL,
    );
    // 本体の中の外の glob から来る名前（`BTreeMap`）は通す（この例外を外すと落ちる）。
    accepts(&format!(
        "{PRELUDE}pub fn f() -> u64 {{ use std::collections::*; let _m = BTreeMap::<u64, u64>::new(); 0 }}\n"
    ));
    // blocker 4：深さで打ち切った glob の探索は、外の名前にしない（40 段の glob の鎖）。
    let mut globs = String::new();
    for k in 0..40 {
        globs.push_str(&format!(
            "pub mod g{k} {{ pub use super::g{}::*; }}\n",
            k + 1
        ));
    }
    globs.push_str("pub mod g40 { use super::Node; pub fn mk40(v: u64) -> (q: ebykn_prelude::Ptr<Node>) gives q@ { let q = ebykn_prelude::Ptr::new(Node { val: v }); q } }\nuse g0::*;\n");
    let (_, lines) = run(&format!(
        "{PRELUDE}{globs}pub fn f() -> u64 {{ let q = mk40(1); 0 }}\n"
    ));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("E0003") || l.contains("E0505")),
        "{}",
        lines.join("\n")
    );
    // should-fix：トレイトの既定のメソッドの `Self::new2()` は通す（トレイトのメソッドは契約を持てない）。
    accepts(&format!(
        "{PRELUDE}pub trait Mk2: Sized {{ fn new2() -> Self; fn make() -> Self {{ Self::new2() }} }}\n"
    ));
    // should-fix：索引の項目にならない `union` も名前を持つ（その上の gives のメソッドを引く）。
    rejects(
        &format!(
            "{PRELUDE}pub union U {{ a: u64 }}\nimpl U {{ pub fn mku() -> (q: Ptr<Node>) gives q@ {{ let q = Ptr::new(Node {{ val: 1 }}); q }} }}\npub fn f() -> u64 {{ let q = U::mku(); 0 }}\n"
        ),
        "E0505",
        "still owned",
    );
}

/// 6 回目の再確認：肯定の事実の一覧のそれぞれが穴にならない。
#[test]
fn positive_facts_do_not_leak() {
    const MK: &str =
        "pub fn mk(v: u64) -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: v }); q }\n";
    const VALUE: &str = "can only be called directly";
    const CALL: &str = "a path that cannot be resolved";
    let rejects_any = |src: String, needles: &[&str]| {
        let (_, lines) = run(&src);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") && needles.iter().any(|n| l.contains(n))),
            "{src}\n{}",
            lines.join("\n")
        );
    };
    // blocker 1：名前付きの戻り値は契約の中だけのローカル変数（本体の `mk` はモジュールの関数）。
    rejects_any(
        format!("{PRELUDE}{MK}pub fn f() -> (mk: u64) {{ let g = mk; let q = g(1); 0 }}\n"),
        &[VALUE],
    );
    rejects_any(
        format!(
            "{PRELUDE}{MK}pub fn f() -> u64 {{ let c = |v: u64| -> (mk: u64) {{ let g = mk; let q = g(v); 0 }}; c(1) }}\n"
        ),
        &[VALUE],
    );
    // blocker 2：`if let` の束縛は then のブロックの中だけ。
    rejects_any(
        format!(
            "{PRELUDE}{MK}pub fn f() -> u64 {{ let o = Some(1u64); if let Some(mk) = o {{ }} let g = mk; let q = g(1); 0 }}\n"
        ),
        &[VALUE],
    );
    rejects_any(
        format!(
            "{PRELUDE}{MK}pub fn f() -> u64 {{ let o = Some(1u64); if let Some(mk) = o {{ }} else {{ let g = mk; let q = g(1); }} 0 }}\n"
        ),
        &[VALUE],
    );
    // blocker 3：glob のパスを引く探索を深さで打ち切ったら、外の glob があっても止める。
    let mut chain = String::new();
    for k in 0..40 {
        chain.push_str(&format!(
            "pub mod g{k} {{ pub use super::g{}::*; }}\n",
            k + 1
        ));
        chain.push_str(&format!(
            "pub mod h{k} {{ pub use super::h{}::*; }}\n",
            k + 1
        ));
    }
    chain.push_str("pub mod g40 { use super::Node; pub fn mk40(v: u64) -> (q: ebykn_prelude::Ptr<Node>) gives q@ { let q = ebykn_prelude::Ptr::new(Node { val: v }); q } }\n");
    chain.push_str("pub mod h40 { pub mod tgt { use crate::Node; pub fn mk3(v: u64) -> (q: ebykn_prelude::Ptr<Node>) gives q@ { let q = ebykn_prelude::Ptr::new(Node { val: v }); q } } }\n");
    for (uses, body) in [
        (
            "use g0::*;\nuse std::collections::*;\n",
            "let q = mk40(1); 0",
        ),
        (
            "use h0::tgt::*;\nuse std::collections::*;\n",
            "let q = mk3(1); 0",
        ),
        (
            "use h0::tgt::*;\n",
            "use std::collections::*; let q = mk3(1); 0",
        ),
    ] {
        let (_, lines) = run(&format!(
            "{PRELUDE}{chain}{uses}pub fn f() -> u64 {{ {body} }}\n"
        ));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("E0003") || l.contains("E0505")),
            "{uses} {body}\n{}",
            lines.join("\n")
        );
    }
    // 本体の中の `use` の別名とクレートの glob は、行き先のパスに置き換えて検査する（行き先を引けない別名）。
    const M: &str = "pub trait Tr { type Out; }\nimpl Tr for Node { type Out = Ptr<Node>; }\npub mod m { pub type PX<T> = T; pub type X = <crate::Node as crate::Tr>::Out; pub fn id(v: u64) -> u64 { v } }\n";
    for body in [
        "use crate::m as mm; let q = mm::X::new(Node { val: 1 }); 0",
        "use crate::m as mm; let q = mm::PX::<Ptr<Node>>::new(Node { val: 1 }); 0",
        "use crate::m::*; use std::collections::*; let q = X::new(Node { val: 1 }); 0",
        "use crate::m::*; use std::collections::*; let q = PX::<Ptr<Node>>::new(Node { val: 1 }); 0",
    ] {
        rejects_any(
            format!("{PRELUDE}{M}pub fn f() -> u64 {{ {body} }}\n"),
            &[CALL],
        );
    }
    // 権限に行き着かない別名と glob は今のまま。
    accepts(&format!(
        "{PRELUDE}{M}pub fn f() -> u64 {{ use crate::m as mm; use crate::m::*; mm::id(1) + id(2) }}\n"
    ));
    // should-fix：`<Node as Mk2>::make()`（トレイトの後ろが一つの名前）はトレイトのメソッドなので通す。
    accepts(&format!(
        "{PRELUDE}pub trait Mk2 {{ fn make() -> Self; }}\nimpl Mk2 for Node {{ fn make() -> Self {{ Node {{ val: 1 }} }} }}\npub fn f() -> u64 {{ let _n = <Node as Mk2>::make(); 0 }}\n"
    ));
}

/// 戻り値の型の名前が `Option` でも、クレートで定義した型なら `?` を展開できない（E0003）。
#[test]
fn s4_try_with_a_crate_type_named_option_is_e0003() {
    rejects(
        &format!(
            "{PRELUDE}pub enum Option<T> {{ Some(T), None }}\nimpl Pool {{ pub fn f(&mut self, p: Ptr<Node>, o: core::option::Option<u64>) -> Option<u64> {{ borrow mut self@nodes[p] {{ (*p).val = o?; }} Option::None }} }}\n"
        ),
        "E0003",
        "`?` inside `borrow mut`",
    );
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
        "pub fn f(x: u64) { let p = Ptr::new(Node { val: 1 }); let a = p.addr(); assert!(x > 0); println!(\"{}\", a); leak p@; }",
    ));
    // Γ にある名前は、値として使うだけでも数える（綴りを列挙しない。#74 のレビュー）。値はマクロの外で求める。
    rejects(
        &in_pool(
            "pub fn f() { let p = Ptr::new(Node { val: 1 }); println!(\"{}\", p.addr()); leak p@; }",
        ),
        "E0003",
        "arguments of this macro",
    );
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
fn first_last_in_logical_macro_tokens_are_accepted() {
    // #11 の決定（案 B）：`.first()` / `.last()` は書き換えないので、マクロの引数の中でも外でも vstd の
    // 意味になる（M2 の E0003 は、置き換えが引数に届かず意味が食い違うのを防ぐためのものだった）。
    accepts("spec fn nonempty(s: Seq<Option<int>>) -> bool = matches!(s.first(), Some(_));");
    accepts("pub fn f(v: Vec<u64>) -> bool { matches!(v.first(), Some(_)) }");
}

// ---- PR #26 の再レビュー：関数の中の条件付きコンパイル（#27） ----
//
// #27 の作者の方針で、一律の禁止を拒否リストに組み替えた。拒否リストの総当たりは `tests/denylist.rs`。
// ここでは、PR #26 のレビューで見つかった入力が、拒否リストの名前なら拒否され、ほかの cfg なら通る
// ことを確かめる。

#[test]
fn cfg_inside_functions_follows_the_denylist() {
    let denied = "because its condition uses `verus_keep_ghost`";
    for src in [
        "pub fn f(x: u32) -> (r: u32) ensures r == 0, { if cfg!(verus_keep_ghost) { 0 } else { 1 } }",
        "pub fn g(x: u32) -> (r: u32) ensures r == 0, { let mut y = 0u32; #[cfg(not(verus_keep_ghost))] { y = 1; } y }",
        "pub fn f() -> (r: usize) ensures r == 1, { let v: Vec<u32> = vec![#[cfg(not(verus_keep_ghost))] 1u32, 0]; v.len() }",
        // raw の綴りも同じ名前（#122）。
        "pub fn g(x: u32) -> (r: u32) ensures r == 1, { let x: u32 = 0; #[r#cfg(verus_keep_ghost)] let x: u32 = 1; x }",
    ] {
        rejects(src, "E0001", denied);
    }
    // 拒否リストにない cfg は、関数の中のどこでも通る（検証と同じ設定でビルドすれば食い違わない）。
    for src in [
        "pub fn h() -> u32 { #[cfg_attr(test, cfg(test))] let y = 1; y }",
        "pub fn k(x: u32) -> u32 { match x { #[cfg(test)] 0 => 1, _ => 2 } }",
        "pub struct P { a: u32 }\npub fn m() -> P { P { #[cfg(test)] a: 1 } }",
        "pub fn n(#[cfg(test)] x: u32) {}",
        "pub fn o() { #[cfg(test)] fn inner() {} }",
        // raw の綴りも同じ名前（rustc は `r#cfg` を `cfg` と同じに読む。#122）。
        "pub fn h() -> u32 { #[cfg_attr(all(), r#cfg(test))] let y = 1; y }",
        "pub fn h() -> u32 { #[r#cfg_attr(all(), cfg(test))] let y = 1; y }",
        "pub fn k(x: u32) -> u32 { match x { #[r#cfg(test)] 0 => 1, _ => 2 } }",
        "pub fn n(#[r#cfg(test)] x: u32) {}",
        "pub fn p() { struct T { #[cfg(test)] a: u32, b: u32 } }",
        "pub fn q() { struct T(#[cfg(test)] u32, u32); }",
        "pub fn s() { enum E { #[cfg(test)] A, B } }",
        "pub fn t() { enum E { A { #[cfg(test)] a: u32 }, B } }",
        "pub fn u() -> bool { cfg!(test) }",
        "#[cfg(test)]\npub fn f() {}",
        "pub fn f() { #[cfg_attr(test, inline)] let y = 1; }",
        // std のマクロの引数の中の cfg と lint の属性（PR #26 の 3 回目の確認）も、拒否リストの名前でなければ通る。
        "pub fn h() { assert!({ #[cfg(test)] let y = 1; true }); }",
        "pub fn k2() { assert!({ #[cfg_attr(test, deny(unused))] let y = 1; true }); }",
        "pub fn f() { assert!(\"#[cfg(test)]\".len() == 12); }",
    ] {
        accepts(src);
    }
    // マクロの引数はトークンのまま持つので、derive・属性マクロ（PR #57 は AST で探す）を見られない。cfg・
    // `cfg_attr`・lint のほかの属性は拒否する（PR #59 のレビューの S3）。
    for src in [
        "pub fn h() { assert!({ #[inline] let y = 1; y == 1 }); }",
        "pub fn z(x: u32) -> (r: u32) ensures r == 0, { let mut x = x; assert!({ #[ext_macros::zero] fn one() -> u32 { 1 } x = one(); true }); x }",
        "pub fn d() { assert!({ #[derive(Clone)] struct T; true }); }",
        "pub fn c() { assert!({ #[cfg_attr(test, ext_macros::zero)] fn one() {} true }); }",
        "pub fn l() { assert!({ #[clippy::allow(x)] let y = 1; true }); }",
    ] {
        rejects(
            src,
            "E0003",
            "attributes other than `cfg`, `cfg_attr` and lint attributes",
        );
    }
    // 属性の付いた `return` は、拒否リスト（`verus_keep_ghost`）とマクロの中の `return`（#21 の既定 D）の
    // 両方で拒否する。
    let (_, lines) = run(
        "pub fn g(x: u32) -> (r: u32) ensures r == 0, { assert!({ #[cfg(not(verus_keep_ghost))] return 1; true }); 0 }",
    );
    assert_eq!(lines.len(), 2, "{}", lines.join("\n"));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("because its condition uses `verus_keep_ghost`"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("inside the arguments of a macro"))
    );
}

/// #95：クロージャの引数とパターンの欄の `#[cfg]`、raw の `#[r#cfg]`、ジェネリクスの引数の `#[cfg]`。
/// 書き下しが属性を落としていたので、`.eyk` で消える引数や欄が検証するコードに残った。
#[test]
fn cfg_on_closure_params_patterns_and_generics_is_e0003() {
    for src in [
        "pub fn g() -> u32 { let c = |#[cfg(any())] a: u32, b: u32| b; c(1) }",
        "pub fn g() -> u32 { let c = |#[r#cfg(any())] a: u32, b: u32| b; c(1) }",
        "pub fn g() -> u32 { let c = |#[cfg_attr(all(), r#cfg(any()))] a: u32, b: u32| b; c(1) }",
        "pub struct S { pub x: u32 }\npub fn p4(s: S) -> (r: u32) ensures s.x != 0 ==> r == 2, { match s { S { #[r#cfg(any())] x: 0, .. } => 1, _ => 2 } }",
        "pub struct S { pub x: u32 }\npub fn p(s: S) -> u32 { let S { #[cfg_attr(all(), r#cfg(any()))] x } = s; x }",
        "pub struct B { } owns { items: Region<Ptr<u64>> }\npub fn k(b: B) -> B { let B { #[cfg(test)] @items } = b; B { @items: items } }",
        // fn の引数のパターンの欄（引数そのものの属性は受理。PR #59 の 3 回目の再確認の nit）。
        "pub struct S { pub x: u32 }\npub fn t5(S { #[cfg(any())] x, .. }: S) -> u32 { 0 }",
    ] {
        rejects(src, "E0003", "`#[cfg(..)]` on a pattern");
    }
    // `..` と `@..` の前の属性は、rustc と同じく E0001（PR #129 のレビュー。書き下しは `..` に属性を
    // 付けられないので、cfg でない属性も落ちていた）。
    for (src, rest) in [
        (
            "pub struct S { pub x: u32, pub y: u32 }\npub fn p(s: S) -> u32 { let S { x, #[allow(unused)] .. } = s; x }",
            "..",
        ),
        (
            "pub struct S { pub x: u32, pub y: u32 }\npub fn p(s: S) -> u32 { match s { S { x, #[r#cfg(any())] .. } => x } }",
            "..",
        ),
        (
            "pub struct B { } owns { items: Region<Ptr<u64>> }\npub fn k(b: B) -> u32 { let B { #[cfg(test)] @.. } = b; 0 }",
            "@..",
        ),
        (
            "pub struct B { } owns { items: Region<Ptr<u64>> }\npub fn k(b: B) -> u32 { let B { #[allow(unused)] @.. } = b; 0 }",
            "@..",
        ),
    ] {
        rejects(
            src,
            "E0001",
            &format!("an attribute cannot be written on `{rest}` in a struct pattern"),
        );
    }
    for src in [
        "pub fn f<#[cfg(any())] T>(a: u32) -> u32 { a }",
        "pub fn f<#[r#cfg(any())] 'a>(a: u32) -> u32 { a }",
        "pub struct T<#[cfg_attr(all(), cfg(any()))] A> { a: u32 }",
        "pub struct T { a: u32 }\nimpl T { pub fn m<#[cfg(test)] const N: usize>(&self) {} }",
        "pub fn o() { fn inner<#[cfg(test)] T>() {} }",
    ] {
        rejects(src, "E0003", "`#[cfg(..)]` on a generic parameter");
    }
    // 診断は cfg の属性を指す（前に cfg でない属性があっても）。
    let (_, lines) = run("pub fn f<#[allow(unused)] #[cfg(any())] T>(a: u32) -> u32 { a }");
    assert!(
        lines.iter().any(|l| l.contains(":1:27: E0003")),
        "{lines:?}"
    );
    // Ebykn が中を読まずに Rust のまま渡す項目（`union`、trait の `type`）のジェネリクスの引数は、
    // rustc が verify と素のビルドで同じに評価するので通す（現行仕様 §5「マクロ」）。
    accepts("pub union U<#[cfg(any())] T> { a: u32 }");
    accepts("pub trait Tr { type A<#[cfg(any())] T>; }");
    // fn の引数と `let` の cfg は、拒否リストの名前でなければ通す（#27。書き下しに残す）。
    accepts("pub fn n(#[r#cfg(test)] x: u32) {}");
    accepts("pub fn o() { #[r#cfg(test)] let y = 1; }");
    // 拒否リストの名前の cfg は、拒否リストの E0001 だけを出す（同じ属性に E0003 を重ねない）。
    for src in [
        "pub fn g() -> u32 { let c = |#[cfg(verus_keep_ghost)] a: u32, b: u32| b; c(1) }",
        "pub struct S { pub x: u32 }\npub fn p(s: S) -> u32 { match s { S { #[cfg(not(verus_keep_ghost))] x: 0, .. } => 1, _ => 2 } }",
        "pub fn f<#[cfg(verus_keep_ghost)] T>(a: u32) -> u32 { a }",
    ] {
        rejects(
            src,
            "E0001",
            "because its condition uses `verus_keep_ghost`",
        );
    }
    // cfg でない属性は通す（書き下しに残す。`crates/ebykn_lower/tests/rules.rs`）。
    accepts("pub fn f<#[allow(unused)] T>(a: u32) -> u32 { a }");
    accepts("pub fn g() -> u32 { let c = |#[allow(unused)] a: u32| a; c(1) }");
    accepts(
        "pub struct S { pub x: u32 }\npub fn h(s: S) -> u32 { let S { #[allow(unused)] x } = s; x }",
    );
}

/// コンパイル時の環境変数と `file!` は、Verus の実行と素のビルドで値が違いうるので、`unchecked fn` の
/// 本体の外ではどこに書いても拒否する（PR #26 の 4 回目の確認、#27 の拒否リスト）。
#[test]
fn env_and_file_macros_outside_unchecked_fn_are_denied() {
    for src in [
        "pub fn h8() -> (r: u32) ensures r == 0, { if option_env!(\"CARGO_TERM_COLOR\").is_some() { 0 } else { 1 } }",
        "pub const C: Option<&str> = option_env!(\"X\");",
        "unchecked fn u() { fn inner() -> bool { option_env!(\"X\").is_some() } }",
    ] {
        rejects(src, "E0001", "`option_env!` cannot be written in `.eyk`");
    }
    rejects(
        "pub static D: &str = std::env!(\"CARGO_PKG_NAME\");",
        "E0001",
        "`env!` cannot be written in `.eyk`",
    );
    rejects(
        "pub fn n() -> usize { core::file!().len() }",
        "E0001",
        "`file!` cannot be written in `.eyk`",
    );
    // `unchecked fn` の本体では許す（証明なしの前提として verify が一覧に出す）。
    accepts("unchecked fn u() -> (r: bool) { option_env!(\"X\").is_some() }");
    accepts("unchecked fn u() -> usize { let v = vec![env!(\"X\")]; v.len() + file!().len() }");
}

/// `put t@nodes[..]` の診断（#84、`Message::ErasedRegion`）が勧める 3 つの書き方は、どれも検査を通る
/// （PR #113 のレビュー）。新しい値は空の領域から作り、所有する値はすべてのフィールドを束縛して分解して
/// 作り直し、参照は型のメソッドで `self@nodes` を使う。既にある値に `Region::empty()` を当てると前の
/// 領域を捨てる（#108）ので、新しい値のときだけ勧める。
#[test]
fn erased_region_guidance_forms_pass_check() {
    let src = format!(
        "{PRELUDE}
impl Pool {{
    pub fn make(v: u64) -> Self {{
        let p = Ptr::new(Node {{ val: v }});
        let mut nodes: Region<Ptr<Node>> = Region::empty();
        put nodes[p];
        Pool {{ len: 1, @nodes: nodes, @order: Seq::empty() }}
    }}
    pub fn add(&mut self, v: u64) {{
        let p = Ptr::new(Node {{ val: v }});
        put self@nodes[p];
    }}
}}
pub fn grow(t: Pool, v: u64) -> Pool {{
    let Pool {{ len, @nodes, @order }} = t;
    let p = Ptr::new(Node {{ val: v }});
    put nodes[p];
    Pool {{ len: len, @nodes: nodes, @order: order }}
}}
"
    );
    let (_, lines) = run(&src);
    assert!(lines.is_empty(), "{lines:?}");
}
