//! Ebykn から Verus への書き下し（仕様 §7）。
//!
//! 入口は [`lower`]。検査（`ebykn_check`）の結果の事実（[`Facts`]）を使い、`.eyk` ごとに
//! `build/verus/src/**/*.rs` の中身を作る。書き下しは文字列で組み立て、`proc_macro2` の
//! トークン列に読み直してから [`print`] で整形する（仕様 §12 はトークン列で組み立てるとする。
//! 元の位置は M6 の source map で持たせる。`OPEN.md`）。

mod expr;
mod item;
pub mod print;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::str::FromStr;

use ebykn_check::Checked;
use ebykn_check::facts::Facts;
use ebykn_check::index::{Index, ItemPath, StructInfo};
use ebykn_check::project::{Project, TomlValue};
use ebykn_syntax::ast;
use proc_macro2::TokenStream;

pub use expr::PermKind;

/// 生成する各 `.rs` の先頭（#16 の既定。`use vstd::prelude::*;` の glob の代わりに、`.eyk` と
/// `verus!` が使う名前だけを取り込む。`Tracked` / `Ghost` / `old` は `verus!` が裸の名前で読む）。
pub const HEADER_USES: &str = "\
#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
";

/// 生成した一つの `.rs`。
#[derive(Debug, Clone)]
pub struct OutFile {
    /// `build/verus/` からの相対パス（`src/lib.rs`）。
    pub rel: String,
    pub text: String,
    /// 項目の行の範囲（1 から数え、両端を含む）とパス（`stack::Stack::push`）。
    pub items: Vec<(usize, usize, String)>,
}

/// 書き下しの結果。
#[derive(Debug, Clone, Default)]
pub struct Output {
    pub files: Vec<OutFile>,
    /// 本体のない `lemma fn`（`admit()` を出した項目）。
    pub admitted: Vec<String>,
    /// `unchecked` の項目。
    pub unchecked: Vec<String>,
    /// `#[test]` の項目（`#[verifier::external]`。Verus は見ない）。
    pub external: Vec<String>,
    /// Verus の名前をパスで決められない項目（クレートの外の型への impl のメソッド）の、Verus の
    /// `func-details` の名前の形（`k::m::impl&%*::e`。`*` は impl の番号）。
    pub verus_patterns: BTreeMap<String, String>,
    /// std でも組み込みでもない手続きマクロ（依存クレートの derive と属性マクロ）が付いた項目。
    /// マクロが作る項目は Verus が検証しないので、`verify` が前提の一覧に必ず出す（#49）。
    pub macros: Vec<MacroUse>,
}

/// 項目に付いた、Verus が検証しない項目を作る手続きマクロ（#49）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroUse {
    /// マクロが付いた項目の表示（`k::Foo`、`impl k::Tr for Foo`）。
    pub item: String,
    pub kind: ebykn_syntax::macros::MacroKind,
    /// 書かれたとおりのマクロのパス（`serde::Serialize`）。
    pub path: String,
    /// 書き下した `.rs` の `build/verus/` からの相対パス（`verify` で並べる順に使う）。
    pub rel: String,
}

/// 書き下しの間に集めるもの。
#[derive(Debug, Default)]
pub(crate) struct Report {
    admitted: Vec<String>,
    unchecked: Vec<String>,
    external: Vec<String>,
    verus_patterns: BTreeMap<String, String>,
    macros: Vec<MacroUse>,
}

/// 書き下しの文脈。式ごとに複製して使う（[`Cx::sub`]）。
#[derive(Clone)]
pub(crate) struct Cx<'a> {
    pub index: &'a Index,
    pub facts: &'a Facts,
    pub crate_name: &'a str,
    pub module: Vec<String>,
    pub self_ty: Option<ItemPath>,
    /// クレートの外の型への impl の中なら、その表示（`<u32 as Tr>`）。
    pub foreign_impl: Option<String>,
    pub self_struct: Option<&'a StructInfo>,
    /// 権限の変数の種類（関数の中で、文の順に更新する）。
    pub perms: BTreeMap<String, PermKind>,
    /// `ensures` で付け替える名前（`gives` の戻り値）。
    pub rename: BTreeMap<String, String>,
    /// `gives q@` の `q`（本体の末尾と `return` に権限を付ける）。
    pub gives: Option<String>,
    /// 抜ける文の行き先（ループ・ラベル付きのブロック）と、開いている `borrow mut`。内側ほど後ろ
    /// （#21 の既定 D：ブロックの外へ抜ける文の前で権限を領域に戻す）。
    pub exits: Vec<expr::ExitScope>,
    /// 関数の戻り値の型の形（`borrow mut` の中の `?` を展開するとき）。
    pub try_kind: Option<expr::TryKind>,
    pub logic: bool,
    pub ensures: bool,
    pub report: &'a RefCell<Report>,
    /// クレートで束縛している名前（手続きマクロの判定に使う。#49）。
    pub bindings: &'a ebykn_syntax::macros::Bindings,
    /// 書き下している `.rs` の `build/verus/` からの相対パス。
    pub rel: &'a str,
}

impl<'a> Cx<'a> {
    pub(crate) fn sub(&self, logic: bool) -> Cx<'a> {
        let mut c = self.clone();
        c.logic = logic;
        c
    }

    /// 項目のパスの表示（`stack::Stack::push`）。Verus の `func-details` の名前に合わせる：
    /// impl のメソッドは型の完全なパス（型を定義したモジュール）の下、trait の既定のメソッドは
    /// trait の下。クレートの外の型への impl のメソッドは `<u32 as Tr>` の下に表示し、Verus の名前
    /// （impl を置いたモジュールの `impl&%N`）は形で持つ。
    pub(crate) fn item_path(&self, name: &str) -> String {
        let mut p = vec![self.crate_name.to_string()];
        match &self.self_ty {
            Some(t) => p.extend(t.iter().cloned()),
            None => {
                p.extend(self.module.iter().cloned());
                if let Some(f) = &self.foreign_impl {
                    let mut pat = vec![self.crate_name.replace('-', "_")];
                    pat.extend(self.module.iter().cloned());
                    pat.push("impl&%*".into());
                    pat.push(name.to_string());
                    p.push(f.clone());
                    p.push(name.to_string());
                    let shown = p.join("::");
                    self.report
                        .borrow_mut()
                        .verus_patterns
                        .insert(shown.clone(), pat.join("::"));
                    return shown;
                }
            }
        }
        p.push(name.to_string());
        p.join("::")
    }

    /// 項目の属性のうち、std でも組み込みでもない手続きマクロを前提として記録する（#49）。
    /// `item` は項目の表示を作る（マクロがなければ呼ばない）。
    pub(crate) fn note_macros(&self, attrs: &[ast::Attr], item: impl FnOnce() -> String) {
        let found: Vec<_> = attrs
            .iter()
            .flat_map(|a| ebykn_syntax::macros::external_macros(a, self.bindings))
            .collect();
        self.record_macros(found, item);
    }

    /// トークン列の先頭に属性が並ぶ宣言（`extern` ブロックの中の宣言。AST を持たない）の版。
    pub(crate) fn note_macros_in_tokens(&self, ts: &TokenStream, item: impl FnOnce() -> String) {
        let found = ebykn_syntax::macros::external_macros_in_tokens(ts, self.bindings);
        self.record_macros(found, item);
    }

    fn record_macros(
        &self,
        found: Vec<ebykn_syntax::macros::ExternalMacro>,
        item: impl FnOnce() -> String,
    ) {
        if found.is_empty() {
            return;
        }
        let item = item();
        let mut r = self.report.borrow_mut();
        for m in found {
            r.macros.push(MacroUse {
                item: item.clone(),
                kind: m.kind,
                path: m.path,
                rel: self.rel.to_string(),
            });
        }
    }
}

/// `.eykp` だけの構文（仕様 §5.7）。書き下しの入力は `.eyk` の AST だけで、`parse_eyk` は証明構文を
/// AST に入れない（E0303 にする）ので、ここには来ない。`.eykp` の書き下しは #67 で足す。
pub(crate) fn eykp_only(what: &str) -> ! {
    unreachable!("{what} は .eykp にだけ現れる（.eykp の書き下しは #67）")
}

/// 属性を書き出す。
/// 属性の並び（各属性の後に空白）。どの位置の属性も、書き下しで落とさない（#95）。
pub(crate) fn attrs(a: &[ast::Attr]) -> String {
    a.iter().map(|x| attr(x) + " ").collect()
}

pub(crate) fn attr(a: &ast::Attr) -> String {
    let path = a
        .path
        .segments
        .iter()
        .map(|s| expr::ident(&s.ident))
        .collect::<Vec<_>>()
        .join("::");
    let lead = if a.path.leading_colon { "::" } else { "" };
    format!(
        "#{}[{lead}{path}{}]",
        if a.inner { "!" } else { "" },
        if a.args.is_empty() {
            String::new()
        } else {
            format!("{}", a.args)
        }
    )
}

/// 生成するファイルの一片：そのまま書く行か、整形するトークン列か。
pub(crate) enum Piece {
    Raw(String),
    Tokens {
        text: String,
        item: Option<String>,
        indent: usize,
    },
}

/// プロジェクトを書き下す。検査が通っていること（エラーがないこと）が前提。
pub fn lower(project: &Project, checked: &Checked) -> Result<Output, String> {
    lower_with_crates(project, checked, None)
}

/// [`lower`] の、依存クレートがコードで使われる名前を外から与える版（`verify` が `cargo metadata`
/// で取った名前。#49）。`None` なら `Ebykn.toml` と path の依存の `Cargo.toml` から決め、決められない
/// ものがあれば、名前が分からないとして扱う（std のパスとツールの属性を前提として出す側に倒す）。
pub fn lower_with_crates(
    project: &Project,
    checked: &Checked,
    crates: Option<&[String]>,
) -> Result<Output, String> {
    let report = RefCell::new(Report::default());
    let mut out = Output::default();
    // 名前を覆い隠す書き方（`use evil::Clone;`、同じ名前の依存クレート）を見分ける（#49）。
    let mut bindings = ebykn_syntax::macros::Bindings::default();
    let (names, known) = project.dependency_crate_names();
    for n in names.iter().chain(crates.into_iter().flatten()) {
        bindings.add_crate(n);
    }
    if !known && crates.is_none() {
        bindings.crate_names_unknown();
    }
    for m in &project.modules {
        let ts = TokenStream::from_str(&project.file(m.file).text)
            .map_err(|e| format!("{}: トークンに分けられません（{e}）", m.rel))?;
        bindings.scan(&ts);
    }
    for m in &project.modules {
        let Some(ast) = project.ast(m.file) else {
            continue;
        };
        let rel = m
            .rel
            .strip_suffix(".eyk")
            .map(|r| format!("{r}.rs"))
            .ok_or_else(|| format!("{}: .eyk ではありません", m.rel))?;
        let cx = Cx {
            index: &checked.index,
            facts: &checked.facts,
            crate_name: &project.manifest.name,
            module: m.module.clone(),
            self_ty: None,
            foreign_impl: None,
            self_struct: None,
            perms: BTreeMap::new(),
            rename: BTreeMap::new(),
            gives: None,
            exits: Vec::new(),
            try_kind: None,
            logic: false,
            ensures: false,
            report: &report,
            bindings: &bindings,
            rel: &rel,
        };
        let mut pieces = Vec::new();
        for a in &ast.attrs {
            pieces.push(Piece::Raw(attr(a)));
        }
        for l in HEADER_USES.lines() {
            pieces.push(Piece::Raw(l.to_string()));
        }
        pieces.push(Piece::Raw("verus! {".into()));
        pieces.push(Piece::Raw(String::new()));
        item::items(&cx, &ast.items, 0, &mut pieces);
        pieces.push(Piece::Raw("} // verus!".into()));
        out.files.push(assemble(rel, pieces)?);
    }
    let r = report.into_inner();
    out.admitted = r.admitted;
    out.unchecked = r.unchecked;
    out.external = r.external;
    out.verus_patterns = r.verus_patterns;
    out.macros = r.macros;
    Ok(out)
}

/// 一片ずつ整形してつなげ、項目の行の範囲を記録する。
fn assemble(rel: String, pieces: Vec<Piece>) -> Result<OutFile, String> {
    let mut text = String::new();
    let mut items = Vec::new();
    for p in pieces {
        match p {
            Piece::Raw(s) => {
                text.push_str(&s);
                text.push('\n');
            }
            Piece::Tokens {
                text: t,
                item,
                indent,
            } => {
                let ts = TokenStream::from_str(&t)
                    .map_err(|e| format!("{rel}: 書き下した Verus を読めません（{e}）:\n{t}"))?;
                let printed = print::print(&ts, indent);
                let start = text.lines().count() + 1;
                text.push_str(&printed);
                let end = text.lines().count();
                if let Some(name) = item {
                    items.push((start, end, name));
                }
                text.push('\n');
            }
        }
    }
    Ok(OutFile { rel, text, items })
}

/// 固定した vstd と prelude の置き場所（`build/verus/Cargo.toml` に書く）。
pub struct Deps<'a> {
    /// `=0.0.0-..`。
    pub vstd: &'a str,
    /// `build/verus/` から `ebykn_prelude` への相対パス（または絶対パス）。
    pub prelude_path: &'a str,
}

/// `build/verus/Cargo.toml`（仕様 §3.2、§1.3）。`[dependencies]` を写し、`vstd` と `ebykn_prelude` を
/// 足し、`panic = "abort"` を書く。`Ebykn.toml` の相対の `path` は `build/verus/` から見た形に直す。
pub fn cargo_toml(project: &Project, deps: &Deps) -> String {
    let m = &project.manifest;
    let mut s = String::new();
    s.push_str("[package]\n");
    s.push_str(&format!("name = {}\n", toml_str(&m.name)));
    s.push_str(&format!("version = {}\n", toml_str(&m.version)));
    s.push_str(&format!("edition = {}\n", toml_str(&m.edition)));
    s.push_str("\n[dependencies]\n");
    s.push_str(&format!("vstd = {}\n", toml_str(deps.vstd)));
    s.push_str(&format!(
        "ebykn_prelude = {{ path = {} }}\n",
        toml_str(deps.prelude_path)
    ));
    for (k, v) in &m.dependencies {
        s.push_str(&format!("{} = {}\n", toml_key(k), toml_value(v, true)));
    }
    s.push_str("\n[package.metadata.verus]\nverify = true\n");
    s.push_str(
        "\n[lints.rust]\nunexpected_cfgs = { level = \"warn\", check-cfg = ['cfg(verus_keep_ghost)'] }\n",
    );
    s.push_str("\n[profile.dev]\npanic = \"abort\"\n\n[profile.release]\npanic = \"abort\"\n");
    s
}

fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_key(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        k.to_string()
    } else {
        toml_str(k)
    }
}

/// `top` は依存の値の表の直下か（`path` を `build/verus/` から見た形に直す）。
fn toml_value(v: &TomlValue, top: bool) -> String {
    match v {
        TomlValue::Str(s) => toml_str(s),
        TomlValue::Raw(r) => r.clone(),
        TomlValue::Bool(b) => b.to_string(),
        TomlValue::Array(xs) => format!(
            "[{}]",
            xs.iter()
                .map(|x| toml_value(x, false))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TomlValue::Table(kvs) => format!(
            "{{ {} }}",
            kvs.iter()
                .map(|(k, x)| {
                    let val = match (k.as_str(), x) {
                        ("path", TomlValue::Str(p))
                            if top && !std::path::Path::new(p).is_absolute() =>
                        {
                            toml_str(&format!("../../{p}"))
                        }
                        _ => toml_value(x, false),
                    };
                    format!("{} = {val}", toml_key(k))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
