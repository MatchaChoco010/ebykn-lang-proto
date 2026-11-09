//! 構文解析の入口と、解析中の状態。
//!
//! 字句は proc_macro2、構文は verus_syn（`vendor/verus_syn`）で読む。Ebykn が足す構文は、
//! フォークの差し込み口（`verus_syn::set_ebykn_hook`）から [`hook`] が読み、フォークの
//! 構文木には `Verbatim` として残る。[`convert`] がフォークの構文木を Ebykn の AST に写し、
//! `Verbatim` は [`hook`] の関数で読み直す。項目の文法（契約の句、`owns` / `spec`、
//! `lemma fn` など）は [`item`] が直接読む。

mod contract;
mod convert;
mod hook;
mod item;
mod tokens;

use std::cell::RefCell;
use std::str::FromStr as _;

use proc_macro2::{TokenStream, TokenTree};
use verus_syn as syn;
use verus_syn::parse::{ParseBuffer, ParseStream, Parser as _};

use crate::ast;
use crate::diag::{Diagnostic, Message};
use crate::span::{SourceFile, Span};

/// 構文解析の結果。`file` は読めた項目だけを含む（読めなかった項目は診断になる）。
#[derive(Debug)]
pub struct Parsed {
    pub file: ast::File,
    /// 位置の順に並べた診断。
    pub diags: Vec<Diagnostic>,
}

impl Parsed {
    pub fn is_ok(&self) -> bool {
        self.diags.is_empty()
    }
}

/// `.eyk` を読む。
pub fn parse_eyk(file: &SourceFile) -> Parsed {
    syn::set_ebykn_hook(hook::hook);
    let tokens = match TokenStream::from_str(&file.text) {
        Ok(t) => t,
        Err(e) => {
            let span = span_in(file, e.span());
            return Parsed {
                file: ast::File {
                    attrs: Vec::new(),
                    items: Vec::new(),
                    span: file.span(0, file.text.len()),
                },
                diags: vec![Diagnostic::new(span, Message::Lex)],
            };
        }
    };
    let prev = CX.with(|cx| {
        cx.borrow_mut().replace(Cx {
            file: file.clone(),
            diags: Vec::new(),
            suppress: 0,
            pending: Vec::new(),
        })
    });
    tokens::scan(&tokens);
    let result = (|input: ParseStream| item::parse_file(input)).parse2(tokens);
    let mut cx = CX
        .with(|cx| std::mem::replace(&mut *cx.borrow_mut(), prev))
        .expect("Cx");
    let parsed = match result {
        Ok(f) => f,
        Err(e) => {
            // parse_file は項目ごとに回復するので、ここに来るのはファイル先頭の内部属性の誤りだけ。
            for d in error_diags(&mut cx, e) {
                cx.diags.push(d);
            }
            ast::File {
                attrs: Vec::new(),
                items: Vec::new(),
                span: file.span(0, file.text.len()),
            }
        }
    };
    let mut diags = cx.diags;
    diags.sort_by_key(|d| (d.span.lo, d.span.hi));
    diags.dedup();
    Parsed {
        file: parsed,
        diags,
    }
}

/// 解析中の状態。差し込み口の関数は引数で状態を受け取れないので、スレッドローカルに置く。
struct Cx {
    file: SourceFile,
    diags: Vec<Diagnostic>,
    /// 0 より大きい間は、[`push`] した診断を捨てる。差し込み口が項目を先読みする間に使う
    /// （同じ項目は後で読み直すので、そのときに診断を出す）。
    suppress: u32,
    /// [`err`] で作ったエラーの中身。エラーの文言には添字だけを書く。
    pending: Vec<Message>,
}

thread_local! {
    static CX: RefCell<Option<Cx>> = const { RefCell::new(None) };
}

fn with_cx<R>(f: impl FnOnce(&mut Cx) -> R) -> R {
    CX.with(|cx| {
        f(cx.borrow_mut()
            .as_mut()
            .expect("parse_eyk の外から呼ばれた"))
    })
}

fn span_in(file: &SourceFile, s: proc_macro2::Span) -> Span {
    let r = s.byte_range();
    if r.start == 0 && r.end == 0 {
        // call_site（入力の末尾で起きたエラーなど）。長さのあるトークンは 0..0 にならない。
        file.eof()
    } else {
        file.span(r.start, r.end)
    }
}

/// proc_macro2 の位置を Ebykn の位置にする。
pub(crate) fn sp(s: proc_macro2::Span) -> Span {
    with_cx(|cx| span_in(&cx.file, s))
}

/// 構文木の節点の位置（最初のトークンから最後のトークンまで）。
pub(crate) fn span_of<T: syn::spanned::Spanned>(t: &T) -> Span {
    sp(t.span())
}

/// `begin` から `end` までに読んだトークンの位置。何も読んでいなければ `end` の次のトークン。
pub(crate) fn span_between(begin: &ParseBuffer, end: ParseStream) -> Span {
    let end = end.cursor();
    let mut cursor = begin.cursor();
    let mut first: Option<proc_macro2::Span> = None;
    let mut last: Option<proc_macro2::Span> = None;
    while cursor != end {
        let Some((tt, next)) = cursor.token_tree() else {
            break;
        };
        first.get_or_insert(tt.span());
        last = Some(tt.span());
        cursor = next;
    }
    match (first, last) {
        (Some(a), Some(b)) => sp(a).to(sp(b)),
        _ => sp(end.span()),
    }
}

/// トークン列の位置。
pub(crate) fn span_of_tokens(ts: &TokenStream) -> Option<Span> {
    let mut it = ts.clone().into_iter();
    let first = it.next()?;
    let last = it.last().unwrap_or_else(|| first.clone());
    Some(sp(first.span()).to(sp(last.span())))
}

/// 診断を足す（先読みの間は捨てる）。
pub(crate) fn push(span: Span, message: Message) {
    with_cx(|cx| {
        if cx.suppress == 0 {
            cx.diags.push(Diagnostic::new(span, message));
        }
    });
}

/// 先読みの間、診断を捨てる。
pub(crate) fn suppressed<R>(f: impl FnOnce() -> R) -> R {
    with_cx(|cx| cx.suppress += 1);
    let r = f();
    with_cx(|cx| cx.suppress -= 1);
    r
}

const PENDING: &str = "\u{1}ebykn-pending#";

/// Ebykn の診断を、構文解析器のエラーとして返す。
pub(crate) fn err(span: proc_macro2::Span, message: Message) -> syn::Error {
    let i = with_cx(|cx| {
        cx.pending.push(message);
        cx.pending.len() - 1
    });
    syn::Error::new(span, format!("{PENDING}{i}"))
}

/// 構文解析器のエラーを診断にして足す。
pub(crate) fn push_error(e: syn::Error) {
    let diags = with_cx(|cx| error_diags(cx, e));
    for d in diags {
        push(d.span, d.message);
    }
}

fn error_diags(cx: &mut Cx, e: syn::Error) -> Vec<Diagnostic> {
    e.into_iter()
        .map(|e| {
            let msg = e.to_string();
            let span = span_in(&cx.file, e.span());
            let message = if let Some(i) = msg.strip_prefix(PENDING) {
                let i: usize = i.parse().expect("添字");
                cx.pending[i].clone()
            } else if msg.starts_with("unexpected end of input") {
                Message::UnexpectedEof
            } else {
                Message::Parser(msg)
            };
            Diagnostic::new(span, message)
        })
        .collect()
}

/// 次の `n` 個のトークン木（グループは一つと数える）。
pub(crate) fn peek_tts(input: ParseStream, n: usize) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(n);
    let mut cursor = input.cursor();
    while out.len() < n {
        let Some((tt, next)) = cursor.token_tree() else {
            break;
        };
        out.push(tt);
        cursor = next;
    }
    out
}

/// トークン木が識別子 `name`（`r#` なし）か。
pub(crate) fn is_ident(tt: Option<&TokenTree>, name: &str) -> bool {
    matches!(tt, Some(TokenTree::Ident(i)) if i == name)
}

/// トークン木が記号 `c` か。
pub(crate) fn is_punct(tt: Option<&TokenTree>, c: char) -> bool {
    matches!(tt, Some(TokenTree::Punct(p)) if p.as_char() == c)
}

/// トークン木が区切り `d` のグループか。
pub(crate) fn is_group(tt: Option<&TokenTree>, d: proc_macro2::Delimiter) -> bool {
    matches!(tt, Some(TokenTree::Group(g)) if g.delimiter() == d)
}

/// 次のトークンが識別子 `name` か。
pub(crate) fn peek_kw(input: ParseStream, name: &str) -> bool {
    input.cursor().ident().is_some_and(|(i, _)| i == name)
}

/// 識別子 `name` を読む。
pub(crate) fn parse_kw(input: ParseStream, name: &str) -> syn::Result<proc_macro2::Ident> {
    input.step(|cursor| match cursor.ident() {
        Some((i, rest)) if i == name => Ok((i, rest)),
        _ => Err(cursor.error(format!("expected `{name}`"))),
    })
}

/// 次が `fork` の上で `f` が読めるものか（入力は進めない）。
pub(crate) fn lookahead(
    input: ParseStream,
    f: impl FnOnce(ParseStream) -> syn::Result<()>,
) -> bool {
    let fork = input.fork();
    f(&fork).is_ok()
}
