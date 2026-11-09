//! `.eykp` の証明の文（仕様 §5.7）の走査。証明の文は論理の文脈で読む（呼び出し側が
//! `logic` を立てる）。`proof { }` の中の権限の扱いは、`.eykp` を検査するときに決める（#67）。

use ebykn_syntax::ast;
use ebykn_syntax::span::Span;

use super::Walker;

impl Walker<'_> {
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
                binders, body, by, ..
            } => {
                for b in binders {
                    if let ast::BinderKind::In(x) = &b.kind {
                        self.expr(x);
                    }
                }
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
}
