//! 剥がしたファイルを、比べるためのトークン列にする（仕様 §6.1 の 3 の正規化）。
//!
//! - 空白とコメントは字句の段階で消える。doc コメント（`///`、`//!`、`/** */`）もコメントとして捨てる。
//!   ただし、記号のあいだの空白はトークンの区切りを変える（`&&` と `& &` は別の式）ので、続けて書いた
//!   記号を rustc と同じく複数文字の演算子（`&&`、`::`、`->` など）にまとめてから比べる。
//! - 続けて書いた属性（`#[..]` の並び、`#![..]` の並び）は、並びがすべて Rust の組み込みの属性なら
//!   並べ替えて比べる。属性マクロを含む並びは、展開の順が意味を持つので書いた順に比べる（#144 の既定）。
//! - 末尾のカンマ（閉じ括弧の前、`;` の前、契約とループの句の前）は捨てる。`{` の前のカンマは、本体を
//!   開く `{` の前（丸括弧と角括弧の中でないところ）でだけ捨てる。要素が一つの丸括弧の `(a,)` は、
//!   カンマがあるとタプルなので残す（呼び出しと引数の並び `f(a,)` は捨てる）。
//! - マクロの引数と属性の引数の中は、何も正規化しない（中の意味を Ebykn が知らないため）。
//!
//! 取り除く範囲（[`Cut`]）に収まるトークンは出さない。

use std::str::FromStr as _;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

/// 取り除く範囲（バイト位置）。`put` があれば、範囲の先頭に代わりのトークンを一つ置く
/// （`lemma fn` の本体を剥がして `;` にするとき）。
#[derive(Debug, Clone)]
pub struct Cut {
    pub lo: usize,
    pub hi: usize,
    pub put: Option<&'static str>,
}

/// 比べるトークン一つ。`lo..hi` は元のファイルのバイト位置。
#[derive(Debug, Clone)]
pub struct Tok {
    pub text: String,
    pub lo: usize,
    pub hi: usize,
}

/// 末尾のカンマを捨てる、カンマの次の名前（契約とループの句のキーワード。仕様 §4）。
const CLAUSE_KEYWORDS: &[&str] = &[
    "requires",
    "ensures",
    "returns",
    "decreases",
    "no_panic",
    "gives",
    "invariant",
    "by",
];

/// rustc の複数文字の記号（長いものから。Verus と Ebykn の `==>`、`<==>`、`=~=` を含む）。
const MULTI_PUNCTS: &[&str] = &[
    "<==>", "...", "..=", "<<=", ">>=", "==>", "=~=", "!~=", "&&", "||", "<<", ">>", "==", "!=",
    "<=", ">=", "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "..", "::", "->", "=>", "<-",
];

/// ファイルの本文をトークン列にする。`proof` なら `.eykp` の側で、証明の属性（仕様 §5.7 の
/// `ProofAttr`）を捨てる。構文解析を通ったファイルだけを渡す（字句として読めなければ内部の誤り）。
pub fn flatten(text: &str, cuts: &[Cut], proof: bool) -> Vec<Tok> {
    let ts = TokenStream::from_str(text).expect("構文解析を通ったファイルは字句として読める");
    let cx = Cx { text, cuts, proof };
    let trees: Vec<TokenTree> = ts.into_iter().collect();
    let mut out = Vec::new();
    for piece in cx.level(&trees, None, false, false) {
        out.extend(piece.toks);
    }
    out
}

/// ファイルの識別子（`r#` を外したもの。マクロと属性の引数の中も含む）を集める。
pub fn idents(text: &str, out: &mut std::collections::HashSet<String>) {
    fn walk(ts: TokenStream, out: &mut std::collections::HashSet<String>) {
        for t in ts {
            match t {
                TokenTree::Ident(id) => {
                    let s = id.to_string();
                    out.insert(s.strip_prefix("r#").unwrap_or(&s).to_string());
                }
                TokenTree::Group(g) => walk(g.stream(), out),
                _ => {}
            }
        }
    }
    if let Ok(ts) = TokenStream::from_str(text) {
        walk(ts, out);
    }
}

struct Cx<'a> {
    text: &'a str,
    cuts: &'a [Cut],
    proof: bool,
}

/// 一つの階層の、まとまり一つ（トークン一つ、括弧のグループ一つ、属性一つ）。
struct Piece {
    toks: Vec<Tok>,
    /// 属性なら `Some(内部属性か)`。
    attr: Option<bool>,
}

impl Piece {
    fn is(&self, s: &str) -> bool {
        self.toks.len() == 1 && self.toks[0].text == s
    }

    fn first(&self) -> Option<&str> {
        self.toks.first().map(|t| t.text.as_str())
    }
}

fn range(t: &TokenTree) -> (usize, usize) {
    match t {
        TokenTree::Group(g) => (
            g.span_open().byte_range().start,
            g.span_close().byte_range().end,
        ),
        _ => {
            let r = t.span().byte_range();
            (r.start, r.end)
        }
    }
}

fn delims(d: Delimiter) -> (&'static str, &'static str) {
    match d {
        Delimiter::Parenthesis => ("(", ")"),
        Delimiter::Brace => ("{", "}"),
        Delimiter::Bracket => ("[", "]"),
        Delimiter::None => ("", ""),
    }
}

fn is_punct(t: Option<&TokenTree>, c: char) -> bool {
    matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == c)
}

impl Cx<'_> {
    fn cut(&self, lo: usize, hi: usize) -> Option<&Cut> {
        self.cuts.iter().find(|c| c.lo <= lo && hi <= c.hi)
    }

    /// 字句の上で doc コメントだったか（`///` などは `#[doc = ".."]` のトークンになる）。
    fn is_doc_comment(&self, lo: usize) -> bool {
        self.text
            .get(lo..)
            .is_some_and(|s| s.starts_with("//") || s.starts_with("/*"))
    }

    /// `opaque` は、マクロの引数か属性の引数の中か（正規化しない）。`args` は、この階層が呼び出しや
    /// 引数の並びの丸括弧か（`(a,)` のカンマがタプルを作らない）。
    fn level(
        &self,
        trees: &[TokenTree],
        delim: Option<Delimiter>,
        opaque: bool,
        args: bool,
    ) -> Vec<Piece> {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut i = 0;
        while i < trees.len() {
            let t = &trees[i];
            let (lo, hi) = range(t);
            if let Some(c) = self.cut(lo, hi) {
                if let Some(put) = c.put
                    && c.lo == lo
                {
                    pieces.push(Piece {
                        toks: vec![Tok {
                            text: put.into(),
                            lo,
                            hi: lo,
                        }],
                        attr: None,
                    });
                }
                i += 1;
                continue;
            }
            if !opaque
                && let TokenTree::Punct(p) = t
                && p.as_char() == '#'
            {
                let inner = is_punct(trees.get(i + 1), '!');
                let gi = if inner { i + 2 } else { i + 1 };
                if let Some(TokenTree::Group(g)) = trees.get(gi)
                    && g.delimiter() == Delimiter::Bracket
                {
                    let next = gi + 1;
                    if self.is_doc_comment(lo) || (self.proof && proof_attr(g.stream())) {
                        i = next;
                        continue;
                    }
                    let mut toks = Vec::new();
                    for x in &trees[i..gi] {
                        toks.extend(self.tree(x, true, false));
                    }
                    toks.extend(self.tree(&trees[gi], true, false));
                    pieces.push(Piece {
                        toks,
                        attr: Some(inner),
                    });
                    i = next;
                    continue;
                }
            }
            // 続けて書いた記号は、複数文字の演算子にまとめる（`&&` と `& &` を区別する）。
            if let TokenTree::Punct(_) = t {
                let mut j = i;
                while let Some(TokenTree::Punct(p)) = trees.get(j) {
                    if self.cut(range(&trees[j]).0, range(&trees[j]).1).is_some() {
                        break;
                    }
                    j += 1;
                    if p.spacing() != Spacing::Joint {
                        break;
                    }
                }
                for tok in puncts(&trees[i..j]) {
                    pieces.push(Piece {
                        toks: vec![tok],
                        attr: None,
                    });
                }
                i = j;
                continue;
            }
            // マクロの引数（`m!(..)`、`macro_rules! m { .. }`）の中は正規化しない。
            let macro_args = matches!(t, TokenTree::Group(_)) && is_macro_args(trees, i);
            pieces.push(Piece {
                toks: self.tree(t, opaque || macro_args, is_arg_list(trees, i)),
                attr: None,
            });
            i += 1;
        }
        if opaque {
            return pieces;
        }
        sort_attrs(&mut pieces);
        drop_trailing_commas(pieces, delim, args)
    }

    fn tree(&self, t: &TokenTree, opaque: bool, args: bool) -> Vec<Tok> {
        let (lo, hi) = range(t);
        match t {
            TokenTree::Group(g) => {
                let (open, close) = delims(g.delimiter());
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                let mut toks = vec![Tok {
                    text: open.into(),
                    lo,
                    hi: g.span_open().byte_range().end,
                }];
                for p in self.level(&inner, Some(g.delimiter()), opaque, args) {
                    toks.extend(p.toks);
                }
                toks.push(Tok {
                    text: close.into(),
                    lo: g.span_close().byte_range().start,
                    hi,
                });
                toks
            }
            // 属性の中など、まとめずに読む記号（属性の中は書いたとおりに比べるので、空白も見る）。
            TokenTree::Punct(p) => {
                let text = match p.spacing() {
                    Spacing::Joint => format!("{}+", p.as_char()),
                    Spacing::Alone => p.as_char().to_string(),
                };
                vec![Tok { text, lo, hi }]
            }
            _ => vec![Tok {
                text: t.to_string(),
                lo,
                hi,
            }],
        }
    }
}

/// 続けて書いた記号の並び（最後のもの以外は `Spacing::Joint`）を、rustc と同じく長い演算子から
/// 順に切り出す。
fn puncts(run: &[TokenTree]) -> Vec<Tok> {
    let chars: Vec<(char, usize, usize)> = run
        .iter()
        .filter_map(|t| match t {
            TokenTree::Punct(p) => {
                let (lo, hi) = range(t);
                Some((p.as_char(), lo, hi))
            }
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut k = 0;
    while k < chars.len() {
        let rest: String = chars[k..].iter().map(|c| c.0).collect();
        let n = MULTI_PUNCTS
            .iter()
            .find(|op| rest.starts_with(*op))
            .map(|op| op.chars().count())
            .unwrap_or(1);
        out.push(Tok {
            text: rest.chars().take(n).collect(),
            lo: chars[k].1,
            hi: chars[k + n - 1].2,
        });
        k += n;
    }
    out
}

/// `i` のグループが、呼び出しか引数の並びの丸括弧か：前が名前（`f(..)`、`fn g(..)`、`S(..)`）、
/// ジェネリクスの閉じ（`f::<T>(..)`）、括弧（`f(a)(..)`）。キーワードの後（`return (a,)`）はタプル。
fn is_arg_list(trees: &[TokenTree], i: usize) -> bool {
    let Some(TokenTree::Group(g)) = trees.get(i) else {
        return false;
    };
    if g.delimiter() != Delimiter::Parenthesis {
        return false;
    }
    match i.checked_sub(1).and_then(|k| trees.get(k)) {
        Some(TokenTree::Ident(id)) => !is_keyword(&id.to_string()),
        Some(TokenTree::Punct(p)) => p.as_char() == '>',
        Some(TokenTree::Group(g)) => g.delimiter() != Delimiter::Brace,
        _ => false,
    }
}

/// `i` のグループがマクロの引数か：`m!(..)`（`!` の前が名前）か、`macro_rules! m { .. }`。`!(a, b)`
/// （否定）のように、`!` の前がキーワードや演算子なら引数ではない。
fn is_macro_args(trees: &[TokenTree], i: usize) -> bool {
    let before = |k: usize| i.checked_sub(k).and_then(|j| trees.get(j));
    let name = |t: Option<&TokenTree>| matches!(t, Some(TokenTree::Ident(id)) if !is_keyword(&id.to_string()));
    (is_punct(before(1), '!') && name(before(2)))
        || (matches!(before(1), Some(TokenTree::Ident(_)))
            && is_punct(before(2), '!')
            && matches!(before(3), Some(TokenTree::Ident(id)) if id == "macro_rules"))
}

/// `!` や `(` の前に来ても、マクロや関数の名前ではない語（`if !(..)`、`ensures !(..)`、
/// `return (a,)` など）。Rust のキーワードと、Ebykn の契約・ループの句・証明の文のキーワード。
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "while"
            | "return"
            | "match"
            | "in"
            | "else"
            | "break"
            | "let"
            | "loop"
            | "for"
            | "yield"
            | "move"
            | "mut"
            | "ref"
            | "as"
            | "where"
            | "impl"
            | "dyn"
            | "const"
            | "static"
            | "unsafe"
            | "box"
            | "requires"
            | "ensures"
            | "returns"
            | "decreases"
            | "invariant"
            | "no_panic"
            | "gives"
            | "uses"
            | "by"
            | "assert"
    )
}

/// 属性のブラケットの中身が証明の属性（`trigger ..`、`auto`、`opaque` など）か。
fn proof_attr(ts: TokenStream) -> bool {
    let mut it = ts.into_iter();
    match (it.next(), it.next()) {
        (Some(TokenTree::Ident(id)), next) => {
            let path_continues = matches!(&next, Some(TokenTree::Punct(p)) if p.as_char() == ':');
            !path_continues && ebykn_syntax::is_proof_attr(&id.to_string())
        }
        _ => false,
    }
}

/// 属性の名前（パスの最初の名前。`r#` は外す）。
fn attr_name(p: &Piece) -> Option<&str> {
    let k = p.toks.iter().position(|t| t.text == "[")?;
    let name = p.toks.get(k + 1)?.text.as_str();
    Some(name.strip_prefix("r#").unwrap_or(name))
}

/// 続けて書いた属性（同じ種類のもの）を、トークン列の順に並べ替える。並びがすべて Rust の組み込みの
/// 属性のときだけ並べ替える（属性マクロは書いた順に展開されるので、順が意味を持つ。#144 の既定）。
fn sort_attrs(pieces: &mut [Piece]) {
    let mut i = 0;
    while i < pieces.len() {
        let Some(inner) = pieces[i].attr else {
            i += 1;
            continue;
        };
        let mut j = i;
        while j < pieces.len() && pieces[j].attr == Some(inner) {
            j += 1;
        }
        let builtin = pieces[i..j].iter().all(|p| {
            attr_name(p).is_some_and(|n| ebykn_syntax::macros::BUILTIN_ATTRS.contains(&n))
        });
        if !builtin {
            i = j;
            continue;
        }
        pieces[i..j].sort_by_cached_key(|p| {
            p.toks
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        });
        i = j;
    }
}

/// 末尾のカンマを捨てる。`args` は、この階層が呼び出しか引数の並びの丸括弧か。
fn drop_trailing_commas(pieces: Vec<Piece>, delim: Option<Delimiter>, args: bool) -> Vec<Piece> {
    let commas = pieces.iter().filter(|p| p.is(",")).count();
    // 本体を開く `{` の前のカンマを捨てるのは、丸括弧と角括弧の中でないときだけ（`(S, {})` は
    // `(S {})` と別の式）。
    let body_level = matches!(delim, None | Some(Delimiter::Brace));
    let n = pieces.len();
    let mut keep = vec![true; n];
    for k in 0..n {
        if !pieces[k].is(",") {
            continue;
        }
        let drop = match pieces.get(k + 1) {
            // 閉じ括弧の前。`(a,)` はタプルなので残す（呼び出しと引数の並びは除く）。
            None => {
                delim.is_some() && !(delim == Some(Delimiter::Parenthesis) && commas == 1 && !args)
            }
            Some(next) => {
                next.attr.is_none()
                    && (next.is(";")
                        || body_level && next.first() == Some("{")
                        || next.toks.len() == 1
                            && CLAUSE_KEYWORDS.contains(&next.toks[0].text.as_str()))
            }
        };
        if drop {
            keep[k] = false;
        }
    }
    pieces
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect()
}
