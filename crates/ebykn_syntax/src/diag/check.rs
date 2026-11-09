//! §3（プロジェクト）と §6（静的検査）の診断の文言。

use super::{Code, Lang};
use crate::ast::PermMode;

/// 呼び出しで領域を渡す形（#19 の既定 P1）の、宣言の型の形。
fn pass_decl(m: PermMode) -> &'static str {
    match m {
        PermMode::Move => "Region<..>",
        PermMode::Shared => "&Region<..>",
        PermMode::Mut => "&mut Region<..>",
    }
}

fn pass_en(m: PermMode) -> &'static str {
    match m {
        PermMode::Move => "a move (`r`)",
        PermMode::Shared => "a shared borrow (`&r`)",
        PermMode::Mut => "a mutable borrow (`&mut r`)",
    }
}

fn pass_ja(m: PermMode) -> &'static str {
    match m {
        PermMode::Move => "移動（`r`）",
        PermMode::Shared => "共有の借用（`&r`）",
        PermMode::Mut => "可変の借用（`&mut r`）",
    }
}

/// 権限の状態（仕様 §6.2 の `State` と、Γ に無いこと）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PermState {
    Owned,
    /// `BorrowedShared`。
    Shared,
    /// `BorrowedMut`。
    Mut,
    /// Γ に無い。
    Absent,
}

impl PermState {
    /// 英語の状態の語（`is ..` の補語）。
    fn en(self) -> &'static str {
        match self {
            PermState::Owned => "owned",
            PermState::Shared => "borrowed (shared)",
            PermState::Mut => "borrowed (mut)",
            PermState::Absent => "not held",
        }
    }

    /// 日本語で、状態を述語にした形（「権限 `p@` を」に続く）。状態の語を名詞のまま文に埋め込むと
    /// 「ここではなしなので」のように読めなくなるので（#34）、活用の形ごとに持つ。
    /// 返すのは（連体形「〜ている」、連用形「〜し」、文末「〜ています」）。
    fn ja(self) -> (&'static str, &'static str, &'static str) {
        match self {
            PermState::Owned => ("所有している", "所有し", "所有しています"),
            PermState::Shared => ("共有で借りている", "共有で借り", "共有で借りています"),
            PermState::Mut => ("可変で借りている", "可変で借り", "可変で借りています"),
            PermState::Absent => ("持っていない", "持っておらず", "持っていません"),
        }
    }
}

/// 分岐・ループの点で食い違った権限。`a` が比べる基準の側、`b` が食い違った側。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub name: String,
    pub a: PermState,
    pub b: PermState,
}

/// 食い違いの一覧を、どちらの側がどの状態かが分かる文にする。`side_a` / `side_b` は基準の側と
/// 食い違った側の呼び名（英語は `in another branch` のような句、日本語は「ほかの分岐」のような名詞句）。
fn mismatches(diffs: &[Mismatch], lang: Lang, side_a: &str, side_b: &str) -> String {
    let parts = diffs.iter().map(|m| match lang {
        Lang::En => format!(
            "`{}@` is {} {side_a} but {} {side_b}",
            m.name,
            m.a.en(),
            m.b.en()
        ),
        Lang::Ja => format!(
            "`{}@` を{side_a}{}では{}、{side_b}{}では{}",
            m.name,
            ja_gap(side_a),
            m.a.ja().1,
            ja_gap(side_b),
            m.b.ja().2
        ),
    });
    let sep = if lang == Lang::En { "; " } else { "。" };
    parts.collect::<Vec<_>>().join(sep)
}

/// コードで終わる語句と、続く助詞の間に置く空白（ほかの文言と同じく `x` と助詞の間は空ける）。
pub(super) fn ja_gap(s: &str) -> &'static str {
    if s.ends_with('`') { " " } else { "" }
}

/// 権限を消費・借用する操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `p.free()`。
    Free,
    /// `put r[p];`。
    Put,
    /// `uses p@` の引数に渡す（移動）。
    Move,
    /// `leak p@;`。
    Leak,
    /// `uses &mut p@` の引数に渡す。
    MutBorrow,
    /// `uses &p@` の引数に渡す。
    SharedBorrow,
    /// `gives p@` で返す。
    Give,
}

impl Op {
    /// 英語は `cannot ..` に続く動詞句。日本語は「〜できません」まで含めた述語（動詞ごとに否定の形が違うので、
    /// 「できません」を後ろに付けると「戻すできません」のようになる。#34）。
    fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Op::Free, Lang::En) => "free it with `.free()`",
            (Op::Free, Lang::Ja) => "`.free()` で解放できません",
            (Op::Put, Lang::En) => "put it back with `put`",
            (Op::Put, Lang::Ja) => "`put` で戻せません",
            (Op::Move, Lang::En) => "move it to a `uses p@` parameter",
            (Op::Move, Lang::Ja) => "`uses p@` の引数に移動できません",
            (Op::Leak, Lang::En) => "leak it with `leak`",
            (Op::Leak, Lang::Ja) => "`leak` で捨てられません",
            (Op::MutBorrow, Lang::En) => "lend it to a `uses &mut p@` parameter",
            (Op::MutBorrow, Lang::Ja) => "`uses &mut p@` の引数に貸せません",
            (Op::SharedBorrow, Lang::En) => "lend it to a `uses &p@` parameter",
            (Op::SharedBorrow, Lang::Ja) => "`uses &p@` の引数に貸せません",
            (Op::Give, Lang::En) => "return it with `gives`",
            (Op::Give, Lang::Ja) => "`gives` で返せません",
        }
    }
}

/// ループの中で Γ を比べた点（仕様 §6.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopPoint {
    /// 本体の終端。
    End,
    /// `continue`。
    Continue,
    /// `break`。
    Break,
}

/// `.eykp` だけの項目の名前が何とぶつかったか（E0304）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofOnlyClash {
    /// `.eyk` に現れる名前か、Ebykn がソースのために使う名前。
    Source,
    /// 同じファイルのほかの `.eykp` だけの項目。
    ProofOnly,
}

/// 未実装の機能（`E0003`）。各マイルストーンで実装する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unimpl {
    /// `.eykp` があるときの `verify` / `build`（書き下しが `.eykp` を使うのは M3 の #67）。
    EykpLowering,
    /// `props/` の性質ファイル（M5）。
    PropsDir,
    /// `Ebykn.toml` の `[props]`（M5）。
    PropsTable,
    /// `Ebykn.toml` の `[ebykn-dependencies]`（M5）。
    EbyknDeps,
    /// 部分集合型（M4）。
    SubsetType,
    /// 契約・`spec fn`・`lemma fn` を持つ trait（M4）。
    ContractTrait,
    /// 型の不変条件を持つ型の `impl Drop`（M4。§6.4 の展開が `drop` に `requires` を足すが、Verus は
    /// `Drop` の `requires` を受け付けない）。
    InvariantDrop,
    /// `impl spec`（M5）。
    ImplSpec,
    /// `check --base`（M3）。
    Base,
    /// `uses` を持つ `async fn`（Verus が `Tracked(..)` の引数を扱えない。#101）。
    AsyncUses,
    /// `build --verified`（M5）。
    BuildVerified,
    /// クレートの外の型への `impl Invariant` / `impl View`（性質ファイル、M5）。
    ExternalTypeSpec,
    /// 関数の本体の中の Ebykn の項目（索引に入らない）。
    NestedItem,
    /// 中を読まないマクロの引数の中の権限の操作。
    PermInMacro,
    /// マクロの引数の中の `return` / `break` / `continue` / `?`（#21 の既定 D。抜ける前に権限を戻せない）。
    JumpInMacro,
    /// Rust のトークンのまま持つ式（`.await` など）の中の `return` / `break` / `continue` / `?`（同上）。
    JumpInRustExpr,
    /// 同じモジュールに同じ名前の定義が二つ以上ある（cfg で分けた定義。索引は cfg を評価しない）。
    CfgDuplicate,
    /// 関数の本体の中の `unchecked fn` / `lemma fn`（`verify` が証明なしの前提として一覧に出せない）。
    NestedAssumption,
    /// ジェネリクスの引数に付けた `#[cfg(..)]`（`fn f<#[cfg(x)] T>`。#95）。
    CfgOnGenericParam,
    /// パターン（クロージャの引数、構造体パターンの欄と `@x`）に付けた `#[cfg(..)]`（#95）。
    CfgOnPattern,
    /// 戻り値の型を `Option<..>` / `Result<..>` と書いていない関数の、`borrow mut` の中の `?`
    /// （#21 の既定 D。失敗の経路で権限を戻す展開の形を決められない）。
    TryInBorrowMut,
    /// §6 が権限の操作として読むか、ブロックを抜ける文・式・欄に付いた `#[cfg]`（と、代入・二項演算・型変換の文
    /// と `take` / `put` / `leak` / 仕様の代入に付いた属性）。§6 の検査が、コンパイルされないコードを見てしまう（#27）。
    CfgStmt,
    /// 安定版の Rust が式の属性を許さない位置の、式の属性（`let x = #[allow(..)] a;`、`if` の条件、`return` の値
    /// など）。Verus の toolchain では通るが、Verus なしのビルドで落ちる。
    ExprAttr,
    /// std のマクロの引数の中の、cfg・`cfg_attr`・lint のほかの属性（トークンのまま持つので、derive や
    /// 属性マクロを見られない。#27）。
    AttrInMacro,
    /// std のマクロの引数の中の、安定版の Rust が許さない位置の属性（`assert!(#[allow(..)] (a > 0))`）。
    /// Verus の toolchain では通るが、Verus なしのビルドで落ちる（PR #59 の再確認）。
    AttrPosInMacro,
}

impl Unimpl {
    fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Unimpl::EykpLowering, Lang::En) => {
                "`verify` and `build` with proof files (`.eykp`) are implemented in M3: the lowering does not read `.eykp` yet, so it would drop the proofs written there (`check` reads `.eykp` and checks that stripping it gives the `.eyk`)"
            }
            (Unimpl::EykpLowering, Lang::Ja) => {
                "証明ファイル（`.eykp`）があるときの `verify` と `build` は M3 で実装します。書き下しがまだ `.eykp` を読まないので、そこに書いた証明を捨ててしまうためです（`check` は `.eykp` を読み、剥がすと `.eyk` に一致することを確かめます）"
            }
            (Unimpl::PropsDir, Lang::En) => "props files (`props/`) are implemented in M5",
            (Unimpl::PropsDir, Lang::Ja) => "性質ファイル（`props/`）は M5 で実装します",
            (Unimpl::PropsTable, Lang::En) => "`[props]` in `Ebykn.toml` is implemented in M5",
            (Unimpl::PropsTable, Lang::Ja) => "`Ebykn.toml` の `[props]` は M5 で実装します",
            (Unimpl::EbyknDeps, Lang::En) => {
                "`[ebykn-dependencies]` in `Ebykn.toml` is implemented in M5"
            }
            (Unimpl::EbyknDeps, Lang::Ja) => {
                "`Ebykn.toml` の `[ebykn-dependencies]` は M5 で実装します"
            }
            (Unimpl::SubsetType, Lang::En) => {
                "subset types (`type T = B where P;`) are implemented in M4"
            }
            (Unimpl::SubsetType, Lang::Ja) => {
                "部分集合型（`type T = B where P;`）は M4 で実装します"
            }
            (Unimpl::ContractTrait, Lang::En) => {
                "traits with contracts, `spec fn` or `lemma fn` are implemented in M4"
            }
            (Unimpl::ContractTrait, Lang::Ja) => {
                "契約・`spec fn`・`lemma fn` を持つ trait は M4 で実装します"
            }
            (Unimpl::InvariantDrop, Lang::En) => {
                "`impl Drop` for a type with an invariant (`impl Invariant`) is implemented in M4: §6.4 would add the invariant to `drop` as `requires`, and Verus does not allow `requires` on `Drop`"
            }
            (Unimpl::InvariantDrop, Lang::Ja) => {
                "型の不変条件（`impl Invariant`）を持つ型の `impl Drop` は M4 で実装します。§6.4 の展開が `drop` に不変条件の `requires` を足しますが、Verus は `Drop` の `requires` を受け付けません"
            }
            (Unimpl::ImplSpec, Lang::En) => "`impl spec` is implemented in M5",
            (Unimpl::ImplSpec, Lang::Ja) => "`impl spec` は M5 で実装します",
            (Unimpl::Base, Lang::En) => "`check --base` is implemented in M3",
            (Unimpl::Base, Lang::Ja) => "`check --base` は M3 で実装します",
            (Unimpl::AsyncUses, Lang::En) => {
                "an `async fn` cannot have `uses` yet, because Verus cannot read the `Tracked(..)` parameters it is lowered to; move the code that uses the permissions or regions into a non-async `fn` with `uses`, and call it from the `async fn`"
            }
            (Unimpl::AsyncUses, Lang::Ja) => {
                "`async fn` にはまだ `uses` を書けません。書き下した `Tracked(..)` の引数を Verus が読めないためです。権限や領域を使う処理を `uses` を持つ async でない `fn` に移し、`async fn` からそれを呼びます"
            }
            (Unimpl::BuildVerified, Lang::En) => {
                "`build --verified` needs `ebykn status`, which is implemented in M5"
            }
            (Unimpl::BuildVerified, Lang::Ja) => {
                "`build --verified` は `ebykn status` が要るので M5 で実装します"
            }
            (Unimpl::ExternalTypeSpec, Lang::En) => {
                "`impl Invariant` / `impl View` for a type outside this crate belongs in a props file, which is implemented in M5"
            }
            (Unimpl::ExternalTypeSpec, Lang::Ja) => {
                "クレートの外の型への `impl Invariant` / `impl View` は性質ファイルに書くもので、M5 で実装します"
            }
            (Unimpl::NestedItem, Lang::En) => {
                "`impl Invariant`, `impl View`, structs with `owns` / `spec` and modules inside a function body are not supported yet; move them to the module level"
            }
            (Unimpl::NestedItem, Lang::Ja) => {
                "関数の本体の中の `impl Invariant`・`impl View`・`owns` / `spec` を持つ構造体・モジュールはまだ扱えません。モジュールの直下に移します"
            }
            (Unimpl::CfgDuplicate, Lang::En) => {
                "this name is defined more than once in the module (for example under different `#[cfg]`); Ebykn's index does not evaluate `#[cfg]` yet, so it cannot tell which definition remains"
            }
            (Unimpl::CfgDuplicate, Lang::Ja) => {
                "この名前はモジュールの中で二度以上定義されています（`#[cfg]` で分けた定義など）。Ebykn の索引はまだ `#[cfg]` を評価しないので、どちらの定義が残るかを決められません"
            }
            (Unimpl::NestedAssumption, Lang::En) => {
                "`unchecked fn` and `lemma fn` inside a function body are not supported yet, because `verify` could not list them as assumptions; move them to the module level"
            }
            (Unimpl::NestedAssumption, Lang::Ja) => {
                "関数の本体の中の `unchecked fn` と `lemma fn` はまだ扱えません。`verify` が証明なしの前提として一覧に出せないためです。モジュールの直下に移します"
            }
            (Unimpl::TryInBorrowMut, Lang::En) => {
                "`?` inside `borrow mut` is supported only when the function's return type is written as `Option<..>` or `Result<..>`, because the permission is put back before returning on the failure path, which needs the shape of the failure value; write the `match` explicitly"
            }
            (Unimpl::TryInBorrowMut, Lang::Ja) => {
                "`borrow mut` の中の `?` は、関数の戻り値の型を `Option<..>` か `Result<..>` と書いたときだけ扱えます。失敗の経路では権限を戻してから返すので、失敗の値の形が要るためです。`match` で明示的に書きます"
            }
            (Unimpl::CfgOnGenericParam, Lang::En) => {
                "`#[cfg(..)]` on a generic parameter is not supported yet, because Ebykn's checks read the parameter list without evaluating `cfg`; write a separate item for each configuration"
            }
            (Unimpl::CfgOnGenericParam, Lang::Ja) => {
                "ジェネリクスの引数に付けた `#[cfg(..)]` はまだ扱えません。Ebykn の検査は `cfg` を評価せずに引数の並びを読むためです。構成ごとに別の項目に分けます"
            }
            (Unimpl::CfgOnPattern, Lang::En) => {
                "`#[cfg(..)]` on a pattern (closure parameters and fields of struct patterns, including `@x`) is not supported yet, because Ebykn's checks bind the names in the pattern without evaluating `cfg`; write the code for each configuration separately"
            }
            (Unimpl::CfgOnPattern, Lang::Ja) => {
                "パターン（クロージャの引数、構造体パターンの欄と `@x`）に付けた `#[cfg(..)]` はまだ扱えません。Ebykn の検査は `cfg` を評価せずにパターンの名前を束縛するためです。構成ごとにコードを分けます"
            }
            (Unimpl::JumpInMacro, Lang::En) => {
                "`return`, `break`, `continue` and `?` inside the arguments of a macro are not supported yet, because Ebykn does not rewrite them: a jump out of a `borrow mut` block there would skip putting the permission back, and the permission checks would not see it; move the jump out of the macro (if it is inside a closure, bind the closure with `let` outside the macro)"
            }
            (Unimpl::JumpInMacro, Lang::Ja) => {
                "マクロの引数の中の `return`・`break`・`continue`・`?` はまだ扱えません。Ebykn はこれらを書き換えないので、そこから `borrow mut` のブロックを抜けると権限を戻さずに抜け、権限の検査も見ないためです。抜ける文をマクロの外に出します（クロージャの中にあるなら、クロージャを `let` でマクロの外に出します）"
            }
            (Unimpl::JumpInRustExpr, Lang::En) => {
                "`return`, `break`, `continue` and `?` inside this expression (such as `.await`, which Ebykn keeps as Rust tokens) are not supported yet, because Ebykn does not read it: a jump out there would skip putting a `borrow mut` permission back, and the permission checks would not see it; compute the value with `let` before the expression"
            }
            (Unimpl::JumpInRustExpr, Lang::Ja) => {
                "この式（Ebykn が Rust のトークンのまま持つ `.await` など）の中の `return`・`break`・`continue`・`?` はまだ扱えません。Ebykn はこの式の中を読まないので、そこから抜けると `borrow mut` の権限を戻さずに抜け、権限の検査も見ないためです。値は式の前で `let` に束縛してから使います"
            }
            (Unimpl::CfgStmt, Lang::En) => {
                "`#[cfg]` on a statement, an expression (such as an array or tuple element or an argument) or a struct literal field that operates on permissions (`take`, `put`, `leak`, `free`, a call that consumes or returns a permission, a spec assignment `x@f = e`) or leaves the block (`return`, `break`, `continue`) is not supported yet, and neither is any attribute on an assignment, binary operation or cast statement or on `take` / `put` / `leak` / a spec assignment, because the check of §6 would see code that is not compiled (and attributes on assignment, binary operation and cast statements are not stable Rust); put the conditional code in separate functions"
            }
            (Unimpl::CfgStmt, Lang::Ja) => {
                "権限を操作する（`take`、`put`、`leak`、`free`、権限を消費する・返す呼び出し、仕様の代入 `x@f = e`）か、ブロックを抜ける（`return`、`break`、`continue`）文・式（配列とタプルの要素、引数など）・構造体リテラルの欄に付けた `#[cfg]` と、代入・二項演算・型変換の文と `take` / `put` / `leak` / 仕様の代入に付けた属性は、まだ扱えません。§6 の検査が、コンパイルされないコードを見てしまうためです（代入・二項演算・型変換の文の属性は、安定版の Rust でも書けない）。条件付きのコードは別の関数に分けます"
            }
            (Unimpl::ExprAttr, Lang::En) => {
                "an attribute on this expression is not supported, because stable Rust accepts attributes on expressions only at the start of a statement, on the tail expression of a block, on array and tuple elements and on call and method arguments (elsewhere it is `attributes on expressions are experimental`, so the build without Verus fails); move the attribute to the statement"
            }
            (Unimpl::ExprAttr, Lang::Ja) => {
                "この式の属性は扱えません。安定版の Rust が式の属性を許すのは、文の先頭、ブロックの末尾、配列とタプルの要素、呼び出しとメソッドの引数だけで、それ以外では「attributes on expressions are experimental」になり、Verus なしのビルドが落ちるためです。属性は文に付けます"
            }
            (Unimpl::AttrInMacro, Lang::En) => {
                "attributes other than `cfg`, `cfg_attr` and lint attributes (`allow`, `warn`, `deny`, `forbid`, `expect`) are not supported yet inside the arguments of this macro, because Ebykn does not read them (a derive or attribute macro there could generate code that is not checked); move the code out of the macro"
            }
            (Unimpl::AttrInMacro, Lang::Ja) => {
                "このマクロの引数の中では、`cfg`・`cfg_attr`・lint の属性（`allow`、`warn`、`deny`、`forbid`、`expect`）のほかの属性はまだ扱えません。Ebykn が引数を読まないためです（derive や属性マクロが、検査しないコードを作りうる）。コードをマクロの外に出します"
            }
            (Unimpl::AttrPosInMacro, Lang::En) => {
                "an attribute at this position inside the arguments of this macro is not supported, because Ebykn keeps the arguments as tokens and can tell only these positions where stable Rust accepts an attribute: the elements of `vec!` and the start of statements, match arms and fields inside `{ .. }` (outer attributes only); stable Rust rejects some other positions with `attributes on expressions are experimental`, so the build without Verus would fail; move the attribute or the code out of the macro"
            }
            (Unimpl::AttrPosInMacro, Lang::Ja) => {
                "このマクロの引数の中のこの位置の属性は扱えません。Ebykn は引数をトークンのまま持つので、安定版の Rust が属性を許す位置のうち見分けられるのは、`vec!` の要素と、`{ .. }` の中の文・match の腕・欄の頭（外側の属性だけ）です。ほかには安定版の Rust が「attributes on expressions are experimental」で拒否する位置があり、Verus なしのビルドが落ちうるためです。属性かコードをマクロの外に出します"
            }
            (Unimpl::PermInMacro, Lang::En) => {
                "permissions cannot be used in the arguments of this macro yet, because Ebykn does not read them; compute the value outside the macro"
            }
            (Unimpl::PermInMacro, Lang::Ja) => {
                "このマクロの引数は Ebykn が読まないので、その中ではまだ権限を使えません。値はマクロの外で求めます"
            }
        }
    }
}

/// §3 と §6 の診断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckMsg {
    // E0002：プロジェクト
    /// `Ebykn.toml` が見つからない。
    ManifestNotFound {
        dir: String,
    },
    /// `Ebykn.toml` を TOML として読めない。`detail` は TOML の構文解析器の英語の文言。
    ManifestParse {
        detail: String,
    },
    /// `Ebykn.toml` に必要な鍵がない（`package.name` など）。
    ManifestMissing {
        key: String,
    },
    /// `Ebykn.toml` の知らない鍵。
    ManifestUnknown {
        key: String,
    },
    /// `Ebykn.toml` の値の型が違う。`expected` は英語の型名（`string`、`table`）。
    ManifestType {
        key: String,
        expected: &'static str,
    },
    /// `src/lib.eyk` も `src/main.eyk` もない。
    CrateRootMissing,
    /// `src/lib.eyk` と `src/main.eyk` の両方がある。
    CrateRootAmbiguous,
    /// `mod foo;` のファイルがない。
    ModuleNotFound {
        name: String,
        a: String,
        b: String,
    },
    /// `mod foo;` のファイルが二つある。
    ModuleAmbiguous {
        name: String,
        a: String,
        b: String,
    },
    /// 同じモジュールを二度宣言した。
    ModuleDuplicate {
        name: String,
    },
    /// `Ebykn.toml` の `[verus]` が、この `ebykn` の固定した Verus と違う。
    VerusPinMismatch {
        key: &'static str,
        written: String,
        pinned: String,
    },
    /// ファイルを読めない。
    ReadFailed {
        path: String,
        detail: String,
    },
    /// `[dependencies]` の鍵が、std などの名前（`ebykn_syntax::reserved` の `RESERVED_CRATE_NAMES` と
    /// `RESERVED_DEPENDENCY_NAMES`）。
    /// その名前で依存クレートを使うと、`::core::..` のパスが依存クレートを指す（#49、#16 の既定）。
    ReservedDependency {
        name: String,
    },

    // E0003
    Unimplemented(Unimpl),

    // E0301 / E0302：証明ファイルの置き場所（仕様 §3.1）
    /// `proof/<p>.eykp` に対応する `src/<p>.eyk` のモジュールがない。`exists` は、ファイルはあるが
    /// クレートのモジュールとして読まれていない（`mod` で宣言していない）か。
    ProofWithoutSource {
        src: String,
        exists: bool,
    },
    /// `proof/` の外の `.eykp`、または `proof/` の中の `.eyk`。
    ProofMisplaced {
        eyk: bool,
    },

    // E0304 / E0305：剥がし一致（仕様 §6.1）
    /// `.eykp` を剥がすと `.eyk` と一致しない。`item` は最初に異なる項目（`fn f` など。項目の外なら
    /// `None`）、`other` は `.eyk` 側の位置（`src/lib.eyk:3:5`）、`found` / `expected` は最初に異なる
    /// トークン（`.eykp` 側 / `.eyk` 側。ファイルの終わりなら `None`）。
    StripMismatch {
        item: Option<String>,
        other: String,
        found: Option<String>,
        expected: Option<String>,
    },
    /// `.eykp` だけの項目（剥がす `spec fn`・本体付きの `lemma fn`・`lemma group`）の名前が使えない。
    /// `item` は `spec fn ok` などの表示。
    ProofOnlyName {
        item: String,
        clash: ProofOnlyClash,
    },
    /// ループの `.eyk` 側の句（`invariant e` / `decreases e`）が、`.eykp` 側の句の列に順序を保って
    /// 現れない。`clause` は見つからなかった `.eyk` 側の句、`other` はその位置。
    LoopClauseNotKept {
        clause: String,
        other: String,
    },

    // E0004
    /// `ensures` の中の、`old(..)` / `final(..)` の下にない `&mut` 引数（#10）。`written` は書いた形
    /// （`self`、`x`、`p@`、`*p`）。`perm` は `uses &mut p@` の権限なら `Some(p)`。
    BareMutInEnsures {
        written: String,
        perm: Option<String>,
    },
    /// 事後条件の、中を読まないマクロの引数の中の裸の `&mut` 引数。
    BareMutInMacro {
        written: String,
    },
    /// 契約の、中を読まないマクロの引数の中の `final(..)`（Verus が受け付けない）。
    FinalInMacro,
    /// `old(e)` / `final(e)` の `e` が、`&mut` の引数、`uses &mut p@` の `p@` / `*p`、それらの
    /// 消去フィールドのどれでもない。
    OldFinalArg,

    // E0001
    /// `take` / `put` / `borrow` の後の `x[..]` の `x` が領域でない。
    NotARegion {
        name: String,
        keyword: &'static str,
    },
    /// 呼び出しの `uses` の後に、領域でない名前を書いた（#19 の既定 P1）。
    NotARegionArg {
        name: String,
    },
    /// `impl Invariant for T` の中身の形が違う。
    InvariantImplShape,
    /// `impl View for T` の中に `type V` と `spec fn` 以外がある。
    ViewImplShape,
    /// `Invariant` / `View` を `impl Invariant for T` 以外の綴り（パス付き、改名、`use`）で書いた。
    EbyknTraitSpelling {
        written: String,
    },

    // E0402
    /// 構造体リテラル・パターンに消去フィールドがない。
    MissingErasedField {
        ty: String,
        field: String,
        pattern: bool,
    },
    /// `@name` の `name` が消去フィールドでない。
    NotErasedField {
        ty: String,
        field: String,
    },

    // E0501
    UsesNotParam {
        name: String,
    },
    UsesType {
        name: String,
    },
    UsesRegionType {
        name: String,
    },
    GivesNotRet {
        name: String,
    },
    GivesType {
        name: String,
    },

    // E0502
    RegionIndexNotIdent,

    // E0503
    PermAlreadyHeld {
        name: String,
    },

    // E0504
    PermNotOwned {
        name: String,
        op: Op,
        state: PermState,
    },
    /// `uses` の引数に、権限を持つ変数でない式を渡した。
    UsesArgNotVar {
        param: String,
    },

    // E0505
    PermLeft {
        names: Vec<String>,
    },
    /// 権限を返す式（`Ptr::new(..)` や `gives` を持つ関数の呼び出し）を `let X = ..;` で受けていない。
    PermDiscarded,

    // E0506
    AssignHeld {
        name: String,
    },

    // E0507
    ShadowHeld {
        name: String,
    },
    /// 契約の式で、権限を持つ名前 `name` を量化子・クロージャ・パターンで束縛し直した中の
    /// `p@` / `*p`（`written`）。書き下しは外側の引数の権限を指す（#10 のレビュー）。
    PermOfRebound {
        written: String,
        name: String,
    },

    // E0508
    WriteShared {
        name: String,
    },

    // E0509
    ConsumeBorrowed {
        name: String,
        op: Op,
    },

    // E0510
    BranchMismatch {
        diffs: Vec<Mismatch>,
    },

    // E0511
    LoopMismatch {
        diffs: Vec<Mismatch>,
        at: LoopPoint,
    },

    // E0512
    PermInClosure {
        name: String,
    },

    // E0513
    SpecFieldAsRegion {
        field: String,
    },

    // E0514
    MethodCallUses {
        method: String,
    },

    // E0515
    SelfErasedWithoutSelf,

    // E0516
    AssignOwnsField {
        field: String,
    },

    // E0517
    /// `Ptr<T>` の `T` がゼロサイズ型（#12）。`ty` は書かれた `T`。
    PtrZeroSized {
        ty: String,
    },

    // E0518：呼び出しの `uses` の領域が、呼ばれる側の `uses` の宣言と合わない（#19 の既定 P1）
    /// 領域の数が違う。`expected` は宣言の順の書き方（`&mut r` など）。
    RegionArgCount {
        callee: String,
        expected: Vec<String>,
        found: usize,
    },
    /// 渡し方（`r` / `&r` / `&mut r`）が違う。`param` は宣言の名前。
    RegionArgMode {
        callee: String,
        param: String,
        expected: PermMode,
        found: PermMode,
    },

    // E0519：呼び出しで渡す領域の状態（#19 の既定 P1）
    /// 移動した領域を使った。
    RegionMoved {
        name: String,
    },
    /// 借りた領域（`uses r: &Region<..>` / `&mut Region<..>`）か `self@x` を移動で渡した。
    RegionNotMovable {
        region: String,
    },
    /// 共有で借りた領域（`uses r: &Region<..>`）を `&mut` で渡した。
    RegionSharedAsMut {
        name: String,
    },
    /// 同じ呼び出しで同じ領域を二度渡し、少なくとも一つが `&mut` か移動。
    RegionPassedTwice {
        region: String,
    },

    // W0601
    RefinementNested {
        ty: String,
    },
}

impl CheckMsg {
    pub fn code(&self) -> Code {
        use CheckMsg::*;
        match self {
            ManifestNotFound { .. }
            | ManifestParse { .. }
            | ManifestMissing { .. }
            | ManifestUnknown { .. }
            | ManifestType { .. }
            | CrateRootMissing
            | CrateRootAmbiguous
            | ModuleNotFound { .. }
            | ModuleAmbiguous { .. }
            | ModuleDuplicate { .. }
            | VerusPinMismatch { .. }
            | ReadFailed { .. }
            | ReservedDependency { .. } => Code::E0002,
            Unimplemented(_) => Code::E0003,
            ProofWithoutSource { .. } => Code::E0301,
            ProofMisplaced { .. } => Code::E0302,
            StripMismatch { .. } | ProofOnlyName { .. } => Code::E0304,
            LoopClauseNotKept { .. } => Code::E0305,
            BareMutInEnsures { .. } | BareMutInMacro { .. } | FinalInMacro | OldFinalArg => {
                Code::E0004
            }
            NotARegion { .. }
            | NotARegionArg { .. }
            | InvariantImplShape
            | ViewImplShape
            | EbyknTraitSpelling { .. } => Code::E0001,
            MissingErasedField { .. } | NotErasedField { .. } => Code::E0402,
            UsesNotParam { .. }
            | UsesType { .. }
            | UsesRegionType { .. }
            | GivesNotRet { .. }
            | GivesType { .. } => Code::E0501,
            RegionIndexNotIdent => Code::E0502,
            PermAlreadyHeld { .. } => Code::E0503,
            PermNotOwned { .. } | UsesArgNotVar { .. } => Code::E0504,
            PermLeft { .. } | PermDiscarded => Code::E0505,
            AssignHeld { .. } => Code::E0506,
            ShadowHeld { .. } | PermOfRebound { .. } => Code::E0507,
            WriteShared { .. } => Code::E0508,
            ConsumeBorrowed { .. } => Code::E0509,
            BranchMismatch { .. } => Code::E0510,
            LoopMismatch { .. } => Code::E0511,
            PermInClosure { .. } => Code::E0512,
            SpecFieldAsRegion { .. } => Code::E0513,
            MethodCallUses { .. } => Code::E0514,
            SelfErasedWithoutSelf => Code::E0515,
            AssignOwnsField { .. } => Code::E0516,
            PtrZeroSized { .. } => Code::E0517,
            RegionArgCount { .. } | RegionArgMode { .. } => Code::E0518,
            RegionMoved { .. }
            | RegionNotMovable { .. }
            | RegionSharedAsMut { .. }
            | RegionPassedTwice { .. } => Code::E0519,
            RefinementNested { .. } => Code::W0601,
        }
    }

    pub fn text(&self, lang: Lang) -> String {
        use CheckMsg::*;
        use Lang::{En, Ja};
        match (self, lang) {
            (ManifestNotFound { dir }, En) => {
                format!("`Ebykn.toml` was not found in `{dir}` or its parent directories")
            }
            (ManifestNotFound { dir }, Ja) => {
                format!("`{dir}` とその上のディレクトリに `Ebykn.toml` がありません")
            }
            (ManifestParse { detail }, En) => format!("cannot read `Ebykn.toml`: {detail}"),
            (ManifestParse { detail }, Ja) => {
                format!("`Ebykn.toml` を読めません: {detail}")
            }
            (ManifestMissing { key }, En) => format!("`Ebykn.toml` has no `{key}`"),
            (ManifestMissing { key }, Ja) => format!("`Ebykn.toml` に `{key}` がありません"),
            (ManifestUnknown { key }, En) => format!("unknown key `{key}` in `Ebykn.toml`"),
            (ManifestUnknown { key }, Ja) => format!("`Ebykn.toml` の `{key}` は知らない鍵です"),
            (ManifestType { key, expected }, En) => {
                format!("`{key}` in `Ebykn.toml` must be a {expected}")
            }
            (ManifestType { key, expected }, Ja) => {
                let t = match *expected {
                    "string" => "文字列",
                    "table" => "表",
                    _ => expected,
                };
                format!("`Ebykn.toml` の `{key}` は{t}にします")
            }
            (CrateRootMissing, En) => {
                "the crate root is missing; create `src/lib.eyk` or `src/main.eyk`".into()
            }
            (CrateRootMissing, Ja) => {
                "クレートルートがありません。`src/lib.eyk` か `src/main.eyk` を作ります".into()
            }
            (CrateRootAmbiguous, En) => {
                "both `src/lib.eyk` and `src/main.eyk` exist; keep only one crate root".into()
            }
            (CrateRootAmbiguous, Ja) => {
                "`src/lib.eyk` と `src/main.eyk` の両方があります。クレートルートは一つにします"
                    .into()
            }
            (ModuleNotFound { name, a, b }, En) => {
                format!("file not found for module `{name}`; create `{a}` or `{b}`")
            }
            (ModuleNotFound { name, a, b }, Ja) => {
                format!("モジュール `{name}` のファイルがありません。`{a}` か `{b}` を作ります")
            }
            (ModuleAmbiguous { name, a, b }, En) => {
                format!("module `{name}` has two files, `{a}` and `{b}`; remove one")
            }
            (ModuleAmbiguous { name, a, b }, Ja) => format!(
                "モジュール `{name}` のファイルが `{a}` と `{b}` の二つあります。一つにします"
            ),
            (ModuleDuplicate { name }, En) => format!("module `{name}` is declared twice"),
            (ModuleDuplicate { name }, Ja) => format!("モジュール `{name}` を二度宣言しています"),
            (VerusPinMismatch { key, written, pinned }, En) => format!(
                "`verus.{key}` in `Ebykn.toml` is `{written}`, but this `ebykn` uses the Verus pinned at `{pinned}`"
            ),
            (VerusPinMismatch { key, written, pinned }, Ja) => format!(
                "`Ebykn.toml` の `verus.{key}` は `{written}` ですが、この `ebykn` が使う固定した Verus は `{pinned}` です"
            ),
            (ReadFailed { path, detail }, En) => format!("cannot read `{path}`: {detail}"),
            (ReadFailed { path, detail }, Ja) => format!("`{path}` を読めません: {detail}"),
            (ReservedDependency { name }, En) => format!(
                "the dependency `{name}` in `Ebykn.toml` cannot use the name `{name}`; the names `std`, `core`, `alloc`, `vstd`, `ebykn_prelude`, `verus_builtin` and `verus_builtin_macros` cannot be given to a dependency (rename it, e.g. `my_{name} = {{ package = \"..\" }}`)"
            ),
            (ReservedDependency { name }, Ja) => format!(
                "`Ebykn.toml` の依存クレート `{name}` に、`{name}` という名前は付けられません。`std`、`core`、`alloc`、`vstd`、`ebykn_prelude`、`verus_builtin`、`verus_builtin_macros` は依存クレートの名前にできません（`my_{name} = {{ package = \"..\" }}` のように別の名前にします）"
            ),

            (Unimplemented(u), En) => format!("not implemented yet: {}", u.text(En)),
            (Unimplemented(u), Ja) => format!("未実装: {}", u.text(Ja)),

            (ProofWithoutSource { src, exists: false }, En) => format!(
                "this proof file has no source file `{src}`; `proof/<p>.eykp` belongs to the module in `src/<p>.eyk`"
            ),
            (ProofWithoutSource { src, exists: false }, Ja) => format!(
                "この証明ファイルに対応する `{src}` がありません。`proof/<p>.eykp` は `src/<p>.eyk` のモジュールの証明です"
            ),
            (ProofWithoutSource { src, exists: true }, En) => format!(
                "`{src}`, which this proof file belongs to, is not a module of the crate; declare it with `mod`"
            ),
            (ProofWithoutSource { src, exists: true }, Ja) => format!(
                "この証明ファイルに対応する `{src}` は、クレートのモジュールではありません。`mod` で宣言します"
            ),
            (ProofMisplaced { eyk: false }, En) => {
                "a proof file (`.eykp`) must be under `proof/`; `proof/<p>.eykp` belongs to `src/<p>.eyk`".into()
            }
            (ProofMisplaced { eyk: false }, Ja) => {
                "証明ファイル（`.eykp`）は `proof/` の下に置きます。`proof/<p>.eykp` が `src/<p>.eyk` の証明です".into()
            }
            (ProofMisplaced { eyk: true }, En) => {
                "a source file (`.eyk`) cannot be under `proof/`; put the source in `src/`, and the proof in `proof/` with the extension `.eykp`".into()
            }
            (ProofMisplaced { eyk: true }, Ja) => {
                "`proof/` の下にソースファイル（`.eyk`）は置けません。ソースは `src/` に、証明は拡張子 `.eykp` で `proof/` に置きます".into()
            }
            (StripMismatch { item, other, found, expected }, En) => {
                let at = match item {
                    Some(i) => format!(" in `{i}`"),
                    None => String::new(),
                };
                let tok = |t: &Option<String>| match t {
                    Some(t) => format!("`{t}`"),
                    None => "the end of the file".into(),
                };
                format!(
                    "stripping the proofs from this file does not give its source{at}: here it has {}, but `{other}` has {}; the proof file may change only proofs, not the code or the contracts (run `ebykn sync` to bring changes of `src/` into the proof file)",
                    tok(found),
                    tok(expected)
                )
            }
            (StripMismatch { item, other, found, expected }, Ja) => {
                let at = match item {
                    Some(i) => format!("`{i}` の"),
                    None => String::new(),
                };
                let tok = |t: &Option<String>| match t {
                    Some(t) => format!("`{t}`"),
                    None => "ファイルの終わり".into(),
                };
                format!(
                    "このファイルから証明を剥がしても、ソースと一致しません。{at}ここは {}、`{other}` は {} です。証明ファイルで変えてよいのは証明だけで、コードと契約は変えられません（`src/` の変更を証明ファイルに取り込むには `ebykn sync` を実行します）",
                    tok(found),
                    tok(expected)
                )
            }
            (ProofOnlyName { item, clash: ProofOnlyClash::Source }, En) => format!(
                "the proof-only item `{item}` uses a name that the source (`.eyk`) uses or that Ebykn uses for it (`invariant`, `view` and the names of `ebykn_prelude`); an item only in the proof file could hide what the source refers to (a glob import or a trait method) and change the meaning of its code and contracts, so give it another name"
            ),
            (ProofOnlyName { item, clash: ProofOnlyClash::Source }, Ja) => format!(
                "証明ファイルだけの項目 `{item}` の名前は、ソース（`.eyk`）が使う名前か、Ebykn がソースのために使う名前（`invariant`、`view`、`ebykn_prelude` の名前）です。証明ファイルだけの項目は、ソースが指すもの（glob の import やトレイトのメソッド）を覆い、コードと契約の意味を変えうるので、別の名前にします"
            ),
            (ProofOnlyName { item, clash: ProofOnlyClash::ProofOnly }, En) => format!(
                "the proof-only item `{item}` has the same name as another proof-only item in this file; give it another name"
            ),
            (ProofOnlyName { item, clash: ProofOnlyClash::ProofOnly }, Ja) => format!(
                "証明ファイルだけの項目 `{item}` の名前が、このファイルのほかの証明ファイルだけの項目と同じです。別の名前にします"
            ),
            (LoopClauseNotKept { clause, other }, En) => format!(
                "the loop clause `{clause}` of the source (`{other}`) is missing here or out of order; the proof file may only add `invariant` / `decreases` clauses to a loop, keeping the source's clauses in the same order"
            ),
            (LoopClauseNotKept { clause, other }, Ja) => format!(
                "ソース（`{other}`）のループの句 `{clause}` が、ここにないか、順序が違います。証明ファイルではループに `invariant` / `decreases` の句を足せるだけで、ソースの句は同じ順序で残します"
            ),

            (NotARegion { name, keyword }, En) => format!(
                "syntax error: `{name}` is not a region, so `{keyword} {name}[..]` cannot be written; a region is an `owns` field (`self@{name}`) or a variable of type `Region<..>`"
            ),
            (NotARegion { name, keyword }, Ja) => format!(
                "構文エラー: `{name}` は領域ではないので、`{keyword} {name}[..]` とは書けません。領域は `owns` フィールド（`self@{name}`）か `Region<..>` 型の変数です"
            ),
            (NotARegionArg { name }, En) => format!(
                "syntax error: `{name}` is not a region, so it cannot be passed after `uses`; a region is an `owns` field (`self@{name}`) or a variable of type `Region<..>`"
            ),
            (NotARegionArg { name }, Ja) => format!(
                "構文エラー: `{name}` は領域ではないので、`uses` の後に書いて渡せません。領域は `owns` フィールド（`self@{name}`）か `Region<..>` 型の変数です"
            ),
            (InvariantImplShape, En) => {
                "syntax error: `impl Invariant for T` must contain exactly one item, `spec fn invariant(&self) -> bool = ..;`".into()
            }
            (EbyknTraitSpelling { written }, En) => format!(
                "syntax error: `{written}` cannot be written; Ebykn's `Invariant` and `View` are always in scope and are written only as `impl Invariant for T` / `impl View for T` (no path, rename or generics)"
            ),
            (EbyknTraitSpelling { written }, Ja) => format!(
                "構文エラー: `{written}` とは書けません。Ebykn の `Invariant` と `View` は常に使える名前で、`impl Invariant for T` / `impl View for T` とだけ書きます（パス・改名・ジェネリクスなし）"
            ),
            (ViewImplShape, En) => {
                "syntax error: `impl View for T` can only contain `type V = ..;` and `spec fn view(&self) -> V = ..;`".into()
            }
            (ViewImplShape, Ja) => {
                "構文エラー: `impl View for T` の中には `type V = ..;` と `spec fn view(&self) -> V = ..;` だけを書きます".into()
            }
            (InvariantImplShape, Ja) => {
                "構文エラー: `impl Invariant for T` の中には `spec fn invariant(&self) -> bool = ..;` を一つだけ書きます".into()
            }

            (MissingErasedField { ty, field, pattern }, En) => {
                if *pattern {
                    format!(
                        "the pattern for `{ty}` omits the `owns` field `{field}`; write `@{field}` or `@..` (which leaks the permissions)"
                    )
                } else {
                    format!(
                        "the literal of `{ty}` does not give the erased field `{field}`; write `@{field}: ..` or `..base`"
                    )
                }
            }
            (MissingErasedField { ty, field, pattern }, Ja) => {
                if *pattern {
                    format!(
                        "`{ty}` のパターンが `owns` フィールド `{field}` を省いています。`@{field}` か `@..`（権限を捨てる）を書きます"
                    )
                } else {
                    format!(
                        "`{ty}` のリテラルに消去フィールド `{field}` がありません。`@{field}: ..` か `..base` を書きます"
                    )
                }
            }
            (NotErasedField { ty, field }, En) => {
                format!("`{field}` is not an `owns` or `spec` field of `{ty}`")
            }
            (NotErasedField { ty, field }, Ja) => {
                format!("`{field}` は `{ty}` の `owns` / `spec` フィールドではありません")
            }

            (UsesNotParam { name }, En) => {
                format!("`uses {name}@` must name a parameter, but `{name}` is not a parameter")
            }
            (UsesNotParam { name }, Ja) => {
                format!("`uses {name}@` には引数の名前を書きますが、`{name}` は引数ではありません")
            }
            (UsesType { name }, En) => format!(
                "the type of `{name}` must be written directly as `Ptr<..>`, `Cell<..>`, `&Ptr<..>` or `&Cell<..>` to use `{name}@`"
            ),
            (UsesType { name }, Ja) => format!(
                "`{name}@` を使うには、`{name}` の型を直接 `Ptr<..>`、`Cell<..>`、`&Ptr<..>`、`&Cell<..>` と書きます"
            ),
            (UsesRegionType { name }, En) => format!(
                "the type of the erased parameter `{name}` must be `Region<..>`, `&Region<..>` or `&mut Region<..>`"
            ),
            (UsesRegionType { name }, Ja) => format!(
                "消去される引数 `{name}` の型は `Region<..>`、`&Region<..>`、`&mut Region<..>` にします"
            ),
            (GivesNotRet { name }, En) => format!(
                "`gives {name}@` must name the named return value, like `-> ({name}: Ptr<..>)`"
            ),
            (GivesNotRet { name }, Ja) => format!(
                "`gives {name}@` には名前付き戻り値の名前を書きます（`-> ({name}: Ptr<..>)`）"
            ),
            (GivesType { name }, En) => format!(
                "the type of the return value `{name}` must be written directly as `Ptr<..>` or `Cell<..>` to give `{name}@`"
            ),
            (GivesType { name }, Ja) => format!(
                "`{name}@` を返すには、戻り値 `{name}` の型を直接 `Ptr<..>` か `Cell<..>` と書きます"
            ),

            (RegionIndexNotIdent, En) => {
                "the index of a region must be a variable name, like `r[p]`".into()
            }
            (RegionIndexNotIdent, Ja) => {
                "領域の添字には変数の名前を書きます（`r[p]`）".into()
            }

            (PermAlreadyHeld { name }, En) => format!(
                "the permission `{name}@` is already held here; put it back before taking it again"
            ),
            (PermAlreadyHeld { name }, Ja) => format!(
                "権限 `{name}@` はすでにここで持っています。もう一度取り出す前に戻します"
            ),

            (PermNotOwned { name, op, state }, En) => format!(
                "cannot {} because the permission `{name}@` is {} here",
                op.text(En),
                state.en()
            ),
            (PermNotOwned { name, op, state }, Ja) => format!(
                "権限 `{name}@` をここでは{}ので、{}",
                state.ja().0,
                op.text(Ja)
            ),
            (UsesArgNotVar { param }, En) => format!(
                "the argument for `{param}`, which `uses` a permission, must be a variable that holds the permission"
            ),
            (UsesArgNotVar { param }, Ja) => format!(
                "権限を `uses` する引数 `{param}` には、権限を持つ変数を渡します"
            ),

            (PermLeft { names }, En) => format!(
                "the permission(s) {} are still owned here; free, put back, move or `leak` them (only `gives` return values may remain)",
                names
                    .iter()
                    .map(|n| format!("`{n}@`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            (PermLeft { names }, Ja) => format!(
                "権限 {} をここでまだ所有しています。解放・`put`・移動・`leak` のどれかをします（残してよいのは `gives` の戻り値だけです）",
                names
                    .iter()
                    .map(|n| format!("`{n}@`"))
                    .collect::<Vec<_>>()
                    .join("、")
            ),

            (PermDiscarded, En) => {
                "the permission returned here is discarded; bind the result to a variable with `let x = ..;`".into()
            }
            (PermDiscarded, Ja) => {
                "ここで返る権限が捨てられます。結果を `let x = ..;` で変数に束縛します".into()
            }
            (AssignHeld { name }, En) => format!(
                "cannot assign to `{name}` while its permission `{name}@` is held"
            ),
            (AssignHeld { name }, Ja) => format!(
                "権限 `{name}@` を持っている間は、`{name}` に代入できません"
            ),

            (ShadowHeld { name }, En) => format!(
                "cannot bind `{name}` again while its permission `{name}@` is held"
            ),
            (ShadowHeld { name }, Ja) => format!(
                "権限 `{name}@` を持っている間は、`{name}` を束縛し直せません"
            ),
            (PermOfRebound { written, name }, En) => format!(
                "`{written}` refers to the permission of the parameter `{name}`, but `{name}` is bound again around it (by a quantifier, closure, pattern or `let`); give the bound variable another name"
            ),
            (PermOfRebound { written, name }, Ja) => format!(
                "`{written}` は引数 `{name}` の権限を指しますが、その外側で `{name}` を束縛し直しています（量化子、クロージャ、パターン、`let`）。束縛する変数の名前を変えます"
            ),

            (WriteShared { name }, En) => format!(
                "cannot write through `{name}` because the permission `{name}@` is only borrowed (shared)"
            ),
            (WriteShared { name }, Ja) => format!(
                "権限 `{name}@` は共有の借用なので、`{name}` を通して書き込めません"
            ),

            (ConsumeBorrowed { name, op }, En) => format!(
                "cannot {} inside `borrow` of `{name}`; the permission `{name}@` is only borrowed in the block",
                op.text(En)
            ),
            (ConsumeBorrowed { name, op }, Ja) => format!(
                "`{name}` の `borrow` の中では {}。権限 `{name}@` はブロックの中で借りているだけです",
                op.text(Ja)
            ),

            (BranchMismatch { diffs }, En) => format!(
                "the branches end holding different permissions: {}",
                mismatches(
                    diffs,
                    En,
                    "in the first branch that does not diverge",
                    "in another branch"
                )
            ),
            (BranchMismatch { diffs }, Ja) => format!(
                "分岐の終わりで持っている権限が違います。{}",
                mismatches(diffs, Ja, "途中で抜けない最初の分岐", "ほかの分岐")
            ),

            (LoopMismatch { diffs, at }, En) => {
                let (head, a, b) = match at {
                    LoopPoint::End => (
                        "at the start of the loop body than at its end",
                        "at the start of the body",
                        "at its end",
                    ),
                    LoopPoint::Continue => (
                        "at the start of the loop body than at this `continue`",
                        "at the start of the body",
                        "at this `continue`",
                    ),
                    LoopPoint::Break => (
                        "after the loop than at this `break`",
                        "after the loop",
                        "at this `break`",
                    ),
                };
                format!(
                    "the loop holds different permissions {head}: {}",
                    mismatches(diffs, En, a, b)
                )
            }
            (LoopMismatch { diffs, at }, Ja) => {
                let (head, a, b) = match at {
                    LoopPoint::End => ("ループの本体の始めと終わり", "本体の始め", "本体の終わり"),
                    LoopPoint::Continue => (
                        "ループの本体の始めとこの `continue`",
                        "本体の始め",
                        "この `continue`",
                    ),
                    LoopPoint::Break => ("ループの後とこの `break`", "ループの後", "この `break`"),
                };
                format!(
                    "{head}{}で持っている権限が違います。{}",
                    ja_gap(head),
                    mismatches(diffs, Ja, a, b)
                )
            }

            (PermInClosure { name }, En) => format!(
                "permissions cannot be used inside a closure, but this uses `{name}`"
            ),
            (PermInClosure { name }, Ja) => format!(
                "クロージャの中では権限を使えませんが、ここで `{name}` を使っています"
            ),

            (SpecFieldAsRegion { field }, En) => format!(
                "`self@{field}` is a `spec` field, not a region; only `owns` fields can be indexed here"
            ),
            (SpecFieldAsRegion { field }, Ja) => format!(
                "`self@{field}` は `spec` フィールドで、領域ではありません。ここで添字を付けられるのは `owns` フィールドだけです"
            ),

            (MethodCallUses { method }, En) => format!(
                "`{method}` uses or gives permissions, so it cannot be called as a method `x.{method}(..)`; call it by path (`Type::{method}(x, ..)`) or as `self.{method}(..)`"
            ),
            (MethodCallUses { method }, Ja) => format!(
                "`{method}` は権限を `uses` / `gives` するので、メソッド `x.{method}(..)` の形では呼べません。パス（`Type::{method}(x, ..)`）か `self.{method}(..)` で呼びます"
            ),

            (RegionArgCount { callee, expected, found }, En) => {
                let list = expected.join(", ");
                if expected.is_empty() {
                    format!("`{callee}` takes no regions; remove `uses ..` from this call")
                } else if *found == 0 {
                    format!(
                        "`{callee}` takes regions; pass them at the end of the arguments as `uses {list}`"
                    )
                } else {
                    format!(
                        "`{callee}` takes {} regions, `uses {list}` in this order, but this call passes {found}",
                        expected.len()
                    )
                }
            }
            (RegionArgCount { callee, expected, found }, Ja) => {
                let list = expected.join(", ");
                if expected.is_empty() {
                    format!("`{callee}` は領域を受け取りません。この呼び出しの `uses ..` を消します")
                } else if *found == 0 {
                    format!(
                        "`{callee}` は領域を受け取ります。引数の末尾に `uses {list}` と書いて渡します"
                    )
                } else {
                    format!(
                        "`{callee}` が受け取る領域は `uses {list}` の {} 個（この順）ですが、この呼び出しは {found} 個渡しています",
                        expected.len()
                    )
                }
            }
            (
                RegionArgMode {
                    callee,
                    param,
                    expected,
                    found,
                },
                En,
            ) => format!(
                "`{callee}` declares `uses {param}: {}`, so pass it as {} (this call passes it as {})",
                pass_decl(*expected),
                pass_en(*expected),
                pass_en(*found)
            ),
            (
                RegionArgMode {
                    callee,
                    param,
                    expected,
                    found,
                },
                Ja,
            ) => format!(
                "`{callee}` は `uses {param}: {}` と宣言しているので、{}で渡します（この呼び出しは{}で渡しています）",
                pass_decl(*expected),
                pass_ja(*expected),
                pass_ja(*found)
            ),
            (RegionMoved { name }, En) => format!(
                "the region `{name}` was moved into a call (`uses {name}`), so it cannot be used after that"
            ),
            (RegionMoved { name }, Ja) => format!(
                "領域 `{name}` は呼び出しに移動した（`uses {name}`）ので、その後では使えません"
            ),
            (RegionNotMovable { region }, En) => format!(
                "`{region}` is borrowed, not owned, so it cannot be moved into a call; pass it as `&mut {region}` or `&{region}`"
            ),
            (RegionNotMovable { region }, Ja) => format!(
                "`{region}` は借りている領域で、持っていないので、呼び出しに移動できません。`&mut {region}` か `&{region}` で渡します"
            ),
            (RegionSharedAsMut { name }, En) => format!(
                "`{name}` is borrowed as shared (`uses {name}: &Region<..>`), so it cannot be passed as `&mut {name}`"
            ),
            (RegionSharedAsMut { name }, Ja) => format!(
                "`{name}` は共有で借りている領域（`uses {name}: &Region<..>`）なので、`&mut {name}` では渡せません"
            ),
            (RegionPassedTwice { region }, En) => format!(
                "`{region}` is passed twice in this call while one of them is `&mut` or a move; a region can be lent mutably to only one parameter at a time"
            ),
            (RegionPassedTwice { region }, Ja) => format!(
                "この呼び出しは `{region}` を二度渡していて、一方が `&mut` か移動です。領域を `&mut` で貸せるのは一度に一つの引数だけです"
            ),

            (SelfErasedWithoutSelf, En) => {
                "`self@..` can only be used in a function that takes `self`".into()
            }
            (SelfErasedWithoutSelf, Ja) => {
                "`self@..` は `self` を取る関数の中でだけ使えます".into()
            }

            (AssignOwnsField { field }, En) => format!(
                "cannot assign to the `owns` field `{field}`; change a region with `take`, `put` and `borrow`"
            ),
            (AssignOwnsField { field }, Ja) => format!(
                "`owns` フィールド `{field}` には代入できません。領域は `take`・`put`・`borrow` で変えます"
            ),

            (BareMutInEnsures { written, perm }, En) => {
                let what = match perm {
                    Some(p) => format!(
                        "`{written}` refers to the permission borrowed with `uses &mut {p}@`"
                    ),
                    None => format!("`{written}` is a `&mut` argument"),
                };
                format!(
                    "{what}, so a postcondition (`ensures` / `returns`) must say which value it means: write `final({written})` for the value after the call or `old({written})` for the value at entry"
                )
            }
            (BareMutInEnsures { written, perm }, Ja) => {
                let what = match perm {
                    Some(p) => format!("`{written}` は `uses &mut {p}@` で借りた権限を指すので"),
                    None => format!("`{written}` は `&mut` の引数なので"),
                };
                format!(
                    "{what}、事後条件（`ensures` / `returns`）ではどちらの値かを書きます。呼び出しの後の値なら `final({written})`、入口の値なら `old({written})` と書きます"
                )
            }

            (BareMutInMacro { written }, En) => format!(
                "`{written}` is a `&mut` argument, so a postcondition (`ensures` / `returns`) must say which value it means; inside the arguments of this macro write `old({written})` for the value at entry, but `final(..)` cannot be written there, so for the value after the call write the condition outside the macro with `final({written})` (for example `*final({written}) == 3` instead of `matches!(*{written}, 3)`)"
            ),
            (BareMutInMacro { written }, Ja) => format!(
                "`{written}` は `&mut` の引数なので、事後条件（`ensures` / `returns`）ではどちらの値かを書きます。このマクロの引数の中では、入口の値なら `old({written})` と書けますが、`final(..)` は書けません。呼び出しの後の値なら、条件をマクロの外で `final({written})` を使って書きます（例：`matches!(*{written}, 3)` ではなく `*final({written}) == 3`）"
            ),
            (FinalInMacro, En) => {
                "`final(..)` cannot be written inside the arguments of this macro (Verus does not accept it there); write the condition outside the macro (for example `*final(x) == 3` instead of `matches!(*final(x), 3)`)".into()
            }
            (FinalInMacro, Ja) => {
                "このマクロの引数の中には `final(..)` を書けません（Verus が受け付けません）。条件はマクロの外で書きます（例：`matches!(*final(x), 3)` ではなく `*final(x) == 3`）".into()
            }
            (OldFinalArg, En) => {
                "`old(..)` / `final(..)` can only be applied to a `&mut` argument (`self`, `x`), the permission of `uses &mut p@` (`p@`, `*p`), or an erased field of these (`self@f`); apply it to the argument and access the field outside (for example `old(self).len` instead of `old(self.len)`)".into()
            }
            (OldFinalArg, Ja) => {
                "`old(..)` / `final(..)` を付けられるのは、`&mut` の引数（`self`、`x`）、`uses &mut p@` の権限（`p@`、`*p`）、それらの消去フィールド（`self@f`）だけです。引数に付けて、フィールドは外で読みます（例：`old(self.len)` ではなく `old(self).len`）".into()
            }

            (PtrZeroSized { ty }, En) => format!(
                "`{ty}` is a zero-sized type, so it cannot be the target of `Ptr`: every allocation of a zero-sized type has the same address and cannot be told apart; use `Cell<{ty}>` if you need distinct identities"
            ),
            (PtrZeroSized { ty }, Ja) => format!(
                "`{ty}` はゼロサイズ型なので `Ptr` の指す先にできません。ゼロサイズ型の確保はどれも同じアドレスになり、互いを区別できません。識別が要るなら `Cell<{ty}>` を使ってください"
            ),

            (RefinementNested { ty }, En) => format!(
                "the invariant of `{ty}` is not added to contracts at this position; only `T`, `&T`, `&mut T`, `Option<T>` and `Result<T, E>` are expanded"
            ),
            (RefinementNested { ty }, Ja) => format!(
                "この位置では `{ty}` の不変条件を契約に足しません。展開するのは `T`、`&T`、`&mut T`、`Option<T>`、`Result<T, E>` だけです"
            ),
        }
    }
}
