//! 剥がしたファイルを、比べるためのトークン列にする（仕様 §6.1 の 3 の正規化）。
//!
//! - 空白とコメントは字句の段階で消える。doc コメント（`///`、`//!`、`/** */`）もコメントとして捨てる。
//! - 続けて書いた属性（`#[..]` の並び、`#![..]` の並び）は、並べ替えて比べる。
//! - 末尾のカンマ（閉じ括弧の前、`;` の前、契約とループの句の前、`{` の前）は捨てる。要素が一つの
//!   丸括弧の `(a,)` は、カンマがあるとタプルなので残す。
//! - マクロの引数と属性の引数の中は、何も正規化しない（中の意味を Ebykn が知らないため）。
//!
//! 取り除く範囲（[`Cut`]）に収まるトークンは出さない。

use std::str::FromStr as _;

use proc_macro2::{Delimiter, TokenStream, TokenTree};

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

/// ファイルの本文をトークン列にする。`proof` なら `.eykp` の側で、証明の属性（仕様 §5.7 の
/// `ProofAttr`）を捨てる。字句として読めなければ `None`（構文解析が診断を出している）。
pub fn flatten(text: &str, cuts: &[Cut], proof: bool) -> Option<Vec<Tok>> {
    let ts = TokenStream::from_str(text).ok()?;
    let cx = Cx { text, cuts, proof };
    let trees: Vec<TokenTree> = ts.into_iter().collect();
    let mut out = Vec::new();
    for piece in cx.level(&trees, None, false) {
        out.extend(piece.toks);
    }
    Some(out)
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

    /// `opaque` は、マクロの引数か属性の引数の中か（正規化しない）。
    fn level(&self, trees: &[TokenTree], delim: Option<Delimiter>, opaque: bool) -> Vec<Piece> {
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
                        toks.extend(self.tree(x, true));
                    }
                    toks.extend(self.tree(&trees[gi], true));
                    pieces.push(Piece {
                        toks,
                        attr: Some(inner),
                    });
                    i = next;
                    continue;
                }
            }
            // マクロの引数（`m!(..)`、`macro_rules! m { .. }`）の中は正規化しない。
            let macro_args = matches!(t, TokenTree::Group(_)) && is_macro_args(trees, i);
            pieces.push(Piece {
                toks: self.tree(t, opaque || macro_args),
                attr: None,
            });
            i += 1;
        }
        if opaque {
            return pieces;
        }
        sort_attrs(&mut pieces);
        drop_trailing_commas(pieces, delim)
    }

    fn tree(&self, t: &TokenTree, opaque: bool) -> Vec<Tok> {
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
                for p in self.level(&inner, Some(g.delimiter()), opaque) {
                    toks.extend(p.toks);
                }
                toks.push(Tok {
                    text: close.into(),
                    lo: g.span_close().byte_range().start,
                    hi,
                });
                toks
            }
            // 空白の有無（`Spacing`）は比べない。`> >` と `>>` の違いは構文が決める。
            TokenTree::Punct(p) => {
                vec![Tok {
                    text: p.as_char().to_string(),
                    lo,
                    hi,
                }]
            }
            _ => vec![Tok {
                text: t.to_string(),
                lo,
                hi,
            }],
        }
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

/// `!` の前に来ても、マクロの名前ではない語（`if !(..)`、`return !(..)` など）。
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

/// 続けて書いた属性（同じ種類のもの）を、トークン列の順に並べ替える。
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

/// 末尾のカンマを捨てる。
fn drop_trailing_commas(pieces: Vec<Piece>, delim: Option<Delimiter>) -> Vec<Piece> {
    let commas = pieces.iter().filter(|p| p.is(",")).count();
    let n = pieces.len();
    let mut keep = vec![true; n];
    for k in 0..n {
        if !pieces[k].is(",") {
            continue;
        }
        let drop = match pieces.get(k + 1) {
            // 閉じ括弧の前。`(a,)` はタプルなので残す。
            None => delim.is_some() && !(delim == Some(Delimiter::Parenthesis) && commas == 1),
            Some(next) => {
                next.attr.is_none()
                    && (next.is(";")
                        || next.first() == Some("{")
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
