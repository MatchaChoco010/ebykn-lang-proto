//! フォーク（verus_syn）の構文木を Ebykn の AST に写す。
//!
//! Ebykn にない Verus の書き方はここで診断にし（[`super::push`]）、写しには中を読まない
//! トークン列を置いて続ける。一つのファイルの誤りをまとめて報告するため。

mod expr;

pub(super) use expr::{attr_name, block, expr, pat};

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
use quote::ToTokens;
use verus_syn as syn;
use verus_syn::parse::Parser as _;

use super::{push, sp, span_of, span_of_tokens};
use crate::ast;
use crate::diag::{BindingForm, Message, RawNameKind};

pub(super) fn ident(i: &proc_macro2::Ident) -> ast::Ident {
    let s = i.to_string();
    let (name, raw) = match s.strip_prefix("r#") {
        Some(n) => (n.to_string(), true),
        None => (s, false),
    };
    // `uses` は予約語（呼び出しで領域を渡す `f(x, uses &mut r)` と区別する。#19 の既定 P1）。
    if !raw && crate::reserved::EBYKN_KEYWORDS.contains(&name.as_str()) {
        push(sp(i.span()), Message::ReservedWord { word: name.clone() });
    }
    ast::Ident {
        name,
        raw,
        span: sp(i.span()),
    }
}

/// 局所の名前を束縛する位置の名前（パターンの名前、量化子の変数、名前付きの戻り値、`uses` の
/// 領域、消去フィールドのパターン、const generics の引数）。Rust の予約語の raw の綴り（`r#type`）は
/// Verus が検証できないので E0103（#119。[`crate::reserved::RAW_KEYWORDS`]）。版で予約語になった
/// 名前の診断は、検査が `Ebykn.toml` の `edition` で捨てる。
pub(in crate::parse) fn binding(i: &proc_macro2::Ident) -> ast::Ident {
    binding_as(i, BindingForm::Name)
}

/// [`binding`] の、束縛の書き方（`form`）で直し方の案内を変える版。
pub(in crate::parse) fn binding_as(i: &proc_macro2::Ident, form: BindingForm) -> ast::Ident {
    let id = ident(i);
    if id.raw && crate::reserved::raw_keyword_since(&id.name).is_some() {
        push(
            id.span,
            Message::RawKeywordBinding {
                word: id.name.clone(),
                form,
            },
        );
    }
    id
}

/// Verus が raw の綴り（`r#foo`）を扱えない位置の項目の名前（#147。[`RawNameKind`]）。予約語でない
/// 名前の raw の綴りも E0104。固定した Verus は、`const`・`static`・`const fn`・本体のない trait の
/// 関数の名前から作る記号（`VERUS_UNERASED_PROXY__r#foo`、`VERUS_SPEC__r#foo`）で proc macro が panic
/// し、列挙子と列挙子のフィールドは `no variant` / `no field` で拒否する
/// （`tests/verus-probes/raw_name_*.rs`）。
pub(in crate::parse) fn raw_checked_name(i: &proc_macro2::Ident, kind: RawNameKind) -> ast::Ident {
    let id = ident(i);
    if id.raw {
        push(id.span, Message::raw_item_name(id.name.clone(), kind));
    }
    id
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

/// 中を読まない Rust のトークン列。証明の構文と呼べないマクロを字句的に探す。
pub(super) fn scanned<T: ToTokens>(t: &T) -> ast::Tokens {
    let tk = tokens(t);
    super::tokens::scan_opaque(&tk.tokens);
    tk
}

pub(super) fn lit(l: &syn::Lit) -> ast::Lit {
    ast::Lit {
        text: l.to_token_stream().to_string(),
        span: span_of(l),
    }
}

/// 型・パターンのパスの最後の名前が Verus の ghost の包み（`Tracked` / `Ghost`）なら、裸の名前と先頭が
/// `::` のものは診断にし、それ以外は検査に任せる（#99）。`suffix` は書き方の後ろ（`<..>`、`(..)`）。
pub(super) fn wrapper_path(p: &syn::Path, suffix: &str) {
    let segs: Vec<String> = p
        .segments
        .iter()
        .map(|s| {
            let n = s.ident.to_string();
            n.strip_prefix("r#").unwrap_or(&n).to_string()
        })
        .collect();
    let lead = if p.leading_colon.is_some() { "::" } else { "" };
    // 型の引数がなければ `<..>` を付けない（`Ptr<Ghost>` の `Ghost`）。
    let suffix = match p.segments.last().map(|s| &s.arguments) {
        Some(syn::PathArguments::None) if suffix == "<..>" => "",
        _ => suffix,
    };
    super::reserved_path(
        &segs,
        p.leading_colon.is_some(),
        crate::reserved::is_wrapper,
        format!("{lead}{}{suffix}", segs.join("::")),
        span_of(p),
    );
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
        other => ast::GenericArg::Rust(scanned(other)),
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
        syn::Type::Path(p) => {
            wrapper_path(&p.path, "<..>");
            ast::TypeKind::Path {
                qself: qself(&p.qself),
                path: path(&p.path),
            }
        }
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
        syn::Type::Macro(m) => {
            mac(&m.mac);
            ast::TypeKind::Rust(tokens(t))
        }
        other => {
            let tk = tokens(other);
            super::tokens::scan_opaque(&tk.tokens);
            ast::TypeKind::Rust(tk)
        }
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
    let params: Vec<ast::GenericParam> = g
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Lifetime(l) => ast::GenericParam::Lifetime {
                attrs: attrs(&l.attrs),
                lifetime: lifetime(&l.lifetime),
                bounds: (!l.bounds.is_empty()).then(|| scanned(&l.bounds)),
            },
            syn::GenericParam::Type(t) => ast::GenericParam::Type {
                attrs: attrs(&t.attrs),
                ident: ident(&t.ident),
                bounds: (!t.bounds.is_empty()).then(|| scanned(&t.bounds)),
                default: t.default.as_ref().map(ty),
            },
            syn::GenericParam::Const(c) => ast::GenericParam::Const {
                attrs: attrs(&c.attrs),
                ident: binding(&c.ident),
                ty: ty(&c.ty),
                default: c.default.clone().map(expr),
            },
        })
        .collect();
    // ジェネリクスの引数の `#[cfg]`：検査は cfg を評価せずに引数の並びを読む（#95）。
    for p in &params {
        let (ast::GenericParam::Lifetime { attrs, .. }
        | ast::GenericParam::Type { attrs, .. }
        | ast::GenericParam::Const { attrs, .. }) = p;
        // 診断は cfg の属性を指す（`<#[allow(unused)] #[cfg(x)] T>` の `allow` ではない）。
        // 拒否リストの名前の cfg を持つ属性は、トークンの検査の E0001 だけにする（ほかの属性は見る）。
        if let Some(a) = attrs
            .iter()
            .find(|a| ast::has_cfg(std::slice::from_ref(*a)) && !crate::denylist::attr_denied(a))
        {
            push(
                a.span,
                Message::Check(crate::diag::CheckMsg::Unimplemented(
                    crate::diag::Unimpl::CfgOnGenericParam,
                )),
            );
        }
    }
    ast::Generics {
        params,
        span: g.lt_token.map(|lt| {
            let s = sp(lt.span);
            g.gt_token.map_or(s, |gt| s.to(sp(gt.span)))
        }),
    }
}

/// マクロの呼び出し。`.eyk` で呼べるマクロは固定の一覧だけ（#16 の既定、`docs/current-spec.md`）。
/// vstd の仕様用のマクロの引数は Ebykn の式として読み、std のマクロの引数からは証明の
/// 構文と Verus の構文を字句的に探す。
pub(super) fn mac(m: &syn::Macro) -> ast::Macro {
    let span = span_of(m);
    let names: Vec<String> = m
        .path
        .segments
        .iter()
        .map(|s| {
            let n = s.ident.to_string();
            n.strip_prefix("r#").unwrap_or(&n).to_string()
        })
        .collect();
    let last = names.last().cloned().unwrap_or_default();
    let args = if let Some(msg) = super::tokens::macro_call_message(&names) {
        push(span, msg);
        ast::MacroArgs::Tokens
    } else if super::tokens::macro_allowed(&names) == Some(super::tokens::MacroKind::Vstd) {
        spec_macro_args(&last, m)
    } else {
        // 引数の中の属性の cfg は、ファイル全体のトークンの検査（`tokens::scan`）が拒否リストで見る（#27）。
        super::tokens::scan_opaque(&m.tokens);
        // 引数はトークンのまま持つので、derive や属性マクロ（PR #57 は AST で探す）を見られない。cfg・
        // `cfg_attr`・lint のほかの属性は拒否する（PR #59 のレビュー）。
        if has_unread_attribute(&m.tokens) {
            push(
                span,
                Message::Check(crate::diag::CheckMsg::Unimplemented(
                    crate::diag::Unimpl::AttrInMacro,
                )),
            );
        } else if has_misplaced_attribute(&m.tokens, last == "vec") {
            // 安定版の Rust が式の属性を許さない位置（PR #59 の再確認。`Unimpl::AttrPosInMacro`）。
            push(
                span,
                Message::Check(crate::diag::CheckMsg::Unimplemented(
                    crate::diag::Unimpl::AttrPosInMacro,
                )),
            );
        }
        ast::MacroArgs::Tokens
    };
    ast::Macro {
        path: path(&m.path),
        delimiter: match m.delimiter {
            syn::MacroDelimiter::Paren(_) => ast::Delimiter::Paren,
            syn::MacroDelimiter::Brace(_) => ast::Delimiter::Brace,
            syn::MacroDelimiter::Bracket(_) => ast::Delimiter::Bracket,
        },
        tokens: m.tokens.clone(),
        args,
        span,
    }
}

/// マクロの引数の中で許す属性（cfg と lint）。`cfg_attr` は中の属性もこの一覧で見る。
const MACRO_ARG_ATTRS: &[&str] = &["cfg", "allow", "warn", "deny", "forbid", "expect"];

/// トークン列に、[`MACRO_ARG_ATTRS`] と `cfg_attr` のほかの属性（`#[..]`、`#![..]`）があるか
/// （グループの中を含む）。
fn has_unread_attribute(ts: &proc_macro2::TokenStream) -> bool {
    use proc_macro2::{Delimiter, TokenTree};
    /// 属性の中身（`path args`）が読まない属性か。
    fn unread_meta(body: &[TokenTree]) -> bool {
        let mut k = 0;
        while matches!(body.get(k), Some(TokenTree::Punct(p)) if p.as_char() == ':') {
            k += 1;
        }
        let Some(TokenTree::Ident(head)) = body.get(k) else {
            return true;
        };
        let name = head.to_string();
        let name = name.strip_prefix("r#").unwrap_or(&name);
        // パスの属性（`a::b`）は lint の道具の名前（`clippy::x`）でも、読まない属性として扱う。
        let path = matches!(body.get(k + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':');
        if path {
            return true;
        }
        if name == "cfg_attr" {
            let Some(TokenTree::Group(g)) = body.get(k + 1) else {
                return true;
            };
            let args: Vec<TokenTree> = g.stream().into_iter().collect();
            let mut parts: Vec<&[TokenTree]> = Vec::new();
            let mut start = 0;
            for (i, t) in args.iter().enumerate() {
                if matches!(t, TokenTree::Punct(p) if p.as_char() == ',') {
                    parts.push(&args[start..i]);
                    start = i + 1;
                }
            }
            if start < args.len() {
                parts.push(&args[start..]);
            }
            return parts.iter().skip(1).any(|a| unread_meta(a));
        }
        !MACRO_ARG_ATTRS.contains(&name)
    }
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    tts.iter().enumerate().any(|(i, t)| match t {
        TokenTree::Group(g) => has_unread_attribute(&g.stream()),
        TokenTree::Punct(p) if p.as_char() == '#' => {
            let j = if matches!(tts.get(i + 1), Some(TokenTree::Punct(q)) if q.as_char() == '!') {
                i + 2
            } else {
                i + 1
            };
            match tts.get(j) {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => {
                    let body: Vec<TokenTree> = g.stream().into_iter().collect();
                    unread_meta(&body)
                }
                _ => false,
            }
        }
        _ => false,
    })
}

/// マクロの引数の中の属性の位置の文脈。
#[derive(Clone, Copy, PartialEq, Eq)]
enum AttrCx {
    /// `vec![..]` の要素の並び（要素の頭は、安定版の Rust が式の属性を許す）。
    Elements,
    /// `{ .. }` の中（文の頭、match の腕と構造体の欄の頭）。
    Brace,
    /// ほか（式の中。`assert!(..)` の引数、括弧、添字など）。
    Other,
}

/// トークン列に、安定版の Rust が許さない位置の属性があるか（グループの中を含む）。std のマクロの引数は
/// トークンのまま持ち、Ebykn が式として読まないので、位置をトークンで決める。許すのは、`vec!` の要素と
/// 繰り返しの数の頭（先頭と `,`・`;` の後）と、`{ .. }` の中の文・腕・欄の頭（先頭、`;`・`,`・`{ .. }` の
/// 後）と、そこに続く外側の属性だけ。内側の属性 `#![..]` はどこでも許さない（`{ #![..] .. }` はブロック式
/// の属性になる。入れ子の `fn` の本体のように許される場所もあるが、トークンでは見分けない）。ほかの位置の
/// 式の属性は、安定版の Rust では
/// `attributes on expressions are experimental`（E0658）になり、Verus の toolchain では通るので、
/// verified なのに Verus なしのビルドが落ちる。トークンでは決めきれないもの（代入・二項演算の文の頭の
/// 属性）は残る（#130）。
fn has_misplaced_attribute(ts: &proc_macro2::TokenStream, vec: bool) -> bool {
    misplaced(ts, if vec { AttrCx::Elements } else { AttrCx::Other })
}

fn misplaced(ts: &proc_macro2::TokenStream, cx: AttrCx) -> bool {
    use proc_macro2::{Delimiter, TokenTree};
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    let punct =
        |t: Option<&TokenTree>, c: char| matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == c);
    let bracket = |t: Option<&TokenTree>| matches!(t, Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket);
    // `tts[k]` が属性の `[..]` か（`#[..]` / `#![..]` の `[..]`）。
    let attr_body = |k: usize| {
        bracket(tts.get(k))
            && (punct(k.checked_sub(1).and_then(|j| tts.get(j)), '#')
                || (punct(k.checked_sub(1).and_then(|j| tts.get(j)), '!')
                    && punct(k.checked_sub(2).and_then(|j| tts.get(j)), '#')))
    };
    // `i` から始まる外側の属性の、直前のトークン（`#` の前）が属性を許す位置か。属性の並びの二つ目からは、
    // 一つ目が許されていれば許す（一つ目が許されなければ、そこで拒否している）。
    let allowed = |i: usize| {
        let Some(prev) = i.checked_sub(1) else {
            return cx != AttrCx::Other;
        };
        if attr_body(prev) {
            return true;
        }
        match cx {
            AttrCx::Elements => punct(tts.get(prev), ',') || punct(tts.get(prev), ';'),
            AttrCx::Brace => {
                punct(tts.get(prev), ';')
                    || punct(tts.get(prev), ',')
                    || matches!(tts.get(prev), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace)
            }
            AttrCx::Other => false,
        }
    };
    tts.iter().enumerate().any(|(i, t)| match t {
        TokenTree::Group(g) => {
            if attr_body(i) {
                return false;
            }
            let inner_cx = match g.delimiter() {
                Delimiter::Brace => AttrCx::Brace,
                // 入れ子の `vec![..]` / `vec!(..)`。
                Delimiter::Bracket | Delimiter::Parenthesis
                    if punct(i.checked_sub(1).and_then(|j| tts.get(j)), '!')
                        && matches!(i.checked_sub(2).and_then(|j| tts.get(j)), Some(TokenTree::Ident(n)) if n == "vec") =>
                {
                    AttrCx::Elements
                }
                _ => AttrCx::Other,
            };
            misplaced(&g.stream(), inner_cx)
        }
        TokenTree::Punct(p) if p.as_char() == '#' => {
            let inner = punct(tts.get(i + 1), '!');
            let body = if inner { i + 2 } else { i + 1 };
            bracket(tts.get(body)) && (inner || !allowed(i))
        }
        _ => false,
    })
}

/// `seq![a, b]` / `set![a, b]` / `map![k => v]` の引数を Ebykn の式として読む。
fn spec_macro_args(name: &str, m: &syn::Macro) -> ast::MacroArgs {
    if name == "map" {
        let parsed = (|input: syn::parse::ParseStream| {
            let mut out = Vec::new();
            while !input.is_empty() {
                let k: syn::Expr = input.parse()?;
                input.parse::<syn::Token![=>]>()?;
                let v: syn::Expr = input.parse()?;
                out.push((k, v));
                if input.is_empty() {
                    break;
                }
                input.parse::<syn::Token![,]>()?;
            }
            Ok(out)
        })
        .parse2(m.tokens.clone());
        match parsed {
            Ok(p) => ast::MacroArgs::Map(p.into_iter().map(|(k, v)| (expr(k), expr(v))).collect()),
            Err(e) => {
                super::push_error(e);
                ast::MacroArgs::Tokens
            }
        }
    } else {
        let parsed = (|input: syn::parse::ParseStream| {
            input.parse_terminated(<syn::Expr as syn::parse::Parse>::parse, syn::Token![,])
        })
        .parse2(m.tokens.clone());
        match parsed {
            Ok(p) => ast::MacroArgs::Exprs(p.into_iter().map(expr).collect()),
            Err(e) => {
                super::push_error(e);
                ast::MacroArgs::Tokens
            }
        }
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

/// 列挙子のフィールド。名前の raw の綴りは E0104（[`raw_checked_name`]）。
pub(super) fn variant_fields(f: &syn::Fields) -> ast::Fields {
    let fs = fields(f);
    if let ast::Fields::Named(named) = &fs {
        for f in named {
            if let Some(i) = &f.ident
                && i.raw
            {
                push(
                    i.span,
                    Message::raw_item_name(i.name.clone(), RawNameKind::VariantField),
                );
            }
        }
    }
    fs
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
