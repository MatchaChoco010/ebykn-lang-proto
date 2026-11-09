//! 消去フィールドのパターン `@x` の書き下し（#152）。
//!
//! Verus は `Tracked(x)` / `Ghost(x)` のパターンを、実行文脈の `let` の文（`let-else` を含む）でしか
//! 受けない。match の腕・`if let` / `while let`・`for`・クロージャの引数・関数の引数の中のパターンでは
//! `E0532`（タプル構造体のパターンが見つからない）になり、論理文脈（`spec fn` の本体など）では `let` の
//! 文でも `Tracked(x)` を受けない（`tests/verus-probes/erased_pat_*.rs`）。
//!
//! そこで、実行文脈の `let` の文のほかは、消去フィールドを別の名前 `__eyk_pat_x`（型は `Tracked<..>` /
//! `Ghost<..>`）に束縛し（[`Cx::pat_fresh`]）、束縛が効く範囲（腕・`if` / ループ・クロージャ・関数の本体、
//! 論理文脈の `let` の後）の先頭で `x` に束縛し直す（[`Cx::erased_rebind`]）。実行文脈では
//! `let Tracked(x) = __eyk_pat_x;` / `let Ghost(x) = __eyk_pat_x;`（`let` の文と同じ形）、論理文脈では
//! `let x = __eyk_pat_x@;`。束縛し直す前の位置（match のガード、`&&` で続く条件、ループ句、関数と
//! クロージャの契約）で `x` を使うものは、検査が E0003 にする（`ebykn_check` の `body/erased_pat.rs`）。

use ebykn_check::facts::SpanKey;
use ebykn_check::index::ErasedKind;
use ebykn_syntax::ast::{self, Expr, ExprKind, Param};

use crate::Cx;
use crate::expr::ident;

/// 消去フィールド `@x` を束縛する別の名前。`__eyk_` は利用者が書けない（E0101）。
pub fn fresh_name(i: &ast::Ident) -> String {
    format!("__eyk_pat_{}", i.name)
}

impl Cx<'_> {
    /// 消去フィールドを別の名前に束縛したパターン（`@x` ⇒ `__eyk_erased_x: __eyk_pat_x`）。
    pub fn pat_fresh(&self, p: &ast::Pat) -> String {
        self.pat_m(p, true)
    }

    /// [`Cx::pat_fresh`] で別の名前にした `@x` を `x` に束縛し直す文。`@x` がなければ空。
    pub fn erased_rebind(&self, p: &ast::Pat) -> String {
        let found = p.erased_idents();
        // `|` の各選択肢は同じ名前を束縛するので、一つにまとめる。
        let mut seen = std::collections::BTreeSet::new();
        found
            .into_iter()
            .filter(|i| seen.insert(i.name.clone()))
            .map(|i| {
                let v = fresh_name(i);
                let x = ident(i);
                if self.logic {
                    return format!("let {x} = {v}@; ");
                }
                match self.facts.erased.get(&SpanKey::from(i.span)) {
                    Some(ErasedKind::Owns) => format!("let Tracked({x}) = {v}; "),
                    _ => format!("let Ghost({x}) = {v}; "),
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
