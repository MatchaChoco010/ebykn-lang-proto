//! 項目（仕様 §5.1）。項目の文法は Ebykn が直接読み、葉（型、パターン、式、ブロック）は
//! フォークに読ませる。誤りは項目ごとに回復して、まとめて報告する。

use proc_macro2::TokenTree;
use verus_syn as syn;
use verus_syn::Token;
use verus_syn::parse::discouraged::Speculative as _;
use verus_syn::parse::{Parse as _, ParseStream};

use super::contract::{self, peek_eq, tokens_between};
use super::convert::{self, attrs, block, expr, fields, generics, ident, ty, vis};
use super::{
    err, is_ident, is_punct, parse_kw, peek_kw, peek_tts, push, push_error, sp, span_between,
};
use crate::ast;
use crate::diag::{Expected, Message, TypeKind};

/// 項目の置かれた場所。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ctx {
    /// モジュール（ファイル、インラインの `mod`）。
    Module,
    /// 関数本体などのブロックの中。
    Block,
    /// `impl`。`inherent` は trait のない `impl T`。
    Impl { inherent: bool },
    /// `impl spec`。
    ImplSpec,
    /// trait の中。
    Trait,
}

pub(super) fn parse_file(input: ParseStream) -> syn::Result<ast::File> {
    let begin = input.fork();
    let inner = input.call(syn::Attribute::parse_inner)?;
    let items = parse_items(input, Ctx::Module);
    Ok(ast::File {
        attrs: attrs(&inner),
        items,
        span: span_between(&begin, input),
    })
}

/// 項目の並びを最後まで読む。読めない項目は診断にして、次の項目の先頭まで飛ばす。
fn parse_items(input: ParseStream, ctx: Ctx) -> Vec<ast::Item> {
    let mut items = Vec::new();
    while !input.is_empty() {
        let ahead = input.fork();
        match parse_item(&ahead, ctx) {
            Ok(i) => {
                input.advance_to(&ahead);
                items.push(i);
            }
            Err(e) => {
                push_error(e);
                recover(input);
            }
        }
    }
    items
}

/// 誤りの後、`;` か `{ .. }` の後で、次が項目の先頭になるところまで飛ばす。
fn recover(input: ParseStream) {
    let _ = input.step(|cursor| {
        let mut c = *cursor;
        while let Some((tt, next)) = c.token_tree() {
            let end = matches!(&tt, TokenTree::Punct(p) if p.as_char() == ';')
                || matches!(&tt, TokenTree::Group(g) if g.delimiter() == proc_macro2::Delimiter::Brace);
            c = next;
            if end && (c.eof() || starts_item_at(c)) {
                return Ok(((), c));
            }
        }
        Ok(((), c))
    });
}

fn starts_item_at(c: syn::buffer::Cursor) -> bool {
    let mut t = Vec::new();
    let mut c = c;
    while t.len() < 3 {
        let Some((tt, next)) = c.token_tree() else {
            break;
        };
        t.push(tt);
        c = next;
    }
    is_punct(t.first(), '#') || item_start(&t)
}

/// 項目の先頭か（属性の後）。文の位置で項目を見分けるのにも使う。
pub(super) fn peek_item_start(input: ParseStream) -> bool {
    item_start(&peek_tts(input, 3))
}

fn item_start(t: &[TokenTree]) -> bool {
    let a = t.first();
    let b = t.get(1);
    let any = |names: &[&str], tt: Option<&TokenTree>| names.iter().any(|n| is_ident(tt, n));
    any(
        &[
            "pub", "fn", "struct", "enum", "trait", "impl", "mod", "type", "use", "extern",
        ],
        a,
    ) || verus_item_start(t)
        || is_ident(a, "static") && matches!(b, Some(TokenTree::Ident(i)) if i != "move")
        || is_ident(a, "union") && matches!(b, Some(TokenTree::Ident(_)))
        || is_ident(a, "macro_rules") && is_punct(b, '!')
        || is_ident(a, "const") && matches!(b, Some(TokenTree::Ident(_)))
        || is_ident(a, "unsafe") && any(&["fn", "impl", "trait", "extern", "mod"], b)
        || is_ident(a, "async") && any(&["fn", "unsafe"], b)
        || is_ident(a, "auto") && is_ident(b, "trait")
        || is_ident(a, "spec") && any(&["fn", "const"], b)
        || is_ident(a, "closed") && is_ident(b, "spec")
        || is_ident(a, "open") && is_ident(b, "spec")
        || is_ident(a, "lemma") && any(&["fn", "group"], b)
        || is_ident(a, "unchecked") && any(&["fn", "lemma", "const", "unsafe", "async"], b)
        || any(&["proof", "exec", "axiom"], a) && is_ident(b, "fn")
        || any(&["ghost", "tracked"], a) && any(&["struct", "enum"], b)
}

/// Verus の項目の先頭か。後ろに Verus の形が続くときだけそう見る（`broadcast(tx, 1);` は
/// Rust の呼び出し）。
fn verus_item_start(t: &[TokenTree]) -> bool {
    let (a, b) = (t.first(), t.get(1));
    let any = |names: &[&str], tt: Option<&TokenTree>| names.iter().any(|n| is_ident(tt, n));
    is_ident(a, "broadcast") && any(&["use", "group", "proof", "fn", "axiom"], b)
        || is_ident(a, "uninterp") && is_ident(b, "spec")
        || is_ident(a, "assume_specification")
            && (is_punct(b, '<') || super::is_group(b, proc_macro2::Delimiter::Bracket))
}

/// 一つの項目を読む。
pub(super) fn parse_item(input: ParseStream, ctx: Ctx) -> syn::Result<ast::Item> {
    let begin = input.fork();
    let ahead = input.fork();
    let a = ahead.call(syn::Attribute::parse_outer)?;
    let v: syn::Visibility = ahead.parse()?;
    if is_rust_passthrough(&ahead) {
        let it: syn::Item = input.parse()?;
        check_rust_item(&it);
        let ts = tokens_between(&begin, input);
        let span = span_between(&begin, input);
        return Ok(ast::Item {
            attrs: attrs(&a),
            vis: vis(&v),
            kind: ast::ItemKind::Rust(ast::Tokens { tokens: ts, span }),
            span,
        });
    }
    input.advance_to(&ahead);
    let kind = item_kind(input, ctx, &begin)?;
    if ctx == (Ctx::Impl { inherent: true })
        && let Some(name) = item_name(&kind)
        && (name.is("invariant") || name.is("view"))
    {
        push(
            name.span,
            Message::DirectInvariantOrView {
                name: name.name.clone(),
            },
        );
    }
    Ok(ast::Item {
        attrs: attrs(&a),
        vis: vis(&v),
        kind,
        span: span_between(&begin, input),
    })
}

fn item_name(k: &ast::ItemKind) -> Option<&ast::Ident> {
    Some(match k {
        ast::ItemKind::Fn(f) => &f.sig.ident,
        ast::ItemKind::SpecFn(f) => &f.sig.ident,
        ast::ItemKind::LemmaFn(f) => &f.sig.ident,
        ast::ItemKind::Const(c) => &c.ident,
        ast::ItemKind::TypeAlias(t) => &t.ident,
        ast::ItemKind::Subset(t) => &t.ident,
        _ => return None,
    })
}

/// フォークに読ませた Rust の項目の中を検査する。マクロは名前と引数（`macro_rules!` の
/// 定義も呼べないマクロとして拒否する）、`extern` ブロックは中の宣言を項目として読み、
/// `union` はフィールドを読む。それ以外は中を読まないので字句的に探す。
fn check_rust_item(it: &syn::Item) {
    match it {
        syn::Item::Macro(m) => {
            convert::mac(&m.mac);
        }
        syn::Item::ForeignMod(f) => {
            for fi in &f.items {
                match fi {
                    syn::ForeignItem::Fn(f) => {
                        for a in &f.sig.inputs {
                            convert::param(a);
                        }
                        if let syn::ReturnType::Type(_, _, _, t) = &f.sig.output {
                            ty(t);
                        }
                    }
                    syn::ForeignItem::Static(s) => {
                        ty(&s.ty);
                    }
                    syn::ForeignItem::Macro(m) => {
                        convert::mac(&m.mac);
                    }
                    other => super::tokens::scan_opaque(&quote::ToTokens::to_token_stream(other)),
                }
            }
        }
        syn::Item::ExternCrate(e) => {
            if let Some((_, r)) = &e.rename
                && RESERVED_NAMES.contains(&r.to_string().as_str())
                && *r != e.ident
            {
                push(
                    sp(r.span()),
                    Message::ReservedRename {
                        written: format!("extern crate {} as {r}", e.ident),
                    },
                );
            }
        }
        syn::Item::Union(u) => {
            fields(&syn::Fields::Named(u.fields.clone()));
        }
        other => super::tokens::scan_opaque(&quote::ToTokens::to_token_stream(other)),
    }
}

/// 改名して付けられない名前（#16 の既定）。
const RESERVED_NAMES: &[&str] = &["std", "core", "alloc", "vstd", "ebykn_prelude"];

/// `use` の木を検査する（#16 の既定）。Verus のマクロの名前を取り込む・改名するものと、
/// `.eyk` で呼べるマクロの名前を外のクレートから取り込む・その名前に改名するもの
/// （`use x::verus as vec;`）は書けない。
fn check_use(tree: &syn::UseTree, prefix: &mut Vec<String>) {
    let unraw = |i: &proc_macro2::Ident| {
        let n = i.to_string();
        n.strip_prefix("r#").unwrap_or(&n).to_string()
    };
    let leaf = |prefix: &[String], orig: &proc_macro2::Ident, name: &proc_macro2::Ident| {
        let (o, n) = (unraw(orig), unraw(name));
        let first = prefix.first().map(String::as_str).unwrap_or(o.as_str());
        let own = matches!(first, "crate" | "self" | "super");
        let std = matches!(first, "std" | "core" | "alloc") && o == n;
        let vstd = first == "vstd" && o == n && crate::VSTD_MACROS.contains(&n.as_str());
        let written = if o == n {
            format!(
                "use {}",
                [prefix, std::slice::from_ref(&o)].concat().join("::")
            )
        } else {
            format!(
                "use {} as {n}",
                [prefix, std::slice::from_ref(&o)].concat().join("::")
            )
        };
        // 表（`crate::reserved`）の関数とトレイトの名前を取り込む `use` は、クレートの中の定義を
        // 指すときだけ書ける（裸の `use requires;` は拒否、それ以外は検査が名前解決して決める）。
        if let Some(kind) = crate::reserved::kind_of(&o)
            && kind != crate::reserved::ReservedKind::VerusMacro
        {
            let segs = [prefix, std::slice::from_ref(&o)].concat();
            super::reserved_path(&segs, false, |_| true, written.clone(), sp(orig.span()));
        }
        if RESERVED_NAMES.contains(&n.as_str()) && !(prefix.is_empty() && o == n) {
            push(sp(name.span()), Message::ReservedRename { written });
        } else if super::tokens::VERUS_MACROS.contains(&o.as_str())
            || super::tokens::VERUS_MACROS.contains(&n.as_str())
        {
            push(sp(orig.span()), Message::VerusOnly { written });
        } else if crate::ALLOWED_MACROS.contains(&n.as_str()) && !(own || std || vstd) {
            push(sp(orig.span()), Message::Unread { written });
        }
    };
    // 木の先頭の名前（`use {vstd as v}` のグループの中を含む）が参照しないクレートなら拒否する。
    // トークンの検査は `vstd` が `::` の隣か `use` の直後にあるときしか見ないので、ここで補う。
    let root = match tree {
        syn::UseTree::Path(p) => Some(&p.ident),
        syn::UseTree::Name(n) => Some(&n.ident),
        syn::UseTree::Rename(r) => Some(&r.ident),
        _ => None,
    };
    if prefix.is_empty()
        && let Some(i) = root
        && super::tokens::FORBIDDEN_CRATES.contains(&unraw(i).as_str())
    {
        push(sp(i.span()), Message::VstdDirect { name: unraw(i) });
    }
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(unraw(&p.ident));
            check_use(&p.tree, prefix);
            prefix.pop();
        }
        syn::UseTree::Name(n) => leaf(prefix, &n.ident, &n.ident),
        syn::UseTree::Rename(r) => leaf(prefix, &r.ident, &r.rename),
        syn::UseTree::Glob(_) => {}
        syn::UseTree::Group(g) => {
            for t in &g.items {
                check_use(t, prefix);
            }
        }
    }
}

/// フォークにそのまま読ませる Rust の項目。
fn is_rust_passthrough(input: ParseStream) -> bool {
    let t = peek_tts(input, 3);
    let a = t.first();
    let b = t.get(1);
    is_ident(a, "extern") && (is_ident(b, "crate") || !is_ident(t.get(2), "fn"))
        || is_ident(a, "unsafe") && is_ident(b, "extern")
        || is_ident(a, "macro_rules") && is_punct(b, '!')
        || is_ident(a, "union") && matches!(b, Some(TokenTree::Ident(_)))
        || matches!(a, Some(TokenTree::Ident(i)) if !is_keywordish(i)) && is_punct(b, '!')
}

fn is_keywordish(i: &proc_macro2::Ident) -> bool {
    matches!(
        i.to_string().as_str(),
        "fn" | "struct" | "enum" | "impl" | "trait"
    )
}

/// Verus の項目で、Ebykn にないもの。
fn verus_item(input: ParseStream) -> Option<Message> {
    let t = peek_tts(input, 3);
    let (a, b) = (t.first(), t.get(1));
    let verus = |w: &str, e: &str| {
        Some(Message::VerusSyntax {
            written: w.into(),
            ebykn: e.into(),
        })
    };
    let only = |w: &str| Some(Message::VerusOnly { written: w.into() });
    if is_ident(a, "open") && is_ident(b, "spec") {
        verus("open spec fn", "spec fn")
    } else if is_ident(a, "proof") && is_ident(b, "fn") {
        verus("proof fn", "lemma fn")
    } else if is_ident(a, "exec") && is_ident(b, "fn") {
        verus("exec fn", "fn")
    } else if is_ident(a, "axiom") && is_ident(b, "fn") {
        verus("axiom fn", "unchecked lemma fn")
    } else if is_ident(a, "broadcast") && verus_item_start(&t) {
        verus("broadcast", "lemma group / use lemmas")
    } else if is_ident(a, "assume_specification") && verus_item_start(&t) {
        verus("assume_specification", "unchecked fn path::f(..)")
    } else if (is_ident(a, "ghost") || is_ident(a, "tracked"))
        && (is_ident(b, "struct") || is_ident(b, "enum"))
    {
        only("ghost struct / tracked struct")
    } else if is_ident(a, "uninterp") && verus_item_start(&t) {
        only("uninterp spec fn")
    } else if is_ident(a, "spec") && is_ident(b, "const") {
        only("spec const")
    } else if is_ident(a, "spec") && is_group_paren(b) {
        only("spec(checked) fn")
    } else {
        None
    }
}

fn is_group_paren(tt: Option<&TokenTree>) -> bool {
    super::is_group(tt, proc_macro2::Delimiter::Parenthesis)
}

fn item_kind(
    input: ParseStream,
    ctx: Ctx,
    begin: &syn::parse::ParseBuffer,
) -> syn::Result<ast::ItemKind> {
    if let Some(m) = verus_item(input) {
        return Err(err(input.span(), m));
    }
    let t = peek_tts(input, 3);
    let (a, b) = (t.first(), t.get(1));
    if is_ident(a, "unchecked") {
        let kw = parse_kw(input, "unchecked")?;
        let unchecked = Some(sp(kw.span()));
        if peek_kw(input, "lemma") {
            return lemma_fn(input, unchecked);
        }
        let ahead = input.fork();
        let _ = qualifiers(&ahead)?;
        if !ahead.peek(Token![fn]) {
            return Err(err(
                input.span(),
                Message::Expected {
                    what: Expected::UncheckedTarget,
                },
            ));
        }
        ahead.parse::<Token![fn]>()?;
        if ahead.peek(syn::Ident) && ahead.peek2(Token![::]) {
            return Err(err(
                ahead.span(),
                Message::PropsOnly {
                    written: "unchecked fn path::f(..);".into(),
                },
            ));
        }
        return fn_item(input, ctx, unchecked);
    }
    if is_ident(a, "closed") {
        let kw = parse_kw(input, "closed")?;
        if !(peek_kw(input, "spec") && input.peek2(Token![fn])) {
            return Err(err(
                input.span(),
                Message::Expected {
                    what: Expected::SpecFn,
                },
            ));
        }
        if ctx == Ctx::Trait {
            push(
                sp(kw.span()),
                Message::NotAllowedHere {
                    written: "closed".into(),
                },
            );
        }
        return spec_fn(input, ctx, Some(sp(kw.span())));
    }
    if is_ident(a, "spec") && is_ident(b, "fn") {
        return spec_fn(input, ctx, None);
    }
    if is_ident(a, "lemma") && is_ident(b, "fn") {
        return lemma_fn(input, None);
    }
    if is_ident(a, "lemma") && is_ident(b, "group") {
        return Err(err(
            input.span(),
            Message::ProofSyntax {
                written: "lemma group".into(),
            },
        ));
    }
    if starts_fn(input) {
        return fn_item(input, ctx, None);
    }
    if is_ident(a, "struct") {
        return struct_item(input);
    }
    if is_ident(a, "enum") {
        return enum_item(input);
    }
    if is_ident(a, "trait")
        || is_ident(a, "unsafe") && is_ident(b, "trait")
        || is_ident(a, "auto") && is_ident(b, "trait")
    {
        return trait_item(input);
    }
    if is_ident(a, "impl") || is_ident(a, "unsafe") && is_ident(b, "impl") {
        return impl_item(input);
    }
    if is_ident(a, "type") {
        return type_item(input);
    }
    if is_ident(a, "mod") || is_ident(a, "unsafe") && is_ident(b, "mod") {
        return mod_item(input);
    }
    if is_ident(a, "use") {
        return use_item(input, begin);
    }
    if is_ident(a, "const") {
        return const_item(input);
    }
    if is_ident(a, "static") {
        return static_item(input);
    }
    Err(err(
        input.span(),
        Message::Expected {
            what: Expected::Item,
        },
    ))
}

/// `const`、`async`、`unsafe`、`extern "abi"` のあとに `fn` が来るか。
fn starts_fn(input: ParseStream) -> bool {
    let ahead = input.fork();
    qualifiers(&ahead).is_ok() && ahead.peek(Token![fn])
}

fn qualifiers(input: ParseStream) -> syn::Result<ast::FnQualifiers> {
    let mut q = ast::FnQualifiers::default();
    if let Some(t) = input.parse::<Option<Token![const]>>()? {
        q.constness = Some(sp(t.span));
    }
    if let Some(t) = input.parse::<Option<Token![async]>>()? {
        q.asyncness = Some(sp(t.span));
    }
    if let Some(t) = input.parse::<Option<Token![unsafe]>>()? {
        q.unsafety = Some(sp(t.span));
    }
    if input.peek(Token![extern]) {
        let abi: syn::Abi = input.parse()?;
        q.abi = Some(convert::tokens(&abi));
    }
    Ok(q)
}

/// `fn` から where 句まで。
fn signature(input: ParseStream, q: ast::FnQualifiers) -> syn::Result<ast::Signature> {
    input.parse::<Token![fn]>()?;
    let name: syn::Ident = input.parse()?;
    let g: syn::Generics = input.parse()?;
    let params = contract::params(input)?;
    let ret = contract::ret(input)?;
    let where_clause = contract::where_clause(input)?;
    Ok(ast::Signature {
        qualifiers: q,
        ident: ident(&name),
        generics: generics(&g),
        params,
        ret,
        where_clause,
    })
}

fn fn_item(
    input: ParseStream,
    ctx: Ctx,
    unchecked: Option<crate::span::Span>,
) -> syn::Result<ast::ItemKind> {
    let q = qualifiers(input)?;
    let sig = signature(input, q)?;
    let contract = contract::contract(input)?;
    let body = if input.peek(syn::token::Brace) {
        // 入れ子の `fn` は自分の `unchecked` で決め直す。
        let saved = super::set_unchecked_body(unchecked.is_some());
        let b = input.parse().map(block);
        super::set_unchecked_body(saved);
        Some(ast::FnBody::Block(b?))
    } else if peek_eq(input) {
        let eq: Token![=] = input.parse()?;
        let e = expr(input.parse()?);
        input.parse::<Token![;]>()?;
        if ctx == Ctx::Trait {
            push(
                sp(eq.span),
                Message::Expected {
                    what: Expected::TraitFnBody,
                },
            );
        }
        Some(ast::FnBody::Pure(e))
    } else if input.peek(Token![;]) {
        let semi: Token![;] = input.parse()?;
        if ctx != Ctx::Trait {
            push(
                sp(semi.span),
                Message::Expected {
                    what: Expected::FnBody,
                },
            );
        }
        None
    } else {
        return Err(err(
            input.span(),
            Message::Expected {
                what: Expected::ClauseOrBody,
            },
        ));
    };
    Ok(ast::ItemKind::Fn(ast::Fn {
        unchecked,
        sig,
        contract,
        body,
    }))
}

fn spec_fn(
    input: ParseStream,
    ctx: Ctx,
    closed: Option<crate::span::Span>,
) -> syn::Result<ast::ItemKind> {
    parse_kw(input, "spec")?;
    let sig = signature(input, ast::FnQualifiers::default())?;
    match &sig.ret {
        None => {
            return Err(err(
                input.span(),
                Message::Expected {
                    what: Expected::SpecFnRet,
                },
            ));
        }
        Some(r) if r.name.is_some() => push(
            r.span,
            Message::NotAllowedHere {
                written: "spec fn .. -> (r: T)".into(),
            },
        ),
        Some(_) => {}
    }
    // trait の spec fn は契約を持たない（仕様 §5.1 の TraitItem）。
    let contract = if ctx == Ctx::Trait {
        ast::Contract::default()
    } else {
        contract::contract(input)?
    };
    let body = if peek_eq(input) {
        input.parse::<Token![=]>()?;
        let e = expr(input.parse()?);
        input.parse::<Token![;]>()?;
        Some(e)
    } else if input.peek(Token![;]) {
        let semi: Token![;] = input.parse()?;
        if ctx != Ctx::Trait {
            push(
                sp(semi.span),
                Message::Expected {
                    what: Expected::SpecFnBody,
                },
            );
        }
        None
    } else {
        return Err(err(
            input.span(),
            Message::Expected {
                what: if ctx == Ctx::Trait {
                    Expected::SpecFnBody
                } else {
                    Expected::ClauseOrBody
                },
            },
        ));
    };
    Ok(ast::ItemKind::SpecFn(ast::SpecFn {
        closed,
        sig,
        contract,
        body,
    }))
}

fn lemma_fn(
    input: ParseStream,
    unchecked: Option<crate::span::Span>,
) -> syn::Result<ast::ItemKind> {
    parse_kw(input, "lemma")?;
    let sig = signature(input, ast::FnQualifiers::default())?;
    let contract = contract::contract(input)?;
    if input.peek(syn::token::Brace) {
        // 本体は `.eykp` にだけ書ける（仕様 §5.1、§5.7）。読んで診断にする。
        let b: syn::Block = input.parse()?;
        push(
            sp(b.brace_token.span.join()),
            Message::ProofSyntax {
                written: "lemma fn .. { .. }".into(),
            },
        );
    } else if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
    } else {
        return Err(err(
            input.span(),
            Message::Expected {
                what: if sig.ret.is_some() || contract.is_empty() {
                    Expected::LemmaFnEnd
                } else {
                    Expected::ClauseOrBody
                },
            },
        ));
    }
    if let Some(r) = &sig.ret {
        push(
            r.span,
            Message::NotAllowedHere {
                written: "lemma fn .. -> T".into(),
            },
        );
    }
    Ok(ast::ItemKind::LemmaFn(ast::LemmaFn {
        unchecked,
        sig,
        contract,
    }))
}

/// `owns { .. }` / `spec { .. }` を読む。
fn erased_sections(
    input: ParseStream,
) -> syn::Result<(Option<ast::ErasedFields>, Option<ast::ErasedFields>)> {
    let mut owns = None;
    let mut spec = None;
    for (name, slot) in [("owns", &mut owns), ("spec", &mut spec)] {
        if peek_kw(input, name) && input.peek2(syn::token::Brace) {
            let begin = input.fork();
            let kw = parse_kw(input, name)?;
            let f: syn::FieldsNamed = input.parse()?;
            *slot = Some(ast::ErasedFields {
                fields: f.named.iter().map(convert::field).collect(),
                keyword: sp(kw.span()),
                span: span_between(&begin, input),
            });
        }
    }
    Ok((owns, spec))
}

fn e0401(
    name: &ast::Ident,
    kind: TypeKind,
    sections: &(Option<ast::ErasedFields>, Option<ast::ErasedFields>),
) {
    for (section, s) in [("owns", &sections.0), ("spec", &sections.1)] {
        if let Some(s) = s {
            push(
                s.keyword,
                Message::ErasedOnNonNamed {
                    section,
                    name: name.name.clone(),
                    kind,
                },
            );
        }
    }
}

fn struct_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    input.parse::<Token![struct]>()?;
    let name = ident(&input.parse::<syn::Ident>()?);
    let g: syn::Generics = input.parse()?;
    let mut where_clause = contract::where_clause(input)?;
    let (fields, sections) = if input.peek(syn::token::Brace) {
        let f: syn::FieldsNamed = input.parse()?;
        let sections = erased_sections(input)?;
        (fields(&syn::Fields::Named(f)), sections)
    } else if input.peek(syn::token::Paren) {
        let f: syn::FieldsUnnamed = input.parse()?;
        let mut sections = erased_sections(input)?;
        if sections.0.is_none() && sections.1.is_none() {
            where_clause = where_clause.or(contract::where_clause(input)?);
            input.parse::<Token![;]>()?;
            sections = erased_sections(input)?;
        } else {
            // `struct S(A) owns { .. }` の後の `;` はあってもなくてもよい（どちらも E0401）。
            input.parse::<Option<Token![;]>>()?;
        }
        e0401(&name, TypeKind::TupleStruct, &sections);
        (fields(&syn::Fields::Unnamed(f)), sections)
    } else {
        input.parse::<Token![;]>()?;
        let sections = erased_sections(input)?;
        e0401(&name, TypeKind::UnitStruct, &sections);
        (ast::Fields::Unit, sections)
    };
    Ok(ast::ItemKind::Struct(ast::Struct {
        ident: name,
        generics: generics(&g),
        where_clause,
        fields,
        owns: sections.0,
        spec: sections.1,
    }))
}

fn enum_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    input.parse::<Token![enum]>()?;
    let name = ident(&input.parse::<syn::Ident>()?);
    let g: syn::Generics = input.parse()?;
    let where_clause = contract::where_clause(input)?;
    let content;
    syn::braced!(content in input);
    let variants = content
        .parse_terminated(syn::Variant::parse, Token![,])?
        .into_iter()
        .map(|v| {
            let span = super::span_of(&v);
            ast::Variant {
                attrs: attrs(&v.attrs),
                ident: ident(&v.ident),
                fields: fields(&v.fields),
                discriminant: v.discriminant.map(|(_, e)| expr(e)),
                span,
            }
        })
        .collect();
    let sections = erased_sections(input)?;
    e0401(&name, TypeKind::Enum, &sections);
    Ok(ast::ItemKind::Enum(ast::Enum {
        ident: name,
        generics: generics(&g),
        where_clause,
        variants,
    }))
}

fn trait_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    let unsafety = input.parse::<Option<Token![unsafe]>>()?.map(|t| sp(t.span));
    let auto = input.parse::<Option<Token![auto]>>()?.map(|t| sp(t.span));
    input.parse::<Token![trait]>()?;
    let name = ident(&input.parse::<syn::Ident>()?);
    let g: syn::Generics = input.parse()?;
    let supertraits = if input.peek(Token![:]) {
        input.parse::<Token![:]>()?;
        let b = input.fork();
        syn::punctuated::Punctuated::<syn::TypeParamBound, Token![+]>::parse_separated_nonempty(
            input,
        )?;
        Some(ast::Tokens {
            tokens: tokens_between(&b, input),
            span: span_between(&b, input),
        })
    } else {
        None
    };
    let where_clause = contract::where_clause(input)?;
    let content;
    syn::braced!(content in input);
    let mut items = Vec::new();
    while !content.is_empty() {
        let ahead = content.fork();
        match trait_member(&ahead) {
            Ok(i) => {
                content.advance_to(&ahead);
                items.push(i);
            }
            Err(e) => {
                push_error(e);
                recover(&content);
            }
        }
    }
    Ok(ast::ItemKind::Trait(ast::Trait {
        unsafety,
        auto,
        ident: name,
        generics: generics(&g),
        supertraits,
        where_clause,
        items,
    }))
}

fn trait_member(input: ParseStream) -> syn::Result<ast::TraitItem> {
    let begin = input.fork();
    let ahead = input.fork();
    let a = ahead.call(syn::Attribute::parse_outer)?;
    let t = peek_tts(&ahead, 2);
    let rust = is_ident(t.first(), "type")
        || is_ident(t.first(), "const") && !is_ident(t.get(1), "fn")
        || matches!(t.first(), Some(TokenTree::Ident(_))) && is_punct(t.get(1), '!');
    if rust {
        let ti: syn::TraitItem = input.parse()?;
        match &ti {
            syn::TraitItem::Macro(m) => {
                convert::mac(&m.mac);
            }
            other => super::tokens::scan_opaque(&quote::ToTokens::to_token_stream(other)),
        }
        let span = span_between(&begin, input);
        return Ok(ast::TraitItem {
            attrs: attrs(&a),
            kind: ast::TraitItemKind::Rust(ast::Tokens {
                tokens: tokens_between(&begin, input),
                span,
            }),
            span,
        });
    }
    input.advance_to(&ahead);
    if let Some(m) = verus_item(input) {
        return Err(err(input.span(), m));
    }
    let t = peek_tts(input, 2);
    let kind = if is_ident(t.first(), "closed") || is_ident(t.first(), "spec") {
        match item_kind(input, Ctx::Trait, &begin)? {
            ast::ItemKind::SpecFn(f) => ast::TraitItemKind::SpecFn(f),
            _ => unreachable!("spec fn"),
        }
    } else if is_ident(t.first(), "lemma") && is_ident(t.get(1), "fn") {
        match lemma_fn(input, None)? {
            ast::ItemKind::LemmaFn(f) => ast::TraitItemKind::LemmaFn(f),
            _ => unreachable!("lemma fn"),
        }
    } else if starts_fn(input) {
        match fn_item(input, Ctx::Trait, None)? {
            ast::ItemKind::Fn(f) => ast::TraitItemKind::Fn(f),
            _ => unreachable!("fn"),
        }
    } else {
        return Err(err(
            input.span(),
            Message::Expected {
                what: Expected::Item,
            },
        ));
    };
    Ok(ast::TraitItem {
        attrs: attrs(&a),
        kind,
        span: span_between(&begin, input),
    })
}

/// `{ 項目* }`（内部属性を先に読む）。
fn item_body(input: ParseStream, ctx: Ctx) -> syn::Result<(Vec<ast::Attr>, Vec<ast::Item>)> {
    let content;
    syn::braced!(content in input);
    let inner = content.call(syn::Attribute::parse_inner)?;
    Ok((attrs(&inner), parse_items(&content, ctx)))
}

fn impl_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    let unsafety = input.parse::<Option<Token![unsafe]>>()?.map(|t| sp(t.span));
    input.parse::<Token![impl]>()?;
    let g: syn::Generics = if input.peek(Token![<]) && !input.peek(Token![<=]) {
        input.parse()?
    } else {
        syn::Generics::default()
    };
    let t = peek_tts(input, 2);
    if is_ident(t.first(), "spec") && matches!(t.get(1), Some(TokenTree::Ident(i)) if i != "for") {
        let kw = parse_kw(input, "spec")?;
        let p: syn::Path = input.parse()?;
        let self_ty = if input.peek(Token![for]) {
            input.parse::<Token![for]>()?;
            Some(ty(&input.parse()?))
        } else {
            None
        };
        let where_clause = contract::where_clause(input)?;
        let (_, items) = item_body(input, Ctx::ImplSpec)?;
        for i in &items {
            if !matches!(i.kind, ast::ItemKind::SpecFn(_) | ast::ItemKind::LemmaFn(_)) {
                push(
                    i.span,
                    Message::Expected {
                        what: Expected::SpecOrLemmaFn,
                    },
                );
            }
        }
        return Ok(ast::ItemKind::ImplSpec(ast::ImplSpec {
            generics: generics(&g),
            spec: sp(kw.span()),
            trait_: convert::path(&p),
            self_ty,
            where_clause,
            items,
        }));
    }
    let negative = input.parse::<Option<Token![!]>>()?.is_some();
    let ahead = input.fork();
    let trait_ = match ahead.parse::<syn::Path>() {
        Ok(p) if ahead.peek(Token![for]) => {
            input.advance_to(&ahead);
            input.parse::<Token![for]>()?;
            Some(convert::path(&p))
        }
        _ => None,
    };
    let self_ty = ty(&input.parse()?);
    let where_clause = contract::where_clause(input)?;
    let inherent = trait_.is_none();
    let (_, items) = item_body(input, Ctx::Impl { inherent })?;
    Ok(ast::ItemKind::Impl(ast::Impl {
        unsafety,
        generics: generics(&g),
        negative,
        trait_,
        self_ty,
        where_clause,
        items,
    }))
}

fn type_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    input.parse::<Token![type]>()?;
    let name = ident(&input.parse::<syn::Ident>()?);
    let g: syn::Generics = input.parse()?;
    let mut where_clause = contract::where_clause(input)?;
    input.parse::<Token![=]>()?;
    let base: syn::Type = input.parse()?;
    if input.peek(Token![where]) {
        // 仕様 §5.1：`where` の後が `Type :` の形なら Rust の where 句、そうでなければ部分集合型。
        let rust_where = super::lookahead(input, |f| {
            f.parse::<Token![where]>()?;
            let _: Option<syn::BoundLifetimes> = f.parse()?;
            let _: syn::Type = f.parse()?;
            if super::hook::is_single_colon(&peek_tts(f, 2)) {
                Ok(())
            } else {
                Err(f.error("not a Rust where predicate"))
            }
        });
        if !rust_where {
            let w: Token![where] = input.parse()?;
            let pred = expr(input.parse()?);
            input.parse::<Token![;]>()?;
            return Ok(ast::ItemKind::Subset(ast::SubsetType {
                ident: name,
                generics: generics(&g),
                base: ty(&base),
                where_token: sp(w.span),
                pred,
            }));
        }
        where_clause = contract::where_clause(input)?;
    }
    input.parse::<Token![;]>()?;
    Ok(ast::ItemKind::TypeAlias(ast::TypeAlias {
        ident: name,
        generics: generics(&g),
        where_clause,
        ty: ty(&base),
    }))
}

fn mod_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    let unsafety = input.parse::<Option<Token![unsafe]>>()?.map(|t| sp(t.span));
    input.parse::<Token![mod]>()?;
    let name = ident(&input.parse::<syn::Ident>()?);
    let content = if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        None
    } else {
        Some(item_body(input, Ctx::Module)?)
    };
    Ok(ast::ItemKind::Mod(ast::Mod {
        unsafety,
        ident: name,
        content,
    }))
}

fn use_item(input: ParseStream, begin: &syn::parse::ParseBuffer) -> syn::Result<ast::ItemKind> {
    input.parse::<Token![use]>()?;
    // `use lemmas g;`（`.eykp` だけ）と `use lemmas::x;`（Rust）を分ける。
    let t = peek_tts(input, 2);
    if is_ident(t.first(), "lemmas") && matches!(t.get(1), Some(TokenTree::Ident(_))) {
        return Err(err(
            input.span(),
            Message::ProofSyntax {
                written: "use lemmas".into(),
            },
        ));
    }
    let tree: syn::UseTree = input.parse()?;
    check_use(&tree, &mut Vec::new());
    input.parse::<Token![;]>()?;
    Ok(ast::ItemKind::Use(ast::Tokens {
        tokens: tokens_between(begin, input),
        span: span_between(begin, input),
    }))
}

fn const_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    input.parse::<Token![const]>()?;
    let name: proc_macro2::Ident = if input.peek(Token![_]) {
        let u: Token![_] = input.parse()?;
        proc_macro2::Ident::new("_", u.span)
    } else {
        input.parse::<syn::Ident>()?
    };
    let g: syn::Generics = input.parse()?;
    input.parse::<Token![:]>()?;
    let t: syn::Type = input.parse()?;
    let e = if peek_eq(input) {
        input.parse::<Token![=]>()?;
        Some(expr(input.parse()?))
    } else {
        None
    };
    input.parse::<Token![;]>()?;
    Ok(ast::ItemKind::Const(ast::Const {
        ident: ident(&name),
        generics: generics(&g),
        ty: ty(&t),
        expr: e,
    }))
}

fn static_item(input: ParseStream) -> syn::Result<ast::ItemKind> {
    input.parse::<Token![static]>()?;
    let mutable = input.parse::<Option<Token![mut]>>()?.is_some();
    let name: syn::Ident = input.parse()?;
    input.parse::<Token![:]>()?;
    let t: syn::Type = input.parse()?;
    input.parse::<Token![=]>()?;
    let e = expr(input.parse()?);
    input.parse::<Token![;]>()?;
    Ok(ast::ItemKind::Static(ast::Static {
        mutable,
        ident: ident(&name),
        ty: ty(&t),
        expr: e,
    }))
}
