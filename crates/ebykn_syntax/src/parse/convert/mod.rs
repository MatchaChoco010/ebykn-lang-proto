//! フォーク（verus_syn）の構文木を Ebykn の AST に写す。
//!
//! Ebykn にない Verus の書き方はここで診断にし（[`super::push`]）、写しには中を読まない
//! トークン列を置いて続ける。一つのファイルの誤りをまとめて報告するため。

mod expr;

pub(super) use expr::{block, expr, pat};

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
use quote::ToTokens;
use verus_syn as syn;

use super::{push, sp, span_of, span_of_tokens};
use crate::ast;
use crate::diag::Message;

pub(super) fn ident(i: &proc_macro2::Ident) -> ast::Ident {
    let s = i.to_string();
    let (name, raw) = match s.strip_prefix("r#") {
        Some(n) => (n.to_string(), true),
        None => (s, false),
    };
    ast::Ident {
        name,
        raw,
        span: sp(i.span()),
    }
}

pub(super) fn lifetime(l: &syn::Lifetime) -> ast::Lifetime {
    ast::Lifetime {
        name: l.ident.to_string(),
        span: span_of(l),
    }
}

pub(super) fn tokens<T: ToTokens>(t: &T) -> ast::Tokens {
    let ts = t.to_token_stream();
    let span = span_of_tokens(&ts).unwrap_or_else(|| span_of(t));
    ast::Tokens { tokens: ts, span }
}

pub(super) fn lit(l: &syn::Lit) -> ast::Lit {
    ast::Lit {
        text: l.to_token_stream().to_string(),
        span: span_of(l),
    }
}

pub(super) fn path(p: &syn::Path) -> ast::Path {
    ast::Path {
        leading_colon: p.leading_colon.is_some(),
        segments: p
            .segments
            .iter()
            .map(|s| ast::PathSegment {
                ident: ident(&s.ident),
                args: path_args(&s.arguments),
            })
            .collect(),
        span: span_of(p),
    }
}

fn path_args(a: &syn::PathArguments) -> ast::GenericArgs {
    match a {
        syn::PathArguments::None => ast::GenericArgs::None,
        syn::PathArguments::AngleBracketed(a) => ast::GenericArgs::Angle {
            turbofish: a.colon2_token.is_some(),
            args: a.args.iter().map(generic_arg).collect(),
            span: span_of(a),
        },
        syn::PathArguments::Parenthesized(p) => ast::GenericArgs::Paren {
            inputs: p.inputs.iter().map(ty).collect(),
            output: ret_type(&p.output).map(Box::new),
            span: span_of(p),
        },
    }
}

pub(super) fn generic_arg(a: &syn::GenericArgument) -> ast::GenericArg {
    match a {
        syn::GenericArgument::Lifetime(l) => ast::GenericArg::Lifetime(lifetime(l)),
        syn::GenericArgument::Type(t) => ast::GenericArg::Type(ty(t)),
        syn::GenericArgument::Const(e) => ast::GenericArg::Const(expr(e.clone())),
        syn::GenericArgument::AssocType(a) if a.generics.is_none() => ast::GenericArg::AssocType {
            ident: ident(&a.ident),
            ty: ty(&a.ty),
        },
        other => ast::GenericArg::Rust(tokens(other)),
    }
}

/// パスの位置の `ReturnType`（`Fn(A) -> B` の `-> B`）。
fn ret_type(r: &syn::ReturnType) -> Option<ast::Type> {
    match r {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, tracked, named, t) => {
            if tracked.is_some() || named.is_some() {
                push(
                    span_of(r),
                    Message::VerusOnly {
                        written: r.to_token_stream().to_string(),
                    },
                );
            }
            Some(ty(t))
        }
    }
}

pub(super) fn qself(q: &Option<syn::QSelf>) -> Option<ast::QSelf> {
    q.as_ref().map(|q| ast::QSelf {
        ty: Box::new(ty(&q.ty)),
        position: q.position,
    })
}

pub(super) fn ty(t: &syn::Type) -> ast::Type {
    let span = span_of(t);
    let kind = match t {
        syn::Type::Path(p) => ast::TypeKind::Path {
            qself: qself(&p.qself),
            path: path(&p.path),
        },
        syn::Type::Reference(r) => ast::TypeKind::Ref {
            lifetime: r.lifetime.as_ref().map(lifetime),
            mutable: r.mutability.is_some(),
            elem: Box::new(ty(&r.elem)),
        },
        syn::Type::Tuple(t) => ast::TypeKind::Tuple(t.elems.iter().map(ty).collect()),
        syn::Type::Array(a) => ast::TypeKind::Array {
            elem: Box::new(ty(&a.elem)),
            len: Box::new(expr(a.len.clone())),
        },
        syn::Type::Slice(s) => ast::TypeKind::Slice(Box::new(ty(&s.elem))),
        syn::Type::Paren(p) => ast::TypeKind::Paren(Box::new(ty(&p.elem))),
        syn::Type::Group(g) => return ty(&g.elem),
        syn::Type::Never(_) => ast::TypeKind::Never,
        syn::Type::Infer(_) => ast::TypeKind::Infer,
        syn::Type::FnSpec(_) | syn::Type::FnProof(_) => {
            push(
                span,
                Message::VerusOnly {
                    written: t.to_token_stream().to_string(),
                },
            );
            ast::TypeKind::Rust(tokens(t))
        }
        other => ast::TypeKind::Rust(tokens(other)),
    };
    ast::Type { kind, span }
}

pub(super) fn attr(a: &syn::Attribute) -> ast::Attr {
    let (p, args) = match &a.meta {
        syn::Meta::Path(p) => (p, TokenStream::new()),
        syn::Meta::List(l) => {
            let d = match l.delimiter {
                syn::MacroDelimiter::Paren(_) => Delimiter::Parenthesis,
                syn::MacroDelimiter::Brace(_) => Delimiter::Brace,
                syn::MacroDelimiter::Bracket(_) => Delimiter::Bracket,
            };
            let mut g = Group::new(d, l.tokens.clone());
            g.set_span(l.delimiter.span().join());
            (&l.path, TokenStream::from(TokenTree::Group(g)))
        }
        syn::Meta::NameValue(nv) => {
            let mut ts = nv.eq_token.to_token_stream();
            nv.value.to_tokens(&mut ts);
            (&nv.path, ts)
        }
    };
    ast::Attr {
        inner: matches!(a.style, syn::AttrStyle::Inner(_)),
        path: path(p),
        args,
        span: span_of(a),
    }
}

pub(super) fn attrs(a: &[syn::Attribute]) -> Vec<ast::Attr> {
    a.iter().map(attr).collect()
}

pub(super) fn vis(v: &syn::Visibility) -> ast::Vis {
    match v {
        syn::Visibility::Inherited => ast::Vis::Inherited,
        syn::Visibility::Public(p) => ast::Vis::Pub(sp(p.span)),
        other => ast::Vis::Restricted(tokens(other)),
    }
}

pub(super) fn generics(g: &syn::Generics) -> ast::Generics {
    let params = g
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Lifetime(l) => ast::GenericParam::Lifetime {
                lifetime: lifetime(&l.lifetime),
                bounds: (!l.bounds.is_empty()).then(|| tokens(&l.bounds)),
            },
            syn::GenericParam::Type(t) => ast::GenericParam::Type {
                ident: ident(&t.ident),
                bounds: (!t.bounds.is_empty()).then(|| tokens(&t.bounds)),
                default: t.default.as_ref().map(ty),
            },
            syn::GenericParam::Const(c) => ast::GenericParam::Const {
                ident: ident(&c.ident),
                ty: ty(&c.ty),
                default: c.default.clone().map(expr),
            },
        })
        .collect();
    ast::Generics {
        params,
        span: g.lt_token.map(|lt| {
            let s = sp(lt.span);
            g.gt_token.map_or(s, |gt| s.to(sp(gt.span)))
        }),
    }
}

pub(super) fn mac(m: &syn::Macro) -> ast::Macro {
    ast::Macro {
        path: path(&m.path),
        delimiter: match m.delimiter {
            syn::MacroDelimiter::Paren(_) => ast::Delimiter::Paren,
            syn::MacroDelimiter::Brace(_) => ast::Delimiter::Brace,
            syn::MacroDelimiter::Bracket(_) => ast::Delimiter::Bracket,
        },
        tokens: m.tokens.clone(),
        span: span_of(m),
    }
}

pub(super) fn member(m: &syn::Member) -> ast::Member {
    match m {
        syn::Member::Named(i) => ast::Member::Named(ident(i)),
        syn::Member::Unnamed(i) => ast::Member::Unnamed {
            index: i.index,
            span: sp(i.span),
        },
    }
}

/// フィールド（`ghost` / `tracked` は Ebykn にない）。
pub(super) fn field(f: &syn::Field) -> ast::Field {
    if !matches!(f.mode, syn::DataMode::Default) {
        push(
            span_of(f),
            Message::VerusSyntax {
                written: f.mode.to_token_stream().to_string(),
                ebykn: "owns { .. } / spec { .. }".into(),
            },
        );
    }
    ast::Field {
        attrs: attrs(&f.attrs),
        vis: vis(&f.vis),
        ident: f.ident.as_ref().map(ident),
        ty: ty(&f.ty),
        span: span_of(f),
    }
}

pub(super) fn fields(f: &syn::Fields) -> ast::Fields {
    match f {
        syn::Fields::Named(n) => ast::Fields::Named(n.named.iter().map(field).collect()),
        syn::Fields::Unnamed(u) => ast::Fields::Tuple(u.unnamed.iter().map(field).collect()),
        syn::Fields::Unit => ast::Fields::Unit,
    }
}

/// 関数の引数（`tracked` は Ebykn にない）。
pub(super) fn param(a: &syn::FnArg) -> ast::Param {
    if a.tracked.is_some() {
        push(
            span_of(a),
            Message::VerusSyntax {
                written: "tracked".into(),
                ebykn: "uses".into(),
            },
        );
    }
    match &a.kind {
        syn::FnArgKind::Receiver(r) => ast::Param::Receiver {
            attrs: attrs(&r.attrs),
            reference: r.reference.as_ref().map(|(_, l)| l.as_ref().map(lifetime)),
            mutable: r.mutability.is_some(),
            ty: r.colon_token.map(|_| ty(&r.ty)),
            span: span_of(r),
        },
        syn::FnArgKind::Typed(t) => ast::Param::Typed {
            attrs: attrs(&t.attrs),
            pat: pat((*t.pat).clone()),
            ty: ty(&t.ty),
            span: span_of(t),
        },
    }
}
