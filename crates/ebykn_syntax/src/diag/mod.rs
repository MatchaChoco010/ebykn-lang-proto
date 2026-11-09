//! 診断（仕様 §10.1）。
//!
//! 形式は `ファイル:行:列: コード: メッセージ`。文言は英語と日本語を持ち、既定は英語。
//! `--json` 用に rustc 互換の JSON も出せる。構文エラーのコードは `E0001`、`.eyk` の証明構文は `E0303`、プロジェクトの誤りは
//! `E0002`、未実装の機能は `E0003`、`ensures` の裸の `&mut` 引数は `E0004`（`OPEN.md`）。§6 の検査の文言は [`check`] にある。

mod check;

use std::fmt::Write as _;

pub use check::{CheckMsg, LoopPoint, Mismatch, Op, PermState, ProofOnlyClash, Unimpl};

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
    /// 局所の名前に Rust の予約語の raw の綴り（`r#type`）（仕様 §10.1 にない。#119）。
    E0103,
    /// 対応する `.eyk` のない `.eykp`。
    E0301,
    /// `.eykp` の置き場所の誤り。
    E0302,
    /// `.eyk` に証明構文。
    E0303,
    /// 剥がし不一致。
    E0304,
    /// ループ句が部分列でない。
    E0305,
    /// tuple struct・enum（・unit struct）に `owns` / `spec`。
    E0401,
    /// `impl T` の中での `invariant` / `view` の直接定義。
    E0403,
    /// プロジェクトの誤り（`Ebykn.toml`、モジュールのファイル。仕様 §10.1 にない。`OPEN.md`）。
    E0002,
    /// 未実装の機能（仕様 §10.1 にない。`OPEN.md`）。
    E0003,
    /// `ensures` に `&mut` 引数を `old` / `final` なしで書いた（仕様 §10.1 にない。#10、`OPEN.md`）。
    E0004,
    /// 消去フィールドの欠落。
    E0402,
    /// `uses` / `gives` の対象の型が直接 `Ptr` / `Cell` でない。
    E0501,
    /// 領域添字が識別子でない。
    E0502,
    E0503,
    E0504,
    E0505,
    E0506,
    E0507,
    E0508,
    E0509,
    E0510,
    E0511,
    E0512,
    /// `spec` フィールドを領域として使用。
    E0513,
    /// `uses` / `gives` 関数のメソッド呼び出し。
    E0514,
    /// `self` のない関数で `self@`。
    E0515,
    /// `owns` フィールドへの代入。
    E0516,
    /// `Ptr<T>` の `T` がゼロサイズ型（仕様 §10.1 にない。`docs/current-spec.md` §10.1）。
    E0517,
    /// 呼び出しの `uses` の領域が、呼ばれる側の `uses` の宣言と合わない（仕様 §10.1 にない。#19）。
    E0518,
    /// 呼び出しで渡す領域の状態の誤り（仕様 §10.1 にない。#19）。
    E0519,
    /// 絞り込み型を展開しない位置で使用（警告）。
    W0601,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::E0001 => "E0001",
            Code::E0101 => "E0101",
            Code::E0102 => "E0102",
            Code::E0103 => "E0103",
            Code::E0301 => "E0301",
            Code::E0302 => "E0302",
            Code::E0303 => "E0303",
            Code::E0304 => "E0304",
            Code::E0305 => "E0305",
            Code::E0401 => "E0401",
            Code::E0403 => "E0403",
            Code::E0002 => "E0002",
            Code::E0003 => "E0003",
            Code::E0004 => "E0004",
            Code::E0402 => "E0402",
            Code::E0501 => "E0501",
            Code::E0502 => "E0502",
            Code::E0503 => "E0503",
            Code::E0504 => "E0504",
            Code::E0505 => "E0505",
            Code::E0506 => "E0506",
            Code::E0507 => "E0507",
            Code::E0508 => "E0508",
            Code::E0509 => "E0509",
            Code::E0510 => "E0510",
            Code::E0511 => "E0511",
            Code::E0512 => "E0512",
            Code::E0513 => "E0513",
            Code::E0514 => "E0514",
            Code::E0515 => "E0515",
            Code::E0516 => "E0516",
            Code::E0517 => "E0517",
            Code::E0518 => "E0518",
            Code::E0519 => "E0519",
            Code::W0601 => "W0601",
        }
    }

    /// 警告か（`W` で始まるコード）。警告だけなら検査は通る。
    pub fn is_warning(self) -> bool {
        self.as_str().starts_with('W')
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
    /// E0303：`.eyk` に証明構文（仕様 §5.7）。証明構文は `.eykp` にだけ書ける。
    ProofSyntax { written: String },
    /// `.eykp` で、証明の文（`assert`、`proof { .. }` など）を文でない位置に書いた。
    ProofStmtOnly { written: String },
    /// Ebykn が中を読まないコード（マクロの引数など）の中の証明構文。`.eykp` でも書けない。
    ProofInOpaque { written: String },
    /// 証明の属性の引数の形が違う。`form` は正しい形。
    ProofAttrArgs { written: String, form: &'static str },
    /// 性質ファイル（仕様 §8）にだけ書ける。
    PropsOnly { written: String },
    /// 比較演算子の連鎖（Rust と同じく書けない）。
    ChainedComparison,
    /// `.eyk` で呼べるマクロの一覧（[`crate::ALLOWED_MACROS`]）にないマクロ（#16 の既定）。
    MacroNotAllowed { name: String },
    /// Ebykn が読まないコードを取り込む書き方（`#[path]`、`#[macro_use]`、Verus のマクロの
    /// 取り込みなど。#16 の既定）。
    Unread { written: String },
    /// 項目の外側の属性のほかの位置（`#![..]`、文・式・`let`・match の腕）の、Rust の組み込みでない
    /// 属性（#49。`crate::macros::non_builtin`）。`written` は属性のパス。
    AttrPosition { written: String },
    /// `.eyk` が直接参照しないクレート（vstd など。#16 の既定）。
    VstdDirect { name: String },
    /// std / core / alloc / vstd / ebykn_prelude という名前への改名（#16 の既定）。
    ReservedRename { written: String },
    /// その位置には書けないもの（文法にない組み合わせ）。
    NotAllowedHere { written: String },
    /// 構造体のパターンの `..` / `@..` に付けた属性（#95）。rustc は `..` の前の属性を拒否する
    /// （「expected identifier, found `..`」）。`rest` は書いた形。
    AttrOnRest { rest: &'static str },
    /// 予約語（`uses`。#19 の既定 P1）を名前に使った。
    ReservedWord { word: String },
    /// Rust の版で予約語になった名前（`gen`。[`crate::reserved::EDITION_KEYWORDS`]）を `r#` なしで
    /// 使った。検査が `Ebykn.toml` の `edition` を見て、その版より前なら捨てる（#104）。
    EditionKeyword { word: String, since: u32 },
    /// 呼び出しの引数の末尾でない位置の `uses`（#19 の既定 P1）。
    UsesOutsideCall,
    /// `take` / `put` / `borrow` の領域に、`self` でない値の消去フィールド（`t@nodes`）を書いた
    /// （仕様 §5.4 の `RegionExpr` は `self@x` か領域変数だけ。#84）。
    ErasedRegion {
        kw: &'static str,
        base: String,
        field: String,
    },
    /// E0101。
    ReservedPrefix { ident: String },
    /// E0102。
    VerifierAttr,
    /// E0103。局所の名前を束縛する位置の、Rust の予約語の raw の綴り（#119）。
    RawKeywordBinding { word: String },
    /// E0401。
    ErasedOnNonNamed {
        section: &'static str,
        name: String,
        kind: TypeKind,
    },
    /// E0403。
    DirectInvariantOrView { name: String },
    /// §3、§6 の検査（[`check`]）。
    Check(CheckMsg),
    /// Verus の句・証明の組み込み関数（[`crate::reserved::VERUS_BUILTIN_CALLS`]）。
    VerusBuiltin { name: String, written: String },
    /// 構文解析では判定できず検査に任せる名前（診断としては出さない。[`crate::Parsed::deferred`]）。
    Deferred(crate::reserved::DeferredName),
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
    /// 呼び出しの `uses` の後の領域（`r`、`&r`、`&mut r`、`self@x`）。
    UsesArg,
    /// 証明器の指定（`by(..)` の中）。
    ProverMode,
    /// `spec let` の初期値（`= e`）。
    SpecLetInit,
    /// `#![trigger ..]` の式（一つ以上）。
    TriggerExpr,
    /// `lemma group` の中の補題（一つ以上）。
    LemmaGroupMember,
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
            (Expected::UsesArg, Lang::En) => {
                "a region after `uses` (`r`, `&r`, `&mut r`, `&mut self@x`), separated by `,` and closed by the `)` of the call"
            }
            (Expected::UsesArg, Lang::Ja) => {
                "`uses` の後に、`,` で並べて呼び出しの `)` で閉じる領域（`r`、`&r`、`&mut r`、`&mut self@x`）"
            }
            (Expected::ProverMode, Lang::En) => {
                "a prover mode (`bit_vector`, `nonlinear_arith`, `integer_ring`, `compute` or `compute_only`)"
            }
            (Expected::LemmaGroupMember, Lang::En) => {
                "at least one lemma in `lemma group g { .. }`"
            }
            (Expected::LemmaGroupMember, Lang::Ja) => "`lemma group g { .. }` の中に一つ以上の補題",
            (Expected::TriggerExpr, Lang::En) => "at least one expression in `#![trigger ..]`",
            (Expected::TriggerExpr, Lang::Ja) => "`#![trigger ..]` の中に一つ以上の式",
            (Expected::SpecLetInit, Lang::En) => "`= expr` in `spec let` (`spec let p = e;`)",
            (Expected::SpecLetInit, Lang::Ja) => {
                "`spec let` の初期値の `= 式`（`spec let p = e;`）"
            }
            (Expected::ProverMode, Lang::Ja) => {
                "証明器の指定（`bit_vector`、`nonlinear_arith`、`integer_ring`、`compute`、`compute_only` のどれか）"
            }
        }
    }
}

impl Message {
    pub fn code(&self) -> Code {
        match self {
            Message::ReservedPrefix { .. } => Code::E0101,
            Message::VerifierAttr => Code::E0102,
            Message::RawKeywordBinding { .. } => Code::E0103,
            Message::ProofSyntax { .. } => Code::E0303,
            Message::ErasedOnNonNamed { .. } => Code::E0401,
            Message::DirectInvariantOrView { .. } => Code::E0403,
            Message::Check(m) => m.code(),
            _ => Code::E0001,
        }
    }

    /// `.eyk` の診断としての文言。
    pub fn text(&self, lang: Lang) -> String {
        self.text_in(lang, ".eyk")
    }

    /// 拡張子 `src`（`.eyk` か `.eykp`）のファイルの診断としての文言。`.eykp` は `.eyk` の写しなので
    /// `.eyk` と同じ規則に従うが、文言はそのファイルの拡張子で書く。
    pub fn text_in(&self, lang: Lang, src: &str) -> String {
        use Lang::{En, Ja};
        match (self, lang) {
            (Message::Check(m), _) => m.text(lang),
            (Message::VerusBuiltin { name, written }, En) => format!(
                "syntax error: `{written}` is Verus syntax and cannot be written in `{src}`; to use your own function or item named `{name}`, write its path from the crate (`crate::..::{name}`)"
            ),
            (Message::VerusBuiltin { name, written }, Ja) => format!(
                "構文エラー: `{written}` は Verus の書き方で、`{src}` には書けません。`{name}` という名前の自分の関数や項目を使うなら、クレートからのパス（`crate::..::{name}`）で書きます"
            ),
            (Message::Deferred(n), _) => n.written.clone(),
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
                let what = what.text(Ja);
                format!("構文エラー: {what}{}が必要です", check::ja_gap(what))
            }
            (Message::VerusSyntax { written, ebykn }, En) => format!(
                "syntax error: `{written}` is Verus syntax; in Ebykn write `{ebykn}`"
            ),
            (Message::VerusSyntax { written, ebykn }, Ja) => format!(
                "構文エラー: `{written}` は Verus の書き方です。Ebykn では `{ebykn}` と書きます"
            ),
            (Message::VerusOnly { written }, En) => format!(
                "syntax error: `{written}` is Verus syntax and cannot be written in `{src}`"
            ),
            (Message::VerusOnly { written }, Ja) => format!(
                "構文エラー: `{written}` は Verus の書き方で、`{src}` には書けません"
            ),
            (Message::ProofSyntax { written }, En) => format!(
                "`{written}` is proof syntax and cannot be written in `.eyk`; write it in the proof file (`proof/*.eykp`)"
            ),
            (Message::ProofSyntax { written }, Ja) => format!(
                "`{written}` は証明構文で、`.eyk` には書けません。証明ファイル（`proof/*.eykp`）に書きます"
            ),
            (Message::ProofInOpaque { written }, En) => format!(
                "syntax error: `{written}` cannot be written in code that Ebykn does not read (such as the arguments of a macro)"
            ),
            (Message::ProofInOpaque { written }, Ja) => format!(
                "構文エラー: `{written}` は、Ebykn が読まないコード（マクロの引数など）の中には書けません"
            ),
            (Message::ProofAttrArgs { written, form }, En) => {
                format!("syntax error: the arguments of `{written}` are malformed; write `{form}`")
            }
            (Message::ProofAttrArgs { written, form }, Ja) => {
                format!("構文エラー: `{written}` の引数の形が違います。`{form}` と書きます")
            }
            (Message::ProofStmtOnly { written }, En) => {
                format!("syntax error: `{written}` can only be written as a statement")
            }
            (Message::ProofStmtOnly { written }, Ja) => {
                format!("構文エラー: `{written}` は文としてだけ書けます")
            }
            (Message::PropsOnly { written }, En) => format!(
                "syntax error: `{written}` can only be written in a props file, which is not supported yet"
            ),
            (Message::PropsOnly { written }, Ja) => format!(
                "構文エラー: `{written}` は性質ファイルにだけ書けます（性質ファイルはまだ読めません）"
            ),
            (Message::ReservedWord { word }, En) => format!(
                "syntax error: `{word}` is a keyword in Ebykn and cannot be used as a name; write `r#{word}` to use it as a name"
            ),
            (Message::ReservedWord { word }, Ja) => format!(
                "構文エラー: `{word}` は Ebykn の予約語で、名前には使えません。名前に使うなら `r#{word}` と書きます"
            ),
            (Message::EditionKeyword { word, since }, En) => format!(
                "syntax error: `{word}` is a reserved keyword in Rust {since} and later (`edition` in `Ebykn.toml`) and cannot be used as a name; write `r#{word}` to use it as a name of a function, type, module, or field, and another name such as `{word}_` for a local name (variable, parameter, pattern binding)"
            ),
            (Message::EditionKeyword { word, since }, Ja) => format!(
                "構文エラー: `{word}` は Rust {since} 以降（`Ebykn.toml` の `edition`）の予約語で、名前には使えません。関数・型・モジュール・フィールドの名前に使うなら `r#{word}` と書き、局所の名前（変数、引数、パターンの束縛）には `{word}_` などの別の名前を使います"
            ),
            (Message::RawKeywordBinding { word }, En) => format!(
                "`r#{word}` cannot be used as a local name (variable, parameter, pattern binding, quantified variable, return value name, region, const generic parameter) because Verus cannot verify it; use another name such as `{word}_`"
            ),
            (Message::RawKeywordBinding { word }, Ja) => format!(
                "`r#{word}` は局所の名前（変数、引数、パターンの束縛、量化子の変数、戻り値の名前、領域、const generics の引数）には使えません。Verus が検証できないためです。`{word}_` などの別の名前を使います"
            ),
            (Message::UsesOutsideCall, En) => {
                "syntax error: `uses` passes regions only at the end of the arguments of a call (`f(x, uses &mut r)`)".into()
            }
            (Message::UsesOutsideCall, Ja) => {
                "構文エラー: `uses` で領域を渡せるのは、呼び出しの引数の末尾だけです（`f(x, uses &mut r)`）".into()
            }
            (Message::ChainedComparison, En) => {
                "syntax error: comparison operators cannot be chained; write `a < b && b < c`"
                    .into()
            }
            (Message::ChainedComparison, Ja) => {
                "構文エラー: 比較演算子は連ねて書けません。`a < b && b < c` と書きます".into()
            }
            (Message::MacroNotAllowed { name }, En) => format!(
                "syntax error: macro `{name}!` cannot be called in `{src}`; only these macros can: {}. To use your own macro, write the function that needs it as an `unchecked fn`",
                crate::ALLOWED_MACROS.join(", ")
            ),
            (Message::MacroNotAllowed { name }, Ja) => format!(
                "構文エラー: `{src}` ではマクロ `{name}!` を呼べません。呼べるのは次のマクロだけです: {}。自作のマクロが要るなら、それを使う関数を `unchecked fn` にします",
                crate::ALLOWED_MACROS.join("、")
            ),
            (Message::Unread { written }, En) => format!(
                "syntax error: `{written}` cannot be written in `{src}`, because it brings in code that Ebykn does not read"
            ),
            (Message::Unread { written }, Ja) => format!(
                "構文エラー: `{written}` は `{src}` に書けません。Ebykn が読まないコードを取り込むためです"
            ),
            (Message::AttrPosition { written }, En) => format!(
                "syntax error: the attribute `{written}` cannot be written here; an inner attribute (`#![..]`) or an attribute on a statement or expression must be one of Rust's built-in attributes (`allow`, `cfg`, `doc`, ..), and `#![feature(..)]` cannot be written. A derive or attribute macro of a dependency can be written on an item, where `ebykn verify` lists the items it makes as assumptions"
            ),
            (Message::AttrPosition { written }, Ja) => format!(
                "構文エラー: 属性 `{written}` はここに書けません。内側の属性（`#![..]`）と、文・式に付ける属性は、Rust の組み込みの属性（`allow`、`cfg`、`doc` など）だけです。`#![feature(..)]` は書けません。依存クレートの derive や属性マクロは項目に付けて書けます。そのとき、マクロが作る項目は `ebykn verify` が前提の一覧に出します"
            ),
            (Message::VstdDirect { name }, En) => format!(
                "syntax error: `{src}` cannot refer to the crate `{name}` directly; write the bare names instead (`Seq`, `Set`, `Map`, `seq!`, `set!`, `map!`, ..) without `{name}::`"
            ),
            (Message::VstdDirect { name }, Ja) => format!(
                "構文エラー: `{src}` はクレート `{name}` を直接参照できません。`{name}::` を書かずに、裸の名前（`Seq`、`Set`、`Map`、`seq!`、`set!`、`map!` など）を使います"
            ),
            (Message::ReservedRename { written }, En) => format!(
                "syntax error: `{written}` cannot be written in `{src}`; the names `std`, `core`, `alloc`, `vstd` and `ebykn_prelude` cannot be given to something else"
            ),
            (Message::ReservedRename { written }, Ja) => format!(
                "構文エラー: `{written}` は `{src}` に書けません。`std`、`core`、`alloc`、`vstd`、`ebykn_prelude` という名前をほかのものに付けることはできません"
            ),
            (Message::ErasedRegion { kw, base, field }, En) => format!(
                "syntax error: the region of `{kw}` cannot be `{base}@{field}`; a region in `take` / `put` / `borrow` is `self@field` or a region variable. If `{base}` is a reference (`&mut` / `&`), write this in a method of its type and use `self@{field}`. If you own `{base}`, take it apart with a pattern that binds every field (`let S {{ @{field}, a, b }} = {base};`), write `{kw} {field}[..]`, and build the value again with `@{field}: {field}` in the struct literal. Only when you make a new value, start from an empty region (`let mut {field}: Region<..> = Region::empty();`) and pass `@{field}: {field}` in its struct literal"
            ),
            (Message::ErasedRegion { kw, base, field }, Ja) => format!(
                "構文エラー: `{kw}` の領域に `{base}@{field}` は書けません。`take` / `put` / `borrow` の領域は `self@フィールド` か領域変数だけです。`{base}` が参照（`&mut` / `&`）なら、その型のメソッドに書き、`self@{field}` を使います。`{base}` を所有しているなら、すべてのフィールドを束縛するパターンで分解し（`let S {{ @{field}, a, b }} = {base};`）、`{kw} {field}[..]` と書いてから、構造体のリテラルに `@{field}: {field}` を書いて作り直します。新しい値を作るときだけ、空の領域から始め（`let mut {field}: Region<..> = Region::empty();`）、構造体のリテラルに `@{field}: {field}` を書いて渡します"
            ),
            (Message::AttrOnRest { rest }, En) => format!(
                "syntax error: an attribute cannot be written on `{rest}` in a struct pattern; Rust does not accept attributes before `..`"
            ),
            (Message::AttrOnRest { rest }, Ja) => format!(
                "構文エラー: 構造体のパターンの `{rest}` に属性は付けられません。Rust は `..` の前の属性を受け付けません"
            ),
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
                    TypeKind::TupleStruct => "タプル構造体",
                    TypeKind::UnitStruct => "ユニット構造体",
                    TypeKind::Enum => "列挙型",
                };
                format!(
                    "`{section} {{ .. }}` は名前付きフィールドを持つ構造体にだけ付けられますが、`{name}` は{kind}です"
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
            self.message.text_in(lang, src_ext(&file.path))
        )
    }

    /// rustc の `--error-format=json` と同じ形の 1 行。
    pub fn to_json(&self, file: &SourceFile, lang: Lang) -> String {
        let s = &self.span;
        let mut out = String::new();
        out.push_str("{\"$message_type\":\"diagnostic\",\"message\":");
        json_str(&mut out, &self.message.text_in(lang, src_ext(&file.path)));
        out.push_str(",\"code\":{\"code\":");
        json_str(&mut out, self.code().as_str());
        let level = if self.code().is_warning() {
            "warning"
        } else {
            "error"
        };
        let _ = write!(
            out,
            ",\"explanation\":null}},\"level\":\"{level}\",\"spans\":[{{\"file_name\":"
        );
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

/// 診断の文言に書く拡張子。`.eykp` のファイルなら `.eykp`、ほかは `.eyk`。
fn src_ext(path: &str) -> &'static str {
    if path.ends_with(".eykp") {
        ".eykp"
    } else {
        ".eyk"
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
