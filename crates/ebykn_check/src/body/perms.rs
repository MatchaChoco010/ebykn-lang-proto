//! 権限の操作（仕様 §6.2 の消費・貸し・導入）。証明の文の中では、権限・領域の状態を変える操作を
//! 記録し、E0520 にする（[`Walker::proof_op`]。#67）。

use ebykn_syntax::diag::{CheckMsg, Op, PermState};
use ebykn_syntax::span::Span;

use super::{Perm, Walker};

impl Walker<'_> {
    /// 証明の文の中なら、権限・領域の状態を変える操作として `name` を記録する。
    pub(super) fn proof_op(&mut self, name: &str) {
        if let Some(ops) = &mut self.proof_ops {
            ops.push(name.to_string());
        }
    }

    /// 権限を消費する（`free`、`put`、移動、`leak`）。
    pub(super) fn consume(&mut self, name: &str, op: Op, span: Span) {
        self.proof_op(&format!("{name}@"));
        if self.closure_use(name, span) {
            return;
        }
        match self.env.get(name).copied() {
            Some(Perm {
                state: PermState::Owned,
                ..
            }) => {
                self.env.remove(name);
            }
            Some(Perm { block: true, .. }) => self.push(
                span,
                CheckMsg::ConsumeBorrowed {
                    name: name.to_string(),
                    op,
                },
            ),
            Some(p) => self.push(
                span,
                CheckMsg::PermNotOwned {
                    name: name.to_string(),
                    op,
                    state: p.state,
                },
            ),
            None => self.push(
                span,
                CheckMsg::PermNotOwned {
                    name: name.to_string(),
                    op,
                    state: PermState::Absent,
                },
            ),
        }
    }

    /// 権限を貸す（`uses &mut p@` / `uses &p@` の引数）。
    pub(super) fn lend(&mut self, name: &str, mutable: bool, span: Span) {
        if mutable {
            self.proof_op(&format!("{name}@"));
        }
        if self.closure_use(name, span) {
            return;
        }
        let op = if mutable {
            Op::MutBorrow
        } else {
            Op::SharedBorrow
        };
        match self.env.get(name).copied() {
            None => self.push(
                span,
                CheckMsg::PermNotOwned {
                    name: name.to_string(),
                    op,
                    state: PermState::Absent,
                },
            ),
            Some(p) if mutable && p.state == PermState::Shared => self.push(
                span,
                CheckMsg::WriteShared {
                    name: name.to_string(),
                },
            ),
            Some(_) => {}
        }
    }

    /// 権限を導入する（`take`、`Ptr::new`、`gives`）。既にあれば E0503。
    pub(super) fn introduce(
        &mut self,
        name: &str,
        state: PermState,
        block: bool,
        span: Span,
    ) -> bool {
        // 読むための借用（`borrow r[p] { }`）は証明の中でも書ける。
        if !(block && state == PermState::Shared) {
            self.proof_op(&format!("{name}@"));
        }
        // クロージャの中では権限を導入することもできない（§5.6、E0512）。
        if self.closure_outer.is_some() {
            self.push(
                span,
                CheckMsg::PermInClosure {
                    name: name.to_string(),
                },
            );
            return false;
        }
        if self.env.contains_key(name) {
            self.push(
                span,
                CheckMsg::PermAlreadyHeld {
                    name: name.to_string(),
                },
            );
            return false;
        }
        self.env.insert(name.to_string(), Perm { state, block });
        self.known.insert(name.to_string());
        self.ever_held.insert(name.to_string());
        true
    }
}
