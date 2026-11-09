//! トークン列だけで決まる検査（仕様 §4）。構文解析の成否によらず、ファイル全体を見る。
//!
//! - E0101：接頭辞 `__eyk_` の識別子（ライフタイムとマクロの引数の中を含む）。
//! - E0102：`#[verifier::..]` / `#![verifier::..]`（古い形の `#[verifier(..)]`、`r#verifier`、
//!   先頭の `::`、`cfg_attr(.., verifier::..)` の中を含む）。
//! - 証明の属性（仕様 §5.7 の `ProofAttr`）と Verus の属性。`.eyk` には書けない。
//!
//! [`scan_opaque`] は、Ebykn が中を構文として読まないトークン列（マクロの引数など）から、
//! 証明の構文と Verus の構文に当たる形を字句的に探す。

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

use super::{push, sp};
use crate::diag::Message;

/// 仕様 §5.7 の証明の属性（`.eykp` だけ）。
const PROOF_ATTRS: &[&str] = &[
    "trigger",
    "auto",
    "opaque",
    "rlimit",
    "spinoff_prover",
    "memoize",
];

/// Verus の属性で、Ebykn に対応する書き方がないもの。
const VERUS_ATTRS: &[&str] = &["all_triggers", "via_fn", "when_used_as_spec"];

pub(super) fn scan(ts: &TokenStream) {
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    for (i, tt) in tts.iter().enumerate() {
        match tt {
            TokenTree::Ident(id) => {
                let name = unraw(&id.to_string());
                if name.starts_with("__eyk_") {
                    push(sp(id.span()), Message::ReservedPrefix { ident: name });
                }
            }
            TokenTree::Punct(p) if p.as_char() == '#' => attr(&tts, i),
            TokenTree::Group(g) => scan(&g.stream()),
            _ => {}
        }
    }
}

fn unraw(s: &str) -> String {
    s.strip_prefix("r#").unwrap_or(s).to_string()
}

/// `tts[i]` が `#` のとき、続く属性を検査する。
fn attr(tts: &[TokenTree], i: usize) {
    let mut j = i + 1;
    let inner = matches!(tts.get(j), Some(TokenTree::Punct(p)) if p.as_char() == '!');
    if inner {
        j += 1;
    }
    let Some(TokenTree::Group(g)) = tts.get(j) else {
        return;
    };
    if g.delimiter() != Delimiter::Bracket {
        return;
    }
    let span = sp(tts[i].span()).to(sp(g.span()));
    let body: Vec<TokenTree> = g.stream().into_iter().collect();
    meta(&body, inner, span);
}

/// 属性の中身（`path args`）を検査する。`cfg_attr(条件, 属性, ..)` は中の属性を再帰的に見る
/// （rustc は Verus が属性を読む前に `cfg_attr` を展開する）。
fn meta(body: &[TokenTree], inner: bool, span: crate::span::Span) {
    // 先頭の `::` を飛ばす。
    let mut k = 0;
    while matches!(body.get(k), Some(TokenTree::Punct(p)) if p.as_char() == ':') {
        k += 1;
    }
    let Some(TokenTree::Ident(head)) = body.get(k) else {
        return;
    };
    let name = unraw(&head.to_string());
    if name == "verifier" {
        push(span, Message::VerifierAttr);
    } else if name == "verus" {
        push(
            span,
            Message::VerusOnly {
                written: "#[verus::..]".into(),
            },
        );
    } else if name == "cfg_attr" {
        if let Some(TokenTree::Group(g)) = body.get(k + 1) {
            let args: Vec<TokenTree> = g.stream().into_iter().collect();
            // 最初の引数は条件。残りが属性。
            for a in split_commas(&args).into_iter().skip(1) {
                meta(a, inner, span);
            }
        }
    } else if PROOF_ATTRS.contains(&name.as_str()) {
        let written = if inner {
            format!("#![{name}]")
        } else {
            format!("#[{name}]")
        };
        push(span, Message::ProofSyntax { written });
    } else if VERUS_ATTRS.contains(&name.as_str()) {
        push(
            span,
            Message::VerusOnly {
                written: format!("#[{name}]"),
            },
        );
    }
}

/// 最上位の `,` で分ける（グループの中の `,` は分けない）。
fn split_commas(tts: &[TokenTree]) -> Vec<&[TokenTree]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, tt) in tts.iter().enumerate() {
        if matches!(tt, TokenTree::Punct(p) if p.as_char() == ',') {
            out.push(&tts[start..i]);
            start = i + 1;
        }
    }
    if start < tts.len() {
        out.push(&tts[start..]);
    }
    out
}

/// Verus の構文を処理するマクロ（`verus!` と、Verus と vstd の証明用・内部用のマクロ）。
/// `.eyk` からは呼べない。
pub(super) const VERUS_MACROS: &[&str] = &[
    "verus",
    "verus_",
    "proof",
    "proof_decl",
    "proof_with",
    "calc",
    "assert_by_contradiction",
    "assert_seqs_equal",
    "assert_sets_equal",
    "assert_maps_equal",
    "open_atomic_invariant",
    "open_local_invariant",
    "open_atomic_invariant_in_proof",
    "open_local_invariant_in_proof",
    "struct_with_invariants",
    "atomic_with_ghost",
    "state_machine",
    "tokenized_state_machine",
    "case_on_next",
    "case_on_init",
    "builtin_macros",
    "verus_proof_macro_exprs",
    "verus_exec_macro_exprs",
    "verus_proof_expr",
];

/// Ebykn が中を構文として読まないトークン列から、証明の構文と Verus の構文に当たる形を
/// 探して診断にする。読まないものについて黙らないため（`OPEN.md`）。
pub(super) fn scan_opaque(ts: &TokenStream) {
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    let ident = |tt: Option<&TokenTree>| match tt {
        Some(TokenTree::Ident(i)) => Some(unraw(&i.to_string())),
        _ => None,
    };
    let punct = |tt: Option<&TokenTree>, c: char| matches!(tt, Some(TokenTree::Punct(p)) if p.as_char() == c);
    let group = |tt: Option<&TokenTree>, d: Delimiter| matches!(tt, Some(TokenTree::Group(g)) if g.delimiter() == d);
    for (i, tt) in tts.iter().enumerate() {
        if let TokenTree::Group(g) = tt {
            scan_opaque(&g.stream());
            continue;
        }
        let Some(name) = ident(Some(tt)) else {
            // `&&&` / `|||`。
            if let TokenTree::Punct(p) = tt
                && (p.as_char() == '&' || p.as_char() == '|')
                && p.spacing() == Spacing::Joint
                && punct(tts.get(i + 1), p.as_char())
                && punct(tts.get(i + 2), p.as_char())
                && !punct(tts.get(i.wrapping_sub(1)), p.as_char())
            {
                let s = p.as_char().to_string().repeat(3);
                push(sp(p.span()), Message::VerusOnly { written: s });
            }
            continue;
        };
        let next = tts.get(i + 1);
        let span = sp(tt.span());
        let m = if name == "proof" && group(next, Delimiter::Brace) {
            Some(Message::ProofSyntax {
                written: "proof { .. }".into(),
            })
        } else if ["assert", "reveal", "reveal_with_fuel", "hide"].contains(&name.as_str())
            && group(next, Delimiter::Parenthesis)
        {
            Some(Message::ProofSyntax {
                written: format!("{name}(..)"),
            })
        } else if ["assume", "admit"].contains(&name.as_str())
            && group(next, Delimiter::Parenthesis)
        {
            Some(Message::VerusOnly {
                written: format!("{name}(..)"),
            })
        } else if ["forall", "exists", "choose"].contains(&name.as_str()) && punct(next, '|') {
            Some(Message::VerusOnly {
                written: format!("{name}|x| .."),
            })
        } else if name == "let" && matches!(ident(next).as_deref(), Some("ghost" | "tracked")) {
            Some(Message::VerusOnly {
                written: "let ghost / let tracked".into(),
            })
        } else if (name == "proof" || name == "exec") && ident(next).as_deref() == Some("fn") {
            Some(Message::VerusOnly {
                written: format!("{name} fn"),
            })
        } else if VERUS_MACROS.contains(&name.as_str()) && punct(next, '!') {
            Some(Message::VerusOnly {
                written: format!("{name}!"),
            })
        } else {
            None
        };
        if let Some(m) = m {
            push(span, m);
        }
    }
}
