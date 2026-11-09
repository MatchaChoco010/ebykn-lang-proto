//! `.eykp` の証明の文（仕様 §5.7）の走査。証明の文は論理の文脈で読む。証明は権限と領域を読めるが、
//! 状態は変えられない（E0520。剥がした `.eyk` が同じ権限を持つように。#67）。

use ebykn_syntax::ast;
use ebykn_syntax::diag::CheckMsg;
use ebykn_syntax::span::Span;

use super::Walker;

impl Walker<'_> {
    /// 証明の文を論理の文脈で読み、権限か領域の状態を変えたら E0520 にして、変える前の状態に戻す。
    pub(super) fn proof_stmt_in_logic(&mut self, p: &ast::ProofStmt, span: Span) {
        let before = (
            self.env.clone(),
            self.regions.clone(),
            self.moved_regions.clone(),
        );
        let prev = std::mem::replace(&mut self.logic, true);
        self.proof_stmt(p, span);
        self.logic = prev;
        let mut names: Vec<String> = self
            .env
            .keys()
            .chain(before.0.keys())
            .filter(|k| self.env.get(*k) != before.0.get(*k))
            .map(|k| format!("{k}@"))
            .collect();
        names.extend(
            self.regions
                .symmetric_difference(&before.1)
                .chain(self.moved_regions.symmetric_difference(&before.2))
                .cloned(),
        );
        names.sort();
        names.dedup();
        if !names.is_empty() {
            self.push(span, CheckMsg::PermChangedInProof { names });
        }
        (self.env, self.regions, self.moved_regions) = before;
    }

    pub(super) fn proof_stmt(&mut self, p: &ast::ProofStmt, span: Span) {
        use ast::{AssertBy, ProofStmt};
        match p {
            ProofStmt::Block(b) => {
                self.block(b);
            }
            ProofStmt::Assert { expr, by } => {
                self.expr(expr);
                match by {
                    Some(AssertBy::Block(b)) => {
                        self.block(b);
                    }
                    Some(AssertBy::Prover { requires, .. }) => {
                        self.exprs(requires);
                    }
                    None => {}
                }
            }
            ProofStmt::AssertForall {
                binders,
                triggers,
                body,
                by,
            } => {
                for b in binders {
                    if let ast::BinderKind::In(x) = &b.kind {
                        self.expr(x);
                        if self.is_map(x) {
                            self.facts.map_binders.insert(x.span.into());
                        }
                    }
                }
                self.triggers(triggers);
                self.expr(body);
                self.block(by);
            }
            ProofStmt::SpecLet(l) => {
                self.local(l, span);
            }
            ProofStmt::Reveal { fuel, .. } => {
                if let Some(e) = fuel {
                    self.expr(e);
                }
            }
            ProofStmt::Hide(_) => {}
        }
    }

    /// 量化子の `#![trigger e, ..]` の式。
    pub(super) fn triggers(&mut self, triggers: &[ast::Trigger]) {
        for t in triggers {
            if let ast::Trigger::Exprs { exprs, .. } = t {
                self.exprs(exprs);
            }
        }
    }
}
