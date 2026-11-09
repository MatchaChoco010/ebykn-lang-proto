//! トークン列だけで決まる検査（仕様 §4）。構文解析の成否によらず、ファイル全体を見る。
//!
//! - E0101：接頭辞 `__eyk_` の識別子（ライフタイムとマクロの引数の中を含む）。
//! - E0102：`#[verifier::..]` / `#![verifier::..]`（古い形の `#[verifier(..)]` を含む）。
//! - 証明の属性（仕様 §5.7 の `ProofAttr`）と Verus の属性。`.eyk` には書けない。

use proc_macro2::{Delimiter, TokenStream, TokenTree};

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
                let s = id.to_string();
                let name = s.strip_prefix("r#").unwrap_or(&s);
                if name.starts_with("__eyk_") {
                    push(
                        sp(id.span()),
                        Message::ReservedPrefix {
                            ident: name.to_string(),
                        },
                    );
                }
            }
            TokenTree::Punct(p) if p.as_char() == '#' => attr(&tts, i),
            TokenTree::Group(g) => scan(&g.stream()),
            _ => {}
        }
    }
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
    let Some(TokenTree::Ident(head)) = g.stream().into_iter().next() else {
        return;
    };
    let span = sp(tts[i].span()).to(sp(g.span()));
    let name = head.to_string();
    if name == "verifier" {
        push(span, Message::VerifierAttr);
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
