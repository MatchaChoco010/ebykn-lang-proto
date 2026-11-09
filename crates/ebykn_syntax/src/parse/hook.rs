//! フォークの差し込み口から読む Ebykn の構文（式・文・パターンに埋め込まれるもの）。
//!
//! 差し込み口では、ここの関数で読んで入力を進めるだけにする（`Raw*` は捨てる）。フォークの
//! 構文木に残った `Verbatim` は、[`super::convert`] が同じ関数で読み直して AST に写す。
//! 差し込み口で AST まで作ると、入れ子の深さについて指数的に読み直すことになるため。

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use verus_syn as syn;
use verus_syn::parse::discouraged::Speculative as _;
use verus_syn::parse::{Parse as _, ParseStream, Parser as _};
use verus_syn::{EbyknSite, Token};

use super::{err, is_group, is_ident, is_punct, item, lookahead, parse_kw, peek_kw, peek_tts};
use crate::ast;
use crate::diag::{Expected, Message};

/// 消去フィールド `e@name` の `name` になれない語。契約の句とループ句のキーワードで、
/// `requires x == p@ ensures ..` のように権限 `p@` の直後に来る（`docs/current-spec.md`）。
pub(super) const CLAUSE_KEYWORDS: &[&str] = &[
    "requires",
    "ensures",
    "returns",
    "decreases",
    "no_panic",
    "uses",
    "gives",
    "by",
    "invariant",
];

pub(super) fn hook(site: EbyknSite, input: ParseStream) -> syn::Result<bool> {
    // `parse_eyk` の外では上流と同じに動く。
    if !super::in_parse() {
        return Ok(false);
    }
    match site {
        EbyknSite::Stmt => stmt(input),
        EbyknSite::Atom { .. } if let Some(e) = erased_region(input) => Err(e),
        EbyknSite::Atom { allow_struct } => match peek_atom(input) {
            Some(kind) => parse_atom_in(input, kind, allow_struct).map(|_| true),
            None => Ok(false),
        },
        EbyknSite::Trailer => {
            if peek_erased_trailer(input) {
                input.parse::<Token![@]>()?;
                input.parse::<proc_macro2::Ident>()?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        EbyknSite::StructBody => {
            if brace_has_erased_field(input, false) {
                parse_struct_body(input).map(|_| true)
            } else {
                Ok(false)
            }
        }
        EbyknSite::PatStructBody => {
            if brace_has_erased_field(input, true) {
                parse_pat_struct_body(input).map(|_| true)
            } else {
                Ok(false)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 式の原子

#[derive(Clone, Copy)]
pub(super) enum AtomKind {
    Quant,
    AndOr,
    Borrow,
    Loop,
    /// 呼び出しの引数の末尾の `uses &mut r, ..`（#19 の既定 P1）。`uses` は予約語なので、
    /// 式の先頭で `&` か名前が続く `uses` はいつもこれ。呼び出しの引数の外にあれば写すときに
    /// 診断にする。
    Uses,
}

pub(super) fn peek_atom(input: ParseStream) -> Option<AtomKind> {
    let t = peek_tts(input, 4);
    // 後ろが `&`・名前・`,`・入力の終わり（呼び出しの `)` の前）のときは領域の並び（裸の `uses` は
    // 領域がないとして診断にする）。`uses::f`・`uses.x` などは名前として読み、写すときに予約語の
    // 診断にする。
    if is_ident(t.first(), "uses")
        && (is_punct(t.get(1), '&')
            || is_punct(t.get(1), ',')
            || t.get(1).is_none()
            || matches!(t.get(1), Some(TokenTree::Ident(_))))
    {
        return Some(AtomKind::Uses);
    }
    if (is_ident(t.first(), "forall") || is_ident(t.first(), "exists"))
        && matches!(t.get(1), Some(TokenTree::Ident(_)))
    {
        return Some(AtomKind::Quant);
    }
    if (is_ident(t.first(), "and") || is_ident(t.first(), "or"))
        && is_group(t.get(1), Delimiter::Brace)
    {
        return Some(AtomKind::AndOr);
    }
    if is_ident(t.first(), "borrow") && lookahead(input, |f| parse_borrow_head(f).map(|_| ())) {
        return Some(AtomKind::Borrow);
    }
    if peek_loop(&t) {
        return Some(AtomKind::Loop);
    }
    None
}

fn peek_loop(t: &[TokenTree]) -> bool {
    let kw = |tt: Option<&TokenTree>| {
        is_ident(tt, "while") || is_ident(tt, "loop") || is_ident(tt, "for") && !is_punct_next_lt(t)
    };
    if kw(t.first()) {
        return true;
    }
    // ラベル付き：'a: while ..
    is_punct(t.first(), '\'')
        && matches!(t.get(1), Some(TokenTree::Ident(_)))
        && is_punct(t.get(2), ':')
        && (is_ident(t.get(3), "while") || is_ident(t.get(3), "loop") || is_ident(t.get(3), "for"))
}

/// `for<'a> |x| ..`（ライフタイム付きのクロージャ）の `for` はループではない。
fn is_punct_next_lt(t: &[TokenTree]) -> bool {
    is_ident(t.first(), "for") && is_punct(t.get(1), '<')
}

pub(super) enum RawAtom {
    Quant(RawQuant),
    AndOr(RawAndOr),
    Borrow(RawBorrow),
    Loop(RawLoop),
    Uses(Vec<RawRegionArg>),
}

pub(super) fn parse_atom(input: ParseStream, kind: AtomKind) -> syn::Result<RawAtom> {
    parse_atom_in(input, kind, true)
}

/// `allow_struct` が偽の位置（`if` の条件、契約の句など）では、束縛子の `in` の右辺と
/// 量化子の本体で構造体リテラルを読まない（`ensures forall x, x == y { .. }` の `{` は本体）。
fn parse_atom_in(input: ParseStream, kind: AtomKind, allow_struct: bool) -> syn::Result<RawAtom> {
    Ok(match kind {
        AtomKind::Quant => RawAtom::Quant(parse_quant(input, allow_struct)?),
        AtomKind::AndOr => RawAtom::AndOr(parse_and_or(input)?),
        AtomKind::Borrow => RawAtom::Borrow(parse_borrow(input)?),
        AtomKind::Loop => RawAtom::Loop(parse_loop(input)?),
        AtomKind::Uses => RawAtom::Uses(parse_uses_args(input)?),
    })
}

/// 呼び出しで渡す領域（`r` / `&r` / `&mut r`、領域は `r` か `self@x`）。
pub(super) struct RawRegionArg {
    pub mode: ast::PermMode,
    pub region: RawRegion,
    pub span: crate::span::Span,
}

/// `uses` と、続けて `,` で並べた領域（#19 の既定 P1）。読むのは呼び出しの `)` の前まで。
fn parse_uses_args(input: ParseStream) -> syn::Result<Vec<RawRegionArg>> {
    parse_kw(input, "uses")?;
    let mut out = Vec::new();
    loop {
        out.push(parse_region_arg(input)?);
        if input.is_empty() {
            break;
        }
        if !input.peek(Token![,]) {
            return Err(expected_uses_arg(input));
        }
        input.parse::<Token![,]>()?;
        if input.is_empty() {
            break;
        }
    }
    Ok(out)
}

fn expected_uses_arg(input: ParseStream) -> syn::Error {
    err(
        input.span(),
        Message::Expected {
            what: Expected::UsesArg,
        },
    )
}

fn parse_region_arg(input: ParseStream) -> syn::Result<RawRegionArg> {
    let begin = input.fork();
    let mode = if input.peek(Token![&]) {
        input.parse::<Token![&]>()?;
        if input.parse::<Option<Token![mut]>>()?.is_some() {
            ast::PermMode::Mut
        } else {
            ast::PermMode::Shared
        }
    } else {
        ast::PermMode::Move
    };
    let region = if input.peek(Token![self]) && input.peek2(Token![@]) {
        let s: Token![self] = input.parse()?;
        input.parse::<Token![@]>()?;
        RawRegion::SelfField {
            self_span: s.span,
            field: input.parse::<syn::Ident>()?,
        }
    } else if input.peek(syn::Ident) && !input.peek(Token![self]) {
        RawRegion::Var(input.parse::<syn::Ident>()?)
    } else {
        return Err(expected_uses_arg(input));
    };
    Ok(RawRegionArg {
        mode,
        region,
        span: super::span_between(&begin, input),
    })
}

pub(super) struct RawQuant {
    pub kind: ast::QuantKind,
    pub binders: Vec<RawBinder>,
    pub triggers: Vec<RawTrigger>,
    pub body: syn::Expr,
}

/// 量化子の本体の前の `#![trigger ..]` / `#![auto]`（仕様 §5.7 の `ProofAttr`）。
pub(super) enum RawTrigger {
    Exprs {
        exprs: Vec<syn::Expr>,
        span: proc_macro2::Span,
    },
    Auto(proc_macro2::Span),
}

pub(super) struct RawBinder {
    pub ident: proc_macro2::Ident,
    pub kind: RawBinderKind,
}

pub(super) enum RawBinderKind {
    Typed(syn::Type),
    In(syn::Expr),
    Bare,
}

fn expr_in(input: ParseStream, allow_struct: bool) -> syn::Result<syn::Expr> {
    if allow_struct {
        input.parse()
    } else {
        syn::Expr::parse_without_eager_brace(input)
    }
}

fn parse_quant(input: ParseStream, allow_struct: bool) -> syn::Result<RawQuant> {
    let kw: proc_macro2::Ident = input.parse()?;
    let kind = if kw == "forall" {
        ast::QuantKind::Forall
    } else {
        ast::QuantKind::Exists
    };
    let mut binders = Vec::new();
    loop {
        binders.push(parse_binder(input, allow_struct)?);
        input.parse::<Token![,]>()?;
        // 仕様 §5.3：`,` の直後が `Ident :`、`Ident in`、`Ident ,` なら束縛子を続ける。
        if !peek_binder(input) {
            break;
        }
    }
    let triggers = parse_triggers(input)?;
    let body = expr_in(input, allow_struct)?;
    Ok(RawQuant {
        kind,
        binders,
        triggers,
        body,
    })
}

/// 本体の前の内部属性 `#![trigger e, ..]` / `#![auto]`。引数は Ebykn の式として読む。
/// `.eyk` では、字句の検査（[`super::tokens`]）が E0303 にしている。
fn parse_triggers(input: ParseStream) -> syn::Result<Vec<RawTrigger>> {
    let mut out = Vec::new();
    while input.peek(Token![#]) && input.peek2(Token![!]) {
        let pound: Token![#] = input.parse()?;
        input.parse::<Token![!]>()?;
        let content;
        let bracket = syn::bracketed!(content in input);
        let span = pound.span.join(bracket.span.close()).unwrap_or(pound.span);
        let name: proc_macro2::Ident = content.parse()?;
        // 位置は正しいので、誤りがあっても先に印を付ける（「ここには書けない」を重ねない）。
        if name == "trigger" || name == "auto" {
            super::proof_attr_ok(super::sp(span));
        }
        if name == "trigger" {
            let exprs: Vec<syn::Expr> = content
                .parse_terminated(syn::Expr::parse, Token![,])?
                .into_iter()
                .collect();
            if exprs.is_empty() {
                // 仕様 §5.7：`"#![trigger" ExprList "]"`。式を一つ以上持つ。
                return Err(err(
                    span,
                    Message::Expected {
                        what: Expected::TriggerExpr,
                    },
                ));
            }
            out.push(RawTrigger::Exprs { exprs, span });
        } else if name == "auto" {
            // 引数の崩れ（`#![auto(x)]`）は解析の終わりに引数の誤りとして出す。
            content.parse::<TokenStream>()?;
            out.push(RawTrigger::Auto(span));
        } else {
            return Err(err(
                span,
                Message::NotAllowedHere {
                    written: format!("#![{name}]"),
                },
            ));
        }
    }
    Ok(out)
}

fn peek_binder(input: ParseStream) -> bool {
    let t = peek_tts(input, 3);
    if !matches!(t.first(), Some(TokenTree::Ident(_))) {
        return false;
    }
    is_single_colon(&t[1..]) || is_ident(t.get(1), "in") || is_punct(t.get(1), ',')
}

/// `:` だけで `::` ではない。
pub(super) fn is_single_colon(t: &[TokenTree]) -> bool {
    match t.first() {
        Some(TokenTree::Punct(p)) if p.as_char() == ':' => {
            !(p.spacing() == Spacing::Joint && is_punct(t.get(1), ':'))
        }
        _ => false,
    }
}

fn parse_binder(input: ParseStream, allow_struct: bool) -> syn::Result<RawBinder> {
    let ident: proc_macro2::Ident = input.call(syn::ext::IdentExt::parse_any)?;
    let kind = if is_single_colon(&peek_tts(input, 2)) {
        input.parse::<Token![:]>()?;
        RawBinderKind::Typed(input.parse()?)
    } else if input.peek(Token![in]) {
        input.parse::<Token![in]>()?;
        RawBinderKind::In(expr_in(input, allow_struct)?)
    } else {
        RawBinderKind::Bare
    };
    Ok(RawBinder { ident, kind })
}

pub(super) struct RawAndOr {
    pub kind: ast::AndOrKind,
    pub exprs: Vec<syn::Expr>,
}

fn parse_and_or(input: ParseStream) -> syn::Result<RawAndOr> {
    let kw: proc_macro2::Ident = input.parse()?;
    let kind = if kw == "and" {
        ast::AndOrKind::And
    } else {
        ast::AndOrKind::Or
    };
    let content;
    let brace = syn::braced!(content in input);
    let exprs: Vec<syn::Expr> = content
        .parse_terminated(syn::Expr::parse, Token![,])?
        .into_iter()
        .collect();
    if exprs.is_empty() {
        // 仕様 §5.3：`AndOr ::= ("and" | "or") "{" ExprList "}"`。ExprList は式を一つ以上持つ。
        return Err(err(
            brace.span.join(),
            Message::Expected {
                what: Expected::AndOrExpr,
            },
        ));
    }
    Ok(RawAndOr { kind, exprs })
}

pub(super) struct RawRegionIndex {
    pub region: RawRegion,
    pub index: syn::Expr,
    pub span: proc_macro2::Span,
}

pub(super) enum RawRegion {
    SelfField {
        self_span: proc_macro2::Span,
        field: proc_macro2::Ident,
    },
    Var(proc_macro2::Ident),
}

pub(super) fn parse_region_index(input: ParseStream) -> syn::Result<RawRegionIndex> {
    let start = input.span();
    let region = if input.peek(Token![self]) && input.peek2(Token![@]) {
        let s: Token![self] = input.parse()?;
        input.parse::<Token![@]>()?;
        RawRegion::SelfField {
            self_span: s.span,
            field: input.parse::<syn::Ident>()?,
        }
    } else {
        RawRegion::Var(input.parse::<syn::Ident>()?)
    };
    let content;
    let bracket = syn::bracketed!(content in input);
    let index = content.parse()?;
    if !content.is_empty() {
        return Err(content.error("expected `]`"));
    }
    let span = start.join(bracket.span.close()).unwrap_or(start);
    Ok(RawRegionIndex {
        region,
        index,
        span,
    })
}

pub(super) struct RawBorrow {
    pub mutable: bool,
    pub region: RawRegionIndex,
    pub body: syn::Block,
}

fn parse_borrow_head(input: ParseStream) -> syn::Result<(bool, RawRegionIndex)> {
    parse_kw(input, "borrow")?;
    let mutable = input.parse::<Option<Token![mut]>>()?.is_some();
    let region = parse_region_index(input)?;
    if !input.peek(syn::token::Brace) {
        return Err(input.error("expected `{`"));
    }
    Ok((mutable, region))
}

fn parse_borrow(input: ParseStream) -> syn::Result<RawBorrow> {
    let (mutable, region) = parse_borrow_head(input)?;
    Ok(RawBorrow {
        mutable,
        region,
        body: input.parse()?,
    })
}

pub(super) struct RawLoop {
    pub label: Option<syn::Lifetime>,
    pub kind: RawLoopKind,
    pub clauses: Vec<RawLoopClause>,
    pub body: syn::Block,
}

pub(super) enum RawLoopKind {
    While(syn::Expr),
    For { pat: syn::Pat, iter: syn::Expr },
    Loop,
}

pub(super) struct RawLoopClause {
    pub kind: ast::LoopClauseKind,
    pub kw: proc_macro2::Ident,
    pub exprs: Vec<syn::Expr>,
}

fn parse_loop(input: ParseStream) -> syn::Result<RawLoop> {
    let label = if input.peek(syn::Lifetime) {
        let l: syn::Label = input.parse()?;
        Some(l.name)
    } else {
        None
    };
    let kind = if input.peek(Token![while]) {
        input.parse::<Token![while]>()?;
        RawLoopKind::While(syn::Expr::parse_without_eager_brace(input)?)
    } else if input.peek(Token![for]) {
        input.parse::<Token![for]>()?;
        let pat = syn::Pat::parse_multi_with_leading_vert(input)?;
        input.parse::<Token![in]>()?;
        let iter = syn::Expr::parse_without_eager_brace(input)?;
        RawLoopKind::For { pat, iter }
    } else {
        input.parse::<Token![loop]>()?;
        RawLoopKind::Loop
    };
    let mut clauses = Vec::new();
    loop {
        let kind = if peek_kw(input, "invariant") {
            ast::LoopClauseKind::Invariant
        } else if peek_kw(input, "decreases") {
            ast::LoopClauseKind::Decreases
        } else if let Some(m) = verus_loop_clause(input) {
            return Err(err(input.span(), m));
        } else {
            break;
        };
        let kw: proc_macro2::Ident = input.parse()?;
        let mut exprs = Vec::new();
        loop {
            exprs.push(syn::Expr::parse_without_eager_brace(input)?);
            if !input.peek(Token![,]) {
                break;
            }
            input.parse::<Token![,]>()?;
            if input.peek(syn::token::Brace)
                || peek_kw(input, "invariant")
                || peek_kw(input, "decreases")
            {
                break;
            }
        }
        clauses.push(RawLoopClause { kind, kw, exprs });
    }
    let body = input.parse()?;
    Ok(RawLoop {
        label,
        kind,
        clauses,
        body,
    })
}

/// Verus のループ句で、Ebykn にないもの。
fn verus_loop_clause(input: ParseStream) -> Option<Message> {
    for kw in ["invariant_except_break", "invariant_ensures", "ensures"] {
        if peek_kw(input, kw) {
            return Some(Message::VerusOnly {
                written: kw.to_string(),
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 後置の `@`

fn peek_erased_trailer(input: ParseStream) -> bool {
    let t = peek_tts(input, 2);
    if !is_punct(t.first(), '@') || !input.peek2(syn::Ident) {
        return false;
    }
    match t.get(1) {
        Some(TokenTree::Ident(i)) => !CLAUSE_KEYWORDS.iter().any(|k| i == k),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// 構造体リテラルとパターン

/// `{ .. }` の中に、フィールドの先頭の `@` があるか。
fn brace_has_erased_field(input: ParseStream, pat: bool) -> bool {
    let Some(TokenTree::Group(g)) = peek_tts(input, 1).into_iter().next() else {
        return false;
    };
    if g.delimiter() != Delimiter::Brace {
        return false;
    }
    let tts: Vec<TokenTree> = g.stream().into_iter().collect();
    let mut at_start = true;
    for (i, tt) in tts.iter().enumerate() {
        if at_start && is_punct(Some(tt), '@') {
            let next = tts.get(i + 1);
            if matches!(next, Some(TokenTree::Ident(_))) || pat && is_punct(next, '.') {
                return true;
            }
        }
        // 属性はフィールドの先頭に数える。
        if is_punct(Some(tt), '#') || is_group(Some(tt), Delimiter::Bracket) && at_start {
            continue;
        }
        at_start = is_punct(Some(tt), ',');
    }
    false
}

pub(super) struct RawStructBody {
    pub fields: Vec<RawFieldInit>,
    pub rest: Option<Option<syn::Expr>>,
}

pub(super) struct RawFieldInit {
    pub attrs: Vec<syn::Attribute>,
    pub erased: bool,
    pub member: syn::Member,
    /// 略記（`name`）では `None`。
    pub expr: Option<syn::Expr>,
    pub span: proc_macro2::Span,
}

pub(super) fn parse_struct_body(input: ParseStream) -> syn::Result<RawStructBody> {
    let content;
    syn::braced!(content in input);
    let mut fields = Vec::new();
    let mut rest = None;
    while !content.is_empty() {
        if content.peek(Token![..]) {
            content.parse::<Token![..]>()?;
            rest = Some(if content.is_empty() {
                None
            } else {
                Some(content.parse()?)
            });
            break;
        }
        let attrs = content.call(syn::Attribute::parse_outer)?;
        let start = content.span();
        let erased = content.parse::<Option<Token![@]>>()?.is_some();
        let member: syn::Member = content.parse()?;
        let expr = if erased || content.peek(Token![:]) || !matches!(member, syn::Member::Named(_))
        {
            content.parse::<Token![:]>()?;
            Some(content.parse()?)
        } else {
            None
        };
        let end = content.cursor().span();
        fields.push(RawFieldInit {
            attrs,
            erased,
            member,
            expr,
            span: start.join(end).unwrap_or(start),
        });
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    Ok(RawStructBody { fields, rest })
}

pub(super) struct RawPatStructBody {
    pub fields: Vec<RawFieldPat>,
    pub rest: bool,
}

pub(super) enum RawFieldPat {
    Field {
        attrs: Vec<syn::Attribute>,
        member: syn::Member,
        /// 略記（`ref mut name`）では `pat` は `None`。
        by_ref: bool,
        mutable: bool,
        pat: Option<syn::Pat>,
        span: proc_macro2::Span,
    },
    Erased {
        attrs: Vec<syn::Attribute>,
        ident: proc_macro2::Ident,
    },
    ErasedRest {
        attrs: Vec<syn::Attribute>,
        span: proc_macro2::Span,
    },
}

pub(super) fn parse_pat_struct_body(input: ParseStream) -> syn::Result<RawPatStructBody> {
    let content;
    syn::braced!(content in input);
    let mut fields = Vec::new();
    let mut rest = false;
    while !content.is_empty() {
        let attrs = content.call(syn::Attribute::parse_outer)?;
        // `..` / `@..` の前の属性は rustc が拒否する。書き下しは `..` に属性を付けられないので、
        // 落とさずに拒否する（#95）。
        let rest_attr = |rest| match attrs.first() {
            Some(a) => Err(err(
                syn::spanned::Spanned::span(a),
                Message::AttrOnRest { rest },
            )),
            None => Ok(()),
        };
        if content.peek(Token![..]) {
            rest_attr("..")?;
            content.parse::<Token![..]>()?;
            rest = true;
            break;
        }
        let start = content.span();
        if content.peek(Token![@]) {
            content.parse::<Token![@]>()?;
            if content.peek(Token![..]) {
                rest_attr("@..")?;
                content.parse::<Token![..]>()?;
                fields.push(RawFieldPat::ErasedRest { attrs, span: start });
            } else {
                fields.push(RawFieldPat::Erased {
                    attrs,
                    ident: content.parse::<syn::Ident>()?,
                });
            }
        } else {
            let by_ref = content.parse::<Option<Token![ref]>>()?.is_some();
            let mutable = content.parse::<Option<Token![mut]>>()?.is_some();
            let member: syn::Member = content.parse()?;
            let pat = if !by_ref && !mutable && content.peek(Token![:]) {
                content.parse::<Token![:]>()?;
                Some(syn::Pat::parse_multi_with_leading_vert(&content)?)
            } else {
                None
            };
            let end = content.cursor().span();
            fields.push(RawFieldPat::Field {
                attrs,
                member,
                by_ref,
                mutable,
                pat,
                span: start.join(end).unwrap_or(start),
            });
        }
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    Ok(RawPatStructBody { fields, rest })
}

// ---------------------------------------------------------------------------
// 文

pub(super) enum RawStmt {
    Take(RawRegionIndex),
    Put(RawRegionIndex),
    Leak(proc_macro2::Ident),
    /// 文の位置の `borrow` とループ。`;` は含まない（続く `;` は空の文になる）。
    Atom(RawAtom),
    Item(Box<ast::Item>),
    /// 証明の文（`.eykp` だけ）。
    Proof(RawProof),
}

/// 証明の文（仕様 §5.7 の `ProofStmt`）。
pub(super) enum RawProof {
    Block(syn::Block),
    Assert {
        expr: syn::Expr,
        by: Option<RawAssertBy>,
    },
    AssertForall {
        quant: RawQuant,
        by: syn::Block,
    },
    SpecLet(syn::Local),
    Reveal {
        path: syn::Path,
        fuel: Option<syn::Expr>,
    },
    Hide(syn::Path),
}

pub(super) enum RawAssertBy {
    Block(syn::Block),
    Prover {
        mode: proc_macro2::Ident,
        requires: Vec<syn::Expr>,
    },
}

#[derive(Clone, Copy)]
pub(super) enum ProofKind {
    Block,
    Assert,
    SpecLet,
    Reveal,
    Hide,
}

fn stmt(input: ParseStream) -> syn::Result<bool> {
    match peek_stmt(input)? {
        Some(kind) => parse_stmt_kind(input, kind, true).map(|_| true),
        None => Ok(false),
    }
}

#[derive(Clone, Copy)]
pub(super) enum StmtKind {
    Take,
    Put,
    Leak,
    Atom(AtomKind),
    Item,
    Proof(ProofKind),
}

/// 文の先頭を見る。証明構文と Verus の文はここでエラーにする。
pub(super) fn peek_stmt(input: ParseStream) -> syn::Result<Option<StmtKind>> {
    let t = peek_tts(input, 4);
    for (kw, kind) in [("take", StmtKind::Take), ("put", StmtKind::Put)] {
        if is_ident(t.first(), kw)
            && lookahead(input, |f| {
                // `;` は読むときに求める（無ければ `;` の位置で誤りにする）。
                parse_kw(f, kw)?;
                parse_region_index(f).map(|_| ())
            })
        {
            return Ok(Some(kind));
        }
    }
    if is_ident(t.first(), "leak")
        && matches!(t.get(1), Some(TokenTree::Ident(_)))
        && is_punct(t.get(2), '@')
        && is_punct(t.get(3), ';')
    {
        return Ok(Some(StmtKind::Leak));
    }
    if super::in_proof()
        && let Some(kind) = peek_proof_stmt(&t)
    {
        return Ok(Some(StmtKind::Proof(kind)));
    }
    if let Some(m) = proof_or_verus_stmt(&t) {
        if matches!(m, Message::ProofSyntax { .. }) {
            // `.eyk` の証明の文は中を読まない。文の終わり（`proof { }` の `}`、ほかは `;`）までを覚える。
            let (lo, hi) = stmt_extent(input);
            super::mark_unread(lo, hi);
        }
        return Err(err(input.span(), m));
    }
    if let Some(kind) = peek_atom(input)
        && matches!(kind, AtomKind::Borrow | AtomKind::Loop)
    {
        return Ok(Some(StmtKind::Atom(kind)));
    }
    if item::peek_item_start(input) {
        return Ok(Some(StmtKind::Item));
    }
    Ok(None)
}

/// 文の先頭から、文の終わり（`proof { .. }` ならその `}`、ほかは最上位の `;`。なければ入力の
/// 終わり）までのバイト位置。
fn stmt_extent(input: ParseStream) -> (usize, usize) {
    let mut c = input.cursor();
    let lo = c.span().byte_range().start;
    let mut hi = lo;
    let proof_block = peek_kw(input, "proof");
    while let Some((tt, next)) = c.token_tree() {
        hi = tt.span().byte_range().end;
        c = next;
        let end = match &tt {
            TokenTree::Punct(p) => p.as_char() == ';',
            TokenTree::Group(g) => proof_block && g.delimiter() == Delimiter::Brace,
            _ => false,
        };
        if end {
            break;
        }
    }
    (lo, hi)
}

/// `take` / `put` / `borrow` / `borrow mut` の領域に、`self` でない値の消去フィールドを書いた形
/// （`put t@nodes[r]`、`borrow a.b@nodes[r] { .. }`）。領域添字として読めないので、そのままでは
/// `put` が識別子になり「`;` が必要」としか言えない（#84）。どの形でも Rust の式ではない。
fn erased_region(input: ParseStream) -> Option<syn::Error> {
    let t = peek_tts(input, 16);
    let kw = ["take", "put", "borrow"]
        .into_iter()
        .find(|k| is_ident(t.first(), k))?;
    let mut i = 1;
    if kw == "borrow" && is_ident(t.get(1), "mut") {
        i = 2;
    }
    // 受け手：`x` か `x.y.z`（`self` だけなら仕様どおりの領域）。
    let start = i;
    let mut base = Vec::new();
    loop {
        let Some(TokenTree::Ident(x)) = t.get(i) else {
            return None;
        };
        base.push(x.to_string());
        i += 1;
        if is_punct(t.get(i), '.') {
            i += 1;
            continue;
        }
        break;
    }
    if base == ["self"] || !is_punct(t.get(i), '@') {
        return None;
    }
    let Some(TokenTree::Ident(field)) = t.get(i + 1) else {
        return None;
    };
    if !is_group(t.get(i + 2), Delimiter::Bracket) {
        return None;
    }
    let span = t
        .get(start)
        .map(|x| x.span())
        .unwrap_or_else(|| input.span());
    Some(err(
        span,
        Message::ErasedRegion {
            kw,
            base: base.join("."),
            field: field.to_string(),
        },
    ))
}

/// 文の先頭の証明構文（仕様 §5.7）と、Ebykn にない Verus の文。
fn proof_or_verus_stmt(t: &[TokenTree]) -> Option<Message> {
    let proof = |s: &str| {
        Some(Message::ProofSyntax {
            written: s.to_string(),
        })
    };
    // `.eykp` にも書けない Verus の書き方なので、`.eyk` でも証明ファイルへは案内しない。
    if is_ident(t.first(), "reveal_with_fuel") && is_group(t.get(1), Delimiter::Parenthesis) {
        return Some(Message::VerusSyntax {
            written: "reveal_with_fuel(f, n)".into(),
            ebykn: "reveal(f, n)".into(),
        });
    }
    if is_ident(t.first(), "proof") && is_group(t.get(1), Delimiter::Brace) {
        return proof("proof { .. }");
    }
    if is_ident(t.first(), "assert") && !is_punct(t.get(1), '!') {
        return proof("assert");
    }
    if is_ident(t.first(), "spec") && is_ident(t.get(1), "let") {
        return proof("spec let");
    }
    for kw in ["reveal", "hide"] {
        if is_ident(t.first(), kw) && is_group(t.get(1), Delimiter::Parenthesis) {
            return proof(kw);
        }
    }
    if is_ident(t.first(), "assume") && is_group(t.get(1), Delimiter::Parenthesis) {
        return Some(Message::VerusOnly {
            written: "assume(..)".into(),
        });
    }
    None
}

/// `.eykp` の文の先頭の証明構文（仕様 §5.7）。
fn peek_proof_stmt(t: &[TokenTree]) -> Option<ProofKind> {
    if is_ident(t.first(), "proof") && is_group(t.get(1), Delimiter::Brace) {
        Some(ProofKind::Block)
    } else if is_ident(t.first(), "assert") && !is_punct(t.get(1), '!') {
        Some(ProofKind::Assert)
    } else if is_ident(t.first(), "spec") && is_ident(t.get(1), "let") {
        Some(ProofKind::SpecLet)
    } else if is_ident(t.first(), "reveal") && is_group(t.get(1), Delimiter::Parenthesis) {
        Some(ProofKind::Reveal)
    } else if is_ident(t.first(), "hide") && is_group(t.get(1), Delimiter::Parenthesis) {
        Some(ProofKind::Hide)
    } else {
        None
    }
}

fn parse_proof(input: ParseStream, kind: ProofKind) -> syn::Result<RawProof> {
    Ok(match kind {
        ProofKind::Block => {
            parse_kw(input, "proof")?;
            RawProof::Block(input.parse()?)
        }
        ProofKind::Assert => parse_assert(input)?,
        ProofKind::SpecLet => {
            parse_kw(input, "spec")?;
            let l = match input.parse::<syn::Stmt>()? {
                syn::Stmt::Local(l) => l,
                _ => return Err(input.error("expected `let`")),
            };
            // 仕様 §5.7：`"spec" "let" Pat "=" Expr ";"`。初期値のないものと `let-else` は書けない。
            match &l.init {
                None => {
                    return Err(err(
                        l.semi_token.span,
                        Message::Expected {
                            what: Expected::SpecLetInit,
                        },
                    ));
                }
                Some(init) if init.diverge.is_some() => {
                    return Err(err(
                        init.diverge.as_ref().expect("diverge").0.span,
                        Message::NotAllowedHere {
                            written: "spec let .. else { .. }".into(),
                        },
                    ));
                }
                Some(_) => {}
            }
            RawProof::SpecLet(l)
        }
        ProofKind::Reveal => {
            parse_kw(input, "reveal")?;
            let content;
            syn::parenthesized!(content in input);
            let path: syn::Path = content.parse()?;
            let fuel = if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
                Some(content.parse()?)
            } else {
                None
            };
            if !content.is_empty() {
                return Err(content.error("expected `)`"));
            }
            input.parse::<Token![;]>()?;
            RawProof::Reveal { path, fuel }
        }
        ProofKind::Hide => {
            parse_kw(input, "hide")?;
            let content;
            syn::parenthesized!(content in input);
            let path: syn::Path = content.parse()?;
            if !content.is_empty() {
                return Err(content.error("expected `)`"));
            }
            input.parse::<Token![;]>()?;
            RawProof::Hide(path)
        }
    })
}

/// `assert P;`、`assert P by { .. };`、`assert P by(mode) requires Q;`、
/// `assert forall x in r, P by { .. };`。
fn parse_assert(input: ParseStream) -> syn::Result<RawProof> {
    parse_kw(input, "assert")?;
    // `assert forall .. by { .. }` は束縛子を `by` の中で使える形（Verus の `assert forall`）。
    // `by { .. }` が続かない `assert forall ..` は、量化子の式の `assert`。
    if matches!(peek_atom(input), Some(AtomKind::Quant)) && peek_kw(input, "forall") {
        let ahead = input.fork();
        if let Ok(quant) = parse_quant(&ahead, true)
            && peek_kw(&ahead, "by")
            && ahead.peek2(syn::token::Brace)
        {
            input.advance_to(&ahead);
            parse_kw(input, "by")?;
            let by = input.parse()?;
            input.parse::<Token![;]>()?;
            return Ok(RawProof::AssertForall { quant, by });
        }
    }
    let expr: syn::Expr = input.parse()?;
    let by = if peek_kw(input, "by") {
        parse_kw(input, "by")?;
        if input.peek(syn::token::Brace) {
            Some(RawAssertBy::Block(input.parse()?))
        } else if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let mode: proc_macro2::Ident = content.parse()?;
            if !content.is_empty() {
                return Err(content.error("expected `)`"));
            }
            let mut requires = Vec::new();
            if peek_kw(input, "requires") {
                parse_kw(input, "requires")?;
                loop {
                    requires.push(input.parse()?);
                    if !input.peek(Token![,]) {
                        break;
                    }
                    input.parse::<Token![,]>()?;
                    if input.peek(Token![;]) {
                        break;
                    }
                }
            }
            Some(RawAssertBy::Prover { mode, requires })
        } else {
            return Err(input.error("expected `{` or `(` after `by`"));
        }
    } else {
        None
    };
    input.parse::<Token![;]>()?;
    Ok(RawProof::Assert { expr, by })
}

/// 文を読む。`lookahead_only` のとき（差し込み口から）は、項目を読むときの診断を捨てる。
pub(super) fn parse_stmt_kind(
    input: ParseStream,
    kind: StmtKind,
    lookahead_only: bool,
) -> syn::Result<RawStmt> {
    Ok(match kind {
        StmtKind::Take | StmtKind::Put => {
            input.parse::<proc_macro2::Ident>()?;
            let r = parse_region_index(input)?;
            input.parse::<Token![;]>()?;
            if matches!(kind, StmtKind::Take) {
                RawStmt::Take(r)
            } else {
                RawStmt::Put(r)
            }
        }
        StmtKind::Leak => {
            parse_kw(input, "leak")?;
            let ident = input.parse::<proc_macro2::Ident>()?;
            input.parse::<Token![@]>()?;
            input.parse::<Token![;]>()?;
            RawStmt::Leak(ident)
        }
        StmtKind::Atom(k) => RawStmt::Atom(parse_atom(input, k)?),
        StmtKind::Proof(k) => RawStmt::Proof(parse_proof(input, k)?),
        StmtKind::Item => {
            if lookahead_only {
                RawStmt::Item(Box::new(lookahead_item(input)?))
            } else {
                RawStmt::Item(Box::new(item::parse_item(input, item::Ctx::Block)?))
            }
        }
    })
}

/// 差し込み口で、ブロックの中の項目を読む。読んだ項目と診断を覚えておき、写すとき
/// （[`parse_stmt_verbatim`]）に使う。読み直すと、入れ子の項目の深さについて指数的になる。
fn lookahead_item(input: ParseStream) -> syn::Result<ast::Item> {
    let lo = input.span().byte_range().start;
    if let Some(c) = super::cached_item(lo) {
        skip_to(input, c.hi)?;
        return Ok(c.item);
    }
    let begin = input.fork();
    let (item, diags) = super::captured(|| item::parse_item(input, item::Ctx::Block));
    let item = item?;
    let hi = super::span_between(&begin, input).hi;
    super::cache_item(
        lo,
        super::CachedItem {
            item: item.clone(),
            diags,
            hi,
        },
    );
    Ok(item)
}

/// バイト位置 `hi` までのトークンを読み飛ばす。
fn skip_to(input: ParseStream, hi: usize) -> syn::Result<()> {
    input.step(|cursor| {
        let mut c = *cursor;
        while let Some((tt, next)) = c.token_tree() {
            if tt.span().byte_range().start >= hi {
                break;
            }
            c = next;
        }
        Ok(((), c))
    })
}

// ---------------------------------------------------------------------------
// `Verbatim` の読み直し

pub(super) enum RawVerbatimExpr {
    Atom(RawAtom),
    Struct {
        path: syn::Path,
        body: RawStructBody,
    },
    Erased {
        base: syn::Expr,
        field: proc_macro2::Ident,
    },
}

/// 式の位置の `Verbatim`（差し込み口が読んだもの）を読み直す。
pub(super) fn parse_expr_verbatim(
    input: ParseStream,
) -> syn::Result<(Vec<syn::Attribute>, RawVerbatimExpr)> {
    let attrs = input.call(syn::Attribute::parse_outer)?;
    // 原子（全体を読み切れるときだけ）。
    if let Some(kind) = peek_atom(input) {
        let ahead = input.fork();
        if let Ok(raw) = parse_atom(&ahead, kind)
            && ahead.is_empty()
        {
            input.advance_to(&ahead);
            return Ok((attrs, RawVerbatimExpr::Atom(raw)));
        }
    }
    // 構造体リテラル。
    {
        let ahead = input.fork();
        if let Ok(path) = ahead.call(syn::Path::parse)
            && brace_has_erased_field(&ahead, false)
            && let Ok(body) = parse_struct_body(&ahead)
            && ahead.is_empty()
        {
            input.advance_to(&ahead);
            return Ok((attrs, RawVerbatimExpr::Struct { path, body }));
        }
    }
    // 消去フィールド：末尾の `@ name` を切り離し、前を式として読む。
    let tts: Vec<TokenTree> = input
        .step(|c| Ok((c.token_stream(), syn::buffer::Cursor::empty())))?
        .into_iter()
        .collect();
    let n = tts.len();
    if n >= 3
        && is_punct(tts.get(n - 2), '@')
        && let Some(TokenTree::Ident(field)) = tts.get(n - 1)
    {
        let base: TokenStream = tts[..n - 2].iter().cloned().collect();
        let base = syn::Expr::parse.parse2(base)?;
        return Ok((
            attrs,
            RawVerbatimExpr::Erased {
                base,
                field: field.clone(),
            },
        ));
    }
    Err(err(
        tts.first()
            .map_or_else(proc_macro2::Span::call_site, |t| t.span()),
        Message::Parser("unsupported expression".into()),
    ))
}

/// パターンの位置の `Verbatim` を読み直す。
pub(super) fn parse_pat_verbatim(input: ParseStream) -> syn::Result<(syn::Path, RawPatStructBody)> {
    let path = input.call(syn::Path::parse)?;
    let body = parse_pat_struct_body(input)?;
    Ok((path, body))
}

/// 文の位置の `Verbatim`（`Stmt::Item(Item::Verbatim(..))`）を読み直す。
pub(super) fn parse_stmt_verbatim(
    input: ParseStream,
) -> syn::Result<(Vec<syn::Attribute>, RawStmt)> {
    let ahead = input.fork();
    let attrs = ahead.call(syn::Attribute::parse_outer)?;
    match peek_stmt(&ahead)? {
        Some(StmtKind::Item) => {
            // 差し込み口で読んだ項目があれば使う（属性はフォークが先に読んでいるので、ここで足す）。
            let lo = ahead.span().byte_range().start;
            if let Some(c) = super::cached_item(lo) {
                for d in c.diags {
                    super::push(d.span, d.message);
                }
                input.advance_to(&ahead);
                skip_to(input, c.hi)?;
                let mut item = c.item;
                let mut outer = super::convert::attrs(&attrs);
                // 構文解析器が先に読んだ外部属性は、項目を読んだときには見ていない。
                item::proof_attrs_on_item(&outer, &item.kind);
                if let Some(a) = outer.first() {
                    item.span = a.span.to(item.span);
                }
                outer.append(&mut item.attrs);
                item.attrs = outer;
                return Ok((Vec::new(), RawStmt::Item(Box::new(item))));
            }
            // 項目は属性ごと読む。
            let raw = parse_stmt_kind(input, StmtKind::Item, false)?;
            Ok((Vec::new(), raw))
        }
        Some(kind) => {
            input.advance_to(&ahead);
            Ok((attrs, parse_stmt_kind(input, kind, false)?))
        }
        None => Err(input.error("unsupported statement")),
    }
}
