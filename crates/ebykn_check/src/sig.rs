//! 署名の検査（仕様 §5.2 の E0501）と、絞り込みの署名展開（仕様 §6.4）。
//!
//! 展開の対象は `impl Invariant for T` の `T` だけ。部分集合型は M4 で、`E0003` にする
//! （[`crate::check_crate`]）。展開した契約は [`FnExpansion`] として返し、書き下し（M2 の
//! `ebykn_lower`）が Verus の式にする。#10 の決定どおり、`&mut` の位置は
//! `requires P[old(x)]`、`ensures P[final(x)]`。

use ebykn_syntax::ast::{self, GenericArg, GenericArgs, Param, TypeKind, UsesItem};
use ebykn_syntax::diag::{CheckMsg, Diagnostic, Message};
use ebykn_syntax::span::Span;

use crate::index::{Index, ItemPath, is_region_type, pat_ident, ptr_or_cell};

/// 書き下しで無名の戻り値に付ける名前（仕様 §6.4）。
pub const RET_NAME: &str = "__eyk_r";

/// `P[x]` の `x` の形。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// `x`（`self` を含む）。
    Var(String),
    /// `old(x)`。
    Old(String),
    /// `final(x)`（#10 の決定）。
    Final(String),
}

/// 包みの形（仕様 §5.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    None,
    /// `x is Some ==> P[x.unwrap()]`。
    Option,
    /// `x is Ok ==> P[x.unwrap()]`。
    Result,
}

/// 足す契約の一つ：`subject` の型の不変条件 `P[subject]`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refine {
    pub ty: ItemPath,
    pub subject: Subject,
    pub wrap: Wrap,
}

/// 一つの関数に足す契約。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FnExpansion {
    pub requires: Vec<Refine>,
    pub ensures: Vec<Refine>,
    /// 無名の戻り値に [`RET_NAME`] を付けるか。
    pub name_ret: bool,
}

impl FnExpansion {
    pub fn is_empty(&self) -> bool {
        self.requires.is_empty() && self.ensures.is_empty()
    }
}

/// 型が展開の位置にあるときの形：参照（`&` / `&mut`）と包み。
struct Position {
    mutable_ref: bool,
    wrap: Wrap,
    ty: ItemPath,
}

/// 展開の文脈。
pub struct SigCx<'a> {
    pub index: &'a Index,
    pub module: &'a [String],
    pub self_ty: Option<&'a ItemPath>,
    /// `impl T { }`（`T` 自身の固有の impl）の中か。
    pub own_impl: bool,
    pub is_pub: bool,
}

/// `uses` / `gives` の対象の型を検査する（E0501）。
pub fn check_perm_targets(
    sig: &ast::Signature,
    contract: &ast::Contract,
    diags: &mut Vec<Diagnostic>,
) {
    let param_ty = |name: &str| {
        sig.params.iter().find_map(|p| match p {
            Param::Typed { pat, ty, .. } if pat_ident(pat).is_some_and(|i| i.name == name) => {
                Some(ty)
            }
            _ => None,
        })
    };
    let push = |diags: &mut Vec<Diagnostic>, span: Span, m: CheckMsg| {
        diags.push(Diagnostic::new(span, Message::Check(m)));
    };
    for u in contract.uses() {
        match u {
            UsesItem::Perm { ident, span, .. } => match param_ty(&ident.name) {
                None => push(
                    diags,
                    *span,
                    CheckMsg::UsesNotParam {
                        name: ident.name.clone(),
                    },
                ),
                Some(ty) if ptr_or_cell(ty).is_none() => push(
                    diags,
                    *span,
                    CheckMsg::UsesType {
                        name: ident.name.clone(),
                    },
                ),
                Some(_) => {}
            },
            UsesItem::Named { ident, ty, span } => {
                if !is_region_type(ty) {
                    push(
                        diags,
                        *span,
                        CheckMsg::UsesRegionType {
                            name: ident.name.clone(),
                        },
                    );
                }
            }
        }
    }
    for g in contract.gives() {
        match &sig.ret {
            Some(ast::Ret {
                name: Some(n), ty, ..
            }) if n.name == g.name => {
                if !matches!(ty.kind, TypeKind::Path { .. }) || ptr_or_cell(ty).is_none() {
                    push(
                        diags,
                        g.span,
                        CheckMsg::GivesType {
                            name: g.name.clone(),
                        },
                    );
                }
            }
            _ => push(
                diags,
                g.span,
                CheckMsg::GivesNotRet {
                    name: g.name.clone(),
                },
            ),
        }
    }
}

/// 署名の §5.5 の位置にある `impl Invariant` の型を探し、足す契約を返す。展開しない位置にあれば W0601。
pub fn expand(cx: &SigCx, sig: &ast::Signature, diags: &mut Vec<Diagnostic>) -> FnExpansion {
    let mut out = FnExpansion::default();
    // impl Invariant for T 由来の P は、T 自身の impl の非 pub の関数には足さない（§6.4 の例外）。
    let skip = |ty: &ItemPath| cx.own_impl && !cx.is_pub && Some(ty) == cx.self_ty;
    for p in &sig.params {
        let (name, ty_pos) = match p {
            Param::Receiver {
                reference,
                mutable: _,
                ty,
                ..
            } => {
                let pos = match ty {
                    Some(t) => position(cx, t, diags),
                    None => cx.self_ty.and_then(|t| {
                        cx.index.invariants.contains_key(t).then(|| Position {
                            mutable_ref: reference.is_some() && receiver_mut(p),
                            wrap: Wrap::None,
                            ty: t.clone(),
                        })
                    }),
                };
                ("self".to_string(), pos)
            }
            Param::Typed { pat, ty, .. } => {
                let pos = position(cx, ty, diags);
                match pat_ident(pat) {
                    Some(i) => (i.name.clone(), pos),
                    // パターンの引数（`(a, b): (T, U)`）には名前がないので展開できない。
                    None => {
                        if let Some(pos) = pos {
                            diags.push(Diagnostic::new(
                                ty.span,
                                Message::Check(CheckMsg::RefinementNested {
                                    ty: pos.ty.join("::"),
                                }),
                            ));
                        }
                        continue;
                    }
                }
            }
        };
        let Some(pos) = ty_pos else { continue };
        if skip(&pos.ty) {
            continue;
        }
        if pos.mutable_ref {
            out.requires.push(Refine {
                ty: pos.ty.clone(),
                subject: Subject::Old(name.clone()),
                wrap: pos.wrap,
            });
            out.ensures.push(Refine {
                ty: pos.ty,
                subject: Subject::Final(name),
                wrap: pos.wrap,
            });
        } else {
            out.requires.push(Refine {
                ty: pos.ty,
                subject: Subject::Var(name),
                wrap: pos.wrap,
            });
        }
    }
    if let Some(ret) = &sig.ret
        && let Some(pos) = position(cx, &ret.ty, diags)
        && !skip(&pos.ty)
    {
        let name = match &ret.name {
            Some(n) => n.name.clone(),
            None => {
                out.name_ret = true;
                RET_NAME.to_string()
            }
        };
        out.ensures.push(Refine {
            ty: pos.ty,
            subject: Subject::Var(name),
            wrap: pos.wrap,
        });
    }
    out
}

fn receiver_mut(p: &Param) -> bool {
    matches!(
        p,
        Param::Receiver {
            reference: Some(_),
            mutable: true,
            ..
        }
    )
}

/// 型が展開の位置の形（`T`、`&T`、`&mut T`、`Option<T>`、`Result<T, E>` とその `&` / `&mut`）なら、その形。
/// 展開しない位置（`Vec<T>` など）に不変条件を持つ型があれば W0601 を出す。型の別名は右辺の型に
/// 置き換えてから判定する（`type O = Option<S>;` の `O` は `Option<S>` の位置）。
fn position(cx: &SigCx, ty: &ast::Type, diags: &mut Vec<Diagnostic>) -> Option<Position> {
    let (m, ty, site) = norm(cx, cx.module, ty, None);
    let (mutable_ref, inner) = match &ty.kind {
        TypeKind::Ref { mutable, elem, .. } => (*mutable, (**elem).clone()),
        _ => (false, ty.clone()),
    };
    let (m, inner, site) = norm(cx, &m, &inner, site);
    if let Some(t) = invariant_type(cx, &m, &inner) {
        warn_nested_args(cx, &m, &inner, site, diags);
        return Some(Position {
            mutable_ref,
            wrap: Wrap::None,
            ty: t,
        });
    }
    if let Some((wrap, arg, rest)) = wrapper(&inner) {
        let (ma, arg, sa) = norm(cx, &m, arg, site);
        if let Some(t) = invariant_type(cx, &ma, &arg) {
            warn_nested_args(cx, &ma, &arg, sa, diags);
            for r in rest {
                warn_nested(cx, &m, r, site, diags);
            }
            return Some(Position {
                mutable_ref,
                wrap,
                ty: t,
            });
        }
    }
    warn_nested(cx, &m, &inner, site, diags);
    None
}

/// 型の別名を右辺に置き換え、括弧を外す。置き換えたら、診断の位置を元の型（`site`）にする。
fn norm(
    cx: &SigCx,
    module: &[String],
    ty: &ast::Type,
    site: Option<Span>,
) -> (Vec<String>, ast::Type, Option<Span>) {
    let mut m = module.to_vec();
    let mut t = strip_paren(ty).clone();
    let mut site = site;
    for _ in 0..16 {
        match cx.index.expand_alias(&m, &t, cx.self_ty) {
            Some((am, at)) => {
                site = Some(site.unwrap_or(ty.span));
                m = am;
                t = strip_paren(&at).clone();
            }
            None => break,
        }
    }
    (m, t, site)
}

fn strip_paren(ty: &ast::Type) -> &ast::Type {
    match &ty.kind {
        TypeKind::Paren(t) => strip_paren(t),
        _ => ty,
    }
}

/// `Option<A>` / `Result<A, ..>` なら、包みと `A` と残りの型引数。
fn wrapper(ty: &ast::Type) -> Option<(Wrap, &ast::Type, Vec<&ast::Type>)> {
    let TypeKind::Path { qself: None, path } = &ty.kind else {
        return None;
    };
    let seg = path.segments.last()?;
    let wrap = match seg.ident.name.as_str() {
        "Option" => Wrap::Option,
        "Result" => Wrap::Result,
        _ => return None,
    };
    let GenericArgs::Angle { args, .. } = &seg.args else {
        return None;
    };
    let mut tys = args.iter().filter_map(|a| match a {
        GenericArg::Type(t) => Some(t),
        _ => None,
    });
    let first = tys.next()?;
    Some((wrap, first, tys.collect()))
}

fn invariant_type(cx: &SigCx, module: &[String], ty: &ast::Type) -> Option<ItemPath> {
    let p = cx.index.type_path(module, ty, cx.self_ty)?;
    cx.index.invariants.contains_key(&p).then_some(p)
}

/// `ty` の中（`ty` 自身を除く）にある不変条件を持つ型に W0601 を出す。
fn warn_nested_args(
    cx: &SigCx,
    module: &[String],
    ty: &ast::Type,
    site: Option<Span>,
    diags: &mut Vec<Diagnostic>,
) {
    for t in child_types(ty) {
        warn_nested(cx, module, t, site, diags);
    }
}

/// `ty` とその中にある不変条件を持つ型に W0601 を出す（型の別名は右辺をたどる）。
fn warn_nested(
    cx: &SigCx,
    module: &[String],
    ty: &ast::Type,
    site: Option<Span>,
    diags: &mut Vec<Diagnostic>,
) {
    let (m, ty, site) = norm(cx, module, ty, site);
    if let Some(t) = invariant_type(cx, &m, &ty) {
        diags.push(Diagnostic::new(
            site.unwrap_or(ty.span),
            Message::Check(CheckMsg::RefinementNested { ty: t.join("::") }),
        ));
    }
    for t in child_types(&ty) {
        warn_nested(cx, &m, t, site, diags);
    }
}

fn child_types(ty: &ast::Type) -> Vec<&ast::Type> {
    match &ty.kind {
        TypeKind::Path { path, .. } => path
            .segments
            .iter()
            .flat_map(|s| match &s.args {
                GenericArgs::Angle { args, .. } => args
                    .iter()
                    .filter_map(|a| match a {
                        GenericArg::Type(t) => Some(t),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                GenericArgs::Paren { inputs, output, .. } => {
                    inputs.iter().chain(output.as_deref()).collect()
                }
                GenericArgs::None => Vec::new(),
            })
            .collect(),
        TypeKind::Ref { elem, .. }
        | TypeKind::Array { elem, .. }
        | TypeKind::Slice(elem)
        | TypeKind::Paren(elem) => vec![&**elem],
        TypeKind::Tuple(ts) => ts.iter().collect(),
        _ => Vec::new(),
    }
}

/// `impl Invariant for T` の入れ子（仕様 §6.4）：`T` のフィールドで不変条件を持つ型（`Option<U>` を含む）。
pub fn nested(index: &Index, ty: &ItemPath) -> Vec<(String, ItemPath, Wrap)> {
    let Some(s) = index.structs.get(ty) else {
        return Vec::new();
    };
    let cx = SigCx {
        index,
        module: &s.module,
        self_ty: Some(ty),
        own_impl: false,
        is_pub: true,
    };
    let mut out = Vec::new();
    for (name, fty) in &s.fields {
        let (m, fty, _) = norm(&cx, &s.module, fty, None);
        if let Some(t) = invariant_type(&cx, &m, &fty) {
            out.push((name.clone(), t, Wrap::None));
        } else if let Some((Wrap::Option, arg, _)) = wrapper(&fty) {
            let (ma, arg, _) = norm(&cx, &m, arg, None);
            if let Some(t) = invariant_type(&cx, &ma, &arg) {
                out.push((name.clone(), t, Wrap::Option));
            }
        }
    }
    out
}
