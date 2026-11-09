//! 検査が見つけた事実。書き下し（`ebykn_lower`）は型を知らないので、権限付きの `*X` か、
//! どの `x[i]` が領域添字か、呼び出しに足す権限の引数などを、検査の結果から引く。
//! 鍵は式・文の位置（[`SpanKey`]）。

use std::collections::BTreeMap;

use ebykn_syntax::ast::PermMode;
use ebykn_syntax::span::Span;

use crate::index::{ErasedKind, ItemPath};
use crate::sig::{FnExpansion, Wrap};

/// 位置の鍵（ファイルとバイトの範囲）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpanKey(pub u32, pub usize, pub usize);

impl From<Span> for SpanKey {
    fn from(s: Span) -> Self {
        SpanKey(s.file.0, s.lo, s.hi)
    }
}

/// 領域（仕様 §5.4 の `RegionExpr`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionRef {
    /// `self@x`（`x` は `owns` フィールド）。
    SelfField(String),
    /// 領域変数。
    Var(String),
    /// 論理文脈の、`old(..)` / `final(..)` を付けた領域（`old(self)@x[p]`、`final(r)[p]`）。
    /// `old` なら `true`。
    At { old: bool, region: Box<RegionRef> },
}

/// 呼び出しの末尾に足す権限の引数（callee の `uses` の宣言順）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraArg {
    pub var: String,
    pub mode: PermMode,
}

#[derive(Debug, Default)]
pub struct Facts {
    /// `*X`（`X ∈ Γ`）。値は `X`。
    pub derefs: BTreeMap<SpanKey, String>,
    /// `X.free()`（`X ∈ Γ`）。値は `X`。
    pub frees: BTreeMap<SpanKey, String>,
    /// 権限を渡す呼び出し（`Call` / `MethodCall` の位置）。
    pub calls: BTreeMap<SpanKey, Vec<ExtraArg>>,
    /// 権限を受け取る `let X = Ptr::new(..);` / `let X = f(..);`（`f` が `gives`）。値は `X`。
    pub perm_lets: BTreeMap<SpanKey, String>,
    /// 論理の領域添字 `r[p]`（`Index` の位置）。
    pub region_index: BTreeMap<SpanKey, RegionRef>,
    /// `take` / `put` の文と `borrow` の式の領域。
    pub region_stmts: BTreeMap<SpanKey, RegionRef>,
    /// 構造体リテラルの `@name: e`（`FieldInit` の位置）とパターンの `@name`（識別子の位置）の種類。
    pub erased: BTreeMap<SpanKey, ErasedKind>,
    /// 関数に足す契約（関数名の識別子の位置）。
    pub expansions: BTreeMap<SpanKey, FnExpansion>,
    /// 量化子の `x in e` の `e` のうち、型が `Map<..>` と分かるもの（§7.4 の `e.dom().contains(x)`）。
    pub map_binders: std::collections::BTreeSet<SpanKey>,
    /// `impl Invariant for T` の入れ子（`T` → フィールド、型、包み）。
    pub nested: BTreeMap<ItemPath, Vec<(String, ItemPath, Wrap)>>,
}
