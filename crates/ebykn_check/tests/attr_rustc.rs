//! 式と文の属性の位置を、安定版の rustc と突き合わせる（#130 の再発防止）。
//!
//! 安定版の Rust は、式の属性を決まった位置でしか許さない（ほかは `attributes on expressions are
//! experimental`）。Verus の toolchain は許すので、Ebykn が通すと verified になり、書き下した `build/verus/`
//! を Verus なしの cargo でビルドすると落ちる。Ebykn は、本体の式を Walker が AST で（`body/attrs.rs`）、
//! std のマクロの引数をトークンで（`Unimpl::AttrPosInMacro`）見る。
//!
//! 位置と式の形を総当たりで組み、Ebykn が属性を許したものを、安定版の rustc（`rust-toolchain.toml` の版）が
//! 属性の誤りなしで受け付けることを確かめる。Ebykn が拒否して rustc が受け付ける形（安全側の拒否）は、
//! このテストの対象でない。

mod support;

use std::collections::BTreeMap;
use std::process::Command;

/// 属性の位置を拒否する Ebykn の診断の文言（`Unimpl::AttrPosInMacro`、`Unimpl::ExprAttr`、
/// `Unimpl::CfgStmt`）と、構文解析器の E0001（`..` で始まる範囲の属性、ループの後ろに続く `.` など）。
const ATTR_DIAGS: &[&str] = &[
    "E0001: syntax error",
    "an attribute at this position inside the arguments of this macro is not supported",
    "an attribute on this expression is not supported",
    "neither is any attribute on an assignment, binary operation or cast statement",
];

/// 属性の位置。`$A` を属性に、`$E` を式に置き換える。どれも、Ebykn が属性を許しうる位置。
const CONTEXTS: &[&str] = &[
    // 本体（Walker が AST で見る）：配列とタプルの要素、呼び出しの引数、文の頭、ブロックの末尾、欄の頭
    "[$A $E]",
    "($A $E, 1)",
    "h($A $E)",
    "{ $A $E; 1u8 }",
    "{ let y = 0u8; $A $E; y }",
    "{ $A $E }",
    "{ if b {} $A $E }",
    "S { $A v: $E }",
    // std のマクロの引数（トークンで見る）：`vec!` の要素と、`{ .. }` の中の文・末尾・欄の頭
    "vec![$A $E]",
    "vec![$A $E; 2]",
    "vec![{ $A $E; 1u8 }]",
    "vec![{ let y = 0u8; $A $E; y }]",
    "vec![{ $A $E }]",
    "vec![{ let y = 0u8; $A $E }]",
    "vec![{ if b {} $A $E }]",
    "vec![1u8; { $A $E; 2 }]",
    "vec![S { $A v: $E }]",
    "vec![{ $A let y = $E; 1u8 }]",
    // 波括弧で呼んだ入れ子の std のマクロ（`{ .. }` はブロックでない。PR #140 のレビュー）
    "vec![vec!{$A $E}]",
    "vec![{ vec!{$A $E}; 1u8 }]",
    "vec![format!{\"{:?}\", $A $E}]",
    // where 句の述語（`名前: 型` が欄と同じに見える。PR #140 の再確認。式の形によらず属性の位置で決まる）
    "vec![{ fn q<T, U>() where T: Clone, $A U: Copy {} $E }]",
];

/// 式の形。`a: u32`、`b: bool`、`s: S<u32>`、`let mut x = 0u32` が使える。
const FORMS: &[&str] = &[
    "a",
    "1",
    "x",
    "a + 1",
    "a * 2 + 1",
    "a == a",
    "a < 1",
    "a >> 1",
    "b && b",
    "b || b",
    "a as u64",
    "(a + 1)",
    "a.clone()",
    "a.clone() + 1",
    "a.clone() as u64",
    "g(a)",
    "g(a) + 1",
    "-(a as i64)",
    "-(a as i64) + 1",
    "!b",
    "!b & b",
    "!a as u64",
    "*&a",
    "*&a + 1",
    "&a",
    "x = 1",
    "x += 1",
    "{ a }",
    "{ a }.clone()",
    "{ a }.clone() + 1",
    "if b { 1 } else { a }",
    "if b { a } else if b { a } else { a }",
    "if b { a } else { a }.clone()",
    "match a { _ => a }",
    "match a { _ => a }.clone() + 1",
    "unsafe { a }",
    "const { 1 }",
    "const { 1 } + 1",
    "loop { break a + 1 }",
    "'l: loop { break 'l a }",
    "<u32 as Clone>::clone(&a)",
    "<u32 as Clone>::clone(&a) + 1",
    "u32::max(a, 1)",
    "Vec::<u32>::new().len()",
    "Vec::<u32>::new().len() as u32",
    "0..a",
    "..a",
    "a..",
    "0..=a",
    "|y: u32| y + 1",
    "move || a + 1",
    "[a; 2]",
    "[a, 1][0] + 1",
    "(a, 1).0 + 1",
    "Some(a).unwrap() + 1",
    "S { v: a }.v + 1",
    "vec![a].len()",
    "vec![a].len() + 1",
    // ブロックのような式の終わりを見分けにくい形（PR #140 のレビュー）
    "if let S { v } = s { (v,) } else { (0,) }.0 & 1",
    "if let S { v } = s { v } else { a }",
    "if { b } { a } else { a }.clone() + 1",
    "match { a } { _ => a }.clone() + 1",
    "while { false } { }.clone() == ()",
    "for S { v } in vec![S { v: a }] { }.clone() == ()",
    "if b { a } else if let S { v } = s { v } else { a }.clone() + 1",
    "if a == 1 { 1 } else { a }",
    "if b { 1u8 } else { 2 } & 1",
    // 戻り値の型を書いたクロージャ（本体がブロックで終わり、後ろに `as` が続く）
    "|| -> u32 { 1 } as fn() -> u32",
    "move || -> u32 { 1 } as fn() -> u32",
    "|y: u32| -> u32 { y } as fn(u32) -> u32",
    "|y: u32| -> u32 { y }",
];

const ATTRS: &[&str] = &["#[allow(unused)]", "#[allow(unused)] #[allow(unused)]"];

const HEADER: &str =
    "pub struct S<T> { pub v: T }\nfn g(x: u32) -> u32 { x }\nfn h<T>(x: T) -> T { x }\n";

fn fragment(ctx: &str, attr: &str, form: &str) -> String {
    let body = ctx.replace("$A", attr).replace("$E", form);
    format!("pub fn f(a: u32, b: bool, s: S<u32>) {{ let mut x = 0u32; let v = {body}; }}")
}

/// 安定版の rustc で、行ごとの誤りを返す（行番号 → 文言）。各断片を 1 行の関数にして、一つのファイルで
/// まとめてコンパイルする。
fn rustc_errors(fragments: &[String], edition: &str) -> BTreeMap<usize, Vec<String>> {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("attr_rustc");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("probe{edition}.rs"));
    let mut src = String::from(
        "#![allow(unused, unreachable_code, unused_must_use, path_statements, unused_braces)]\n",
    );
    src.push_str(HEADER);
    let first = src.lines().count() + 1;
    for (k, f) in fragments.iter().enumerate() {
        src.push_str(&f.replacen("pub fn f(", &format!("pub fn f{k}("), 1));
        src.push('\n');
    }
    std::fs::write(&file, &src).unwrap();
    let rustc = rustc_of_toolchain();
    let out = Command::new(rustc)
        .args([
            "--edition",
            edition,
            "--crate-type",
            "lib",
            "--emit=metadata",
        ])
        .args(["--error-format=short", "-o"])
        .arg(dir.join(format!("probe{edition}.rmeta")))
        .arg(&file)
        .output()
        .expect("rustc を起動できない");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let name = file.to_string_lossy().to_string();
    let mut errors: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for line in stderr.lines() {
        let Some(rest) = line.strip_prefix(&format!("{name}:")) else {
            continue;
        };
        let mut parts = rest.splitn(3, ':');
        let row: usize = parts.next().unwrap().parse().unwrap();
        let msg = parts.nth(1).unwrap_or_default().trim().to_string();
        if msg.starts_with("error") {
            assert!(row >= first, "前置きに誤り: {line}\n{src}");
            errors.entry(row - first).or_default().push(msg);
        }
    }
    // 誤りがあるのに行に結べないなら、出力を読めていない。
    assert!(
        out.status.success() || !errors.is_empty(),
        "rustc の出力を読めない:\n{stderr}"
    );
    errors
}

/// 使う rustc。`RUSTC` か PATH の rustc が、`rust-toolchain.toml` の版であることを確かめる（別の版の rustc と
/// 突き合わせると、安定版の Rust の判定と食い違いうる）。
fn rustc_of_toolchain() -> std::ffi::OsString {
    let toml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../rust-toolchain.toml"
    ))
    .expect("rust-toolchain.toml を読めない");
    let channel = toml
        .lines()
        .find_map(|l| l.trim().strip_prefix("channel"))
        .and_then(|r| r.trim().strip_prefix('='))
        .map(|r| r.trim().trim_matches('"').to_string())
        .expect("rust-toolchain.toml に channel がない");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let out = Command::new(&rustc)
        .arg("--version")
        .output()
        .expect("rustc を起動できない");
    let version = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        version.starts_with(&format!("rustc {channel} ")),
        "rustc の版が rust-toolchain.toml（{channel}）と違う: {version:?}（{rustc:?}）"
    );
    rustc
}

/// 属性の誤りか（ほかの誤りは、断片が壊れている）。
fn attribute_error(msg: &str) -> bool {
    msg.contains("attributes on expressions are experimental")
        || msg.contains("attributes are not allowed on")
        || msg.contains("attributes in `where` clause are unstable")
        || msg.contains("attributes are not supported in `where` clauses")
}

#[test]
fn attributes_ebykn_accepts_are_accepted_by_stable_rustc() {
    let plain: Vec<String> = CONTEXTS
        .iter()
        .flat_map(|c| FORMS.iter().map(move |f| fragment(c, "", f)))
        .collect();
    let with_attrs: Vec<(String, String)> = ATTRS
        .iter()
        .flat_map(|a| {
            CONTEXTS.iter().flat_map(move |c| {
                FORMS
                    .iter()
                    .map(move |f| (fragment(c, a, f), format!("{c} / {a} / {f}")))
            })
        })
        .collect();
    let srcs: Vec<String> = with_attrs.iter().map(|(s, _)| s.clone()).collect();
    for edition in ["2021", "2024"] {
        // 属性のない断片が rustc で通らない組み合わせ（文の位置の `const { 1 } + 1` など）は、属性の位置を
        // 問えないので見ない。
        let invalid = rustc_errors(&plain, edition);
        assert!(
            invalid.len() * 10 < plain.len(),
            "属性のない断片の誤りが多い: {invalid:?}"
        );
        let errs = rustc_errors(&srcs, edition);
        let mut rustc_rejects = 0;
        let mut ebykn_accepts = 0;
        let mut unsound = Vec::new();
        for (k, (src, what)) in with_attrs.iter().enumerate() {
            if invalid.contains_key(&(k % plain.len())) {
                continue;
            }
            let msgs = errs.get(&k).cloned().unwrap_or_default();
            assert!(
                msgs.iter().all(|m| attribute_error(m)),
                "属性のほかの誤り（断片が壊れている）: {what}: {msgs:?}"
            );
            let rejected = !msgs.is_empty();
            rustc_rejects += usize::from(rejected);
            let (_, lines) = support::run(&format!("{HEADER}{src}"));
            let accepted = !lines
                .iter()
                .any(|l| ATTR_DIAGS.iter().any(|d| l.contains(d)));
            ebykn_accepts += usize::from(accepted);
            if accepted && rejected {
                unsound.push(format!("{what}: {msgs:?}"));
            }
        }
        // 突き合わせが空回りしていない：rustc が拒否する形も、Ebykn が許す形もある。
        assert!(
            rustc_rejects > 50,
            "rustc が拒否した断片が少ない: {rustc_rejects}"
        );
        assert!(
            ebykn_accepts > 50,
            "Ebykn が許した断片が少ない: {ebykn_accepts}"
        );
        assert!(
            unsound.is_empty(),
            "Ebykn が許し、安定版の rustc（edition {edition}）が拒否する:\n{}",
            unsound.join("\n")
        );
    }
}
