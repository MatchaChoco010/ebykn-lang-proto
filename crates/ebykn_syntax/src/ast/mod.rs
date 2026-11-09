//! Ebykn の AST（仕様 §5、§12）。
//!
//! 構文解析器（verus_syn のフォーク）の構文木を、Ebykn 独自の型に写したもの。各節点は
//! 元の位置（[`Span`]）を持つ。Ebykn の構文を含まない Rust の部分（where 句の述語、
//! トレイト境界、`impl Trait` などの型、マクロの引数）は、トークン列（[`Tokens`]）の
//! まま持つ。

mod expr;
mod item;

pub use expr::*;
pub use item::*;

use proc_macro2::TokenStream;

use crate::span::Span;

/// 一つの `.eyk` ファイル。
#[derive(Debug, Clone)]
pub struct File {
    /// ファイル先頭の内部属性（`#![..]`）。
    pub attrs: Vec<Attr>,
    pub items: Vec<Item>,
    pub span: Span,
}

/// 識別子。`r#` は `raw` に分けて持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub raw: bool,
    pub span: Span,
}

impl Ident {
    /// 文脈キーワードなど、raw の綴り（`r#old`）を別の名前として扱う比較。属性の名前には使わない
    /// （[`Ident::is_name`]。`check-attr-names` が検査する）。
    pub fn is(&self, name: &str) -> bool {
        !self.raw && self.name == name
    }

    /// キーワードでない名前（属性の名前など）を、raw かどうかによらず比べる。rustc は `r#cfg` を
    /// `cfg` とまったく同じに読む（#122）。キーワードは渡さない（raw の綴り `r#unsafe` は別の名前）。
    pub fn is_name(&self, name: &str) -> bool {
        debug_assert!(
            !crate::macros::is_keyword(name),
            "`{name}` はキーワード。raw の綴りが別の名前になる"
        );
        self.name == name
    }
}

/// ライフタイム（`'a`。`name` は `'` を含まない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lifetime {
    pub name: String,
    pub span: Span,
}

/// Ebykn の構文を含まない Rust のトークン列。
#[derive(Debug, Clone)]
pub struct Tokens {
    pub tokens: TokenStream,
    pub span: Span,
}

/// リテラル。`text` は書かれたとおり（接尾辞を含む）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lit {
    pub text: String,
    pub span: Span,
}

/// パス。
#[derive(Debug, Clone)]
pub struct Path {
    pub leading_colon: bool,
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

impl Path {
    /// 一つの識別子だけのパスなら、その識別子。
    pub fn as_ident(&self) -> Option<&Ident> {
        match self.segments.as_slice() {
            [seg] if !self.leading_colon && matches!(seg.args, GenericArgs::None) => {
                Some(&seg.ident)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PathSegment {
    pub ident: Ident,
    pub args: GenericArgs,
}

#[derive(Debug, Clone)]
pub enum GenericArgs {
    None,
    /// `<A, B>`（`turbofish` は `::<` の形か）。
    Angle {
        turbofish: bool,
        args: Vec<GenericArg>,
        span: Span,
    },
    /// `Fn(A, B) -> C`。
    Paren {
        inputs: Vec<Type>,
        output: Option<Box<Type>>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum GenericArg {
    Lifetime(Lifetime),
    Type(Type),
    Const(Expr),
    /// `Item = T`。
    AssocType {
        ident: Ident,
        ty: Type,
    },
    /// `N = 3` や `Item: Bound` など、それ以外の Rust の形。
    Rust(Tokens),
}

/// `<T as Trait>::` の `T as Trait` の部分。`position` は `Trait` のパスの長さ。
#[derive(Debug, Clone)]
pub struct QSelf {
    pub ty: Box<Type>,
    pub position: usize,
}

/// 型。
#[derive(Debug, Clone)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Path {
        qself: Option<QSelf>,
        path: Path,
    },
    Ref {
        lifetime: Option<Lifetime>,
        mutable: bool,
        elem: Box<Type>,
    },
    Tuple(Vec<Type>),
    Array {
        elem: Box<Type>,
        len: Box<Expr>,
    },
    Slice(Box<Type>),
    Paren(Box<Type>),
    Never,
    Infer,
    /// `impl Trait`、`dyn Trait`、関数ポインタ、生ポインタ、型の位置のマクロなど。
    Rust(Tokens),
}

/// 属性。
#[derive(Debug, Clone)]
pub struct Attr {
    pub inner: bool,
    pub path: Path,
    /// パスの後ろ（`(..)` や `= ..`）。無ければ空。
    pub args: TokenStream,
    pub span: Span,
}

impl Attr {
    /// 属性のパスが 1 語の `name`（先頭の `::` なし）か。`name` は組み込みの属性の名前で、raw の
    /// 綴り（`#[r#cfg]`）も同じ名前として比べる（[`Ident::is_name`]、#122）。
    pub fn is_builtin(&self, name: &str) -> bool {
        !self.path.leading_colon
            && self.path.segments.len() == 1
            && self.path.segments[0].ident.is_name(name)
    }
}

/// 可視性。
#[derive(Debug, Clone)]
pub enum Vis {
    Inherited,
    Pub(Span),
    /// `pub(crate)`、`pub(super)`、`pub(in path)` など。
    Restricted(Tokens),
}

/// ジェネリクスの引数の宣言（`<T: Bound, 'a, const N: usize>`）。
#[derive(Debug, Clone)]
pub struct Generics {
    pub params: Vec<GenericParam>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone)]
pub enum GenericParam {
    Lifetime {
        lifetime: Lifetime,
        bounds: Option<Tokens>,
    },
    Type {
        ident: Ident,
        bounds: Option<Tokens>,
        default: Option<Type>,
    },
    Const {
        ident: Ident,
        ty: Type,
        default: Option<Expr>,
    },
}

/// Rust の where 句。述語はトークン列のまま持つ。
#[derive(Debug, Clone)]
pub struct WhereClause {
    pub predicates: Vec<Tokens>,
    pub span: Span,
}

/// マクロの呼び出し。`tokens` は引数のトークン列。vstd の仕様用のマクロ（`seq!`、`set!`、
/// `map!`）は引数を Ebykn の式として読み、`args` に持つ（`docs/current-spec.md`）。
#[derive(Debug, Clone)]
pub struct Macro {
    pub path: Path,
    pub delimiter: Delimiter,
    pub tokens: TokenStream,
    pub args: MacroArgs,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum MacroArgs {
    /// 引数を構文として読まないマクロ（`println!`、`vec!` など）。
    Tokens,
    /// `seq![a, b]` / `set![a, b]`。
    Exprs(Vec<Expr>),
    /// `map![k => v, ..]`。
    Map(Vec<(Expr, Expr)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    Paren,
    Brace,
    Bracket,
}
