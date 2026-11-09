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
    /// 構文解析では判定できなかった、[`crate::reserved`] の表の名前を最後に持つパス。
    /// 検査（`ebykn_check`）が名前解決して決める。
    pub deferred: Vec<(Span, crate::reserved::DeferredName)>,
}

impl Parsed {
    pub fn is_ok(&self) -> bool {
        self.diags.is_empty()
    }
}

/// `.eyk` を読む。証明構文（仕様 §5.7）は E0303 にする。
pub fn parse_eyk(file: &SourceFile) -> Parsed {
    parse(file, false)
}

/// `.eykp` を読む。文法は `.eyk` の文法に §5.7 の証明構文を足したもの。
pub fn parse_eykp(file: &SourceFile) -> Parsed {
    parse(file, true)
}

fn parse(file: &SourceFile, proof: bool) -> Parsed {
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
                deferred: Vec::new(),
            };
        }
    };
    let prev = CX.with(|cx| {
        cx.borrow_mut().replace(Cx {
            file: file.clone(),
            diags: Vec::new(),
            capture: Vec::new(),
            items: std::collections::HashMap::new(),
            pending: Vec::new(),
            unchecked_body: false,
            proof,
            proof_attrs: Vec::new(),
            attr_ok: std::collections::HashSet::new(),
        })
    });
    tokens::scan(&tokens);
    let result = (|input: ParseStream| item::parse_file(input)).parse2(tokens);
    let mut cx = CX
        .with(|cx| std::mem::replace(&mut *cx.borrow_mut(), prev))
        .expect("Cx");
    check_proof_attrs(&mut cx);
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
    // 判定を検査に任せる名前は、診断の流れに乗せて運んできた（先読みで捨てた分は捨てられる）。
    let mut deferred = Vec::new();
    diags.retain(|d| match &d.message {
        Message::Deferred(n) => {
            deferred.push((d.span, n.clone()));
            false
        }
        _ => true,
    });
    Parsed {
        file: parsed,
        diags,
        deferred,
    }
}

/// 解析中の状態。差し込み口の関数は引数で状態を受け取れないので、スレッドローカルに置く。
struct Cx {
    file: SourceFile,
    diags: Vec<Diagnostic>,
    /// [`captured`] の中では、[`push`] した診断をここに貯める。
    capture: Vec<Vec<Diagnostic>>,
    /// 差し込み口が文の位置で読んだ項目（ブロックの中の項目）。鍵は項目の最初のトークンの
    /// バイト位置。差し込み口で読んだ項目を、写すときに読み直さないために使う。
    items: std::collections::HashMap<usize, CachedItem>,
    /// [`err`] で作ったエラーの中身。エラーの文言には添字だけを書く。
    pending: Vec<Message>,
    /// `unchecked fn` の本体を読んでいる間だけ真（`env!` / `option_env!` を許す。#27）。
    unchecked_body: bool,
    /// `.eykp` を読んでいるか（証明構文を受理する）。
    proof: bool,
    /// 字句の走査（[`tokens`]）が見つけた証明の属性（仕様 §5.7 の `ProofAttr`）。
    proof_attrs: Vec<ProofAttrUse>,
    /// 書ける位置で読んだ証明の属性の位置（[`ProofAttrUse::span`] の `lo`）。
    attr_ok: std::collections::HashSet<usize>,
}

/// ファイルの中の証明の属性一つ。書ける位置（式の `#[trigger]`、量化子の本体の前の
/// `#![trigger ..]` / `#![auto]`、項目の種類に合う属性）で読んだものだけを通し、ほかは
/// 解析の終わりに E0001 にする（[`check_proof_attrs`]）。
pub(crate) struct ProofAttrUse {
    pub span: Span,
    pub name: String,
    pub inner: bool,
    /// 名前の後ろのトークン列（`(20)` など）。
    pub args: Vec<TokenTree>,
}

pub(crate) fn record_proof_attr(a: ProofAttrUse) {
    with_cx(|cx| cx.proof_attrs.push(a));
}

/// 証明の属性を書ける位置で読んだ。
pub(crate) fn proof_attr_ok(span: Span) {
    with_cx(|cx| {
        cx.attr_ok.insert(span.lo);
    });
}

/// 解析の終わりに、証明の属性の位置と引数を確かめる。書ける位置で読まなかったもの（ファイルや
/// `mod` の先頭の内部属性、文や構造体の属性、マクロの引数の中、`cfg_attr` の中など）は、`.eykp` でも
/// 書けないので、`.eyk` でも E0001 にする（証明ファイルへ案内しない）。書ける位置で正しい形なら、
/// `.eyk` では E0303。
fn check_proof_attrs(cx: &mut Cx) {
    for a in std::mem::take(&mut cx.proof_attrs) {
        let written = if a.inner {
            format!("#![{}]", a.name)
        } else {
            format!("#[{}]", a.name)
        };
        let m = if !cx.attr_ok.contains(&a.span.lo) {
            Message::NotAllowedHere { written }
        } else if let Some(form) = tokens::proof_attr_args_error(&a) {
            Message::ProofAttrArgs { written, form }
        } else if cx.proof {
            continue;
        } else {
            Message::ProofSyntax { written }
        };
        cx.diags.push(Diagnostic::new(a.span, m));
    }
}

/// `.eykp` を読んでいるか。
pub(crate) fn in_proof() -> bool {
    with_cx(|cx| cx.proof)
}

/// `unchecked fn` の本体を読んでいる間かを設定し、前の値を返す。
pub(crate) fn set_unchecked_body(v: bool) -> bool {
    with_cx(|cx| std::mem::replace(&mut cx.unchecked_body, v))
}

pub(crate) fn in_unchecked_body() -> bool {
    with_cx(|cx| cx.unchecked_body)
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

/// パス（`r#` を外した名前の列）の最後の名前が表（[`crate::reserved`]）の `want` の種類なら、
/// 裸の名前と先頭が `::` のものは診断にし、それ以外は検査に任せる。診断にしたら `true`。
pub(crate) fn reserved_path(
    segs: &[String],
    leading_colon: bool,
    want: fn(crate::reserved::ReservedKind) -> bool,
    written: String,
    span: Span,
) -> bool {
    use crate::reserved::{DeferredName, ReservedKind, kind_of};
    let Some(kind) = segs.last().and_then(|n| kind_of(n)).filter(|k| want(*k)) else {
        return false;
    };
    if segs.len() == 1 || leading_colon {
        let name = segs.last().cloned().unwrap_or_default();
        let m = match kind {
            ReservedKind::ProofCall => {
                crate::reserved::proof_call_message(&name, written, in_proof())
            }
            ReservedKind::VerusCall => Message::VerusBuiltin { name, written },
            ReservedKind::VerusMacro => Message::VerusOnly { written },
            ReservedKind::EbyknTrait => Message::NotAllowedHere { written },
        };
        push(span, m);
        return true;
    }
    push(
        span,
        Message::Deferred(DeferredName {
            segments: segs.to_vec(),
            kind,
            written,
        }),
    );
    false
}

/// 診断を足す（[`captured`] の中では貯める）。
pub(crate) fn push(span: Span, message: Message) {
    with_cx(|cx| {
        let d = Diagnostic::new(span, message);
        match cx.capture.last_mut() {
            Some(c) => c.push(d),
            None => cx.diags.push(d),
        }
    });
}

/// `f` の中で足した診断を、足さずに返す。
pub(crate) fn captured<R>(f: impl FnOnce() -> R) -> (R, Vec<Diagnostic>) {
    with_cx(|cx| cx.capture.push(Vec::new()));
    let r = f();
    let d = with_cx(|cx| cx.capture.pop().expect("capture"));
    (r, d)
}

/// 差し込み口で読んだ項目と、そのときの診断。`hi` は項目の最後のトークンの終わり。
#[derive(Clone)]
pub(crate) struct CachedItem {
    pub item: ast::Item,
    pub diags: Vec<Diagnostic>,
    pub hi: usize,
}

pub(crate) fn cache_item(lo: usize, item: CachedItem) {
    with_cx(|cx| {
        cx.items.insert(lo, item);
    });
}

pub(crate) fn cached_item(lo: usize) -> Option<CachedItem> {
    with_cx(|cx| cx.items.get(&lo).cloned())
}

/// `parse_eyk` の中か。差し込み口は、`parse_eyk` の外（ほかの verus_syn の利用者）では
/// 何もしない。
pub(crate) fn in_parse() -> bool {
    CX.with(|cx| cx.try_borrow().map(|c| c.is_some()).unwrap_or(false))
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
