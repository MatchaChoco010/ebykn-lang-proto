//! トークン列だけで決まる検査（仕様 §4）。構文解析の成否によらず、ファイル全体を見る。
//!
//! - E0101：接頭辞 `__eyk_` の識別子（ライフタイムとマクロの引数の中を含む）。
//! - E0102：`#[verifier::..]` / `#![verifier::..]`（古い形の `#[verifier(..)]`、`r#verifier`、
//!   先頭の `::`、`cfg_attr(.., verifier::..)` の中を含む）。
//! - 証明の属性（仕様 §5.7 の `ProofAttr`）と Verus の属性。`.eyk` には書けない。
//!
//! [`scan_opaque`] は、Ebykn が中を構文として読まないトークン列（マクロの引数など）から、
//! 証明の構文と Verus の構文に当たる形を字句的に探す。

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

/// [`PROOF_ATTRS`] のうち、外部属性（`#[..]`）として書けるもの。
const PROOF_OUTER_ATTRS: &[&str] = &["trigger", "opaque", "rlimit", "spinoff_prover", "memoize"];

/// [`PROOF_ATTRS`] のうち、内部属性（`#![..]`）として書けるもの。量化子の本体の前に置く。
const PROOF_INNER_ATTRS: &[&str] = &["trigger", "auto"];

/// Verus の属性で、Ebykn に対応する書き方がないもの。
const VERUS_ATTRS: &[&str] = &["all_triggers", "via_fn", "when_used_as_spec"];

pub(super) fn scan(ts: &TokenStream) {
    scan_in(ts, false);
}

/// `use_group` は、`use` の木の `{ }` のグループの中か。マクロの引数の中の `use` は
/// 構文として読まないので、ここで `use {vstd as v};` のグループの先頭の名前を見る。
fn scan_in(ts: &TokenStream, use_group: bool) {
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    // `use` から `;` までの間か（`use<'a>` の精密な捕捉は除く）。
    let mut in_use = false;
    for (i, tt) in tts.iter().enumerate() {
        match tt {
            TokenTree::Ident(id) if id == "use" => {
                in_use = !matches!(tts.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '<');
            }
            TokenTree::Punct(p) if p.as_char() == ';' => in_use = false,
            _ => {}
        }
        match tt {
            TokenTree::Ident(id) => {
                let name = unraw(&id.to_string());
                if name.starts_with("__eyk_") {
                    push(
                        sp(id.span()),
                        Message::ReservedPrefix {
                            ident: name.clone(),
                        },
                    );
                }
                let group_head = use_group
                    && (i == 0
                        || matches!(tts.get(i - 1), Some(TokenTree::Punct(p)) if p.as_char() == ','));
                if refers_to_crate(&tts, i, &name)
                    || group_head && FORBIDDEN_CRATES.contains(&name.as_str())
                {
                    push(sp(id.span()), Message::VstdDirect { name });
                }
            }
            TokenTree::Punct(p) if p.as_char() == '#' => attr(&tts, i),
            TokenTree::Group(g) => {
                let inner_use = (in_use || use_group) && g.delimiter() == Delimiter::Brace;
                scan_in(&g.stream(), inner_use)
            }
            _ => {}
        }
    }
}

/// `.eyk` が直接参照しないクレート（#16 の既定）。`.eyk` は vstd の名前（`Seq`、`seq!` など）を
/// 裸の名前で書き、生成する各 `.rs` の先頭の `use vstd::prelude::*;`（仕様 §7.5）で解決する。
pub(super) const FORBIDDEN_CRATES: &[&str] =
    &["vstd", "verus_builtin", "verus_builtin_macros", "builtin"];

/// `tts[i]`（名前 `name`）が、参照してはいけないクレートをパスとして指しているか。
/// パスの部分（`vstd::x`、`::vstd`、`a::vstd`）、`use vstd`、`extern crate vstd` を見る。
/// `builtin` はよくある語なので、パスの先頭（`builtin::x`）と `use` / `extern crate` だけを見る。
fn refers_to_crate(tts: &[TokenTree], i: usize, name: &str) -> bool {
    if !FORBIDDEN_CRATES.contains(&name) {
        return false;
    }
    let colon = |j: usize| matches!(tts.get(j), Some(TokenTree::Punct(p)) if p.as_char() == ':');
    let kw = |j: Option<usize>, k: &str| matches!(j.and_then(|j| tts.get(j)), Some(TokenTree::Ident(x)) if x == k);
    let next_path = colon(i + 1) && colon(i + 2);
    let prev_path = i >= 2 && colon(i - 1) && colon(i - 2);
    let prev_path_mid = prev_path && i >= 3 && matches!(tts.get(i - 3), Some(TokenTree::Ident(_)));
    let after_kw = kw(i.checked_sub(1), "use") || kw(i.checked_sub(1), "crate");
    // `a.builtin::<u8>()` はメソッドの呼び出し。
    // `0..builtin::x()` の `..` はメソッドの `.` ではない。
    let dot = |j: Option<usize>| matches!(j.and_then(|j| tts.get(j)), Some(TokenTree::Punct(p)) if p.as_char() == '.');
    let after_dot = dot(i.checked_sub(1)) && !dot(i.checked_sub(2));
    if name == "builtin" {
        next_path && !prev_path_mid && !after_dot || after_kw
    } else {
        next_path || prev_path || after_kw
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
    // 内側の属性は Rust の組み込みのものだけ（#49。`#![feature]` と内側の属性マクロ）。Verus と
    // 証明の属性は `meta` が別の文言で拒否する。
    if inner
        && !diagnosed_by_meta(&body)
        && let Some(written) =
            crate::macros::non_builtin_in_tokens(&body, crate::macros::AttrPosition::Inner)
    {
        push(span, Message::AttrPosition { written });
    }
    meta(&body, inner, span);
}

/// [`meta`] が自分の文言で拒否する属性（Verus と証明の属性）か。
fn diagnosed_by_meta(body: &[TokenTree]) -> bool {
    let names: Vec<String> = body
        .iter()
        .take_while(|t| matches!(t, TokenTree::Ident(_) | TokenTree::Punct(_)))
        .filter_map(|t| match t {
            TokenTree::Ident(i) => Some(unraw(&i.to_string())),
            _ => None,
        })
        .collect();
    let Some(first) = names.first() else {
        return false;
    };
    let last = names.last().expect("空でない");
    matches!(first.as_str(), "verifier" | "verus")
        || PROOF_ATTRS.contains(&first.as_str())
        || VERUS_ATTRS.contains(&first.as_str())
        || last == "verus_verify"
        || last == "verus_spec"
}

/// 属性の中身（`path args`）を検査する。`cfg_attr(条件, 属性, ..)` は中の属性を再帰的に見る
/// （rustc は Verus が属性を読む前に `cfg_attr` を展開する）。
fn meta(body: &[TokenTree], inner: bool, span: crate::span::Span) {
    // 先頭の `::` を飛ばし、パス（`a::b::c`）の名前を集める。
    let mut k = 0;
    while matches!(body.get(k), Some(TokenTree::Punct(p)) if p.as_char() == ':') {
        k += 1;
    }
    let Some(TokenTree::Ident(head)) = body.get(k) else {
        return;
    };
    let name = unraw(&head.to_string());
    let mut last = name.clone();
    let mut end = k + 1;
    while matches!(body.get(end), Some(TokenTree::Punct(p)) if p.as_char() == ':')
        && matches!(body.get(end + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
        && let Some(TokenTree::Ident(seg)) = body.get(end + 2)
    {
        last = unraw(&seg.to_string());
        end += 3;
    }
    // 属性の値の中のマクロの呼び出しと証明の構文（`#[doc = evil!()]`）。`cfg_attr` は下で
    // 中の属性ごとに見る。
    if name != "cfg_attr" {
        let rest: TokenStream = body[end..].iter().cloned().collect();
        scan_opaque(&rest);
    }
    // Verus の属性はパスの最後の名前で見る（`#[vstd::prelude::verus_verify]`）。
    if last == "verus_verify" || last == "verus_spec" {
        push(
            span,
            Message::VerusOnly {
                written: format!("#[{last}]"),
            },
        );
        return;
    }
    if name == "verifier" {
        push(span, Message::VerifierAttr);
    } else if name == "verus" {
        push(
            span,
            Message::VerusOnly {
                written: "#[verus::..]".into(),
            },
        );
    } else if name == "path" || name == "macro_use" {
        // `#[path]` は別のファイルを、`#[macro_use]` は外のマクロを取り込む（#16 の既定）。
        push(
            span,
            Message::Unread {
                written: format!("#[{name}]"),
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
        let allowed = if inner {
            PROOF_INNER_ATTRS
        } else {
            PROOF_OUTER_ATTRS
        };
        if !super::in_proof() {
            push(span, Message::ProofSyntax { written });
        } else if !allowed.contains(&name.as_str()) {
            push(span, Message::NotAllowedHere { written });
        }
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

/// Verus の構文を処理するマクロ。`.eyk` からは呼べない（表は [`crate::reserved`]）。
pub(super) const VERUS_MACROS: &[&str] = crate::reserved::VERUS_MACROS;

/// マクロのパス（`r#` を外した名前の列）が `.eyk` で呼べるものか（#16 の既定）。
/// std のマクロは裸の名前か std / core / alloc から始まるパス、vstd のマクロは裸の名前だけ。
pub(super) fn macro_allowed(names: &[String]) -> Option<MacroKind> {
    let last = names.last()?.as_str();
    let first = names.first()?.as_str();
    let vstd = crate::VSTD_MACROS.contains(&last);
    let std = crate::ALLOWED_MACROS.contains(&last) && !vstd;
    // vstd のマクロは `ebykn_prelude` が再公開する裸の名前だけ（vstd は直接参照しない）。
    match names.len() {
        1 if vstd => Some(MacroKind::Vstd),
        1 if std => Some(MacroKind::Std),
        _ if std && matches!(first, "std" | "core" | "alloc") => Some(MacroKind::Std),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MacroKind {
    /// std のマクロ。引数は読まずに字句的に検査する。
    Std,
    /// vstd の仕様用のマクロ。引数を Ebykn の式として読む。
    Vstd,
}

/// マクロの名前の位置に来ても、マクロの呼び出しではない語（`if !(x)` など）。
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "type", "unsafe", "use",
    "where", "while", "yield", "box",
];

/// Ebykn が中を構文として読まないトークン列から、証明の構文と Verus の構文に当たる形と、
/// `.eyk` で呼べないマクロの呼び出しを探して診断にする。読まないものについて黙らないため
/// （`docs/current-spec.md`）。
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
        let prev = i.checked_sub(1).and_then(|j| tts.get(j));
        // `&&&` / `|||` は探さない。`&&` / `||` と同じ意味で証明を持ち込まず、`&&&x`（参照の参照の
        // 参照）と字句では見分けられない。Ebykn の式として読む位置では構文として拒否する。
        let Some(name) = ident(Some(tt)) else {
            continue;
        };
        // `fn hide(..)` の `hide` は宣言する名前で、呼び出しではない。
        if ident(prev).as_deref() == Some("fn") {
            continue;
        }
        let next = tts.get(i + 1);
        let span = sp(tt.span());
        let m = if name == "proof" && group(next, Delimiter::Brace) {
            // `.eykp` でも、中を読まないトークン列の中の証明は読めないので拒否する。
            let written = "proof { .. }".to_string();
            Some(if super::in_proof() {
                Message::ProofStmtOnly { written }
            } else {
                Message::ProofSyntax { written }
            })
        } else if crate::reserved::kind_of(&name).is_some_and(crate::reserved::is_call)
            && !punct(prev, '.')
            && !punct(next, '!')
            // 構造体のフィールドの名前（`S { requires: 1 }`）は参照ではない。
            && (!punct(next, ':') || punct(tts.get(i + 2), ':'))
            // パスの途中（`requires::x`）は最後の名前ではない。ターボフィッシュ（`requires::<T>`）は見る。
            && (!punct(next, ':') || !punct(tts.get(i + 2), ':') || punct(tts.get(i + 3), '<'))
            // 裸の名前の値（ローカル変数かもしれない）は見ない。呼び出しとパス付きの綴りを見る。
            && (group(next, Delimiter::Parenthesis)
                || punct(next, ':')
                || (punct(prev, ':') && i >= 2 && punct(tts.get(i - 2), ':')))
        {
            // 前の `a::b::` を集める。
            let mut segs = vec![name.clone()];
            let mut j = i;
            while j >= 2 && punct(tts.get(j - 1), ':') && punct(tts.get(j - 2), ':') {
                match j.checked_sub(3).and_then(|k| ident(tts.get(k))) {
                    Some(n) => {
                        segs.insert(0, n);
                        j -= 3;
                    }
                    None => break,
                }
            }
            let leading = j >= 2 && punct(tts.get(j - 1), ':') && punct(tts.get(j - 2), ':');
            let written = format!("{}{}(..)", if leading { "::" } else { "" }, segs.join("::"));
            // 中を読まないトークン列の中は名前解決しないので、どの綴りも拒否する（`docs/current-spec.md`）。
            super::reserved_path(
                std::slice::from_ref(&name),
                true,
                crate::reserved::is_call,
                written,
                span,
            );
            None
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
        } else if punct(next, '!')
            && !KEYWORDS.contains(&name.as_str())
            && matches!(tts.get(i + 2), Some(TokenTree::Group(_)))
        {
            // マクロの呼び出し。前のパスを集めて判定する。
            let mut names = vec![name.clone()];
            let mut j = i;
            while j >= 3 && punct(tts.get(j - 1), ':') && punct(tts.get(j - 2), ':') {
                match ident(tts.get(j - 3)) {
                    Some(n) => {
                        names.insert(0, n);
                        j -= 3;
                    }
                    None => break,
                }
            }
            macro_call_message(&names)
        } else {
            None
        };
        if let Some(m) = m {
            push(span, m);
        }
    }
}

/// マクロの呼び出しが `.eyk` で呼べなければ、その診断。
pub(super) fn macro_call_message(names: &[String]) -> Option<Message> {
    let last = names.last()?;
    if VERUS_MACROS.contains(&last.as_str()) {
        Some(Message::VerusOnly {
            written: format!("{last}!"),
        })
    } else if last == "cfg"
        && matches!(
            names.first().map(String::as_str),
            Some("cfg" | "std" | "core")
        )
    {
        // `cfg!` は Verus のときと Verus なしのときで違う値になり、検証したコードとビルドするコードが
        // 食い違う（#27 の既定 B）。
        Some(Message::Check(crate::diag::CheckMsg::Unimplemented(
            crate::diag::Unimpl::CfgInBody,
        )))
    } else if matches!(last.as_str(), "env" | "option_env" | "file")
        && matches!(
            names.first().map(String::as_str),
            Some("env" | "option_env" | "file" | "std" | "core")
        )
    {
        // コンパイル時の環境変数と、cargo が rustc に渡すファイルのパスは、Verus の実行と素のビルドで
        // 値が違いうる（#27 の既定）。証明なしの前提として一覧に出る `unchecked fn` の本体でだけ許す。
        let u = if last == "file" {
            crate::diag::Unimpl::FileMacro
        } else {
            crate::diag::Unimpl::EnvMacro
        };
        (!super::in_unchecked_body())
            .then_some(Message::Check(crate::diag::CheckMsg::Unimplemented(u)))
    } else if macro_allowed(names).is_none() {
        Some(Message::MacroNotAllowed {
            name: names.join("::"),
        })
    } else {
        None
    }
}
