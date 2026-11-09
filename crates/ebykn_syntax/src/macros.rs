//! 項目に付いた属性のうち、Verus が検証しない項目を作る手続きマクロを見分ける表（#49）。
//!
//! 依存クレートの derive や属性マクロは、`verus!` が書き換えた後のトークンを受け取り、Verus の印の
//! ない項目（trait の impl、`fn`）を作ることがある。Verus はそうした項目を検証しないので、それを
//! 前提に呼び出し側が verified になる（`tests/verus-probes/external_impl_of_spec_trait.rs`）。
//! そこで、どの項目にどのマクロが付いているかを字句的に見て、`ebykn verify` の前提の一覧に出す
//! （拒否はしない。Rust のクレートの資産を使えるようにするため）。
//!
//! 判定は綴りによらない。パス付き（`::core::clone::Clone`）、`cfg_attr` の中、`unsafe(..)` の中を
//! 見る。名前を覆い隠す書き方（`use evil::Clone;` の後の `#[derive(Clone)]`、`use evil::x as derive;`、
//! 同じ名前の依存クレートがある `#[clippy::x]`）は、クレートのどこかでその名前を束縛していれば、
//! std やツールの属性として数えない（[`Bindings`]）。依存クレートの名前（`[lib] name` を含む）が
//! 分からないときも、std のパスとツールの属性を数えない（前提として出す側に倒す）。
//!
//! 項目の外側の属性のほかの位置（`#![..]`、文・式の属性）は、組み込みの属性だけを書ける
//! （[`non_builtin`]。構文解析と検査が拒否する）。安定版の Rust はこれらの位置に属性マクロを
//! 許さないが、Verus は `-Zcrate-attr=feature(custom_inner_attributes, stmt_expr_attributes)` と
//! `RUSTC_BOOTSTRAP=1` で動くので、`verify` ではこれらの位置の属性マクロも効く（PR #57 のレビュー）。
//!
//! 名前で判定する属性を足すときは、この表に足す。綴りの総当たりのテスト
//! （`crates/ebykn_syntax/tests/macros.rs`）が、表のすべての名前に自動で効く。書き下しが
//! どの項目でも記録することは `crates/ebykn_lower/tests/macros.rs` が確かめる。

use std::collections::BTreeSet;

use proc_macro2::{Delimiter, TokenStream, TokenTree};

use crate::ast;

/// std の derive。前提として出さない。固定した Verus は derive した `Clone` の impl だけを検証し
/// （`func-details` に出る）、ほかの impl は検証しないが、どれにも仕様がないので、verified なコードは
/// その振る舞いに頼れない（`a == b` の結果も `S::default()` の値も分からない。
/// `tests/verus-probes/std_derive_no_spec.rs`）。std の derive が作るのは、std が書いた impl だけ。
/// 裸の名前か、`std::` / `core::` から始まるパスで書いたものだけを std の derive として数える。
pub const STD_DERIVES: &[&str] = &[
    "Clone",
    "Copy",
    "Debug",
    "PartialEq",
    "Eq",
    "Hash",
    "Default",
    "PartialOrd",
    "Ord",
];

/// Rust の組み込みの属性（Rust リファレンスの Built-in attributes index）のうち、名前を覆い隠せない
/// もの。`use` で同じ名前を取り込むと、rustc が曖昧さのエラー（E0659）にする（手元で確かめた）。
pub const BUILTIN_ATTRS: &[&str] = &[
    // Conditional compilation
    "cfg",
    "cfg_attr",
    // Testing（`test` は [`PRELUDE_ATTR_MACROS`]）
    "ignore",
    "should_panic",
    // Derive（`derive` は [`PRELUDE_ATTR_MACROS`]）
    "automatically_derived",
    // Macros
    "macro_export",
    "macro_use",
    "proc_macro",
    "proc_macro_derive",
    "proc_macro_attribute",
    // Diagnostics
    "allow",
    "expect",
    "warn",
    "deny",
    "forbid",
    "deprecated",
    "must_use",
    // ABI, linking, symbols, and FFI
    "link",
    "link_name",
    "link_ordinal",
    "no_link",
    "repr",
    "crate_type",
    "no_main",
    "export_name",
    "link_section",
    "no_mangle",
    "used",
    "crate_name",
    // Code generation
    "inline",
    "cold",
    "naked",
    "no_builtins",
    "target_feature",
    "track_caller",
    "instruction_set",
    // Documentation
    "doc",
    // Preludes
    "no_std",
    "no_implicit_prelude",
    // Modules
    "path",
    // Limits
    "recursion_limit",
    "type_length_limit",
    // Runtime（`global_allocator` は [`PRELUDE_ATTR_MACROS`]）
    "panic_handler",
    "windows_subsystem",
    // Features（`feature` は入れない。`#![feature]` は `.eyk` に書けない）
    // Type system
    "non_exhaustive",
    // Debugger
    "debugger_visualizer",
    "collapse_debuginfo",
];

/// Rust の組み込みの属性のうち、std の prelude のマクロで、`use` で名前を覆い隠せるもの
/// （`use evil::x as derive;` の後の `#[derive(..)]` は evil の属性マクロになる。手元で確かめた）。
/// クレートで同じ名前を束縛していなければ組み込みとして数える。
pub const PRELUDE_ATTR_MACROS: &[&str] = &["derive", "test", "global_allocator"];

/// rustc が認識するツールの属性の名前空間（Rust リファレンスの Tool attributes）。同じ名前の
/// 依存クレートがあると、`#[clippy::x]` はそのクレートのマクロになる（手元で確かめた）ので、
/// クレートで同じ名前を束縛していないときだけツールの属性として数える。
pub const TOOL_NAMESPACES: &[&str] = &["clippy", "rustfmt", "diagnostic", "miri", "rust_analyzer"];

// Ebykn が認識する属性のうち、`#[test]` は上の表にある。証明の属性は構文解析が `.eyk` で拒否する。
// `#[override]` は `override` が予約語なので構文解析が読めず（E0001）、rustc に届く `#[r#override]` は
// 利用者が束縛した名前（`use evil::x as r#override;`）なので、外のマクロとして数える。

/// 手続きマクロの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MacroKind {
    /// `#[derive(path)]` の `path`。
    Derive,
    /// 属性マクロ `#[path]`。
    Attribute,
}

/// 項目に付いた、std でも組み込みでもない手続きマクロ。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalMacro {
    pub kind: MacroKind,
    /// 書かれたとおりのパス（`serde::Serialize`、`::tokio::main`）。
    pub path: String,
}

/// クレートのどこかで束縛している名前（`use` の葉と改名、`mod`、`extern crate`、依存クレート）。
/// 名前を覆い隠す書き方を見分けるために使う。字句的に集めるので、どのモジュールかは区別しない
/// （区別しないぶん、前提として出す側に倒れる）。
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    names: BTreeSet<String>,
    /// `std` / `core` / `alloc` から始まる `use` の、改名しない葉（根の名前、葉の名前）。根の名前を
    /// 束縛していれば（`core` という名前の依存クレートなど）、葉も束縛として数える。
    std_leaves: Vec<(String, String)>,
    /// 依存クレートの名前が分からない（[`Bindings::crate_names_unknown`]）。
    unknown_crates: bool,
}

impl Bindings {
    /// 束縛している名前か。
    pub fn binds(&self, name: &str) -> bool {
        self.names.contains(name)
            || self
                .std_leaves
                .iter()
                .any(|(root, leaf)| leaf == name && self.root_shadowed(root))
    }

    /// `std` / `core` / `alloc` の名前が、std のクレートを指すと言えないか。
    fn root_shadowed(&self, root: &str) -> bool {
        self.unknown_crates || self.names.contains(root)
    }

    /// 依存クレートがコードで使われる名前（`-` は `_`）を足す。
    pub fn add_crate(&mut self, name: &str) {
        self.names.insert(name.replace('-', "_"));
    }

    /// 依存クレートがコードで使われる名前を、すべては決められなかった（`[lib] name` を読めない
    /// など）。std のパスとツールの属性を、std やツールのものとして数えなくなる。
    pub fn crate_names_unknown(&mut self) {
        self.unknown_crates = true;
    }

    /// `.eyk` のトークン列（ファイル全体）から、束縛する名前を集める。
    ///
    /// - `use` の葉と改名の名前。ただし `std` / `core` / `alloc` から始まる `use` で、改名しない葉は
    ///   std の項目そのものなので数えない。glob（`*`）は数えない（std の prelude の名前とぶつかると
    ///   rustc が曖昧さのエラーにする。手元で確かめた）。
    /// - `mod` の名前、`extern crate` の名前（改名があれば改名した名前）。
    pub fn scan(&mut self, ts: &TokenStream) {
        let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
        let mut i = 0;
        while i < tts.len() {
            match &tts[i] {
                TokenTree::Ident(id) if id == "use" => {
                    let end = (i + 1..tts.len())
                        .find(|&j| punct(tts.get(j), ';'))
                        .unwrap_or(tts.len());
                    let mut k = i + 1;
                    while punct(tts.get(k), ':') {
                        k += 1;
                    }
                    let root = ident(tts.get(k));
                    let std_root = root.filter(|r| matches!(r.as_str(), "std" | "core" | "alloc"));
                    self.use_tree(&tts[i + 1..end], std_root.as_deref(), None);
                    i = end;
                }
                TokenTree::Ident(id) if id == "mod" => {
                    if let Some(n) = ident(tts.get(i + 1)) {
                        self.names.insert(n);
                    }
                }
                TokenTree::Ident(id)
                    if id == "crate" && ident_is(tts.get(i.wrapping_sub(1)), "extern") =>
                {
                    let renamed = ident_is(tts.get(i + 2), "as")
                        .then(|| ident(tts.get(i + 3)))
                        .flatten();
                    if let Some(n) = renamed.or_else(|| ident(tts.get(i + 1))) {
                        self.names.insert(n);
                    }
                }
                TokenTree::Group(g) => self.scan(&g.stream()),
                _ => {}
            }
            i += 1;
        }
    }

    /// `use` の木（`use` と `;` の間）の葉の名前を集める。`parent` は `{ }` のグループの直前の名前
    /// （`use a::b::{self}` の `b`）。
    fn use_tree(&mut self, tts: &[TokenTree], std_root: Option<&str>, parent: Option<String>) {
        let std_root = std_root.map(str::to_string);
        let prev_ident = |i: usize| i.checked_sub(1).and_then(|j| ident(tts.get(j)));
        for (i, tt) in tts.iter().enumerate() {
            match tt {
                TokenTree::Ident(_) => {
                    let Some(n) = ident(Some(tt)) else { continue };
                    if prev_ident(i).as_deref() == Some("as") {
                        // 改名した名前（`use x as Clone`）。std から始まっても数える。
                        if n != "_" {
                            self.names.insert(n);
                        }
                        continue;
                    }
                    // 葉（木の終わりか `,` の前）。`as` の前は元の名前で、束縛しない。
                    let leaf = tts.get(i + 1).is_none() || punct(tts.get(i + 1), ',');
                    if !leaf {
                        continue;
                    }
                    if let Some(root) = &std_root {
                        // 根が std を指すなら std の項目そのもの。根を束縛していれば数える（`binds`）。
                        self.std_leaves.push((root.clone(), n));
                        continue;
                    }
                    if n == "self" {
                        // `use a::b::{self}` は `b` を束縛する。
                        self.names.extend(parent.clone());
                    } else {
                        self.names.insert(n);
                    }
                }
                TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                    self.use_tree(&inner, std_root.as_deref(), prev_ident(i.saturating_sub(2)));
                }
                _ => {}
            }
        }
    }
}

/// Rust の予約語（厳密な予約語と、将来のための予約語）。raw の綴り（`r#unsafe`）は予約語とは別の
/// 名前で、利用者が束縛した名前を指す。予約語でない名前の raw の綴り（`r#derive`）は、`derive` と
/// 同じ名前。
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
];

/// 属性のパスの一つの名前。予約語の raw の綴りだけ `r#` を残す（`r#unsafe(..)` を組み込みの
/// `unsafe(..)` の包みと取り違えない。PR #57 の再レビューの B1）。
fn segment(name: &str, raw: bool) -> String {
    if raw && KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

/// トークンの名前を [`segment`] の形で。
fn path_ident(tt: Option<&TokenTree>) -> Option<String> {
    match tt {
        Some(TokenTree::Ident(i)) => {
            let s = i.to_string();
            Some(match s.strip_prefix("r#") {
                Some(n) => segment(n, true),
                None => s,
            })
        }
        _ => None,
    }
}

fn ident(tt: Option<&TokenTree>) -> Option<String> {
    match tt {
        Some(TokenTree::Ident(i)) => {
            let s = i.to_string();
            Some(s.strip_prefix("r#").unwrap_or(&s).to_string())
        }
        _ => None,
    }
}

fn ident_is(tt: Option<&TokenTree>, k: &str) -> bool {
    ident(tt).as_deref() == Some(k)
}

fn punct(tt: Option<&TokenTree>, c: char) -> bool {
    matches!(tt, Some(TokenTree::Punct(p)) if p.as_char() == c)
}

/// 属性のパスと残り（`(..)` や `= ..`）。
struct Meta {
    leading_colon: bool,
    segments: Vec<String>,
    args: TokenStream,
}

impl Meta {
    fn written(&self) -> String {
        format!(
            "{}{}",
            if self.leading_colon { "::" } else { "" },
            self.segments.join("::")
        )
    }
}

/// トークン列 `path args` を読む。パスが読めなければ `None`。
fn read_meta(tts: &[TokenTree]) -> Option<Meta> {
    let mut k = 0;
    let leading_colon = punct(tts.first(), ':') && punct(tts.get(1), ':');
    if leading_colon {
        k = 2;
    }
    let mut segments = vec![path_ident(tts.get(k))?];
    k += 1;
    while punct(tts.get(k), ':') && punct(tts.get(k + 1), ':') {
        segments.push(path_ident(tts.get(k + 2))?);
        k += 3;
    }
    Some(Meta {
        leading_colon,
        segments,
        args: tts[k..].iter().cloned().collect(),
    })
}

/// 最上位の `,` で分ける。
fn split_commas(ts: &TokenStream) -> Vec<Vec<TokenTree>> {
    let mut out = vec![Vec::new()];
    for tt in ts.clone() {
        if punct(Some(&tt), ',') {
            out.push(Vec::new());
        } else {
            out.last_mut().expect("空でない").push(tt);
        }
    }
    out.retain(|x| !x.is_empty());
    out
}

/// 括弧のグループの中身（`(..)` でなければ `None`）。
fn paren_inner(ts: &TokenStream) -> Option<TokenStream> {
    let mut it = ts.clone().into_iter();
    match (it.next(), it.next()) {
        (Some(TokenTree::Group(g)), None) if g.delimiter() == Delimiter::Parenthesis => {
            Some(g.stream())
        }
        _ => None,
    }
}

/// 項目に付いた属性（AST）が持ち込む、std でも組み込みでもない手続きマクロ。
pub fn external_macros(a: &ast::Attr, b: &Bindings) -> Vec<ExternalMacro> {
    let m = Meta {
        leading_colon: a.path.leading_colon,
        segments: a
            .path
            .segments
            .iter()
            .map(|s| segment(&s.ident.name, s.ident.raw))
            .collect(),
        args: a.args.clone(),
    };
    let mut out = Vec::new();
    classify(&m, b, &mut out);
    out
}

/// トークン列の先頭に並ぶ外側の属性（`#[..]`）が持ち込む手続きマクロ。AST を持たない宣言
/// （`extern` ブロックの中の宣言）に使う。
pub fn external_macros_in_tokens(ts: &TokenStream, b: &Bindings) -> Vec<ExternalMacro> {
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while punct(tts.get(i), '#') {
        let Some(TokenTree::Group(g)) = tts.get(i + 1) else {
            break;
        };
        if g.delimiter() != Delimiter::Bracket {
            break;
        }
        let body: Vec<TokenTree> = g.stream().into_iter().collect();
        match read_meta(&body) {
            Some(m) => classify(&m, b, &mut out),
            None => out.push(ExternalMacro {
                kind: MacroKind::Attribute,
                path: g.stream().to_string(),
            }),
        }
        i += 2;
    }
    out
}

fn classify(m: &Meta, b: &Bindings, out: &mut Vec<ExternalMacro>) {
    let first = m.segments[0].as_str();
    let single = m.segments.len() == 1 && !m.leading_colon;
    // 中の属性を読む組み込み（rustc は中の属性を展開する）。
    if single && (first == "cfg_attr" || first == "unsafe") {
        let Some(inner) = paren_inner(&m.args) else {
            return;
        };
        let parts = split_commas(&inner);
        // `cfg_attr` の最初の引数は条件。
        let skip = usize::from(first == "cfg_attr");
        for p in parts.iter().skip(skip) {
            match read_meta(p) {
                Some(im) => classify(&im, b, out),
                None => out.push(ExternalMacro {
                    kind: MacroKind::Attribute,
                    path: p.iter().cloned().collect::<TokenStream>().to_string(),
                }),
            }
        }
        return;
    }
    // `std::..::derive` / `core::..::derive` も derive（`core::prelude::v1::derive`）。
    let derive_path = !single && m.segments.last().is_some_and(|l| l == "derive") && std_path(m, b);
    if single && first == "derive" && !b.binds("derive") || derive_path {
        let Some(inner) = paren_inner(&m.args) else {
            return;
        };
        for p in split_commas(&inner) {
            match read_meta(&p) {
                Some(d) if std_derive(&d, b) => {}
                Some(d) => out.push(ExternalMacro {
                    kind: MacroKind::Derive,
                    path: d.written(),
                }),
                None => out.push(ExternalMacro {
                    kind: MacroKind::Derive,
                    path: p.iter().cloned().collect::<TokenStream>().to_string(),
                }),
            }
        }
        return;
    }
    let builtin = if single {
        BUILTIN_ATTRS.contains(&first) || PRELUDE_ATTR_MACROS.contains(&first) && !b.binds(first)
    } else {
        !m.leading_colon && TOOL_NAMESPACES.contains(&first) && !b.binds(first) && !b.unknown_crates
    };
    if !builtin {
        out.push(ExternalMacro {
            kind: MacroKind::Attribute,
            path: m.written(),
        });
    }
}

/// std の derive か。裸の名前は、クレートで同じ名前を束縛していないとき。パスは [`std_path`] で、
/// 最後の名前が [`STD_DERIVES`] にあるとき。
fn std_derive(d: &Meta, b: &Bindings) -> bool {
    let last = d.segments.last().expect("空でない").as_str();
    if !STD_DERIVES.contains(&last) {
        return false;
    }
    if d.segments.len() == 1 && !d.leading_colon {
        return !b.binds(last);
    }
    std_path(d, b)
}

/// `std::..` / `core::..`（先頭の `::` を含む）のパスで、その名前が std のクレートを指すと言えるか。
/// 先頭の `::` があっても、同じ名前の依存クレート（`[lib] name = "core"` など）を指しうるので、
/// クレートで束縛しておらず、依存クレートの名前がすべて分かっているときだけ。
fn std_path(d: &Meta, b: &Bindings) -> bool {
    let first = d.segments[0].as_str();
    d.segments.len() > 1 && matches!(first, "std" | "core") && !b.root_shadowed(first)
}

/// 項目の外側の属性のほかの位置に書いた属性のうち、書けないものの書き方。なければ `None`。
/// `cfg_attr(..)` と `unsafe(..)` の中も見る。
///
/// - 内側の属性（`#![..]`。[`AttrPosition::Inner`]）：Rust の組み込みの属性（[`BUILTIN_ATTRS`]）だけ。
///   Verus は `custom_inner_attributes` を有効にして動くので、内側の属性マクロは検証で効く。ツールの
///   属性（`#![rustfmt::skip]`）は、同じ名前の依存クレートのマクロを指しうるので書けない。
/// - 文・式・`let`・match の腕（[`AttrPosition::StmtExpr`]）：組み込みの属性とツールの属性。ここに付く
///   属性マクロは Verus でも E0658（`proc_macro_hygiene` は有効でない）で大きく失敗するが、二重の守り
///   として拒否する。ツールの属性は、マクロを指しても大きく失敗するだけなので許す。
///
/// std の prelude のマクロ（`test`、`derive`、`global_allocator`）は、どちらの位置にも書けない。
pub fn non_builtin(a: &ast::Attr, pos: AttrPosition) -> Option<String> {
    let m = Meta {
        leading_colon: a.path.leading_colon,
        segments: a
            .path
            .segments
            .iter()
            .map(|s| segment(&s.ident.name, s.ident.raw))
            .collect(),
        args: a.args.clone(),
    };
    non_builtin_meta(&m, pos)
}

/// 項目の外側の属性のほかの位置（[`non_builtin`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrPosition {
    /// `#![..]`。
    Inner,
    /// 文・式・`let`・match の腕の外側の属性。
    StmtExpr,
}

/// [`non_builtin`] の、属性の中身（`#[..]` の括弧の中）のトークン列の版。読めなければその全体。
pub fn non_builtin_in_tokens(body: &[TokenTree], pos: AttrPosition) -> Option<String> {
    match read_meta(body) {
        Some(m) => non_builtin_meta(&m, pos),
        None => Some(body.iter().cloned().collect::<TokenStream>().to_string()),
    }
}

fn non_builtin_meta(m: &Meta, pos: AttrPosition) -> Option<String> {
    let first = m.segments[0].as_str();
    let single = m.segments.len() == 1 && !m.leading_colon;
    if single && (first == "cfg_attr" || first == "unsafe") {
        let inner = paren_inner(&m.args)?;
        let skip = usize::from(first == "cfg_attr");
        return split_commas(&inner)
            .iter()
            .skip(skip)
            .find_map(|p| non_builtin_in_tokens(p, pos));
    }
    let builtin = single && BUILTIN_ATTRS.contains(&first);
    let tool = pos == AttrPosition::StmtExpr
        && !single
        && !m.leading_colon
        && TOOL_NAMESPACES.contains(&first);
    (!(builtin || tool)).then(|| m.written())
}
