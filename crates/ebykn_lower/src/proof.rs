//! `.eykp` の証明の文の書き下し（仕様 §7.4）。
//!
//! 実行する関数の本体の中の証明の文は `proof { .. }` で包む（`spec let` は `let ghost`）。証明の文脈
//! （`proof { }` の中、`lemma fn` の本体）ではそのまま書く。証明の中は論理の文脈で書き下す。証明が
//! 実行するコードを変えないことは、Ebykn の検査（E0520）と Verus のモードの検査が受け持つ。

use ebykn_syntax::ast::{self, AssertBy, ProofStmt};

use crate::Cx;

impl Cx<'_> {
    pub(crate) fn proof_stmt(&mut self, p: &ProofStmt) -> String {
        let s = match p {
            ProofStmt::Block(b) => {
                let inner = self.proof_block(b);
                return if self.proof {
                    inner
                } else {
                    format!("proof {inner}")
                };
            }
            ProofStmt::Assert { expr, by } => {
                let prev = std::mem::replace(&mut self.logic, true);
                let e = self.expr(expr);
                self.logic = prev;
                match by {
                    None => format!("assert({e});"),
                    Some(AssertBy::Block(b)) => format!("assert({e}) by {};", self.proof_block(b)),
                    Some(AssertBy::Prover { mode, requires, .. }) => {
                        let prev = std::mem::replace(&mut self.logic, true);
                        let req: Vec<String> = requires.iter().map(|r| self.expr(r)).collect();
                        self.logic = prev;
                        if req.is_empty() {
                            format!("assert({e}) by ({});", mode.as_str())
                        } else {
                            format!(
                                "assert({e}) by ({}) requires {};",
                                mode.as_str(),
                                req.join(", ")
                            )
                        }
                    }
                }
            }
            ProofStmt::AssertForall {
                binders,
                triggers,
                body,
                by,
            } => self.assert_forall(binders, triggers, body, by),
            ProofStmt::SpecLet(l) => return self.spec_let(l),
            ProofStmt::Reveal { path, fuel } => match fuel {
                None => format!("reveal({});", self.path(path)),
                Some(f) => {
                    let prev = std::mem::replace(&mut self.logic, true);
                    let n = self.expr(f);
                    self.logic = prev;
                    format!("reveal_with_fuel({}, {n});", self.path(path))
                }
            },
            // Verus の `hide` は関数の本体の先頭に置く見出しで、`proof { }` に包むと受け付けない。
            // 先頭でなければ Verus がエラーにする。
            ProofStmt::Hide(path) => return format!("hide({});", self.path(path)),
        };
        if self.proof {
            s
        } else {
            format!("proof {{ {s} }}")
        }
    }

    /// `spec let p = e;`：実行する関数の中では `let ghost`、証明の文脈では `let`（§7.4）。
    fn spec_let(&mut self, l: &ast::Local) -> String {
        let prev = std::mem::replace(&mut self.logic, true);
        let mut out = format!(
            "let {}{}",
            if self.proof { "" } else { "ghost " },
            self.pat(&l.pat)
        );
        if let Some(t) = &l.ty {
            out.push_str(&format!(": {}", self.ty(t)));
        }
        if let Some(i) = &l.init {
            out.push_str(&format!(" = {}", self.expr(i)));
        }
        self.logic = prev;
        out.push(';');
        out
    }

    /// 関数の本体の中の `use lemmas g;`（§7.2）。
    pub(crate) fn use_lemmas_stmt(&self, p: &ast::Path) -> String {
        self.note_use_lemmas(p, false);
        let s = format!("broadcast use {};", self.path(p));
        if self.proof {
            s
        } else {
            format!("proof {{ {s} }}")
        }
    }
}
