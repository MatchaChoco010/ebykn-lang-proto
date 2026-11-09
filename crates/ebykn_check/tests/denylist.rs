//! 拒否リスト（`crates/ebykn_syntax/src/denylist.rs`、#27）の総当たり。表の項目を足すと、各テストが
//! その項目にも自動で効く。
//!
//! どの入力も、ちょうど一つの診断（拒否リストの E0001）が出ることを確かめる（`support::rejects`）。

mod support;
use support::*;

use ebykn_syntax::denylist::{DENYLIST, DenyKind, Entry};

/// 項目の診断に必ず含まれる文言。
fn needle(e: &Entry) -> String {
    match e.kind {
        DenyKind::Cfg => format!("because its condition uses `{}`", e.name),
        DenyKind::Macro { .. } => format!("`{}!` cannot be written in `.eyk`", e.name),
    }
}

fn cfgs() -> impl Iterator<Item = &'static Entry> {
    DENYLIST.iter().filter(|e| e.kind == DenyKind::Cfg)
}

fn macros() -> impl Iterator<Item = &'static Entry> {
    DENYLIST
        .iter()
        .filter(|e| matches!(e.kind, DenyKind::Macro { .. }))
}

/// cfg の述語を書ける位置。`P` を述語に置き換える。
const CFG_POSITIONS: &[&str] = &[
    // 項目
    "#[cfg(P)]\npub fn f() {}",
    "#[cfg(not(P))]\npub fn f() {}",
    "#[cfg(all(test, not(any(P, feature = \"x\"))))]\npub fn f() {}",
    "pub mod m { #![cfg(P)] pub fn f() {} }",
    "pub struct S { #[cfg(P)] pub a: u32 }",
    // cfg_attr の条件と、中の cfg
    "#[cfg_attr(P, inline)]\npub fn f() {}",
    "#[cfg_attr(all(), cfg(P))]\npub fn f() {}",
    "#[cfg_attr(all(), cfg_attr(any(), cfg(P)))]\npub fn f() {}",
    // 本体
    "pub fn f() -> bool { cfg!(P) }",
    "pub fn f() -> bool { std::cfg!(P) }",
    "pub fn f() -> bool { core::cfg!(not(P)) }",
    "pub fn f() -> u32 { let mut x = 0u32; #[cfg(P)] { x = 1; } x }",
    "pub fn f() -> u32 { #[cfg(P)] let y = 1u32; 0 }",
    "pub fn f(x: u32) -> u32 { match x { #[cfg(P)] 0 => 1, _ => 2 } }",
    "pub fn f(#[cfg(P)] x: u32) {}",
    "pub fn f() { struct T { #[cfg(P)] a: u32, b: u32 } }",
    "pub const C: bool = cfg!(P);",
    // ジェネリクスの引数、クロージャの引数、構造体パターンの欄（#95。cfg は E0003 なので cfg_attr の条件で書く）
    "pub fn f<#[cfg_attr(P, allow(unused))] T>(a: u32) -> u32 { a }",
    "pub fn f() -> u32 { let c = |#[cfg_attr(P, allow(unused))] a: u32| a; c(1) }",
    "pub struct S { pub x: u32 }\npub fn f(s: S) -> u32 { let S { #[cfg_attr(P, allow(unused))] x } = s; x }",
    // マクロの引数の中
    "pub fn f() -> usize { let v: Vec<u32> = vec![#[cfg(P)] 1u32, 0]; v.len() }",
    "pub fn f() { assert!(cfg!(P)); }",
    "pub fn f() { assert!({ #[cfg(P)] let y = 1u32; true }); }",
    // unchecked fn の本体（cfg は例外にしない）
    "unchecked fn f() -> bool { cfg!(P) }",
];

/// cfg そのものが E0003 になる位置（#95）。拒否リストの名前なら、E0003 を重ねず拒否リストの E0001 だけを出す。
const CFG_E0003_POSITIONS: &[&str] = &[
    "pub fn f<#[cfg(P)] T>(a: u32) -> u32 { a }",
    "pub struct T<#[cfg_attr(all(), cfg(not(P)))] A> { a: u32 }",
    "pub fn f() -> u32 { let c = |#[cfg(P)] a: u32, b: u32| b; c(1) }",
    "pub struct S { pub x: u32 }\npub fn f(s: S) -> u32 { match s { S { #[cfg(not(P))] x: 0, .. } => 1, _ => 2 } }",
    "pub struct B { } owns { items: Region<Ptr<u64>> }\npub fn k(b: B) -> B { let B { #[cfg(P)] @items } = b; B { @items: items } }",
];

/// 拒否リストの名前を、cfg の述語でない所（lint の名前）に書いた属性。トークンの検査は E0001 にしないので、
/// 同じ並びの `#[cfg(..)]` の E0003 を止めない（PR #59 の 3 回目の再確認の B1）。`L` を名前に置き換える。
const LINT_NEIGHBOR_POSITIONS: &[&str] = &[
    "pub struct S { pub x: u32, pub y: u32 }\npub fn m(s: S) -> (r: u32) ensures s.x != 0 ==> r == 2, { match s { S { #[cfg_attr(all(), allow(L))] #[cfg(any())] x: 0, .. } => 1, _ => 2 } }",
    "pub struct S { pub x: u32, pub y: u32 }\npub fn m(s: S) -> (r: u32) ensures s.x != 0 ==> r == 2, { match s { S { #[cfg_attr(all(), cfg(any()), allow(L))] x: 0, .. } => 1, _ => 2 } }",
    "pub fn f<#[cfg_attr(all(), allow(L))] #[cfg(any())] T>(a: u32) -> u32 { a }",
    "pub fn f<#[cfg_attr(all(), cfg(any()), allow(L))] T>(a: u32) -> u32 { a }",
    "pub fn f() -> u32 { let c = |#[cfg_attr(all(), allow(L))] #[cfg(any())] a: u32| 7u32; c(1) }",
];

#[test]
fn lint_names_do_not_hide_the_e0003() {
    for e in cfgs() {
        for pos in LINT_NEIGHBOR_POSITIONS {
            let src = pos.replace('L', e.name);
            let (_, lines) = run(&src);
            assert_eq!(lines.len(), 1, "{}\n---\n{src}", lines.join("\n"));
            assert!(lines[0].contains(": E0003: "), "{}\n---\n{src}", lines[0]);
        }
    }
}

#[test]
fn every_entry_rejects_its_example() {
    for e in DENYLIST {
        rejects(e.example, "E0001", &needle(e));
    }
}

#[test]
fn denied_cfg_names_are_rejected_everywhere() {
    for e in cfgs() {
        for pos in CFG_POSITIONS {
            rejects(&pos.replace('P', e.name), "E0001", &needle(e));
        }
        for pos in CFG_E0003_POSITIONS {
            rejects(&pos.replace('P', e.name), "E0001", &needle(e));
        }
        // `r#` を付けた綴りも同じ。
        rejects(
            &format!("pub fn f() -> bool {{ cfg!(r#{}) }}", e.name),
            "E0001",
            &needle(e),
        );
    }
}

#[test]
fn other_cfgs_are_accepted_everywhere() {
    for p in [
        "test",
        "unix",
        "feature = \"verus_keep_ghost\"",
        "target_os = \"linux\"",
        "all(test, not(feature = \"debug_assertions\"))",
    ] {
        for pos in CFG_POSITIONS {
            accepts(&pos.replace('P', p));
        }
    }
}

/// マクロを書ける位置。`M` をマクロの名前に置き換える。
const MACRO_POSITIONS: &[&str] = &[
    "pub fn f() -> usize { M!(\"X\").len() }",
    "pub fn f() -> usize { std::M!(\"X\").len() }",
    "pub fn f() -> usize { core::M!(\"X\").len() }",
    "pub const C: &str = M!(\"X\");",
    "pub static D: &str = M!(\"X\");",
    "#[doc = M!(\"X\")]\npub fn f() {}",
    "pub fn f() -> usize { let v = vec![M!(\"X\")]; v.len() }",
    "pub fn f() -> usize { concat!(M!(\"X\")).len() }",
    "pub struct S { a: u32 }\nimpl S { pub fn m(&self) -> usize { M!(\"X\").len() } }",
    // unchecked fn の中の入れ子の fn は、自分の unchecked で決め直す。
    "unchecked fn u() { fn inner() -> usize { M!(\"X\").len() } }",
];

#[test]
fn denied_macros_are_rejected_everywhere() {
    for e in macros() {
        for pos in MACRO_POSITIONS {
            rejects(&pos.replace('M', e.name), "E0001", &needle(e));
        }
    }
}

#[test]
fn unchecked_bodies_allow_only_the_marked_macros() {
    for e in macros() {
        let src = format!("unchecked fn u() -> usize {{ {}!(\"X\").len() }}", e.name);
        match e.kind {
            DenyKind::Macro { unchecked_ok: true } => accepts(&src),
            _ => rejects(&src, "E0001", &needle(e)),
        }
    }
}

#[test]
fn similar_macros_are_accepted() {
    // 素のビルドと Verus で同じ値になるもの（`docs/current-spec.md`）。
    accepts("pub fn f() -> u32 { line!() + column!() }");
    accepts("pub fn f() { assert!(1 == 1); assert_eq!(1, 1); assert_ne!(1, 2); }");
    accepts("pub fn f() -> usize { module_path!().len() }");
    // `env` という名前の変数と `!=`。
    accepts("pub fn e() -> bool { let env = 1u32; env != 2 }");
    // 文字列の中は見ない。
    accepts("pub fn f() { assert!(\"#[cfg(verus_keep_ghost)] env!()\".len() == 31); }");
}
