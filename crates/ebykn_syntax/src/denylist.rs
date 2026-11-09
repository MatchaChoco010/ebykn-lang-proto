//! 拒否リスト（#27 の作者の方針）。
//!
//! `.eyk` はなるべく多くの Rust の書き方を許し、Verus で壊れる形（検証したコードと配るコードが食い違う、
//! 証明が紛れ込む）だけを、この表で拒否する。穴が見つかったら、この表に項目を足して塞ぐ。
//!
//! - [`DenyKind::Cfg`]：この名前を条件に含む `cfg!(..)`、`#[cfg(..)]`、`#![cfg(..)]`、
//!   `#[cfg_attr(条件, ..)]`（条件と、中の `cfg(..)` の両方）を拒否する。`.eyk` のどこでも（項目、本体、
//!   マクロの引数、属性の値）、述語の `all` / `any` / `not` の入れ子の中まで見る（トークン列で見る。
//!   `parse::tokens`）。
//! - [`DenyKind::Macro`]：このマクロの呼び出し（裸の名前か、`std::` / `core::` から始まるパス）を拒否する。
//!   `unchecked_ok` の項目は、`unchecked fn` の本体の中でだけ許す（本体は検査せず、`verify` が証明なしの
//!   前提として一覧に出す）。
//!
//! 項目を足すと、表のすべての項目に効くテスト（`crates/ebykn_check/tests/denylist.rs`）が、
//! 各項目の再現の例と、書ける位置の総当たりに自動で効く。

/// 拒否の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyKind {
    /// cfg の述語の名前。
    Cfg,
    /// マクロの名前。
    Macro {
        /// `unchecked fn` の本体の中では許すか。
        unchecked_ok: bool,
    },
}

/// 拒否リストの項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub name: &'static str,
    pub kind: DenyKind,
    /// 壊す理由。英語は `because` に続く節、日本語は「書けないのは、」と「ためです」のあいだに入る節。
    pub why_en: &'static str,
    pub why_ja: &'static str,
    /// 代わりの書き方（空なら出さない）。
    pub hint_en: &'static str,
    pub hint_ja: &'static str,
    /// 壊れる再現の例（`.eyk`）。テストが、この例が拒否されることを確かめる。
    pub example: &'static str,
}

const VERUS_CFG_EN: &str = "Verus sets this cfg itself when it verifies, and a build without Verus does not, so the code Verus verifies and the code that is built differ";
const VERUS_CFG_JA: &str = "Verus がこの cfg を検証するときに自分で設定し、Verus なしのビルドでは設定されないので、Verus が検証するコードとビルドするコードが食い違う";

const ENV_EN: &str = "a compile-time environment variable can differ between the Verus run and the plain build (`ebykn verify` sets `CARGO_TERM_COLOR`, and `RUSTUP_TOOLCHAIN` is Verus's toolchain)";
const ENV_JA: &str = "コンパイル時の環境変数の値が、Verus の実行と素のビルドで違いうる（`ebykn verify` は `CARGO_TERM_COLOR` を設定し、`RUSTUP_TOOLCHAIN` は Verus の toolchain になる）";

const INCLUDE_DATA_EN: &str = "the file it reads can differ between the Verus run and the build (an absolute path such as `/proc/self/cmdline` or `/proc/self/environ`, or a file that changes in between)";
const INCLUDE_DATA_JA: &str = "読むファイルが、Verus の実行とビルドで違いうる（`/proc/self/cmdline` や `/proc/self/environ` のような絶対パスや、そのあいだに変わるファイル）";

const DEBUG_ASSERT_EN: &str = "std expands it under `cfg(debug_assertions)`, so its arguments run when Verus verifies (the dev profile) and do not run in a release build";
const DEBUG_ASSERT_JA: &str = "std の展開の中で `cfg(debug_assertions)` に届くので、引数が検証（dev のプロファイル）では実行され、release のビルドでは実行されない";

const VALUE_HINT_EN: &str = "if you need the value, wrap it in an `unchecked fn`";
const VALUE_HINT_JA: &str = "値が要るなら `unchecked fn` で包みます";

const DEBUG_HINT_EN: &str =
    "use `assert!`, which runs in both, or write the function that uses it as an `unchecked fn`";
const DEBUG_HINT_JA: &str =
    "どちらでも実行される `assert!` を使うか、それを使う関数を `unchecked fn` にします";

const fn cfg(name: &'static str, example: &'static str) -> Entry {
    Entry {
        name,
        kind: DenyKind::Cfg,
        why_en: VERUS_CFG_EN,
        why_ja: VERUS_CFG_JA,
        hint_en: "",
        hint_ja: "",
        example,
    }
}

/// 拒否リスト。
pub const DENYLIST: &[Entry] = &[
    // Verus は 0 を返すと検証し、素のビルドは 1 を返す。
    cfg(
        "verus_keep_ghost",
        "pub fn f() -> (r: u32) ensures r == 0, { if cfg!(verus_keep_ghost) { 0 } else { 1 } }",
    ),
    // Verus のときだけ return 1 が消える。
    cfg(
        "verus_keep_ghost_body",
        "pub fn f() -> (r: u32) ensures r == 0, { if cfg!(not(verus_keep_ghost_body)) { 1 } else { 0 } }",
    ),
    // Verus のときだけ ensures のある関数になり、素のビルドでは関数がなくなる。
    cfg(
        "verus_only",
        "#[cfg(verus_only)]\npub fn f() -> (r: u32) ensures r == 0, { 0 }",
    ),
    cfg(
        "verus_verify_core",
        "pub fn f() -> (r: u32) ensures r == 0, { if cfg!(any(verus_verify_core, test)) { 0 } else { 1 } }",
    ),
    cfg(
        "verus_no_vstd",
        "pub fn f() -> (r: u32) ensures r == 0, { if cfg!(all(not(verus_no_vstd))) { 0 } else { 1 } }",
    ),
    Entry {
        name: "debug_assertions",
        kind: DenyKind::Cfg,
        why_en: "it is on when Verus verifies (the dev profile) and off in a release build, so the code Verus verifies and the code that is built differ",
        why_ja: "この cfg が検証（dev のプロファイル）では有効で、release のビルドでは無効なので、Verus が検証するコードとビルドするコードが食い違う",
        hint_en: "",
        hint_ja: "",
        // Verus は 0 を返すと検証し、release のビルドは 1 を返す。
        example: "pub fn f() -> (r: u32) ensures r == 0, { if cfg!(debug_assertions) { 0 } else { 1 } }",
    },
    Entry {
        name: "panic",
        kind: DenyKind::Cfg,
        why_en: "the panic strategy is set by the profile of the root crate (`panic = \"abort\"` in `build/verus/Cargo.toml` applies only when that crate is the root), so when another crate depends on it, the code Verus verifies and the code that is built differ",
        why_ja: "パニックの戦略（`abort` / `unwind`）が根になるクレートのプロファイルで決まり（`build/verus/Cargo.toml` の `panic = \"abort\"` は、そのクレートが根のときだけ効く）、別のクレートから依存されると、Verus が検証するコードとビルドするコードが食い違う",
        hint_en: "",
        hint_ja: "",
        // PR #59 のレビュー：Verus は 0 を返すと検証し、依存として使うと 1 を返す。
        example: "pub fn f() -> (r: u32) ensures r == 0, { if cfg!(panic = \"abort\") { 0 } else { 1 } }",
    },
    Entry {
        name: "env",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: ENV_EN,
        why_ja: ENV_JA,
        hint_en: VALUE_HINT_EN,
        hint_ja: VALUE_HINT_JA,
        example: "pub fn f() -> (r: usize) ensures r == 5, { env!(\"CARGO_TERM_COLOR\").len() }",
    },
    Entry {
        name: "option_env",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: ENV_EN,
        why_ja: ENV_JA,
        hint_en: VALUE_HINT_EN,
        hint_ja: VALUE_HINT_JA,
        // PR #26 の 4 回目の確認：Verus では 0、素のビルドでは 1 を返す。
        example: "pub fn h8() -> (r: u32) ensures r == 0, { if option_env!(\"CARGO_TERM_COLOR\").is_some() { 0 } else { 1 } }",
    },
    Entry {
        name: "file",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: "the path depends on how cargo compiles the crate (relative when Verus verifies it, absolute when another crate depends on it)",
        why_ja: "パスが cargo のクレートのコンパイルの仕方で変わる（Verus で検証するときは相対、別のクレートから依存されると絶対）",
        hint_en: VALUE_HINT_EN,
        hint_ja: VALUE_HINT_JA,
        // 検証では `src/lib.rs`（10 文字）、path で依存されると絶対パスになる。
        example: "pub fn f() -> (r: usize) ensures r == 10, { file!().len() }",
    },
    Entry {
        name: "include",
        kind: DenyKind::Macro {
            unchecked_ok: false,
        },
        why_en: "it brings in code that Ebykn does not read (for example `::vstd::prelude::proof!{ assume(false); }`, #16)",
        why_ja: "Ebykn が読まないコードを取り込む（`::vstd::prelude::proof!{ assume(false); }` など。#16）",
        hint_en: "",
        hint_ja: "",
        example: "pub fn f() -> (r: u32) ensures r == 1, { include!(\"../../../smuggled.in\"); 0 }",
    },
    Entry {
        name: "include_bytes",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: INCLUDE_DATA_EN,
        why_ja: INCLUDE_DATA_JA,
        hint_en: VALUE_HINT_EN,
        hint_ja: VALUE_HINT_JA,
        // PR #59 のレビュー：Verus の実行のコマンド行は長いので 0、利用側のビルドでは 1 を返す。
        example: "pub fn f() -> (r: u32) ensures r == 0, { if include_bytes!(\"/proc/self/cmdline\").len() > 1600 { 0 } else { 1 } }",
    },
    Entry {
        name: "include_str",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: INCLUDE_DATA_EN,
        why_ja: INCLUDE_DATA_JA,
        hint_en: VALUE_HINT_EN,
        hint_ja: VALUE_HINT_JA,
        example: "pub fn f() -> (r: u32) ensures r == 0, { if include_str!(\"/proc/self/environ\").contains(\"CARGO_TERM_COLOR=never\") { 0 } else { 1 } }",
    },
    Entry {
        name: "debug_assert",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: DEBUG_ASSERT_EN,
        why_ja: DEBUG_ASSERT_JA,
        hint_en: DEBUG_HINT_EN,
        hint_ja: DEBUG_HINT_JA,
        // PR #59 のレビュー：Verus では bump が走って 1、release の利用側では 0 を返す。
        example: "fn bump(x: &mut u32) -> (b: bool) requires *old(x) < 10, ensures *final(x) == *old(x) + 1, b == true, { *x = *x + 1; true }\npub fn f() -> (r: u32) ensures r == 1, { let mut x: u32 = 0; debug_assert!(bump(&mut x)); x }",
    },
    Entry {
        name: "debug_assert_eq",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: DEBUG_ASSERT_EN,
        why_ja: DEBUG_ASSERT_JA,
        hint_en: DEBUG_HINT_EN,
        hint_ja: DEBUG_HINT_JA,
        example: "pub fn g() -> (r: usize) ensures r == 1, { let mut v: Vec<u32> = Vec::new(); v.push(1); v.push(2); debug_assert_eq!(v.pop(), Some(2)); v.len() }",
    },
    Entry {
        name: "debug_assert_ne",
        kind: DenyKind::Macro { unchecked_ok: true },
        why_en: DEBUG_ASSERT_EN,
        why_ja: DEBUG_ASSERT_JA,
        hint_en: DEBUG_HINT_EN,
        hint_ja: DEBUG_HINT_JA,
        example: "pub fn g() -> (r: usize) ensures r == 1, { let mut v: Vec<u32> = Vec::new(); v.push(1); v.push(2); debug_assert_ne!(v.pop(), None); v.len() }",
    },
];

/// cfg の述語の名前（`r#` を外したもの）が拒否リストにあれば、その項目。
pub fn cfg_entry(name: &str) -> Option<&'static Entry> {
    DENYLIST
        .iter()
        .find(|e| e.kind == DenyKind::Cfg && e.name == name)
}

/// cfg の属性（`#[cfg(..)]` / `#[cfg_attr(..)]`）が拒否リストの cfg の名前を含むか。含むなら、トークンの
/// 検査（`parse::tokens`）がすでに E0001 を出しているので、同じ属性に E0003 を重ねない（#95 の位置）。
pub fn attr_denied(a: &crate::ast::Attr) -> bool {
    fn find(ts: &proc_macro2::TokenStream) -> bool {
        ts.clone().into_iter().any(|t| match t {
            proc_macro2::TokenTree::Ident(i) => {
                let n = i.to_string();
                cfg_entry(n.strip_prefix("r#").unwrap_or(&n)).is_some()
            }
            proc_macro2::TokenTree::Group(g) => find(&g.stream()),
            _ => false,
        })
    }
    (a.is_builtin("cfg") || a.is_builtin("cfg_attr")) && find(&a.args)
}

/// マクロの名前が拒否リストにあれば、その項目。
pub fn macro_entry(name: &str) -> Option<&'static Entry> {
    DENYLIST
        .iter()
        .find(|e| matches!(e.kind, DenyKind::Macro { .. }) && e.name == name)
}
