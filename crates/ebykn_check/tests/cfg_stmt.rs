//! 文の cfg（PR #59 のレビューの B1・S1）。§6 の検査は cfg を評価しないので、代入の文、権限の状態を変える
//! 文、発散する文に付いた `#[cfg]` は、検査が見るコードとコンパイルされるコードを食い違わせる。どれも E0003
//! にする。`take` / `put` / `leak` / 仕様の代入は属性を書き下さず、代入の文の属性は安定版の Rust で書けない
//! ので、どの属性も E0003 にする。
//!
//! 受理する位置で属性が書き下しに残ることは、`crates/ebykn_lower/tests/cfg_attrs.rs` が確かめる。

mod support;
use support::*;

const WANT: &str = "is not supported yet, and neither is any attribute on an assignment, binary operation or cast statement";

/// E0003 になる位置。`$A` を属性に置き換える。
const POSITIONS: &[&str] = &[
    // 代入（消えると Verus が「removing an expression is not supported」で拒否する）
    "pub fn f() -> u32 { let mut x = 0u32; $A x = 1; x }",
    "pub fn f() -> u32 { let mut x = 0u32; $A x += 1; x }",
    // `*p` への書き込み（PR #59 のレビュー：書き下しが属性を落としていた）
    "{PRELUDE}pub fn f() -> u64 { let q = Ptr::new(Node { val: 1 }); $A (*q).val = 5; let x = (*q).val; let n = q.free(); x }",
    "{PRELUDE}pub fn f() -> u64 { let q = Ptr::new(Node { val: 1 }); $A *q = Node { val: 5 }; let x = (*q).val; let n = q.free(); x }",
    // 権限を消費する・返す文（S1：消えると、§6 が見ないまま権限を捨てる）
    "{PRELUDE}pub fn f() -> u64 { let q = Ptr::new(Node { val: 3 }); let x = (*q).val; $A let n = q.free(); x }",
    "{PRELUDE}fn eat(p: Ptr<Node>) uses p@ { leak p@; }\npub fn f() { let q = Ptr::new(Node { val: 3 }); $A eat(q); }",
    "{PRELUDE}pub fn f() { let q = Ptr::new(Node { val: 3 }); $A { let n = q.free(); } }",
    // take / put / leak / 仕様の代入（属性を書き下さない）
    "{PRELUDE}pub fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> requires old(r).contains(p), ensures final(r).contains(p), { take r[p]; $A put r[p]; }",
    "{PRELUDE}pub fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> requires old(r).contains(p), ensures final(r).contains(p), { $A take r[p]; put r[p]; }",
    "{PRELUDE}pub fn f() { let q = Ptr::new(Node { val: 3 }); $A leak q@; }",
    "{PRELUDE}impl Pool { pub fn f(&mut self) { $A self@len = 0; } }",
    // 発散する文（消えると、§6 が到達しないと見た後ろのコードが実行される）
    "pub fn f() -> u32 { $A return 1; 0 }",
    "{PRELUDE}impl Pool { pub fn f(&mut self, p: Ptr<Node>) requires old(self)@nodes.contains(p), { borrow mut self@nodes[p] { $A return; (*p).val = 0; } } }",
    // 二項演算と型変換の文（構文解析器が文の属性を左の項に移し、安定版の Rust で書けない。再レビューの S1）
    "pub fn f(a: u32) -> u32 { $A a == 0; a }",
    "pub fn f(a: u32) -> u64 { $A a as u64; a as u64 }",
];

/// 文でない位置の式と欄（再レビューの X1）。cfg で消えると、権限の操作や発散を §6 が見ない。
const EXPR_POSITIONS: &[&str] = &[
    "{PRELUDE}pub struct Q { pub a: u64, pub b: u64 }\npub fn f() -> Q { let q = Ptr::new(Node { val: 3 }); Q { a: 1, $A b: q.free().val } }",
    "{PRELUDE}pub fn f() -> u64 { let q = Ptr::new(Node { val: 3 }); let a = [$A q.free().val, 1u64]; a[1] }",
    "{PRELUDE}pub fn f() -> u64 { let q = Ptr::new(Node { val: 3 }); let t = ($A q.free().val, 1u64); 1 }",
    "{PRELUDE}pub struct T(pub u64, pub u64);\npub fn f() -> T { let q = Ptr::new(Node { val: 3 }); T($A q.free().val, 1) }",
    "{PRELUDE}pub fn f(v: &mut Vec<u64>) { let q = Ptr::new(Node { val: 3 }); v.push($A q.free().val); }",
    "{PRELUDE}pub struct Q2 { pub a: u64, pub b: u64 }\npub fn f() -> u64 { let p = Q2 { a: 1, $A b: return 1 }; p.a }",
];

/// cfg の属性の綴り（raw の `r#cfg` / `r#cfg_attr` を含む。再レビューの X2）。
const CFG_ATTRS: &[&str] = &[
    "#[cfg(test)]",
    "#[cfg(not(test))]",
    "#[cfg(any())]",
    "#[cfg_attr(test, cfg(any()))]",
    "#[r#cfg(any())]",
    "#[cfg_attr(all(), r#cfg(any()))]",
    "#[r#cfg_attr(all(), cfg(any()))]",
];

/// 代入と `take` / `put` / `leak` / 仕様の代入は、cfg でない属性も拒否する。
const ANY_ATTR: &[usize] = &[0, 1, 2, 3, 7, 8, 9, 10, 13, 14];

fn src(pos: &str, attr: &str) -> String {
    pos.replace("{PRELUDE}", PRELUDE).replace("$A", attr)
}

#[test]
fn cfg_on_these_statements_is_e0003() {
    for attr in CFG_ATTRS {
        for pos in POSITIONS.iter().chain(EXPR_POSITIONS) {
            rejects(&src(pos, attr), "E0003", WANT);
        }
    }
}

#[test]
fn other_attributes_follow_the_position() {
    for (i, pos) in POSITIONS.iter().enumerate() {
        let s = src(pos, "#[allow(unused)]");
        if ANY_ATTR.contains(&i) {
            rejects(&s, "E0003", WANT);
        } else {
            accepts(&s);
        }
    }
}

#[test]
fn the_same_statements_without_attributes_are_accepted() {
    for pos in POSITIONS.iter().chain(EXPR_POSITIONS) {
        accepts(&src(pos, ""));
    }
}

#[test]
fn cfg_on_statements_that_keep_the_permissions_is_accepted() {
    for s in [
        "fn side(x: &mut u32) { *x = 7; }\npub fn f() -> u32 { let mut x = 0u32; #[cfg(test)] side(&mut x); x }",
        "pub fn f() -> u32 { let mut x = 0u32; #[cfg(test)] { x = 1; } x }",
        "pub fn f(v: u32) -> u32 { #[cfg(test)] if v > 0 { return 1; } 0 }",
        "pub fn f() -> u32 { #[cfg(test)] let y = 1u32; 0 }",
        &format!(
            "{PRELUDE}pub fn f() -> u64 {{ let q = Ptr::new(Node {{ val: 1 }}); #[cfg(test)] let y = (*q).val; let n = q.free(); 0 }}"
        ),
        &format!(
            "{PRELUDE}impl Pool {{ pub fn f(&mut self, p: Ptr<Node>) requires old(self)@nodes.contains(p), {{ #[cfg(test)] borrow mut self@nodes[p] {{ (*p).val = 0; }} }} }}"
        ),
    ] {
        accepts(s);
    }
}

/// 安定版の Rust が式の属性を許さない位置（PR #59 の再確認の S1）。Verus の toolchain では通るが、Verus なしの
/// ビルドが E0658（`attributes on expressions are experimental`）で落ちる。
const EXPR_ATTR: &str =
    "an attribute on this expression is not supported, because stable Rust accepts";

const UNSTABLE_POSITIONS: &[&str] = &[
    "pub fn f(a: u32) -> u32 { let x = $A a; x }",
    "pub fn f(a: u32) -> u32 { if $A (a >= 0) { 1 } else { 0 } }",
    "pub fn f(a: u32) -> u32 { return $A a; }",
    "pub fn f(a: u32) -> u32 { match $A a { _ => 1 } }",
    "pub fn f(a: u32) -> u32 { let y = ($A a) + 1; y }",
    "pub fn f(a: u32) -> u32 { match a { _ => $A a } }",
    "pub struct S { pub v: u32 }\npub fn f(a: u32) -> S { S { v: $A a } }",
    "pub fn f(a: u32) -> u32 { let c = || $A a; c() }",
    // `ident_of` / `deref_root` で名前だけを取る受け手・被演算子・左辺（PR #59 の再確認の 2 回目）。
    "pub fn f(v: Vec<u32>) -> usize { ($A v).len() }",
    "{PRELUDE}pub fn f() -> u64 { let q = Ptr::new(Node { val: 3 }); let n = ($A q).free(); 1 }",
    "pub fn f(r: &u32) -> u32 { *($A r) }",
    "pub fn f(r: &mut u32) { *($A r) = 1; }",
    "pub fn f() -> u32 { let mut a = 0u32; ($A a) = 1; a }",
    "pub struct S { pub v: u32 }\npub fn f(s: &mut S) -> u32 { (*($A s)).v = 1; s.v }",
    "pub fn f(r: &mut u32) -> u32 { let m = &mut *($A r); *m }",
    // 許す位置でも、二項演算・型変換で始まる式の属性は左の項に付く。
    "pub fn f(a: u32) -> [u32; 2] { [$A a + 1, 1] }",
    "pub fn f(a: u32) -> (u32, u32) { ($A a + 1, 1) }",
    "fn g(x: u32) -> u32 { x }\npub fn f(a: u32) -> u32 { g($A a + 1) }",
    "pub fn f(a: u32) -> [u64; 1] { [$A a as u64] }",
    // 範囲の式は、許す位置でも属性を受けない（#130。`..b` の属性は構文解析器が E0001 にする）。
    "pub fn f(a: u32) -> u32 { $A 0..a; a }",
    "pub fn f(a: u32) -> u32 { $A a..; a }",
    "pub fn f(a: u32) -> u32 { let r = { $A 0..=a }; a }",
    "pub fn f(a: u32) -> usize { let r = [$A 0..a]; r.len() }",
    "fn g<T>(x: T) -> T { x }\npub fn f(a: u32) -> u32 { let r = g($A 0..a); a }",
];

/// 安定版の Rust が式の属性を許す位置（文の先頭、ブロックの末尾、配列とタプルの要素、呼び出しとメソッドの引数）。
const STABLE_POSITIONS: &[&str] = &[
    "pub fn f(a: u32) -> u32 { $A { a } }",
    "pub fn f(a: u32) -> u32 { let x = a; $A x }",
    "pub fn f(a: u32) -> [u32; 2] { [$A a, 1] }",
    "pub fn f(a: u32) -> (u32, u32) { ($A a, 1) }",
    "fn side(x: &mut u32, y: u32) { *x = y; }\npub fn f(a: u32) -> u32 { let mut x = 0u32; side(&mut x, $A a); x }",
    "pub fn f(a: u32) -> usize { let mut v: Vec<u32> = Vec::new(); v.push($A a); v.len() }",
    "pub fn f(v: Vec<u32>) -> usize { $A v.len(); v.len() }",
    "pub fn f(r: &u32) -> u32 { $A *r; *r }",
    "pub fn f() -> u32 { let mut a = 0u32; let mut b = 0u32; ($A a, b) = (1, 2); a + b }",
];

#[test]
fn expression_attributes_follow_stable_rust() {
    for attr in ["#[allow(unused)]", "#[cfg(not(test))]"] {
        for pos in UNSTABLE_POSITIONS {
            rejects(&src(pos, attr), "E0003", EXPR_ATTR);
        }
        for pos in STABLE_POSITIONS {
            accepts(&src(pos, attr));
        }
    }
    for pos in UNSTABLE_POSITIONS.iter().chain(STABLE_POSITIONS) {
        accepts(&src(pos, ""));
    }
}

/// std のマクロの引数の中の属性の位置（PR #59 の再確認）。引数はトークンのまま持つので、位置をトークンで決める。
const MACRO_ATTR: &str =
    "an attribute at this position inside the arguments of this macro is not supported";

const MACRO_UNSTABLE_POSITIONS: &[&str] = &[
    "pub fn f(a: u32) -> u32 { assert!($A (a >= 0)); a }",
    "pub fn f(a: u32) -> u32 { assert_eq!($A a, a); a }",
    "pub fn f(a: u32) -> u32 { assert!(matches!($A a, 1)); a }",
    "pub fn f(a: u32) -> u32 { assert!({ let x = $A a; x == a }); a }",
    "pub fn f(a: u32) -> u32 { assert!(match a { _ => $A true }); a }",
    "pub fn f(a: u32) -> u32 { assert!(vec![a].len() == $A 1); a }",
    // 許す位置でも、続く式が代入・二項演算・型変換・範囲なら、構文解析器が属性を左の項に移す（#130）
    "pub fn f(a: u32) -> u32 { assert!({ let mut x = 0u32; $A x = 1; x == 1 }); a }",
    "pub fn f(a: u32) -> u32 { assert!({ $A a == a }); a }",
    "pub fn f(a: u32) -> usize { let v = vec![$A a + 1]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![1, $A a as u64]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![1u8; $A a as usize]; v.len() }",
    "pub fn f(a: u32) -> u32 { assert!({ $A 0..a; true }); a }",
    "pub fn f(a: u32) -> u32 { assert!({ $A ..a; true }); a }",
    "pub fn f(a: u32) -> u32 { assert!({ $A a.clone() + 1 } == a + 1); a }",
    "pub fn f(a: u32) -> u32 { assert!({ $A match a { _ => a }.clone() + 1 } == a + 1); a }",
];

const MACRO_STABLE_POSITIONS: &[&str] = &[
    "pub fn f(a: u32) -> usize { let v = vec![$A a, 1]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![1, $A a]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![$A a; 2]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![a; $A 2]; v.len() }",
    "pub fn f(a: u32) -> u32 { assert!(vec![$A a].len() == 1); a }",
    "pub fn f(a: u32) -> u32 { assert!({ $A let x = a; x == a }); a }",
    "pub fn f(a: u32) -> u32 { assert!({ let y = 1u32; $A let x = a; x == a }); a }",
    "pub fn f(a: u32) -> u32 { assert!({ $A { a == a } }); a }",
    "pub fn f(a: u32) -> u32 { assert!(match a { $A 0 => true, $A _ => true }); a }",
    // 続く式が属性をそのまま受ける形（#130）
    "pub fn f(a: u32) -> usize { let v = vec![$A a.clone(), $A *&a]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![$A (a + 1), $A u32::max(a, 1)]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![$A if a == 1 { 1 } else { a }]; v.len() }",
    "pub fn f(a: u32) -> usize { let v = vec![$A <u32 as Clone>::clone(&a)]; v.len() }",
    "pub fn f(a: u32) -> u32 { assert!({ $A if a == 1 { } a == a }); a }",
    "pub fn f(a: u32) -> u32 { assert!(match a { $A 0 | 1 => a + 1 == 2, _ => true }); a }",
    "pub fn f(a: u32) -> u32 { assert!({ $A let y = a + 1; y > a }); a }",
];

#[test]
fn attributes_in_macro_arguments_follow_stable_rust() {
    for attr in ["#[allow(unused)]", "#[cfg(not(test))]"] {
        for pos in MACRO_UNSTABLE_POSITIONS {
            rejects(&src(pos, attr), "E0003", MACRO_ATTR);
        }
        for pos in MACRO_STABLE_POSITIONS {
            accepts(&src(pos, attr));
        }
    }
    for pos in MACRO_UNSTABLE_POSITIONS
        .iter()
        .chain(MACRO_STABLE_POSITIONS)
    {
        accepts(&src(pos, ""));
    }
    // 内側の属性はどこでも拒否する（`{ #![..] .. }` はブロック式の属性になり、安定版の Rust で E0658）。
    for s in [
        "pub fn f() { assert!({ #![allow(unused)] #[allow(unused)] let y = 1; true }); }",
        "pub fn f() { assert!({ let y = 1; #![allow(unused)] true }); }",
    ] {
        rejects(s, "E0003", MACRO_ATTR);
    }
}
