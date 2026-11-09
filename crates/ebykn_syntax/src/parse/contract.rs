//! シグネチャの部品と契約の句（仕様 §5.1、§5.2）。

use proc_macro2::{TokenStream, TokenTree};
use verus_syn as syn;
use verus_syn::Token;
use verus_syn::parse::discouraged::Speculative as _;
use verus_syn::parse::{ParseBuffer, ParseStream, Parser as _};

use super::convert::{self, expr, ident, ty};
use super::hook::is_single_colon;
use super::{err, peek_kw, peek_tts, push, sp, span_between};
use crate::ast;
use crate::diag::{Expected, Message};

/// 契約の句のキーワード（仕様 §4）。
const CLAUSES: &[&str] = &[
    "uses",
    "gives",
    "requires",
    "ensures",
    "returns",
    "decreases",
    "no_panic",
    "by",
];

/// Verus の句で、Ebykn にないもの。`Some(書き方)` は Ebykn での書き方。
const VERUS_CLAUSES: &[(&str, Option<&str>)] = &[
    ("no_unwind", Some("no_panic")),
    ("recommends", None),
    ("opens_invariants", None),
    ("default_ensures", None),
    ("via", None),
    ("when", None),
    ("unwind", None),
    ("invariant", None),
];

pub(super) fn peek_clause(input: ParseStream) -> bool {
    CLAUSES.iter().any(|k| peek_kw(input, k))
}

fn peek_verus_clause(input: ParseStream) -> Option<Message> {
    VERUS_CLAUSES
        .iter()
        .find(|(k, _)| peek_kw(input, k))
        .map(|(k, e)| match e {
            Some(e) => Message::VerusSyntax {
                written: (*k).into(),
                ebykn: (*e).into(),
            },
            None => Message::VerusOnly {
                written: (*k).into(),
            },
        })
}

/// 本体の始まりの `=`（`==` や `=>` ではない）。
pub(super) fn peek_eq(input: ParseStream) -> bool {
    input.peek(Token![=]) && !input.peek(Token![==]) && !input.peek(Token![=>])
}

/// 契約の句の式の並びの終わり。
fn at_list_end(input: ParseStream) -> bool {
    input.is_empty()
        || input.peek(syn::token::Brace)
        || input.peek(Token![;])
        || peek_eq(input)
        || peek_clause(input)
        || peek_verus_clause(input).is_some()
}

/// `begin` から `end` までのトークン列。
pub(super) fn tokens_between(begin: &ParseBuffer, end: ParseStream) -> TokenStream {
    let end = end.cursor();
    let mut cursor = begin.cursor();
    let mut ts = TokenStream::new();
    while cursor != end {
        let Some((tt, next)) = cursor.token_tree() else {
            break;
        };
        ts.extend([tt]);
        cursor = next;
    }
    ts
}

/// Rust の where 句。契約の句、本体、`;` の前で終わる。
pub(super) fn where_clause(input: ParseStream) -> syn::Result<Option<ast::WhereClause>> {
    if !input.peek(Token![where]) {
        return Ok(None);
    }
    let begin = input.fork();
    input.parse::<Token![where]>()?;
    let mut predicates = Vec::new();
    while !at_list_end(input) {
        let pb = input.fork();
        input.parse::<syn::WherePredicate>()?;
        let ts = tokens_between(&pb, input);
        predicates.push(ast::Tokens {
            span: span_between(&pb, input),
            tokens: ts,
        });
        if !input.peek(Token![,]) {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(Some(ast::WhereClause {
        predicates,
        span: span_between(&begin, input),
    }))
}

/// 引数の並び `( .. )`。
pub(super) fn params(input: ParseStream) -> syn::Result<Vec<ast::Param>> {
    let content;
    syn::parenthesized!(content in input);
    let args = content.parse_terminated(<syn::FnArg as syn::parse::Parse>::parse, Token![,])?;
    Ok(args.iter().map(convert::param).collect())
}

/// `-> T` / `-> (r: T)`。
pub(super) fn ret(input: ParseStream) -> syn::Result<Option<ast::Ret>> {
    if !input.peek(Token![->]) {
        return Ok(None);
    }
    let begin = input.fork();
    input.parse::<Token![->]>()?;
    let named = input.peek(syn::token::Paren)
        && match peek_tts(input, 1).first() {
            Some(TokenTree::Group(g)) => {
                let t: Vec<TokenTree> = g.stream().into_iter().take(3).collect();
                matches!(t.first(), Some(TokenTree::Ident(_))) && is_single_colon(&t[1..])
            }
            _ => false,
        };
    if named {
        let content;
        syn::parenthesized!(content in input);
        let name: syn::Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        let t: syn::Type = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("expected `)`"));
        }
        return Ok(Some(ast::Ret {
            name: Some(ident(&name)),
            ty: ty(&t),
            span: span_between(&begin, input),
        }));
    }
    let t: syn::Type = input.parse()?;
    Ok(Some(ast::Ret {
        name: None,
        ty: ty(&t),
        span: span_between(&begin, input),
    }))
}

/// 契約の句の式。最上位の `=` は代入ではなく本体の始まり（`decreases n = e;`、`OPEN.md`）。
fn clause_expr(input: ParseStream) -> syn::Result<ast::Expr> {
    let ahead = input.fork();
    let first = syn::Expr::parse_without_eager_brace(&ahead);
    let eq = match &first {
        // `n = e` を代入として読んだ。`=` の手前までが句の式。
        Ok(syn::Expr::Assign(a)) => Some(a.eq_token.span.byte_range()),
        Ok(_) => None,
        // `n = S { .. }` は構造体リテラルの位置の誤りになる。`=` があれば手前で切る。
        Err(_) => standalone_eq(input),
    };
    let Some(eq) = eq else {
        let e = first?;
        input.advance_to(&ahead);
        return Ok(expr(e));
    };
    let lhs = input.step(|cursor| {
        let mut c = *cursor;
        let mut ts = TokenStream::new();
        while let Some((tt, next)) = c.token_tree() {
            if matches!(&tt, TokenTree::Punct(p) if p.as_char() == '=' && p.span().byte_range() == eq)
            {
                return Ok((ts, c));
            }
            ts.extend([tt]);
            c = next;
        }
        Err(cursor.error("expected `=`"))
    })?;
    let e = syn::Expr::parse_without_eager_brace.parse2(lhs)?;
    Ok(expr(e))
}

/// `;` より前にある、最上位の単独の `=`（`==`、`<=`、`=>` などの一部ではないもの）。
fn standalone_eq(input: ParseStream) -> Option<std::ops::Range<usize>> {
    let mut c = input.cursor();
    let mut prev_joint = false;
    while let Some((tt, next)) = c.token_tree() {
        if let TokenTree::Punct(p) = &tt {
            if p.as_char() == ';' {
                return None;
            }
            if p.as_char() == '=' && p.spacing() == proc_macro2::Spacing::Alone && !prev_joint {
                return Some(p.span().byte_range());
            }
            prev_joint = p.spacing() == proc_macro2::Spacing::Joint;
        } else {
            prev_joint = false;
        }
        c = next;
    }
    None
}

fn expr_list(input: ParseStream) -> syn::Result<Vec<ast::Expr>> {
    let mut out = vec![clause_expr(input)?];
    while input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
        if at_list_end(input) {
            break;
        }
        out.push(clause_expr(input)?);
    }
    Ok(out)
}

/// 契約。`by(..)` は `.eykp` だけの証明構文なので、読んで診断にする。
pub(super) fn contract(input: ParseStream) -> syn::Result<ast::Contract> {
    let mut clauses = Vec::new();
    loop {
        if let Some(m) = peek_verus_clause(input) {
            return Err(err(input.span(), m));
        }
        if !peek_clause(input) {
            break;
        }
        let begin = input.fork();
        let kw: proc_macro2::Ident = input.parse()?;
        let keyword = sp(kw.span());
        let kind = match kw.to_string().as_str() {
            "uses" => ast::ClauseKind::Uses(uses_items(input)?),
            "gives" => ast::ClauseKind::Gives(gives_items(input)?),
            "requires" => ast::ClauseKind::Requires(expr_list(input)?),
            "ensures" => ast::ClauseKind::Ensures(expr_list(input)?),
            "returns" => ast::ClauseKind::Returns(clause_expr(input)?),
            "decreases" => ast::ClauseKind::Decreases(expr_list(input)?),
            "no_panic" => ast::ClauseKind::NoPanic,
            "by" => {
                let content;
                syn::parenthesized!(content in input);
                let _mode: syn::Ident = content.parse()?;
                push(
                    span_between(&begin, input),
                    Message::ProofSyntax {
                        written: "by(..)".into(),
                    },
                );
                continue;
            }
            _ => unreachable!("peek_clause"),
        };
        clauses.push(ast::Clause {
            kind,
            keyword,
            span: span_between(&begin, input),
        });
    }
    Ok(ast::Contract { clauses })
}

fn uses_items(input: ParseStream) -> syn::Result<Vec<ast::UsesItem>> {
    let mut out = Vec::new();
    loop {
        let begin = input.fork();
        let item = if input.peek(Token![&]) || input.peek(syn::Ident) && input.peek2(Token![@]) {
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
            let i: syn::Ident = input.parse()?;
            input.parse::<Token![@]>()?;
            ast::UsesItem::Perm {
                mode,
                ident: ident(&i),
                span: span_between(&begin, input),
            }
        } else if input.peek(syn::Ident) && is_single_colon(&peek_tts(input, 3)[1..]) {
            let i: syn::Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let t: syn::Type = input.parse()?;
            ast::UsesItem::Named {
                ident: ident(&i),
                ty: ty(&t),
                span: span_between(&begin, input),
            }
        } else {
            return Err(err(
                input.span(),
                Message::Expected {
                    what: Expected::UsesItem,
                },
            ));
        };
        out.push(item);
        if !input.peek(Token![,]) {
            break;
        }
        input.parse::<Token![,]>()?;
        if at_list_end(input) {
            break;
        }
    }
    Ok(out)
}

fn gives_items(input: ParseStream) -> syn::Result<Vec<ast::Ident>> {
    let mut out = Vec::new();
    loop {
        if !(input.peek(syn::Ident) && input.peek2(Token![@])) {
            return Err(err(
                input.span(),
                Message::Expected {
                    what: Expected::GivesItem,
                },
            ));
        }
        let i: syn::Ident = input.parse()?;
        input.parse::<Token![@]>()?;
        out.push(ident(&i));
        if !input.peek(Token![,]) {
            break;
        }
        input.parse::<Token![,]>()?;
        if at_list_end(input) {
            break;
        }
    }
    Ok(out)
}
