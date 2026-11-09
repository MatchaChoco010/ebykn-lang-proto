//! 文と式の属性（#27、#49、PR #59 のレビュー）：組み込みでない属性の拒否、権限の状態を変えるか発散する
//! 文・式・欄の `#[cfg]`、代入・二項演算・型変換の文の属性、安定版の Rust が許さない位置の式の属性。

use ebykn_syntax::ast::{self, Expr, ExprKind, StmtKind};
use ebykn_syntax::diag::{CheckMsg, Message, Unimpl};
use ebykn_syntax::span::Span;

use super::{Walker, diff};

/// 文の属性を持つ左端の式（[`Expr::leading_attrs`] をたどった先）。
fn leading_node(e: &Expr) -> &Expr {
    if !e.attrs.is_empty() {
        return e;
    }
    match &e.kind {
        ExprKind::Assign { lhs, .. } | ExprKind::Binary { lhs, .. } => leading_node(lhs),
        ExprKind::Cast { expr, .. } | ExprKind::Try(expr) => leading_node(expr),
        ExprKind::Field { base, .. }
        | ExprKind::Index { base, .. }
        | ExprKind::Erased { base, .. } => leading_node(base),
        ExprKind::MethodCall { receiver, .. } => leading_node(receiver),
        _ => e,
    }
}

/// 文の頭の式が、構文解析器が文の属性を左の項に移す形（代入・二項演算・型変換）か。
fn leading_operand(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Assign { .. } | ExprKind::Binary { .. } | ExprKind::Cast { .. }
    )
}

impl Walker<'_> {
    /// 文・式・`let`・match の腕に付ける属性は、Rust の組み込みの属性とツールの属性（`#[rustfmt::skip]`）
    /// だけ（#49。`macros::non_builtin`）。ここに付く属性マクロは Verus でも E0658 で大きく失敗する
    /// （`proc_macro_hygiene` は有効でない。それを有効にする `#![feature(..)]` は内側の属性の検査が
    /// 拒否する）ので、この拒否は二重の守り。ツールの属性は、同じ名前の依存クレートのマクロを指しても
    /// 大きく失敗するだけなので許す。
    pub(super) fn builtin_attrs_only(&mut self, attrs: &[ast::Attr]) {
        for a in attrs {
            if let Some(written) =
                ebykn_syntax::macros::non_builtin(a, ebykn_syntax::macros::AttrPosition::StmtExpr)
            {
                self.diags.push(a.span, Message::AttrPosition { written });
            }
        }
    }

    /// 式の属性：Rust の組み込みのものだけ（[`Walker::builtin_attrs_only`]）で、安定版の Rust が式の属性を
    /// 許す位置（[`Walker::attr_ok`]）か、代入・二項演算・型変換の文の左端（[`Walker::stmt`] が E0003 を
    /// 出した）でなければ E0003。安定版の Rust は、それ以外の位置の式の属性を拒否する（`attributes on
    /// expressions are experimental`）。Verus の toolchain では通るが、Verus なしのビルドで落ちる（PR #59 の
    /// 再確認）。論理文脈の式は Verus なしのビルドで消える。
    /// パターン（クロージャの引数、構造体パターンの欄と `@x`）の `#[cfg]` は E0003（#95）。§6 の検査は cfg を
    /// 評価せずにパターンの名前（`@x` なら権限）を束縛するので、検査が見るパターンとコンパイルされるパターンが
    /// 食い違う。
    pub(super) fn no_cfg_pattern(&mut self, attrs: &[ast::Attr], span: Span) {
        // 拒否リストの名前の cfg は、構文解析の E0001 だけにする。
        if crate::index::has_cfg(attrs) && !attrs.iter().any(ebykn_syntax::denylist::attr_denied) {
            self.diags.push(
                span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::CfgOnPattern)),
            );
        }
    }

    /// `expr` でたどらない部分木（`ident_of` / `deref_root` で名前だけを取る受け手・被演算子・左辺）の属性を
    /// 見る。`e` と、括弧・単項演算・`?`・欄・添字の土台・消去フィールドの土台をたどった先の式。
    pub(super) fn chain_attrs(&mut self, e: &Expr) {
        self.expr_attrs(e);
        self.chain_children_attrs(e);
    }

    /// [`Walker::chain_attrs`] の、`e` 自身を除いた版（`e` の属性はすでに見たとき）。
    pub(super) fn chain_children_attrs(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Paren(x) | ExprKind::Unary { expr: x, .. } | ExprKind::Try(x) => {
                self.chain_attrs(x)
            }
            ExprKind::Field { base, .. }
            | ExprKind::Index { base, .. }
            | ExprKind::Erased { base, .. } => self.chain_attrs(base),
            _ => {}
        }
    }

    pub(super) fn expr_attrs(&mut self, e: &Expr) {
        self.builtin_attrs_only(&e.attrs);
        let ok = std::mem::replace(&mut self.attr_ok, false)
            || self.attr_lead == Some(e as *const Expr as usize);
        if !ok && !self.logic && !e.attrs.is_empty() {
            self.diags.push(
                e.span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::ExprAttr)),
            );
        }
    }

    /// 文。`#[cfg]`（`cfg_attr` の中を含む）の付いた文と式は、§6 の検査が見るコードと、cfg を展開した後に
    /// コンパイルされるコードが違いうる。権限の状態（Γ）を変えるか発散するものに付いていれば E0003 にする
    /// （[`Walker::cfg_guarded`]。#27、PR #59 のレビュー）。式の文の属性は [`Walker::expr`] が見る。
    ///
    /// 代入・二項演算・型変換の文は、どの属性も E0003（構文解析器が文の属性を左の項に移す形で、安定版の Rust
    /// では書けない。`attributes on expressions are experimental`。cfg で消える代入は Verus も拒否する）。
    pub(super) fn stmt(&mut self, s: &ast::Stmt) -> bool {
        match &s.kind {
            StmtKind::Expr { expr, .. }
                if leading_operand(expr) && !expr.leading_attrs().is_empty() && !self.logic =>
            {
                self.diags.push(
                    s.span,
                    Message::Check(CheckMsg::Unimplemented(Unimpl::CfgStmt)),
                );
                let prev = self
                    .attr_lead
                    .replace(leading_node(expr) as *const Expr as usize);
                let d = self.stmt_inner(s);
                self.attr_lead = prev;
                d
            }
            StmtKind::Let(l) if crate::index::has_cfg(&l.attrs) => {
                self.cfg_guarded(s.span, |w| w.stmt_inner(s))
            }
            _ => self.stmt_inner(s),
        }
    }

    /// `#[cfg]` の付いた文・式・欄をたどる `f` を守る：`f` の前後で Γ が変わるか、`f` が発散するなら E0003。
    /// cfg で消えると、§6 が見ないまま権限を捨てる・作るか、§6 が到達しないと見た後ろのコードが実行される。
    pub(super) fn cfg_guarded(&mut self, span: Span, f: impl FnOnce(&mut Self) -> bool) -> bool {
        if !self.cx.linear || self.logic {
            return f(self);
        }
        let before = self.env.clone();
        let diverges = f(self);
        if diverges || !diff(&before, &self.env).is_empty() {
            self.diags.push(
                span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::CfgStmt)),
            );
        }
        diverges
    }
}
