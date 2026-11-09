//! 式・文・パターンの写し。

use proc_macro2::TokenStream;
use quote::ToTokens;
use verus_syn as syn;
use verus_syn::parse::Parser as _;

use super::{attrs, generic_arg, ident, lifetime, lit, mac, member, path, qself, tokens, ty};
use crate::ast;
use crate::diag::Message;
use crate::parse::hook::{self, RawAtom, RawRegion, RawRegionIndex, RawStmt, RawVerbatimExpr};
use crate::parse::{item, push, push_error, sp, span_of, span_of_tokens};
use crate::span::Span;

fn mk(attrs: Vec<ast::Attr>, kind: ast::ExprKind, span: Span) -> ast::Expr {
    ast::Expr { attrs, kind, span }
}

fn bx(e: syn::Expr) -> Box<ast::Expr> {
    Box::new(expr(e))
}

/// 診断を足し、中を読まないトークン列で置き換える。
fn rejected<T: ToTokens>(t: &T, m: Message) -> ast::Expr {
    let tk = tokens(t);
    push(tk.span, m);
    let span = tk.span;
    mk(Vec::new(), ast::ExprKind::Rust(tk), span)
}

fn verus(written: &str, ebykn: &str) -> Message {
    Message::VerusSyntax {
        written: written.into(),
        ebykn: ebykn.into(),
    }
}

fn verus_only(written: &str) -> Message {
    Message::VerusOnly {
        written: written.into(),
    }
}

fn proof(written: &str) -> Message {
    Message::ProofSyntax {
        written: written.into(),
    }
}

fn args(p: syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>) -> Vec<ast::Expr> {
    p.into_iter().map(expr).collect()
}

pub(in crate::parse) fn expr(e: syn::Expr) -> ast::Expr {
    use ast::ExprKind as K;
    let span = span_of(&e);
    match e {
        syn::Expr::Array(a) => mk(attrs(&a.attrs), K::Array(args(a.elems)), span),
        syn::Expr::Assign(a) => mk(
            attrs(&a.attrs),
            K::Assign {
                lhs: bx(*a.left),
                rhs: bx(*a.right),
            },
            span,
        ),
        syn::Expr::Binary(b) => binary(b, span),
        syn::Expr::Block(b) => mk(
            attrs(&b.attrs),
            K::Block {
                label: b.label.as_ref().map(|l| lifetime(&l.name)),
                block: block(b.block),
            },
            span,
        ),
        syn::Expr::Break(b) => mk(
            attrs(&b.attrs),
            K::Break {
                label: b.label.as_ref().map(lifetime),
                expr: b.expr.map(|e| bx(*e)),
            },
            span,
        ),
        syn::Expr::Call(c) => {
            if c.atomically.is_some() {
                return rejected(&c, verus_only("atomically"));
            }
            mk(
                attrs(&c.attrs),
                K::Call {
                    func: bx(*c.func),
                    args: args(c.args),
                },
                span,
            )
        }
        syn::Expr::Cast(c) => mk(
            attrs(&c.attrs),
            K::Cast {
                expr: bx(*c.expr),
                ty: ty(&c.ty),
            },
            span,
        ),
        syn::Expr::Closure(c) => closure(c, span),
        syn::Expr::Const(c) => mk(attrs(&c.attrs), K::Const(block(c.block)), span),
        syn::Expr::Continue(c) => mk(
            attrs(&c.attrs),
            K::Continue {
                label: c.label.as_ref().map(lifetime),
            },
            span,
        ),
        syn::Expr::Field(f) => mk(
            attrs(&f.attrs),
            K::Field {
                base: bx(*f.base),
                member: member(&f.member),
            },
            span,
        ),
        syn::Expr::ForLoop(f) => {
            if f.invariant_except_break.is_some() || f.ensures.is_some() || f.expr_name.is_some() {
                return rejected(&f, verus_only("for .. ensures / invariant_except_break"));
            }
            let clauses = loop_clauses(f.invariant, f.decreases);
            mk(
                attrs(&f.attrs),
                K::Loop(ast::Loop {
                    label: f.label.as_ref().map(|l| lifetime(&l.name)),
                    kind: ast::LoopKind::For {
                        pat: Box::new(pat(*f.pat)),
                        iter: bx(*f.expr),
                    },
                    clauses,
                    body: block(f.body),
                }),
                span,
            )
        }
        syn::Expr::While(w) => {
            if w.invariant_except_break.is_some()
                || w.ensures.is_some()
                || w.invariant_ensures.is_some()
            {
                return rejected(&w, verus_only("while .. ensures / invariant_except_break"));
            }
            let clauses = loop_clauses(w.invariant, w.decreases);
            mk(
                attrs(&w.attrs),
                K::Loop(ast::Loop {
                    label: w.label.as_ref().map(|l| lifetime(&l.name)),
                    kind: ast::LoopKind::While(bx(*w.cond)),
                    clauses,
                    body: block(w.body),
                }),
                span,
            )
        }
        syn::Expr::Loop(l) => {
            if l.invariant_except_break.is_some()
                || l.ensures.is_some()
                || l.invariant_ensures.is_some()
            {
                return rejected(&l, verus_only("loop .. ensures / invariant_except_break"));
            }
            let clauses = loop_clauses(l.invariant, l.decreases);
            mk(
                attrs(&l.attrs),
                K::Loop(ast::Loop {
                    label: l.label.as_ref().map(|l| lifetime(&l.name)),
                    kind: ast::LoopKind::Loop,
                    clauses,
                    body: block(l.body),
                }),
                span,
            )
        }
        syn::Expr::Group(g) => expr(*g.expr),
        syn::Expr::If(i) => mk(
            attrs(&i.attrs),
            K::If {
                cond: bx(*i.cond),
                then: block(i.then_branch),
                else_: i.else_branch.map(|(_, e)| bx(*e)),
            },
            span,
        ),
        syn::Expr::Index(i) => mk(
            attrs(&i.attrs),
            K::Index {
                base: bx(*i.expr),
                index: bx(*i.index),
            },
            span,
        ),
        syn::Expr::Infer(i) => mk(attrs(&i.attrs), K::Infer, span),
        syn::Expr::Let(l) => mk(
            attrs(&l.attrs),
            K::Let {
                pat: Box::new(pat(*l.pat)),
                expr: bx(*l.expr),
            },
            span,
        ),
        syn::Expr::Lit(l) => mk(attrs(&l.attrs), K::Lit(lit(&l.lit)), span),
        syn::Expr::Macro(m) => mk(attrs(&m.attrs), K::Macro(mac(&m.mac)), span),
        syn::Expr::Match(m) => mk(
            attrs(&m.attrs),
            K::Match {
                expr: bx(*m.expr),
                arms: m.arms.into_iter().map(arm).collect(),
            },
            span,
        ),
        syn::Expr::MethodCall(m) => {
            if m.atomically.is_some() {
                return rejected(&m, verus_only("atomically"));
            }
            mk(
                attrs(&m.attrs),
                K::MethodCall {
                    receiver: bx(*m.receiver),
                    method: ident(&m.method),
                    turbofish: m
                        .turbofish
                        .as_ref()
                        .map(|t| t.args.iter().map(generic_arg).collect()),
                    args: args(m.args),
                },
                span,
            )
        }
        syn::Expr::Paren(p) => mk(attrs(&p.attrs), K::Paren(bx(*p.expr)), span),
        syn::Expr::Path(p) => mk(
            attrs(&p.attrs),
            K::Path {
                qself: qself(&p.qself),
                path: path(&p.path),
            },
            span,
        ),
        syn::Expr::Range(r) => mk(
            attrs(&r.attrs),
            K::Range {
                start: r.start.map(|e| bx(*e)),
                end: r.end.map(|e| bx(*e)),
                closed: matches!(r.limits, syn::RangeLimits::Closed(_)),
            },
            span,
        ),
        syn::Expr::Reference(r) => mk(
            attrs(&r.attrs),
            K::Ref {
                mutable: r.mutability.is_some(),
                expr: bx(*r.expr),
            },
            span,
        ),
        syn::Expr::Repeat(r) => mk(
            attrs(&r.attrs),
            K::Repeat {
                expr: bx(*r.expr),
                len: bx(*r.len),
            },
            span,
        ),
        syn::Expr::Return(r) => mk(attrs(&r.attrs), K::Return(r.expr.map(|e| bx(*e))), span),
        syn::Expr::Struct(s) => {
            let fields = s
                .fields
                .into_iter()
                .map(|f| {
                    let span = span_of(&f);
                    ast::FieldInit {
                        attrs: attrs(&f.attrs),
                        erased: false,
                        member: member(&f.member),
                        shorthand: f.colon_token.is_none(),
                        expr: expr(f.expr),
                        span,
                    }
                })
                .collect();
            mk(
                attrs(&s.attrs),
                K::Struct(ast::StructLit {
                    qself: qself(&s.qself),
                    path: path(&s.path),
                    fields,
                    rest: s.dot2_token.map(|_| s.rest.map(|e| bx(*e))),
                }),
                span,
            )
        }
        syn::Expr::Try(t) => mk(attrs(&t.attrs), K::Try(bx(*t.expr)), span),
        syn::Expr::Tuple(t) => mk(attrs(&t.attrs), K::Tuple(args(t.elems)), span),
        syn::Expr::Unary(u) => unary(u, span),
        syn::Expr::Unsafe(u) => mk(attrs(&u.attrs), K::Unsafe(block(u.block)), span),
        syn::Expr::Verbatim(ts) => verbatim(ts),
        syn::Expr::Final(f) => {
            // `final(e)` は通常の呼び出し構文（仕様 §5.3）。`final` は Rust の予約語なので、
            // フォークは専用の節点にする。
            let kw = proc_macro2::Ident::new("final", f.final_token.span);
            let fspan = sp(f.final_token.span);
            let func = mk(
                Vec::new(),
                K::Path {
                    qself: None,
                    path: ast::Path {
                        leading_colon: false,
                        segments: vec![ast::PathSegment {
                            ident: ident(&kw),
                            args: ast::GenericArgs::None,
                        }],
                        span: fspan,
                    },
                },
                fspan,
            );
            mk(
                attrs(&f.attrs),
                K::Call {
                    func: Box::new(func),
                    args: vec![expr(*f.arg)],
                },
                span,
            )
        }
        syn::Expr::View(v) => match &*v.expr {
            syn::Expr::Path(p) if p.qself.is_none() && p.path.get_ident().is_some() => {
                let id = p.path.get_ident().expect("ident");
                mk(attrs(&v.attrs), K::Perm(ident(id)), span)
            }
            _ => rejected(&v, verus("e@", "e.view()")),
        },
        syn::Expr::Assume(a) => rejected(&a, verus_only("assume(..)")),
        syn::Expr::Assert(a) => rejected(&a, proof("assert")),
        syn::Expr::AssertForall(a) => rejected(&a, proof("assert forall")),
        syn::Expr::RevealHide(r) => rejected(&r, proof("reveal / hide")),
        syn::Expr::BigAnd(b) => rejected(&b, verus("&&&", "and { .. }")),
        syn::Expr::BigOr(b) => rejected(&b, verus("|||", "or { .. }")),
        syn::Expr::Is(x) => rejected(&x, verus("e is V", "matches!(e, V { .. })")),
        syn::Expr::IsNot(x) => rejected(&x, verus("e isnt V", "!matches!(e, V { .. })")),
        syn::Expr::Has(x) => rejected(&x, verus("s has x", "s.contains(x)")),
        syn::Expr::HasNot(x) => rejected(&x, verus("s hasnt x", "!s.contains(x)")),
        syn::Expr::Matches(x) => rejected(&x, verus("e matches p", "matches!(e, p)")),
        syn::Expr::GetField(x) => rejected(&x, verus_only("e->f")),
        // async / await / yield / try ブロック / &raw など。中は読まずに Verus に渡す。
        other => {
            let tk = tokens(&other);
            mk(Vec::new(), K::Rust(tk), span)
        }
    }
}

fn binary(b: syn::ExprBinary, span: Span) -> ast::Expr {
    use ast::BinOp as B;
    let op_span = span_of(&b.op);
    let op = match b.op {
        syn::BinOp::Add(_) => B::Add,
        syn::BinOp::Sub(_) => B::Sub,
        syn::BinOp::Mul(_) => B::Mul,
        syn::BinOp::Div(_) => B::Div,
        syn::BinOp::Rem(_) => B::Rem,
        syn::BinOp::And(_) => B::And,
        syn::BinOp::Or(_) => B::Or,
        syn::BinOp::BitXor(_) => B::BitXor,
        syn::BinOp::BitAnd(_) => B::BitAnd,
        syn::BinOp::BitOr(_) => B::BitOr,
        syn::BinOp::Shl(_) => B::Shl,
        syn::BinOp::Shr(_) => B::Shr,
        syn::BinOp::Eq(_) => B::Eq,
        syn::BinOp::Lt(_) => B::Lt,
        syn::BinOp::Le(_) => B::Le,
        syn::BinOp::Ne(_) => B::Ne,
        syn::BinOp::Ge(_) => B::Ge,
        syn::BinOp::Gt(_) => B::Gt,
        syn::BinOp::AddAssign(_) => B::AddAssign,
        syn::BinOp::SubAssign(_) => B::SubAssign,
        syn::BinOp::MulAssign(_) => B::MulAssign,
        syn::BinOp::DivAssign(_) => B::DivAssign,
        syn::BinOp::RemAssign(_) => B::RemAssign,
        syn::BinOp::BitXorAssign(_) => B::BitXorAssign,
        syn::BinOp::BitAndAssign(_) => B::BitAndAssign,
        syn::BinOp::BitOrAssign(_) => B::BitOrAssign,
        syn::BinOp::ShlAssign(_) => B::ShlAssign,
        syn::BinOp::ShrAssign(_) => B::ShrAssign,
        syn::BinOp::Imply(_) => B::Implies,
        syn::BinOp::Equiv(_) => B::Iff,
        syn::BinOp::Exply(_) => return rejected(&b, verus("a <== b", "b ==> a")),
        syn::BinOp::BigEq(_) => return rejected(&b, verus("===", "==")),
        syn::BinOp::BigNe(_) => return rejected(&b, verus("!==", "!=")),
        syn::BinOp::ExtEq(_) => return rejected(&b, proof("=~=")),
        syn::BinOp::ExtNe(_) => return rejected(&b, verus_only("!~=")),
        syn::BinOp::ExtDeepEq(_) => return rejected(&b, verus_only("=~~=")),
        syn::BinOp::ExtDeepNe(_) => return rejected(&b, verus_only("!~~=")),
        _ => return rejected(&b, Message::Parser("unsupported operator".into())),
    };
    // Verus は比較演算子の連鎖（`a < b < c`）を受理するが、Rust（syn）は受理しない。
    if op.is_comparison()
        && let syn::Expr::Binary(l) = &*b.left
        && is_comparison(&l.op)
    {
        push(op_span, Message::ChainedComparison);
    }
    mk(
        attrs(&b.attrs),
        ast::ExprKind::Binary {
            op,
            op_span,
            lhs: bx(*b.left),
            rhs: bx(*b.right),
        },
        span,
    )
}

fn is_comparison(op: &syn::BinOp) -> bool {
    matches!(
        op,
        syn::BinOp::Eq(_)
            | syn::BinOp::Lt(_)
            | syn::BinOp::Le(_)
            | syn::BinOp::Ne(_)
            | syn::BinOp::Ge(_)
            | syn::BinOp::Gt(_)
            | syn::BinOp::BigEq(_)
            | syn::BinOp::BigNe(_)
            | syn::BinOp::ExtEq(_)
            | syn::BinOp::ExtNe(_)
            | syn::BinOp::ExtDeepEq(_)
            | syn::BinOp::ExtDeepNe(_)
    )
}

fn unary(u: syn::ExprUnary, span: Span) -> ast::Expr {
    let op = match u.op {
        syn::UnOp::Deref(_) => ast::UnOp::Deref,
        syn::UnOp::Not(_) => ast::UnOp::Not,
        syn::UnOp::Neg(_) => ast::UnOp::Neg,
        syn::UnOp::Proof(_) => return rejected(&u, proof("proof { .. }")),
        syn::UnOp::Forall(_) => return rejected(&u, verus("forall|x| ..", "forall x: T, P")),
        syn::UnOp::Exists(_) => return rejected(&u, verus("exists|x| ..", "exists x: T, P")),
        syn::UnOp::Choose(_) => return rejected(&u, verus_only("choose|x| ..")),
        _ => return rejected(&u, Message::Parser("unsupported operator".into())),
    };
    mk(
        attrs(&u.attrs),
        ast::ExprKind::Unary {
            op,
            expr: bx(*u.expr),
        },
        span,
    )
}

fn closure(c: syn::ExprClosure, span: Span) -> ast::Expr {
    if c.proof_fn.is_some() || c.options.is_some() {
        return rejected(&c, verus_only("proof_fn"));
    }
    if c.invariants.is_some() {
        return rejected(&c, verus_only("opens_invariants"));
    }
    if let Some(a) = c.inputs.iter().find(|a| a.tracked_token.is_some()) {
        push(span_of(a), verus("tracked", "uses"));
    }
    let mut contract = ast::Contract::default();
    if let Some(r) = c.requires {
        contract.clauses.push(spec_clause(
            r.token.span,
            &r.exprs,
            ast::ClauseKind::Requires,
        ));
    }
    if let Some(e) = c.ensures {
        contract.clauses.push(spec_clause(
            e.token.span,
            &e.exprs,
            ast::ClauseKind::Ensures,
        ));
    }
    let ret = match &c.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, tracked, named, t) => {
            if tracked.is_some() {
                push(span_of(&c.output), verus("tracked", "uses"));
            }
            Some(ast::Ret {
                name: named.as_ref().and_then(|n| match &n.1 {
                    syn::Pat::Ident(i) => Some(ident(&i.ident)),
                    _ => None,
                }),
                ty: ty(t),
                span: span_of(&c.output),
            })
        }
    };
    mk(
        attrs(&c.attrs),
        ast::ExprKind::Closure(ast::Closure {
            capture: c.capture.is_some(),
            params: c.inputs.into_iter().map(|a| pat(a.pat)).collect(),
            ret,
            contract,
            body: bx(*c.body),
        }),
        span,
    )
}

fn spec_clause(
    kw: proc_macro2::Span,
    s: &syn::Specification,
    f: fn(Vec<ast::Expr>) -> ast::ClauseKind,
) -> ast::Clause {
    let exprs: Vec<ast::Expr> = s.exprs.iter().cloned().map(expr).collect();
    let keyword = sp(kw);
    let span = exprs.last().map_or(keyword, |e| keyword.to(e.span));
    ast::Clause {
        kind: f(exprs),
        keyword,
        span,
    }
}

fn loop_clauses(inv: Option<syn::Invariant>, dec: Option<syn::Decreases>) -> Vec<ast::LoopClause> {
    let mut out = Vec::new();
    let mut add = |kind, kw: proc_macro2::Span, s: &syn::Specification| {
        let exprs: Vec<ast::Expr> = s.exprs.iter().cloned().map(expr).collect();
        let keyword = sp(kw);
        let span = exprs.last().map_or(keyword, |e| keyword.to(e.span));
        out.push(ast::LoopClause {
            kind,
            exprs,
            keyword,
            span,
        });
    };
    if let Some(i) = inv {
        add(ast::LoopClauseKind::Invariant, i.token.span, &i.exprs);
    }
    if let Some(d) = dec {
        add(ast::LoopClauseKind::Decreases, d.token.span, &d.exprs);
    }
    out
}

fn arm(a: syn::Arm) -> ast::Arm {
    let span = span_of(&a);
    ast::Arm {
        attrs: attrs(&a.attrs),
        pat: pat(a.pat),
        guard: a.guard.map(|(_, g)| expr(*g)),
        body: expr(*a.body),
        span,
    }
}

/// 式の位置の `Verbatim`（差し込み口が読んだ Ebykn の式）。
fn verbatim(ts: TokenStream) -> ast::Expr {
    let span = span_of_tokens(&ts).unwrap_or_else(|| sp(proc_macro2::Span::call_site()));
    match hook::parse_expr_verbatim.parse2(ts.clone()) {
        Ok((a, raw)) => {
            let outer = attrs(&a);
            let kind = match raw {
                RawVerbatimExpr::Atom(raw) => raw_atom(raw),
                RawVerbatimExpr::Struct { path: p, body } => {
                    let fields = body
                        .fields
                        .into_iter()
                        .map(|f| {
                            let m = member(&f.member);
                            let shorthand = f.expr.is_none();
                            let e = match f.expr {
                                Some(e) => expr(e),
                                None => match &m {
                                    ast::Member::Named(i) => mk(
                                        Vec::new(),
                                        ast::ExprKind::Path {
                                            qself: None,
                                            path: ast::Path {
                                                leading_colon: false,
                                                segments: vec![ast::PathSegment {
                                                    ident: i.clone(),
                                                    args: ast::GenericArgs::None,
                                                }],
                                                span: i.span,
                                            },
                                        },
                                        i.span,
                                    ),
                                    ast::Member::Unnamed { span, .. } => {
                                        mk(Vec::new(), ast::ExprKind::Infer, *span)
                                    }
                                },
                            };
                            ast::FieldInit {
                                attrs: attrs(&f.attrs),
                                erased: f.erased,
                                member: m,
                                expr: e,
                                shorthand,
                                span: sp(f.span),
                            }
                        })
                        .collect();
                    ast::ExprKind::Struct(ast::StructLit {
                        qself: None,
                        path: path(&p),
                        fields,
                        rest: body.rest.map(|r| r.map(bx)),
                    })
                }
                RawVerbatimExpr::Erased { base, field } => ast::ExprKind::Erased {
                    base: bx(base),
                    field: ident(&field),
                },
            };
            mk(outer, kind, span)
        }
        Err(e) => {
            push_error(e);
            mk(
                Vec::new(),
                ast::ExprKind::Rust(ast::Tokens { tokens: ts, span }),
                span,
            )
        }
    }
}

pub(in crate::parse) fn raw_region_index(r: RawRegionIndex) -> ast::RegionIndex {
    ast::RegionIndex {
        region: match r.region {
            RawRegion::SelfField { self_span, field } => ast::Region::SelfField {
                self_span: sp(self_span),
                field: ident(&field),
            },
            RawRegion::Var(i) => ast::Region::Var(ident(&i)),
        },
        index: bx(r.index),
        span: sp(r.span),
    }
}

pub(in crate::parse) fn raw_atom(raw: RawAtom) -> ast::ExprKind {
    match raw {
        RawAtom::Quant(q) => ast::ExprKind::Quant {
            kind: q.kind,
            binders: q
                .binders
                .into_iter()
                .map(|b| {
                    let id = ident(&b.ident);
                    let (kind, end) = match b.kind {
                        hook::RawBinderKind::Typed(t) => {
                            let t = ty(&t);
                            let end = t.span;
                            (ast::BinderKind::Typed(t), end)
                        }
                        hook::RawBinderKind::In(e) => {
                            let e = expr(e);
                            let end = e.span;
                            (ast::BinderKind::In(e), end)
                        }
                        hook::RawBinderKind::Bare => (ast::BinderKind::Bare, id.span),
                    };
                    ast::Binder {
                        span: id.span.to(end),
                        ident: id,
                        kind,
                    }
                })
                .collect(),
            body: bx(q.body),
        },
        RawAtom::AndOr(a) => ast::ExprKind::AndOr {
            kind: a.kind,
            exprs: a.exprs.into_iter().map(expr).collect(),
        },
        RawAtom::Borrow(b) => ast::ExprKind::Borrow {
            mutable: b.mutable,
            region: raw_region_index(b.region),
            body: block(b.body),
        },
        RawAtom::Loop(l) => ast::ExprKind::Loop(ast::Loop {
            label: l.label.as_ref().map(lifetime),
            kind: match l.kind {
                hook::RawLoopKind::While(c) => ast::LoopKind::While(bx(c)),
                hook::RawLoopKind::For { pat: p, iter } => ast::LoopKind::For {
                    pat: Box::new(pat(p)),
                    iter: bx(iter),
                },
                hook::RawLoopKind::Loop => ast::LoopKind::Loop,
            },
            clauses: l
                .clauses
                .into_iter()
                .map(|c| {
                    let keyword = sp(c.kw.span());
                    let exprs: Vec<ast::Expr> = c.exprs.into_iter().map(expr).collect();
                    let span = exprs.last().map_or(keyword, |e| keyword.to(e.span));
                    ast::LoopClause {
                        kind: c.kind,
                        exprs,
                        keyword,
                        span,
                    }
                })
                .collect(),
            body: block(l.body),
        }),
    }
}

pub(in crate::parse) fn block(b: syn::Block) -> ast::Block {
    let span = sp(b.brace_token.span.join());
    let mut stmts = Vec::new();
    for s in b.stmts {
        stmts.push(stmt(s));
    }
    ast::Block { stmts, span }
}

fn stmt(s: syn::Stmt) -> ast::Stmt {
    let span = span_of(&s);
    let kind = match s {
        syn::Stmt::Local(l) => {
            if l.tracked.is_some() || l.ghost.is_some() {
                push(span, verus_only("let ghost / let tracked"));
            }
            let (p, t) = match l.pat {
                syn::Pat::Type(pt) => (pat(*pt.pat), Some(ty(&pt.ty))),
                other => (pat(other), None),
            };
            let (init, else_) = match l.init {
                Some(init) => {
                    let else_ = init.diverge.map(|(_, e)| match *e {
                        syn::Expr::Block(b) => block(b.block),
                        other => ast::Block {
                            span: span_of(&other),
                            stmts: Vec::new(),
                        },
                    });
                    (Some(expr(*init.expr)), else_)
                }
                None => (None, None),
            };
            ast::StmtKind::Let(ast::Local {
                attrs: attrs(&l.attrs),
                pat: p,
                ty: t,
                init,
                else_,
            })
        }
        syn::Stmt::Item(syn::Item::Verbatim(ts)) => return stmt_verbatim(ts, span),
        syn::Stmt::Item(other) => {
            // 差し込み口が項目と見なさなかった項目。Ebykn の項目の文法で読み直す。
            let ts = other.to_token_stream();
            match (|input: syn::parse::ParseStream| item::parse_item(input, item::Ctx::Block))
                .parse2(ts)
            {
                Ok(i) => ast::StmtKind::Item(Box::new(i)),
                Err(e) => {
                    push_error(e);
                    ast::StmtKind::Empty
                }
            }
        }
        syn::Stmt::Expr(syn::Expr::Verbatim(ts), Some(_)) if ts.is_empty() => ast::StmtKind::Empty,
        syn::Stmt::Expr(e, semi) => {
            let e = expr(e);
            match e.kind {
                ast::ExprKind::Assign { lhs, rhs }
                    if semi.is_some() && matches!(lhs.kind, ast::ExprKind::Erased { .. }) =>
                {
                    ast::StmtKind::SpecAssign {
                        target: *lhs,
                        value: *rhs,
                    }
                }
                kind => ast::StmtKind::Expr {
                    expr: ast::Expr {
                        attrs: e.attrs,
                        kind,
                        span: e.span,
                    },
                    semi: semi.is_some(),
                },
            }
        }
        syn::Stmt::Macro(m) => {
            let mspan = span_of(&m.mac);
            ast::StmtKind::Expr {
                expr: mk(attrs(&m.attrs), ast::ExprKind::Macro(mac(&m.mac)), mspan),
                semi: m.semi_token.is_some(),
            }
        }
    };
    ast::Stmt { kind, span }
}

/// 文の位置の `Verbatim`（take / put / leak / borrow / ループ / 項目）。
pub(in crate::parse) fn stmt_verbatim(ts: TokenStream, span: Span) -> ast::Stmt {
    let kind = match hook::parse_stmt_verbatim.parse2(ts) {
        Ok((a, raw)) => match raw {
            RawStmt::Take(r) => ast::StmtKind::Take(raw_region_index(r)),
            RawStmt::Put(r) => ast::StmtKind::Put(raw_region_index(r)),
            RawStmt::Leak(i) => ast::StmtKind::Leak(ident(&i)),
            RawStmt::Atom(raw) => ast::StmtKind::Expr {
                expr: mk(attrs(&a), raw_atom(raw), span),
                semi: false,
            },
            RawStmt::Item(i) => ast::StmtKind::Item(i),
        },
        Err(e) => {
            push_error(e);
            ast::StmtKind::Empty
        }
    };
    ast::Stmt { kind, span }
}

pub(in crate::parse) fn pat(p: syn::Pat) -> ast::Pat {
    use ast::PatKind as P;
    let span = span_of(&p);
    let kind = match p {
        syn::Pat::Ident(i) => P::Ident {
            by_ref: i.by_ref.is_some(),
            mutable: i.mutability.is_some(),
            ident: ident(&i.ident),
            sub: i.subpat.map(|(_, s)| Box::new(pat(*s))),
        },
        syn::Pat::Wild(_) => P::Wild,
        syn::Pat::Rest(_) => P::Rest,
        syn::Pat::Lit(l) => P::Lit(Box::new(expr(syn::Expr::Lit(l)))),
        syn::Pat::Const(c) => P::Lit(Box::new(expr(syn::Expr::Const(c)))),
        syn::Pat::Range(r) => P::Range {
            start: r.start.map(|e| bx(*e)),
            end: r.end.map(|e| bx(*e)),
            closed: matches!(r.limits, syn::RangeLimits::Closed(_)),
        },
        syn::Pat::Path(p) => P::Path {
            qself: qself(&p.qself),
            path: path(&p.path),
        },
        syn::Pat::Tuple(t) => P::Tuple(t.elems.into_iter().map(pat).collect()),
        syn::Pat::TupleStruct(t) => P::TupleStruct {
            qself: qself(&t.qself),
            path: path(&t.path),
            elems: t.elems.into_iter().map(pat).collect(),
        },
        syn::Pat::Struct(s) => P::Struct {
            qself: qself(&s.qself),
            path: path(&s.path),
            fields: s
                .fields
                .into_iter()
                .map(|f| {
                    let span = span_of(&f);
                    ast::FieldPat::Field {
                        attrs: attrs(&f.attrs),
                        member: member(&f.member),
                        shorthand: f.colon_token.is_none(),
                        pat: pat(*f.pat),
                        span,
                    }
                })
                .collect(),
            rest: s.rest.is_some(),
        },
        syn::Pat::Reference(r) => P::Ref {
            mutable: r.mutability.is_some(),
            pat: Box::new(pat(*r.pat)),
        },
        syn::Pat::Or(o) => P::Or(o.cases.into_iter().map(pat).collect()),
        syn::Pat::Slice(s) => P::Slice(s.elems.into_iter().map(pat).collect()),
        syn::Pat::Paren(p) => P::Paren(Box::new(pat(*p.pat))),
        syn::Pat::Macro(m) => P::Macro(mac(&m.mac)),
        syn::Pat::Type(t) => P::Type {
            pat: Box::new(pat(*t.pat)),
            ty: ty(&t.ty),
        },
        syn::Pat::Verbatim(ts) => return pat_verbatim(ts, span),
        other => {
            push(span, Message::Parser("unsupported pattern".into()));
            let _ = other;
            P::Wild
        }
    };
    ast::Pat { kind, span }
}

fn pat_verbatim(ts: TokenStream, span: Span) -> ast::Pat {
    match hook::parse_pat_verbatim.parse2(ts) {
        Ok((p, body)) => {
            let fields = body
                .fields
                .into_iter()
                .map(|f| match f {
                    hook::RawFieldPat::Field {
                        attrs: a,
                        member: m,
                        by_ref,
                        mutable,
                        pat: p,
                        span,
                    } => {
                        let m = member(&m);
                        let shorthand = p.is_none();
                        let sub = match p {
                            Some(p) => pat(p),
                            None => {
                                let id = match &m {
                                    ast::Member::Named(i) => i.clone(),
                                    ast::Member::Unnamed { span, .. } => ast::Ident {
                                        name: String::new(),
                                        raw: false,
                                        span: *span,
                                    },
                                };
                                ast::Pat {
                                    span: id.span,
                                    kind: ast::PatKind::Ident {
                                        by_ref,
                                        mutable,
                                        ident: id,
                                        sub: None,
                                    },
                                }
                            }
                        };
                        ast::FieldPat::Field {
                            attrs: attrs(&a),
                            member: m,
                            pat: sub,
                            shorthand,
                            span: sp(span),
                        }
                    }
                    hook::RawFieldPat::Erased(i) => ast::FieldPat::Erased(ident(&i)),
                    hook::RawFieldPat::ErasedRest(s) => ast::FieldPat::ErasedRest(sp(s)),
                })
                .collect();
            ast::Pat {
                kind: ast::PatKind::Struct {
                    qself: None,
                    path: path(&p),
                    fields,
                    rest: body.rest,
                },
                span,
            }
        }
        Err(e) => {
            push_error(e);
            ast::Pat {
                kind: ast::PatKind::Wild,
                span,
            }
        }
    }
}
