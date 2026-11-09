//! 式・文・パターン（仕様 §5.3、§5.4、§5.6）。

use super::{
    Attr, Contract, GenericArg, Ident, Item, Lifetime, Lit, Macro, Path, PermMode, QSelf, Ret,
    Tokens, Type,
};
use crate::span::Span;

/// 式。`attrs` は式に付いた外部属性。
#[derive(Debug, Clone)]
pub struct Expr {
    pub attrs: Vec<Attr>,
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    /// `&&` で分けた項（左から）。`if` / `while` の条件の `let` の並び（let chain）を読む。
    pub fn and_chain(&self) -> Vec<&Expr> {
        match &self.kind {
            ExprKind::Binary {
                op: BinOp::And,
                lhs,
                rhs,
                ..
            } => {
                let mut v = lhs.and_chain();
                v.extend(rhs.and_chain());
                v
            }
            _ => vec![self],
        }
    }

    /// 文の先頭に書いた属性。構文解析器は、代入・二項演算・キャスト・フィールド・添字・メソッドの呼び出し・
    /// `x@f` の文の属性を、左端の式に付けることがあるので、属性のない節点は左端の子をたどる。
    pub fn leading_attrs(&self) -> &[Attr] {
        if !self.attrs.is_empty() {
            return &self.attrs;
        }
        match &self.kind {
            ExprKind::Assign { lhs, .. } | ExprKind::Binary { lhs, .. } => lhs.leading_attrs(),
            ExprKind::Cast { expr, .. } | ExprKind::Try(expr) => expr.leading_attrs(),
            ExprKind::Field { base, .. }
            | ExprKind::Index { base, .. }
            | ExprKind::Erased { base, .. } => base.leading_attrs(),
            ExprKind::MethodCall { receiver, .. } => receiver.leading_attrs(),
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // Rust の式
    Array(Vec<Expr>),
    Repeat {
        expr: Box<Expr>,
        len: Box<Expr>,
    },
    Assign {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// 二項演算（複合代入 `+=` などを含む）。
    Binary {
        op: BinOp,
        op_span: Span,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Ref {
        mutable: bool,
        expr: Box<Expr>,
    },
    Block {
        label: Option<Lifetime>,
        block: Block,
    },
    Unsafe(Block),
    Const(Block),
    Break {
        label: Option<Lifetime>,
        expr: Option<Box<Expr>>,
    },
    Continue {
        label: Option<Lifetime>,
    },
    Return(Option<Box<Expr>>),
    /// 呼び出し。`uses` は引数の末尾の `uses` の後に並べた領域（#19 の既定 P1）。
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        uses: Vec<RegionArg>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: Ident,
        turbofish: Option<Vec<GenericArg>>,
        args: Vec<Expr>,
        uses: Vec<RegionArg>,
    },
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
    Closure(Closure),
    Field {
        base: Box<Expr>,
        member: Member,
    },
    /// Rust の添字。領域添字かどうかは M2 で決める（仕様 §5.4、§6.3）。
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    If {
        cond: Box<Expr>,
        then: Block,
        else_: Option<Box<Expr>>,
    },
    /// `if let` などの条件の中の `let p = e`。
    Let {
        pat: Box<Pat>,
        expr: Box<Expr>,
    },
    Loop(Loop),
    Match {
        expr: Box<Expr>,
        arms: Vec<Arm>,
    },
    Lit(Lit),
    Macro(Macro),
    Paren(Box<Expr>),
    Path {
        qself: Option<QSelf>,
        path: Path,
    },
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        closed: bool,
    },
    Struct(StructLit),
    Try(Box<Expr>),
    Tuple(Vec<Expr>),
    /// `_`。
    Infer,
    /// Ebykn が中を読まない Rust の式（`async`、`.await`、`yield`、`try` ブロック、
    /// `&raw`、`become`）。Verus に渡して判定させる。
    Rust(Tokens),

    // Ebykn の式
    /// `forall x: T, P` / `exists x in r, P`。
    Quant {
        kind: QuantKind,
        binders: Vec<Binder>,
        /// 本体の前の `#![trigger ..]` / `#![auto]`（`.eykp` だけ）。
        triggers: Vec<Trigger>,
        body: Box<Expr>,
    },
    /// `and { a, b }` / `or { a, b }`。
    AndOr {
        kind: AndOrKind,
        exprs: Vec<Expr>,
    },
    /// 権限 `p@`。
    Perm(Ident),
    /// 消去フィールド `self@x`（`base` は `self` に限らない。`docs/current-spec.md`）。
    Erased {
        base: Box<Expr>,
        field: Ident,
    },
    /// `borrow r[p] { .. }` / `borrow mut r[p] { .. }`。
    Borrow {
        mutable: bool,
        region: RegionIndex,
        body: Block,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    BitXor,
    BitAnd,
    BitOr,
    Shl,
    Shr,
    Eq,
    Lt,
    Le,
    Ne,
    Ge,
    Gt,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    BitXorAssign,
    BitAndAssign,
    BitOrAssign,
    ShlAssign,
    ShrAssign,
    /// `==>`（右結合、`||` より低い）。
    Implies,
    /// `<==>`（`==>` より低い、非結合）。
    Iff,
    /// `=~=`（`==` と同じ優先順位。`.eykp` だけ）。
    ExtEq,
}

impl BinOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Lt | BinOp::Le | BinOp::Ne | BinOp::Ge | BinOp::Gt | BinOp::ExtEq
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Deref,
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantKind {
    Forall,
    Exists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndOrKind {
    And,
    Or,
}

/// 量化子の本体の前の属性（仕様 §5.7 の `ProofAttr`）。
#[derive(Debug, Clone)]
pub enum Trigger {
    /// `#![trigger e, ..]`。
    Exprs { exprs: Vec<Expr>, span: Span },
    /// `#![auto]`。
    Auto(Span),
}

/// 束縛子。
#[derive(Debug, Clone)]
pub struct Binder {
    pub ident: Ident,
    pub kind: BinderKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum BinderKind {
    /// `x: T`。
    Typed(Type),
    /// `x in e`。
    In(Expr),
    /// `x`（型も `in` もない。OPEN-8 の既定で許す）。
    Bare,
}

/// 領域添字 `r[p]` / `self@r[p]`。添字が識別子かどうか（E0502）は M2 で検査する。
#[derive(Debug, Clone)]
pub struct RegionIndex {
    pub region: Region,
    pub index: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Region {
    /// `self@x`。`self_span` は `self` の位置。
    SelfField { self_span: Span, field: Ident },
    /// 領域変数 `r`。
    Var(Ident),
}

/// 呼び出しで渡す領域（`f(x, uses &mut r)` の `&mut r`。#19 の既定 P1）。`mode` は
/// `r`（移動）/ `&r` / `&mut r`。
#[derive(Debug, Clone)]
pub struct RegionArg {
    pub mode: PermMode,
    pub region: Region,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Member {
    Named(Ident),
    Unnamed { index: u32, span: Span },
}

/// ループ（`while` / `for` / `loop`）とループ句。
#[derive(Debug, Clone)]
pub struct Loop {
    pub label: Option<Lifetime>,
    pub kind: LoopKind,
    /// 書かれた順の句。
    pub clauses: Vec<LoopClause>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum LoopKind {
    While(Box<Expr>),
    For { pat: Box<Pat>, iter: Box<Expr> },
    Loop,
}

#[derive(Debug, Clone)]
pub struct LoopClause {
    pub kind: LoopClauseKind,
    pub exprs: Vec<Expr>,
    pub keyword: Span,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopClauseKind {
    Invariant,
    Decreases,
}

#[derive(Debug, Clone)]
pub struct Arm {
    pub attrs: Vec<Attr>,
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

/// クロージャ。契約（`requires` / `ensures`）は Verus の構文をそのまま通す（OPEN-12）。
#[derive(Debug, Clone)]
pub struct Closure {
    pub capture: bool,
    pub params: Vec<Pat>,
    /// `-> T` / `-> (r: T)`。
    pub ret: Option<Ret>,
    /// `requires` / `ensures` だけを持つ。
    pub contract: Contract,
    pub body: Box<Expr>,
}

/// 構造体リテラル。消去フィールドは `@name: e`。
#[derive(Debug, Clone)]
pub struct StructLit {
    pub qself: Option<QSelf>,
    pub path: Path,
    pub fields: Vec<FieldInit>,
    /// `..base`（`Some(None)` は `..` だけ）。
    pub rest: Option<Option<Box<Expr>>>,
}

#[derive(Debug, Clone)]
pub struct FieldInit {
    pub attrs: Vec<Attr>,
    /// `@name: e` の形か。
    pub erased: bool,
    pub member: Member,
    pub expr: Expr,
    /// `name`（`name: name` の略記）か。
    pub shorthand: bool,
    pub span: Span,
}

/// ブロック。
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// 文。
#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let(Local),
    Item(Box<Item>),
    /// 式の文。`semi` は `;` で終わるか。
    Expr {
        expr: Expr,
        semi: bool,
    },
    /// 単独の `;`。
    Empty,
    /// `take r[p];`。
    Take(RegionIndex),
    /// `put r[p];`。
    Put(RegionIndex),
    /// `leak p@;`。
    Leak(Ident),
    /// `self@x = e;`（`spec` フィールドの代入）。`target` は [`ExprKind::Erased`]。
    SpecAssign {
        target: Expr,
        value: Expr,
    },
    /// 証明の文（`.eykp` だけ。仕様 §5.7）。
    Proof(ProofStmt),
}

/// 証明の文（仕様 §5.7 の `ProofStmt`）。`use lemmas g;` は項目（[`super::ItemKind::UseLemmas`]）
/// として [`StmtKind::Item`] に置く。`lemma fn` の呼び出しは式の文。
#[derive(Debug, Clone)]
pub enum ProofStmt {
    /// `proof { .. }`。
    Block(Block),
    /// `assert P;`、`assert P by { .. };`、`assert P by(mode) requires Q;`。
    Assert { expr: Expr, by: Option<AssertBy> },
    /// `assert forall x in r, P by { .. };`。`by` の中で束縛子を使える。
    AssertForall {
        binders: Vec<Binder>,
        triggers: Vec<Trigger>,
        body: Box<Expr>,
        by: Block,
    },
    /// `spec let p = e;`。
    SpecLet(Local),
    /// `reveal(f);` / `reveal(f, fuel);`。
    Reveal { path: Path, fuel: Option<Expr> },
    /// `hide(f);`。
    Hide(Path),
}

#[derive(Debug, Clone)]
pub enum AssertBy {
    /// `by { .. }`。
    Block(Block),
    /// `by(mode)`、`by(mode) requires Q`。
    Prover {
        mode: super::ProverMode,
        mode_span: Span,
        requires: Vec<Expr>,
    },
}

#[derive(Debug, Clone)]
pub struct Local {
    pub attrs: Vec<Attr>,
    pub pat: Pat,
    pub ty: Option<Type>,
    pub init: Option<Expr>,
    /// `let .. else { .. }`。
    pub else_: Option<Block>,
}

/// パターン。`attrs` はパターンに付いた外部属性（クロージャの引数 `|#[..] a| ..` など）。
#[derive(Debug, Clone)]
pub struct Pat {
    pub attrs: Vec<Attr>,
    pub kind: PatKind,
    pub span: Span,
}

impl Pat {
    /// パターンの中の消去フィールドの束縛 `@x`（書かれた順。`|` の各選択肢のものを含む）。
    pub fn erased_idents(&self) -> Vec<&Ident> {
        let mut out = Vec::new();
        self.erased_idents_into(&mut out);
        out
    }

    fn erased_idents_into<'p>(&'p self, out: &mut Vec<&'p Ident>) {
        match &self.kind {
            PatKind::Ident { sub: Some(q), .. } => q.erased_idents_into(out),
            PatKind::Tuple(ps) | PatKind::Or(ps) | PatKind::Slice(ps) => {
                ps.iter().for_each(|q| q.erased_idents_into(out))
            }
            PatKind::TupleStruct { elems, .. } => {
                elems.iter().for_each(|q| q.erased_idents_into(out))
            }
            PatKind::Ref { pat, .. } | PatKind::Paren(pat) | PatKind::Type { pat, .. } => {
                pat.erased_idents_into(out)
            }
            PatKind::Struct { fields, .. } => {
                for f in fields {
                    match f {
                        FieldPat::Field { pat, .. } => pat.erased_idents_into(out),
                        FieldPat::Erased { ident, .. } => out.push(ident),
                        FieldPat::ErasedRest { .. } => {}
                    }
                }
            }
            PatKind::Ident { sub: None, .. }
            | PatKind::Wild
            | PatKind::Rest
            | PatKind::Lit(_)
            | PatKind::Range { .. }
            | PatKind::Path { .. }
            | PatKind::Macro(_) => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum PatKind {
    Ident {
        by_ref: bool,
        mutable: bool,
        ident: Ident,
        sub: Option<Box<Pat>>,
    },
    Wild,
    /// `..`。
    Rest,
    Lit(Box<Expr>),
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        closed: bool,
    },
    Path {
        qself: Option<QSelf>,
        path: Path,
    },
    Tuple(Vec<Pat>),
    TupleStruct {
        qself: Option<QSelf>,
        path: Path,
        elems: Vec<Pat>,
    },
    Struct {
        qself: Option<QSelf>,
        path: Path,
        fields: Vec<FieldPat>,
        /// 末尾の `..`。
        rest: bool,
    },
    Ref {
        mutable: bool,
        pat: Box<Pat>,
    },
    Or(Vec<Pat>),
    Slice(Vec<Pat>),
    Paren(Box<Pat>),
    Macro(Macro),
    /// `p: T`（引数、クロージャの引数）。
    Type {
        pat: Box<Pat>,
        ty: Type,
    },
}

#[derive(Debug, Clone)]
pub enum FieldPat {
    Field {
        attrs: Vec<Attr>,
        member: Member,
        pat: Pat,
        shorthand: bool,
        span: Span,
    },
    /// `@name`。
    Erased { attrs: Vec<Attr>, ident: Ident },
    /// `@..`（残りの消去フィールドを捨てる）。
    ErasedRest { attrs: Vec<Attr>, span: Span },
}
