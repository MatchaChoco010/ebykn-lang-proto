//! 構文解析が判定を任せた名前（[`ebykn_syntax::reserved`] の表の名前を最後に持つパス）の検査。
//!
//! パスがクレートの中の定義（関数・型・モジュール・メソッド）に解決するときだけ通し、それ以外
//! （prelude・vstd・外のクレート、解決できないもの）は拒否する。綴りによらず、パスの最後の名前で
//! 判定する（PR #20 のレビュー）。

use ebykn_syntax::ast::{self, ItemKind};
use ebykn_syntax::diag::{CheckMsg, Message};
use ebykn_syntax::reserved::{DeferredName, ReservedKind};
use ebykn_syntax::span::Span;

use crate::Diags;
use crate::index::{Index, Resolved};
use crate::project::Project;

/// 名前がクレートの中の定義を指すか。
pub fn is_crate_def(r: &Resolved) -> bool {
    matches!(
        r,
        Resolved::Fn(_) | Resolved::Type(_) | Resolved::Module(_) | Resolved::Method { .. }
    )
}

/// 表の名前を最後に持つパスを拒否する診断。
pub fn message(kind: ReservedKind, name: &str, written: &str) -> Message {
    match kind {
        ReservedKind::ProofCall => Message::ProofSyntax {
            written: written.to_string(),
        },
        ReservedKind::VerusCall => Message::VerusBuiltin {
            name: name.to_string(),
            written: written.to_string(),
        },
        ReservedKind::VerusMacro => Message::VerusOnly {
            written: written.to_string(),
        },
        ReservedKind::EbyknTrait => Message::Check(CheckMsg::EbyknTraitSpelling {
            written: written.to_string(),
        }),
    }
}

/// 全ファイルの任された名前を検査する。
pub fn check(project: &Project, index: &Index, diags: &mut Diags) {
    for m in &project.modules {
        let Some(ast) = project.ast(m.file) else {
            continue;
        };
        let mut mods = Vec::new();
        inline_mods(&ast.items, &m.module, &mut mods);
        for (span, name) in &project.deferred[m.file.0 as usize] {
            let module = mods
                .iter()
                .filter(|(s, _)| s.lo <= span.lo && span.hi <= s.hi)
                .max_by_key(|(_, p)| p.len())
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| m.module.clone());
            check_one(index, &module, *span, name, diags);
        }
    }
}

fn check_one(index: &Index, module: &[String], span: Span, n: &DeferredName, diags: &mut Diags) {
    let r = index.resolve(module, false, &n.segments, None);
    if is_crate_def(&r) {
        return;
    }
    let last = n.segments.last().map(String::as_str).unwrap_or_default();
    diags.push(span, message(n.kind, last, &n.written));
}

/// インラインの `mod` の位置とモジュールのパス。
fn inline_mods(items: &[ast::Item], module: &[String], out: &mut Vec<(Span, Vec<String>)>) {
    for item in items {
        if let ItemKind::Mod(m) = &item.kind
            && let Some((_, inner)) = &m.content
        {
            let mut p = module.to_vec();
            p.push(m.ident.name.clone());
            out.push((item.span, p.clone()));
            inline_mods(inner, &p, out);
        }
    }
}
