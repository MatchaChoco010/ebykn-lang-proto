//! 消去フィールドのパターン `@x` の書き下し（#152）。
//!
//! Verus は `Tracked(x)` / `Ghost(x)` のパターンを、実行文脈の `let` の文（`let-else` を含む）でしか
//! 受けない。match の腕・`if let` / `while let`・`for`・クロージャの引数・関数の引数の中のパターンでは
//! `E0532`（タプル構造体のパターンが見つからない）になり、論理文脈（`spec fn` の本体など）では `let` の
//! 文でも `Tracked(x)` を受けない（`tests/verus-probes/erased_pat_*.rs`）。
//!
//! そこで、実行文脈の `let` の文のほかは、消去フィールドを同じ名前 `x` に、値の `Tracked<..>` / `Ghost<..>`
//! のまま束縛し（[`Cx::pat_wrapped`]）、束縛が効く範囲（腕・`if` / ループ・クロージャ・関数の本体、論理文脈
//! の `let` の後）の先頭で、中身に束縛し直して覆う（[`Cx::erased_rebind`]）。実行文脈では
//! `let Tracked(x) = x;` / `let Ghost(x) = x;`（`let` の文と同じ形）、論理文脈では `let x = x@;`。
//!
//! 名前を変えないので、同じパターンや引数の並びで `x` を二度束縛すれば、`.eyk` で書いたときと同じく
//! rustc が E0415 / E0416 で止める（別の名前に変えると、重複が見えなくなり片方を黙って選んだ。PR #168 の
//! レビュー）。束縛し直す前の位置（match のガード、`&&` で続く条件、ループ句、関数とクロージャの契約）
//! では `x` がまだ包みの値なので、そこで `x` を使うものは検査が E0003 にする（`ebykn_check` の
//! `body/erased_pat.rs`）。

use ebykn_check::facts::SpanKey;
use ebykn_check::index::ErasedKind;
use ebykn_syntax::ast::{self, Expr, ExprKind, Param};

use crate::Cx;
use crate::expr::ident;

impl Cx<'_> {
    /// 消去フィールドを包みの値のまま束縛したパターン（`@x` ⇒ `__eyk_erased_x: x`）。
    pub fn pat_wrapped(&self, p: &ast::Pat) -> String {
        self.pat_m(p, true)
    }

    /// [`Cx::pat_wrapped`] で包みの値に束縛した `@x` を、中身に束縛し直す文。`@x` がなければ空。
    pub fn erased_rebind(&self, p: &ast::Pat) -> String {
        let found = p.erased_idents();
        // `|` の各選択肢は同じ名前を束縛するので、一つにまとめる。
        let mut seen = std::collections::BTreeSet::new();
        found
            .into_iter()
            .filter(|i| seen.insert(i.name.clone()))
            .map(|i| {
                let x = ident(i);
                if self.logic {
                    return format!("let {x} = {x}@; ");
                }
                match self.facts.erased.get(&SpanKey::from(i.span)) {
                    Some(ErasedKind::Owns) => format!("let Tracked({x}) = {x}; "),
                    _ => format!("let Ghost({x}) = {x}; "),
                }
            })
            .collect()
    }

    /// `if` / `while` の条件（`&&` で繋いだ `let p = e`）のパターンの `@x` を束縛し直す文。
    pub fn cond_rebind(&self, cond: &Expr) -> String {
        let mut out = String::new();
        for e in cond.and_chain() {
            if let ExprKind::Let { pat, .. } = &e.kind {
                out.push_str(&self.erased_rebind(pat));
            }
        }
        out
    }

    /// 関数の引数のパターンの `@x` を束縛し直す文（本体の先頭に置く）。
    pub fn params_rebind(&self, sig: &ast::Signature) -> String {
        sig.params
            .iter()
            .map(|p| match p {
                Param::Typed { pat, .. } => self.erased_rebind(pat),
                Param::Receiver { .. } => String::new(),
            })
            .collect()
    }
}

/// ブロックの書き下し（`{ .. }`）の先頭に文を足す。
pub fn prepend_block(block: String, lets: &str) -> String {
    if lets.is_empty() {
        return block;
    }
    let rest = block
        .strip_prefix('{')
        .expect("ブロックの書き下しは `{` で始まる");
    format!("{{ {lets}{rest}")
}

/// 式の書き下しの前に文を足して、ブロックにする（match の腕・クロージャの本体）。
pub fn prepend_expr(e: String, lets: &str) -> String {
    if lets.is_empty() {
        return e;
    }
    format!("{{ {lets}{e} }}")
}
