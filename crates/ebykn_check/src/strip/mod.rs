//! 剥がし一致検査（仕様 §6.1）：`proof/<p>.eykp` から証明構文（§5.7）を取り除くと `src/<p>.eyk` に
//! 一致することを確かめる。
//!
//! 1. 両方の AST から、剥がす範囲（[`collect`]）とループの句を集める。剥がすのは、証明の文、
//!    `lemma fn` の本体と `by(mode)`、`lemma group`、`use lemmas`、`.eyk` にない `spec fn` と本体付きの
//!    `lemma fn`、証明の属性。
//! 2. 両方のファイルを、剥がす範囲を除いて正規化したトークン列にする（[`tokens`]）。ループの句の並びは
//!    取り除いて目印に置き換え、目印の位置で `.eyk` 側の句が `.eykp` 側の句の部分列であることを確かめる
//!    （E0305）。
//! 3. トークン列が最初に異なる位置を、項目と文に結びつけて E0304 にする。
//!
//! どちらかのファイルに構文の診断があれば比べない（読めた項目だけで比べると、読めなかった項目が
//! 不一致に見える）。

mod collect;
mod tokens;

use ebykn_syntax::ast::{self, LoopClauseKind};
use ebykn_syntax::diag::{CheckMsg, Message, ProofOnlyClash};
use ebykn_syntax::span::{SourceFile, Span};

use crate::Diags;
use crate::project::Project;
use collect::{LoopSite, Shape};
use tokens::Tok;

/// `.eykp` だけの項目に付けられない名前のうち、`.eyk` に書かれていなくても使われるもの：§6.4 の展開と
/// 書き下しが呼ぶ Ebykn のトレイトのメソッド（`impl T` に同じ名前の `spec fn` があると、トレイトの
/// メソッドより先に選ばれる）。ほかに、書き下しが glob で取り込む `ebykn_prelude` の名前
/// （[`crate::index::PRELUDE_TYPES`]）も使えない。
const IMPLICIT_METHODS: &[&str] = &["invariant", "view"];

/// プロジェクトのすべての証明ファイルを検査する。
pub fn check(project: &Project, diags: &mut Diags) {
    // `.eyk` に現れる名前。`.eykp` だけの項目がこの名前を持つと、glob の import（暗黙の
    // `use ebykn_prelude::*;` を含む）やトレイトのメソッドを覆い、`.eyk` の契約とコードの意味を変えうる。
    let mut taken = std::collections::HashSet::new();
    for m in &project.modules {
        tokens::idents(&project.file(m.file).text, &mut taken);
    }
    taken.extend(
        IMPLICIT_METHODS
            .iter()
            .chain(crate::index::PRELUDE_TYPES)
            .map(|s| s.to_string()),
    );
    for pf in &project.proofs {
        let ok = |f: ebykn_syntax::span::FileId| project.parsed_ok[f.0 as usize];
        if !ok(pf.file) || !ok(pf.src) {
            continue;
        }
        let (Some(src_ast), Some(proof_ast)) = (project.ast(pf.src), project.ast(pf.file)) else {
            continue;
        };
        check_pair(
            project.file(pf.src),
            src_ast,
            project.file(pf.file),
            proof_ast,
            &taken,
            diags,
        );
    }
}

/// 一つの組を検査する。
pub fn check_pair(
    src: &SourceFile,
    src_ast: &ast::File,
    proof: &SourceFile,
    proof_ast: &ast::File,
    taken: &std::collections::HashSet<String>,
    diags: &mut Diags,
) {
    let s_shape = collect::collect(&src.text, src_ast, None);
    let p_shape = collect::collect(&proof.text, proof_ast, Some(&s_shape.decls));
    // `.eykp` だけの項目の名前：`.eyk` に現れる名前と、ほかの `.eykp` だけの項目の名前は使えない。
    let mut seen = std::collections::HashSet::new();
    for (span, item, name) in &p_shape.proof_only {
        let clash = if taken.contains(name) {
            Some(ProofOnlyClash::Source)
        } else if !seen.insert(name.clone()) {
            Some(ProofOnlyClash::ProofOnly)
        } else {
            None
        };
        if let Some(clash) = clash {
            diags.push(
                *span,
                Message::Check(CheckMsg::ProofOnlyName {
                    item: item.clone(),
                    clash,
                }),
            );
        }
    }
    let s_toks = tokens::flatten(&src.text, &s_shape.cuts, false);
    let p_toks = tokens::flatten(&proof.text, &p_shape.cuts, true);
    let a = Side::new(src, &s_shape, s_toks);
    let b = Side::new(proof, &p_shape, p_toks);
    let (mut i, mut j) = (0, 0);
    loop {
        match (a.seq.get(i), b.seq.get(j)) {
            (None, None) => return,
            (Some(Item::Loop(x)), Some(Item::Loop(y))) => {
                if let Some(missing) = subsequence(&a, &a.loops[*x], &b, &b.loops[*y]) {
                    let e = &a.loops[*x].elems[missing];
                    let kw = match e.kind {
                        LoopClauseKind::Invariant => "invariant",
                        LoopClauseKind::Decreases => "decreases",
                    };
                    diags.push(
                        b.loops[*y].span,
                        Message::Check(CheckMsg::LoopClauseNotKept {
                            clause: format!("{kw} {}", squash(&src.text[e.lo..e.hi])),
                            other: a.at(e.lo),
                        }),
                    );
                    return;
                }
            }
            (Some(Item::Tok(x)), Some(Item::Tok(y))) if a.toks[*x].text == b.toks[*y].text => {}
            _ => {
                mismatch(&a, i, &b, j, diags);
                return;
            }
        }
        i += 1;
        j += 1;
    }
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 比べる列の要素：トークンか、ループの句の並びの目印。
#[derive(Debug, Clone, Copy)]
enum Item {
    Tok(usize),
    Loop(usize),
}

struct Side<'a> {
    file: &'a SourceFile,
    shape: &'a Shape,
    toks: Vec<Tok>,
    loops: Vec<LoopSite>,
    seq: Vec<Item>,
}

impl<'a> Side<'a> {
    fn new(file: &'a SourceFile, shape: &'a Shape, toks: Vec<Tok>) -> Side<'a> {
        // 剥がした範囲の中のループは数えない。
        let loops: Vec<LoopSite> = shape
            .loops
            .iter()
            .filter(|l| {
                !shape
                    .cuts
                    .iter()
                    .any(|c| c.lo <= l.span.lo && l.span.hi <= c.hi)
            })
            .cloned()
            .collect();
        let mut seq = Vec::new();
        for (k, t) in toks.iter().enumerate() {
            if loops
                .iter()
                .any(|l| l.region.is_some_and(|(lo, hi)| lo <= t.lo && t.lo < hi))
            {
                continue;
            }
            if let Some(n) = loops.iter().position(|l| l.body_lo == t.lo) {
                seq.push(Item::Loop(n));
            }
            seq.push(Item::Tok(k));
        }
        Side {
            file,
            shape,
            toks,
            loops,
            seq,
        }
    }

    /// 句の式一つの、正規化したトークン列。
    fn elem(&self, l: &LoopSite, n: usize) -> (LoopClauseKind, Vec<&str>) {
        let e = &l.elems[n];
        let toks = self
            .toks
            .iter()
            .filter(|t| e.lo <= t.lo && t.hi <= e.hi)
            .map(|t| t.text.as_str())
            .collect();
        (e.kind, toks)
    }

    /// `ファイル:行:列`。
    fn at(&self, byte: usize) -> String {
        let lc = self.file.line_col(byte);
        format!("{}:{}:{}", self.file.path, lc.line, lc.col)
    }

    /// 列の `k` 番目の位置（列の終わりならファイルの終わり）と、その表示。
    fn point(&self, k: usize) -> (usize, Option<String>) {
        // 目印の位置では、続くトークン（本体の `{`）を見せる。
        let tok = self.seq[k.min(self.seq.len())..]
            .iter()
            .find_map(|x| match x {
                Item::Tok(t) => Some(&self.toks[*t]),
                Item::Loop(_) => None,
            });
        match tok {
            Some(t) => (t.lo, Some(t.text.clone())),
            None => (self.file.text.len(), None),
        }
    }

    /// `byte` を含むもののうち、いちばん内側のもの。
    fn innermost<T>(list: &[(Span, T)], byte: usize) -> Option<&(Span, T)> {
        list.iter()
            .filter(|(s, _)| s.lo <= byte && byte < s.hi)
            .min_by_key(|(s, _)| s.hi - s.lo)
    }

    fn item(&self, byte: usize) -> Option<&str> {
        Self::innermost(&self.shape.items, byte).map(|(_, d)| d.as_str())
    }

    fn stmt(&self, byte: usize) -> Option<Span> {
        self.shape
            .stmts
            .iter()
            .filter(|s| s.lo <= byte && byte < s.hi)
            .min_by_key(|s| s.hi - s.lo)
            .copied()
    }
}

/// `.eyk` 側の句の式の列が、`.eykp` 側の列の部分列か。違えば、見つからなかった `.eyk` 側の式の番号。
fn subsequence(a: &Side, x: &LoopSite, b: &Side, y: &LoopSite) -> Option<usize> {
    let mut k = 0;
    for n in 0..y.elems.len() {
        if k < x.elems.len() && a.elem(x, k) == b.elem(y, n) {
            k += 1;
        }
    }
    (k < x.elems.len()).then_some(k)
}

/// 最初に異なる位置を E0304 にする。位置は `.eykp` 側の、異なるトークンを含むいちばん内側の文
/// （文の外ならトークン）。`.eyk` 側の位置は文言に入れる。
fn mismatch(a: &Side, i: usize, b: &Side, j: usize, diags: &mut Diags) {
    let (a_at, expected) = a.point(i);
    let (b_at, found) = b.point(j);
    let item = b.item(b_at).or_else(|| a.item(a_at)).map(|s| s.to_string());
    // 閉じ括弧で食い違ったら（ブロックの終わりに文を足した、消した）、その括弧の位置を出す。囲む文の
    // 先頭は、食い違いから遠い。
    // カンマで食い違ったとき（余分なカンマ、足りないカンマ）も、その位置を出す。
    let closing = |t: &Option<String>| matches!(t.as_deref(), Some("}" | ")" | "]" | ","));
    let span = match b.stmt(b_at).filter(|_| !closing(&found)) {
        Some(s) => s,
        None => {
            let hi = b.seq[j.min(b.seq.len())..]
                .iter()
                .find_map(|x| match x {
                    Item::Tok(t) => Some(b.toks[*t].hi),
                    Item::Loop(_) => None,
                })
                .unwrap_or(b_at);
            b.file.span(b_at, hi)
        }
    };
    let other_at = match a.stmt(a_at) {
        Some(s) if !closing(&expected) => s.lo,
        _ => a_at,
    };
    diags.push(
        span,
        Message::Check(CheckMsg::StripMismatch {
            item,
            other: a.at(other_at),
            found,
            expected,
        }),
    );
}
