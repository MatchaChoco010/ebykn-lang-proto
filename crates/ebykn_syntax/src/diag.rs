//! 診断（仕様 §10.1）。
//!
//! 形式は `ファイル:行:列: コード: メッセージ`。文言は英語と日本語を持ち、既定は英語。
//! `--json` 用に rustc 互換の JSON も出せる。構文エラーのコードは `E0001`（`OPEN.md`）。

use std::fmt::Write as _;

use crate::span::{SourceFile, Span};

/// 文言の言語。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Ja,
}

/// 診断のコード。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// 構文エラー（仕様 §10.1 にない。`OPEN.md`）。
    E0001,
    /// `__eyk_` 接頭辞の使用。
    E0101,
    /// `#[verifier::..]` の使用。
    E0102,
    /// tuple struct・enum（・unit struct）に `owns` / `spec`。
    E0401,
    /// `impl T` の中での `invariant` / `view` の直接定義。
    E0403,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::E0001 => "E0001",
            Code::E0101 => "E0101",
            Code::E0102 => "E0102",
            Code::E0401 => "E0401",
            Code::E0403 => "E0403",
        }
    }
}

/// 型の種類（E0401 の文言用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    TupleStruct,
    UnitStruct,
    Enum,
}

/// 診断の中身。文言は [`Message::text`] で言語ごとに作る。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 字句として読めない（閉じていない括弧や文字列など）。
    Lex,
    /// 入力が途中で終わった。
    UnexpectedEof,
    /// 構文解析器（verus_syn）の文言。英語だけで、日本語の文言にもそのまま埋め込む。
    Parser(String),
    /// `what` が必要な位置に、別のものがある。
    Expected { what: Expected },
    /// Verus の書き方で、Ebykn には別の書き方がある。
    VerusSyntax { written: String, ebykn: String },
    /// Verus の書き方で、Ebykn の `.eyk` には対応する書き方がない。
    VerusOnly { written: String },
    /// 証明構文（仕様 §5.7）。`.eykp` にだけ書け、まだ対応していない。
    ProofSyntax { written: String },
    /// 性質ファイル（仕様 §8）にだけ書ける。
    PropsOnly { written: String },
    /// 比較演算子の連鎖（Rust と同じく書けない）。
    ChainedComparison,
    /// その位置には書けないもの（文法にない組み合わせ）。
    NotAllowedHere { written: String },
    /// E0101。
    ReservedPrefix { ident: String },
    /// E0102。
    VerifierAttr,
    /// E0401。
    ErasedOnNonNamed {
        section: &'static str,
        name: String,
        kind: TypeKind,
    },
    /// E0403。
    DirectInvariantOrView { name: String },
}

/// [`Message::Expected`] の対象。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// 関数の本体（`{ .. }` か `= e;`）。
    FnBody,
    /// 契約の句か本体。
    ClauseOrBody,
    /// 項目。
    Item,
    /// `uses` の対象（`p@`、`&p@`、`&mut p@`、`r: Region<..>`）。
    UsesItem,
    /// `gives` の対象（`q@`）。
    GivesItem,
    /// 領域添字（`r[p]`、`self@r[p]`）。
    RegionIndex,
    /// `unchecked` の後（`fn` か `lemma fn`）。
    UncheckedTarget,
    /// `closed` の後（`spec fn`）。
    SpecFn,
    /// `spec fn` の本体（`= e;`）。
    SpecFnBody,
    /// `lemma fn` の終わり（`;`）。
    LemmaFnEnd,
    /// trait のメソッドの本体（`;` か `{ .. }`）。
    TraitFnBody,
    /// `spec fn` の戻り値の型（`-> T`）。
    SpecFnRet,
    /// `impl spec` の中の項目（`spec fn` か `lemma fn`）。
    SpecOrLemmaFn,
    /// `and { .. }` / `or { .. }` の中の式（一つ以上）。
    AndOrExpr,
}

impl Expected {
    fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Expected::FnBody, Lang::En) => "a function body (`{ .. }` or `= expr;`)",
            (Expected::FnBody, Lang::Ja) => "関数の本体（`{ .. }` か `= 式;`）",
            (Expected::ClauseOrBody, Lang::En) => {
                "a contract clause (`requires`, `ensures`, ..) or a function body"
            }
            (Expected::ClauseOrBody, Lang::Ja) => {
                "契約の句（`requires`、`ensures` など）か関数の本体"
            }
            (Expected::Item, Lang::En) => "an item",
            (Expected::Item, Lang::Ja) => "項目",
            (Expected::UsesItem, Lang::En) => {
                "a permission (`p@`, `&p@`, `&mut p@`) or a region (`r: Region<..>`)"
            }
            (Expected::UsesItem, Lang::Ja) => {
                "権限（`p@`、`&p@`、`&mut p@`）か領域（`r: Region<..>`）"
            }
            (Expected::GivesItem, Lang::En) => "a permission of a named return value (`q@`)",
            (Expected::GivesItem, Lang::Ja) => "名前付き戻り値の権限（`q@`）",
            (Expected::RegionIndex, Lang::En) => "a region index (`r[p]` or `self@r[p]`)",
            (Expected::RegionIndex, Lang::Ja) => "領域添字（`r[p]` か `self@r[p]`）",
            (Expected::UncheckedTarget, Lang::En) => "`fn` or `lemma fn` after `unchecked`",
            (Expected::UncheckedTarget, Lang::Ja) => "`unchecked` の後に `fn` か `lemma fn`",
            (Expected::SpecFn, Lang::En) => "`spec fn` after `closed`",
            (Expected::SpecFn, Lang::Ja) => "`closed` の後に `spec fn`",
            (Expected::SpecFnBody, Lang::En) => "the body of a `spec fn` (`= expr;`)",
            (Expected::SpecFnBody, Lang::Ja) => "`spec fn` の本体（`= 式;`）",
            (Expected::LemmaFnEnd, Lang::En) => "`;` at the end of a `lemma fn`",
            (Expected::LemmaFnEnd, Lang::Ja) => "`lemma fn` の終わりの `;`",
            (Expected::TraitFnBody, Lang::En) => "`;` or a block `{ .. }`",
            (Expected::TraitFnBody, Lang::Ja) => "`;` かブロック `{ .. }`",
            (Expected::SpecFnRet, Lang::En) => "the return type of a `spec fn` (`-> Type`)",
            (Expected::SpecFnRet, Lang::Ja) => "`spec fn` の戻り値の型（`-> 型`）",
            (Expected::SpecOrLemmaFn, Lang::En) => "`spec fn` or `lemma fn` in `impl spec`",
            (Expected::SpecOrLemmaFn, Lang::Ja) => "`impl spec` の中に `spec fn` か `lemma fn`",
            (Expected::AndOrExpr, Lang::En) => {
                "at least one expression in `and { .. }` / `or { .. }`"
            }
            (Expected::AndOrExpr, Lang::Ja) => "`and { .. }` / `or { .. }` の中に一つ以上の式",
        }
    }
}

impl Message {
    pub fn code(&self) -> Code {
        match self {
            Message::ReservedPrefix { .. } => Code::E0101,
            Message::VerifierAttr => Code::E0102,
            Message::ErasedOnNonNamed { .. } => Code::E0401,
            Message::DirectInvariantOrView { .. } => Code::E0403,
            _ => Code::E0001,
        }
    }

    pub fn text(&self, lang: Lang) -> String {
        use Lang::{En, Ja};
        match (self, lang) {
            (Message::Lex, En) => {
                "syntax error: cannot read tokens here (unbalanced delimiter or unterminated literal)".into()
            }
            (Message::Lex, Ja) => {
                "構文エラー: ここからトークンを読めません（括弧の対応か、閉じていないリテラル）".into()
            }
            (Message::UnexpectedEof, En) => "syntax error: unexpected end of file".into(),
            (Message::UnexpectedEof, Ja) => "構文エラー: ファイルが途中で終わっています".into(),
            (Message::Parser(m), En) => format!("syntax error: {m}"),
            (Message::Parser(m), Ja) => format!("構文エラー: {m}"),
            (Message::Expected { what }, En) => {
                format!("syntax error: expected {}", what.text(En))
            }
            (Message::Expected { what }, Ja) => {
                format!("構文エラー: {}が必要です", what.text(Ja))
            }
            (Message::VerusSyntax { written, ebykn }, En) => format!(
                "syntax error: `{written}` is Verus syntax; in Ebykn write `{ebykn}`"
            ),
            (Message::VerusSyntax { written, ebykn }, Ja) => format!(
                "構文エラー: `{written}` は Verus の書き方です。Ebykn では `{ebykn}` と書きます"
            ),
            (Message::VerusOnly { written }, En) => format!(
                "syntax error: `{written}` is Verus syntax and cannot be written in `.eyk`"
            ),
            (Message::VerusOnly { written }, Ja) => format!(
                "構文エラー: `{written}` は Verus の書き方で、`.eyk` には書けません"
            ),
            (Message::ProofSyntax { written }, En) => format!(
                "syntax error: `{written}` is proof syntax; it can only be written in `.eykp`, which is not supported yet"
            ),
            (Message::ProofSyntax { written }, Ja) => format!(
                "構文エラー: `{written}` は証明構文で、`.eykp` にだけ書けます（`.eykp` はまだ読めません）"
            ),
            (Message::PropsOnly { written }, En) => format!(
                "syntax error: `{written}` can only be written in a props file, which is not supported yet"
            ),
            (Message::PropsOnly { written }, Ja) => format!(
                "構文エラー: `{written}` は性質ファイルにだけ書けます（性質ファイルはまだ読めません）"
            ),
            (Message::ChainedComparison, En) => {
                "syntax error: comparison operators cannot be chained; write `a < b && b < c`"
                    .into()
            }
            (Message::ChainedComparison, Ja) => {
                "構文エラー: 比較演算子は連ねて書けません。`a < b && b < c` と書きます".into()
            }
            (Message::NotAllowedHere { written }, En) => {
                format!("syntax error: `{written}` cannot be written here")
            }
            (Message::NotAllowedHere { written }, Ja) => {
                format!("構文エラー: ここには `{written}` を書けません")
            }
            (Message::ReservedPrefix { ident }, En) => format!(
                "identifier `{ident}` uses the prefix `__eyk_`, which is reserved for generated names"
            ),
            (Message::ReservedPrefix { ident }, Ja) => format!(
                "識別子 `{ident}` は、生成名のために予約された接頭辞 `__eyk_` を使っています"
            ),
            (Message::VerifierAttr, En) => {
                "`#[verifier::..]` attributes cannot be written in Ebykn".into()
            }
            (Message::VerifierAttr, Ja) => "`#[verifier::..]` 属性は Ebykn では書けません".into(),
            (Message::ErasedOnNonNamed { section, name, kind }, En) => {
                let kind = match kind {
                    TypeKind::TupleStruct => "a tuple struct",
                    TypeKind::UnitStruct => "a unit struct",
                    TypeKind::Enum => "an enum",
                };
                format!(
                    "`{section} {{ .. }}` can only follow a struct with named fields, but `{name}` is {kind}"
                )
            }
            (Message::ErasedOnNonNamed { section, name, kind }, Ja) => {
                let kind = match kind {
                    TypeKind::TupleStruct => "tuple struct",
                    TypeKind::UnitStruct => "unit struct",
                    TypeKind::Enum => "enum",
                };
                format!(
                    "`{section} {{ .. }}` は名前付きフィールドの struct にだけ付けられますが、`{name}` は {kind} です"
                )
            }
            (Message::DirectInvariantOrView { name }, En) => {
                let tr = trait_for(name);
                format!(
                    "`{name}` cannot be defined directly in `impl T`; write `impl {tr} for T` instead"
                )
            }
            (Message::DirectInvariantOrView { name }, Ja) => {
                let tr = trait_for(name);
                format!(
                    "`{name}` は `impl T` の中に直接定義できません。`impl {tr} for T` に書きます"
                )
            }
        }
    }
}

fn trait_for(name: &str) -> &'static str {
    if name == "view" { "View" } else { "Invariant" }
}

/// 一つの診断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub span: Span,
    pub message: Message,
}

impl Diagnostic {
    pub fn new(span: Span, message: Message) -> Self {
        Diagnostic { span, message }
    }

    pub fn code(&self) -> Code {
        self.message.code()
    }

    /// `ファイル:行:列: コード: メッセージ`。
    pub fn render(&self, file: &SourceFile, lang: Lang) -> String {
        format!(
            "{}:{}:{}: {}: {}",
            file.path,
            self.span.start.line,
            self.span.start.col,
            self.code().as_str(),
            self.message.text(lang)
        )
    }

    /// rustc の `--error-format=json` と同じ形の 1 行。
    pub fn to_json(&self, file: &SourceFile, lang: Lang) -> String {
        let s = &self.span;
        let mut out = String::new();
        out.push_str("{\"$message_type\":\"diagnostic\",\"message\":");
        json_str(&mut out, &self.message.text(lang));
        out.push_str(",\"code\":{\"code\":");
        json_str(&mut out, self.code().as_str());
        out.push_str(",\"explanation\":null},\"level\":\"error\",\"spans\":[{\"file_name\":");
        json_str(&mut out, &file.path);
        let _ = write!(
            out,
            ",\"byte_start\":{},\"byte_end\":{},\"line_start\":{},\"line_end\":{},\"column_start\":{},\"column_end\":{}",
            s.lo, s.hi, s.start.line, s.end.line, s.start.col, s.end.col
        );
        out.push_str(",\"is_primary\":true,\"text\":[],\"label\":null,\"suggested_replacement\":null,\"suggestion_applicability\":null,\"expansion\":null}],\"children\":[],\"rendered\":");
        json_str(&mut out, &format!("{}\n", self.render(file, lang)));
        out.push('}');
        out
    }
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
