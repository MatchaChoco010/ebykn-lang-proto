//! どの属性を cfg とみなすか（#155）。cfg とみなすのは、述語の位置（`cfg(..)` の述語、`cfg_attr` の条件、
//! `cfg_attr` の並びの中の `cfg(..)` / `cfg_attr(..)`）に cfg があるときだけ。`cfg_attr` の並びの中の
//! ほかの属性の引数（`allow(cfg)` の lint の名前）は述語ではないので、cfg とみなさない。判定は拒否リストの
//! 判定と同じ走査（`ebykn_syntax::denylist::for_each_cfg_pred`）で行う。

mod support;
use support::*;

/// cfg の属性を E0003 にし、cfg でない属性は受理する位置。`$A` を属性に置き換える。
const POSITIONS: &[&str] = &[
    // 構造体パターンの欄（#95。Issue #155 の再現）
    "pub struct S { pub x: u32 }\npub fn f(s: S) -> u32 { let S { $A x, .. } = s; x }",
    // ジェネリクスの引数（#95）
    "pub fn f<$A T>(a: u32) -> u32 { a }",
    // 権限を消費する文（PR #59 のレビューの S1）
    "{PRELUDE}pub fn f() -> u64 { let q = Ptr::new(Node { val: 3 }); let x = (*q).val; $A let n = q.free(); x }",
];

/// cfg とみなす属性（述語の位置に cfg がある）。
const CFG: &[&str] = &[
    "#[cfg(test)]",
    "#[r#cfg(test)]",
    "#[cfg_attr(test, cfg(any()))]",
    "#[r#cfg_attr(test, cfg(any()))]",
    "#[cfg_attr(test, r#cfg(any()))]",
    // 並びの 2 つ目以降
    "#[cfg_attr(test, allow(unused), cfg(any()))]",
    "#[cfg_attr(test, allow(cfg), cfg(any()))]",
    // 入れ子の `cfg_attr`
    "#[cfg_attr(test, cfg_attr(unix, cfg(any())))]",
    "#[cfg_attr(test, r#cfg_attr(unix, r#cfg(any())))]",
    // 引数の群のない `cfg`（rustc は拒否する。cfg とみなす側に倒す）
    "#[cfg_attr(test, cfg)]",
    "#[cfg_attr(test, cfg = \"x\")]",
    // `cfg_attr` の条件の中の識別子 `cfg`（rustc は拒否する。cfg とみなす側に倒す）
    "#[cfg_attr(all(cfg), allow(unused))]",
    "#[cfg_attr(test, cfg_attr(any(r#cfg), allow(unused)))]",
];

/// cfg とみなさない属性（述語の位置に cfg がない）。
const NOT_CFG: &[&str] = &[
    "#[allow(unused)]",
    "#[cfg_attr(test, allow(unused))]",
    // lint の名前の並びの `cfg`（Issue #155）
    "#[cfg_attr(test, allow(cfg))]",
    "#[cfg_attr(test, allow(r#cfg))]",
    "#[cfg_attr(test, allow(unused, cfg))]",
    "#[cfg_attr(test, allow(cfg), allow(unused))]",
    "#[cfg_attr(test, cfg_attr(unix, allow(cfg)))]",
    "#[r#cfg_attr(test, allow(cfg))]",
];

const WANT: &str = "E0003";

fn src(pos: &str, attr: &str) -> String {
    pos.replace("{PRELUDE}", PRELUDE).replace("$A", attr)
}

#[test]
fn attributes_with_cfg_in_a_predicate_position_are_e0003() {
    for pos in POSITIONS {
        for attr in CFG {
            let s = src(pos, attr);
            assert!(
                codes(&s).iter().any(|c| c == WANT),
                "cfg とみなすべき属性が通った:\n{s}"
            );
        }
    }
}

#[test]
fn cfg_in_other_attribute_arguments_is_not_a_cfg() {
    for pos in POSITIONS {
        for attr in NOT_CFG {
            accepts(&src(pos, attr));
        }
        accepts(&src(pos, ""));
    }
}
