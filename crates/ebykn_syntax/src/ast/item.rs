//! 項目と契約（仕様 §5.1、§5.2）。

use super::{
    Attr, Block, Expr, Generics, Ident, Lifetime, Pat, Path, Tokens, Type, Vis, WhereClause,
};
use crate::span::Span;

/// 項目。`attrs` は外部属性（`#[..]`）。
#[derive(Debug, Clone)]
pub struct Item {
    pub attrs: Vec<Attr>,
    pub vis: Vis,
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    /// `fn`（純粋関数 `fn .. = e;` と `unchecked fn` を含む）。
    Fn(Fn),
    /// `spec fn` / `closed spec fn`。
    SpecFn(SpecFn),
    /// `lemma fn`（`unchecked lemma fn` を含む）。
    LemmaFn(LemmaFn),
    Struct(Struct),
    Enum(Enum),
    Trait(Trait),
    /// `impl`（固有の impl と trait の impl）。
    Impl(Impl),
    /// `impl spec Path (for Type)? { .. }`。
    ImplSpec(ImplSpec),
    /// Rust の型の別名（`type T = U;`、Rust の where 句付きを含む）。
    TypeAlias(TypeAlias),
    /// 部分集合型 `type T = B where P;`。
    Subset(SubsetType),
    Mod(Mod),
    Const(Const),
    Static(Static),
    /// `use` 宣言。
    Use(Tokens),
    /// `lemma group g { a, b }`（`.eykp` だけ。仕様 §5.1、§5.7）。
    LemmaGroup(LemmaGroup),
    /// `use lemmas g;`（`.eykp` だけ。モジュールとブロックの中に書ける）。
    UseLemmas(Path),
    /// それ以外の Rust の項目（`extern crate`、`macro_rules!`、項目の位置のマクロ、
    /// `union`、`extern` ブロック）。
    Rust(Tokens),
}

/// 関数の修飾子（Rust のもの）。
#[derive(Debug, Clone, Default)]
pub struct FnQualifiers {
    pub constness: Option<Span>,
    pub asyncness: Option<Span>,
    pub unsafety: Option<Span>,
    /// `extern "C"` の全体。
    pub abi: Option<Tokens>,
}

/// シグネチャ（`fn` から where 句まで）。
#[derive(Debug, Clone)]
pub struct Signature {
    pub qualifiers: FnQualifiers,
    pub ident: Ident,
    pub generics: Generics,
    pub params: Vec<Param>,
    pub ret: Option<Ret>,
    pub where_clause: Option<WhereClause>,
}

#[derive(Debug, Clone)]
pub enum Param {
    /// `self`、`mut self`、`&self`、`&'a mut self`、`self: Ty`。
    Receiver {
        attrs: Vec<Attr>,
        /// `&` の有無と、そのライフタイム。
        reference: Option<Option<Lifetime>>,
        mutable: bool,
        ty: Option<Type>,
        span: Span,
    },
    Typed {
        attrs: Vec<Attr>,
        pat: Pat,
        ty: Type,
        span: Span,
    },
}

/// 戻り値の型。`name` は名前付き戻り値 `-> (r: T)` の名前。
#[derive(Debug, Clone)]
pub struct Ret {
    pub name: Option<Ident>,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum FnBody {
    Block(Block),
    /// 純粋関数の `= e;`。
    Pure(Expr),
}

#[derive(Debug, Clone)]
pub struct Fn {
    /// `unchecked` の位置。
    pub unchecked: Option<Span>,
    pub sig: Signature,
    pub contract: Contract,
    /// trait の中の宣言では `None`。
    pub body: Option<FnBody>,
}

#[derive(Debug, Clone)]
pub struct SpecFn {
    /// `closed` の位置。無ければ open（仕様 §7.2）。
    pub closed: Option<Span>,
    pub sig: Signature,
    pub contract: Contract,
    /// trait の中の宣言では `None`。
    pub body: Option<Expr>,
}

/// `lemma fn`。`.eyk` には本体を書けない（本体は `.eykp`）。
#[derive(Debug, Clone)]
pub struct LemmaFn {
    pub unchecked: Option<Span>,
    pub sig: Signature,
    pub contract: Contract,
    /// 本体。`.eykp` でだけ `Some` になる。
    pub body: Option<Block>,
}

/// `lemma group g { a, b }`。
#[derive(Debug, Clone)]
pub struct LemmaGroup {
    pub ident: Ident,
    pub members: Vec<Path>,
}

/// 契約。句は書かれた順に並ぶ。同じ種類の句は連結して読む（仕様 §5.2）。
#[derive(Debug, Clone, Default)]
pub struct Contract {
    pub clauses: Vec<Clause>,
}

impl Contract {
    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    pub fn uses(&self) -> impl Iterator<Item = &UsesItem> {
        self.clauses.iter().flat_map(|c| match &c.kind {
            ClauseKind::Uses(v) => v.as_slice(),
            _ => &[],
        })
    }

    pub fn gives(&self) -> impl Iterator<Item = &Ident> {
        self.clauses.iter().flat_map(|c| match &c.kind {
            ClauseKind::Gives(v) => v.as_slice(),
            _ => &[],
        })
    }

    pub fn requires(&self) -> impl Iterator<Item = &Expr> {
        self.exprs(|k| matches!(k, ClauseKind::Requires(_)))
    }

    pub fn ensures(&self) -> impl Iterator<Item = &Expr> {
        self.exprs(|k| matches!(k, ClauseKind::Ensures(_)))
    }

    pub fn decreases(&self) -> impl Iterator<Item = &Expr> {
        self.exprs(|k| matches!(k, ClauseKind::Decreases(_)))
    }

    pub fn returns(&self) -> impl Iterator<Item = &Expr> {
        self.exprs(|k| matches!(k, ClauseKind::Returns(_)))
    }

    pub fn no_panic(&self) -> bool {
        self.clauses
            .iter()
            .any(|c| matches!(c.kind, ClauseKind::NoPanic))
    }

    fn exprs(&self, f: fn(&ClauseKind) -> bool) -> impl Iterator<Item = &Expr> {
        self.clauses
            .iter()
            .filter(move |c| f(&c.kind))
            .flat_map(|c| match &c.kind {
                ClauseKind::Requires(v) | ClauseKind::Ensures(v) | ClauseKind::Decreases(v) => {
                    v.as_slice()
                }
                ClauseKind::Returns(e) => std::slice::from_ref(e),
                _ => &[],
            })
    }
}

/// 契約の句。`span` は句全体（キーワードから最後の式まで）。
#[derive(Debug, Clone)]
pub struct Clause {
    pub kind: ClauseKind,
    pub keyword: Span,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ClauseKind {
    Uses(Vec<UsesItem>),
    /// `gives q@, r@`。名前付き戻り値の名前。
    Gives(Vec<Ident>),
    Requires(Vec<Expr>),
    Ensures(Vec<Expr>),
    Returns(Expr),
    Decreases(Vec<Expr>),
    NoPanic,
    /// `by(mode)`（`lemma fn` だけ、`.eykp` だけ）。
    By(ProverMode),
}

/// 証明器の指定（仕様 §5.2 の `ProverMode`、OPEN-6 の既定の 5 つ）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProverMode {
    BitVector,
    NonlinearArith,
    IntegerRing,
    Compute,
    ComputeOnly,
}

impl ProverMode {
    pub const ALL: [ProverMode; 5] = [
        ProverMode::BitVector,
        ProverMode::NonlinearArith,
        ProverMode::IntegerRing,
        ProverMode::Compute,
        ProverMode::ComputeOnly,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ProverMode::BitVector => "bit_vector",
            ProverMode::NonlinearArith => "nonlinear_arith",
            ProverMode::IntegerRing => "integer_ring",
            ProverMode::Compute => "compute",
            ProverMode::ComputeOnly => "compute_only",
        }
    }

    pub fn from_name(name: &str) -> Option<ProverMode> {
        ProverMode::ALL.into_iter().find(|m| m.as_str() == name)
    }
}

#[derive(Debug, Clone)]
pub enum UsesItem {
    /// `p@`、`&p@`、`&mut p@`（`p` は引数名）。
    Perm {
        mode: PermMode,
        ident: Ident,
        span: Span,
    },
    /// `r: Region<..>` など、名前付きの消去される引数。
    Named { ident: Ident, ty: Type, span: Span },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermMode {
    /// `p@`（移動）。
    Move,
    /// `&p@`。
    Shared,
    /// `&mut p@`。
    Mut,
}

/// 構造体。`owns` / `spec` は名前付きフィールドの struct にだけ付く（他は E0401）。
#[derive(Debug, Clone)]
pub struct Struct {
    pub ident: Ident,
    pub generics: Generics,
    pub where_clause: Option<WhereClause>,
    pub fields: Fields,
    pub owns: Option<ErasedFields>,
    pub spec: Option<ErasedFields>,
}

#[derive(Debug, Clone)]
pub enum Fields {
    Named(Vec<Field>),
    Tuple(Vec<Field>),
    Unit,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub attrs: Vec<Attr>,
    pub vis: Vis,
    /// tuple struct のフィールドでは `None`。
    pub ident: Option<Ident>,
    pub ty: Type,
    pub span: Span,
}

/// `owns { .. }` / `spec { .. }` の節。`keyword` は `owns` / `spec` の位置。
#[derive(Debug, Clone)]
pub struct ErasedFields {
    pub fields: Vec<Field>,
    pub keyword: Span,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub ident: Ident,
    pub generics: Generics,
    pub where_clause: Option<WhereClause>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub attrs: Vec<Attr>,
    pub ident: Ident,
    pub fields: Fields,
    pub discriminant: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Trait {
    pub unsafety: Option<Span>,
    pub auto: Option<Span>,
    pub ident: Ident,
    pub generics: Generics,
    /// `: A + B` の `A + B`。
    pub supertraits: Option<Tokens>,
    pub where_clause: Option<WhereClause>,
    pub items: Vec<TraitItem>,
}

#[derive(Debug, Clone)]
pub struct TraitItem {
    pub attrs: Vec<Attr>,
    pub kind: TraitItemKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TraitItemKind {
    /// 契約付きのメソッド。本体は任意。
    Fn(Fn),
    SpecFn(SpecFn),
    LemmaFn(LemmaFn),
    /// 関連型・関連定数・マクロなど Rust の trait の項目。
    Rust(Tokens),
}

/// `impl`。`items` の可視性は各項目が持つ。
#[derive(Debug, Clone)]
pub struct Impl {
    pub unsafety: Option<Span>,
    pub generics: Generics,
    /// `impl !Trait for T` の `!`。
    pub negative: bool,
    pub trait_: Option<Path>,
    pub self_ty: Type,
    pub where_clause: Option<WhereClause>,
    pub items: Vec<Item>,
}

/// `impl spec Path (for Type)? { (SpecFn | LemmaFn)* }`。
#[derive(Debug, Clone)]
pub struct ImplSpec {
    pub generics: Generics,
    pub spec: Span,
    pub trait_: Path,
    pub self_ty: Option<Type>,
    pub where_clause: Option<WhereClause>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub ident: Ident,
    pub generics: Generics,
    pub where_clause: Option<WhereClause>,
    pub ty: Type,
}

/// 部分集合型 `type T = B where P;`。`pred` は `self` を自由変数に持つ `bool` の式。
#[derive(Debug, Clone)]
pub struct SubsetType {
    pub ident: Ident,
    pub generics: Generics,
    pub base: Type,
    pub where_token: Span,
    pub pred: Expr,
}

#[derive(Debug, Clone)]
pub struct Mod {
    pub unsafety: Option<Span>,
    pub ident: Ident,
    /// `mod foo;` では `None`。インラインの `mod foo { .. }` では中の項目と内部属性。
    pub content: Option<(Vec<Attr>, Vec<Item>)>,
}

#[derive(Debug, Clone)]
pub struct Const {
    pub ident: Ident,
    pub generics: Generics,
    pub ty: Type,
    /// trait の中の宣言では `None`。
    pub expr: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct Static {
    pub mutable: bool,
    pub ident: Ident,
    pub ty: Type,
    pub expr: Expr,
}
