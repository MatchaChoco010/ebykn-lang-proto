//! `.eykp` の証明の文（仕様 §5.7）の走査。証明の文は論理の文脈で読む。証明は権限と領域を読めるが、
//! 状態を変える操作はできない（E0520）。`spec let` はスコープの名前を覆えない（E0521）。どちらも、剥がした
//! `.eyk` の文が同じ意味を持つように（#67）。

use ebykn_syntax::ast;
use ebykn_syntax::diag::CheckMsg;
use ebykn_syntax::span::Span;

use super::Walker;

/// 証明の中で、領域・権限・権限を持つポインタを受け手に呼べる prelude の spec の関数（明示の許すリスト）。
/// prelude に spec の関数を足したら、ここにも足す（`crates/ebykn_check/tests/eykp_checks.rs` の
/// `every_prelude_fn_is_classified` が、prelude の公開の関数がこのリストか E0520 のどちらかに当たることを
/// 確かめる）。
pub const PRELUDE_SPEC_FNS: &[&str] = &[
    // Region
    "perms",
    "contains",
    "index",
    "dom",
    "len",
    // PointsTo / CellPointsTo
    "pptr",
    "is_init",
    "is_uninit",
    "value",
    "id",
    // Ptr
    "spec_addr",
];

impl Walker<'_> {
    /// 証明の文を論理の文脈で読む。中で権限か領域の状態を変える操作（`take`・`put`・`leak`・`borrow mut`・
    /// `x@f = e`・移動や `&mut` で渡す `uses`・領域の作成）を一度でもしたら E0520 にする（前と後の状態が
    /// 同じでも。取り出して捨てる、領域の間で移す、を通さないため）。読む操作（`borrow r[p] { }`、論理の
    /// `*p` / `r[p]`）は通す。E0520 のあとは、証明の前の状態に戻して続ける。
    pub(super) fn proof_stmt_in_logic(&mut self, p: &ast::ProofStmt, span: Span) {
        let before = (
            self.env.clone(),
            self.regions.clone(),
            self.moved_regions.clone(),
        );
        let outer = self.proof_ops.replace(Vec::new());
        let prev = std::mem::replace(&mut self.logic, true);
        self.proof_stmt(p, span);
        self.logic = prev;
        let mut names = std::mem::replace(&mut self.proof_ops, outer).unwrap_or_default();
        names.sort();
        names.dedup();
        if !names.is_empty() {
            // 入れ子の証明の文の中の操作は、いちばん外の証明の文で一度だけ報告する。
            match &mut self.proof_ops {
                Some(o) => o.extend(names),
                None => self.push(span, CheckMsg::PermChangedInProof { names }),
            }
        }
        (self.env, self.regions, self.moved_regions) = before;
    }

    /// `spec let` が束縛する名前は、新しい名前だけを許す（E0521）：スコープにない（引数・ローカル・
    /// 権限・領域でない）、クレートの `.eyk` に識別子として現れない、索引で何にも解決しない（項目・
    /// prelude の名前でない）。`spec let` は関数の本体の `let ghost` になるので、覆うと、後ろの `.eyk` の文
    /// （`take r[p]` の添字、`x@f = e` の右辺、ループの句など、Verus が ghost として読む位置）の指すものが
    /// 変わる。分からない名前は通さない。
    fn spec_let_shadow(&mut self, l: &ast::Local) {
        let saved = std::mem::take(&mut self.locals);
        self.add_locals(&l.pat);
        let bound = std::mem::replace(&mut self.locals, saved);
        for name in bound {
            let resolves = !matches!(
                self.cx.index.resolve(
                    self.cx.module,
                    false,
                    std::slice::from_ref(&name),
                    self.cx.self_ty
                ),
                crate::index::Resolved::Unknown
            );
            let taken = self.locals.contains(&name)
                || self.env.contains_key(&name)
                || self.known.contains(&name)
                || self.regions.contains(&name)
                || self.var_types.contains_key(&name)
                || self.cx.index.source_idents.contains(&name)
                || resolves;
            if taken {
                self.push(l.pat.span, CheckMsg::SpecLetShadows { name });
            }
        }
    }

    /// 証明の中の呼び出し（B4）：領域（領域の変数、`owns` の消去フィールド）・権限（`p@`）・権限を持つ
    /// ポインタを受け手か引数にする呼び出しは、prelude の spec の関数（[`PRELUDE_SPEC_FNS`]）を受け手に
    /// 呼ぶものと、クレートの `spec fn` に渡すものだけを通す。それ以外（prelude の `proof fn` の
    /// `insert` / `remove` / `borrow` / `empty`、補題、`Region::remove(r, p)` の書き方など）は E0520。
    /// 証明の中でだけ呼ぶ（`proof_ops` が `Some`）。
    pub(super) fn proof_call(
        &mut self,
        receiver: Option<&ast::Expr>,
        method: Option<&ast::Ident>,
        func: Option<&ast::Expr>,
        args: &[ast::Expr],
    ) {
        if self.proof_ops.is_none() {
            return;
        }
        let arg_res: Vec<String> = args
            .iter()
            .filter_map(|a| self.ghost_resource(a, false))
            .collect();
        // `Region::remove(r, p)` のような prelude の型の関連関数。
        if let Some(ast::Expr {
            kind: ast::ExprKind::Path { path, .. },
            ..
        }) = func
            && let [.., ty, name] = path.segments.as_slice()
            && crate::index::PRELUDE_TYPES.contains(&ty.ident.name.as_str())
        {
            if !PRELUDE_SPEC_FNS.contains(&name.ident.name.as_str()) {
                let n = arg_res
                    .first()
                    .cloned()
                    .unwrap_or_else(|| format!("{}::{}", ty.ident.name, name.ident.name));
                self.proof_op(&n);
            }
            return;
        }
        let recv_res = receiver.and_then(|r| self.ghost_resource(r, true));
        if recv_res.is_none() && arg_res.is_empty() {
            return;
        }
        let allowed = match (receiver, method, func) {
            // `r.contains(p)`、`p@.value()`：prelude の spec の関数を受け手に呼ぶ。
            (Some(_), Some(m), _) => {
                recv_res.is_some()
                    && arg_res.is_empty()
                    && PRELUDE_SPEC_FNS.contains(&m.name.as_str())
            }
            // クレートの `spec fn` に渡す（spec の関数は権限を使えない）。
            (
                None,
                _,
                Some(ast::Expr {
                    kind: ast::ExprKind::Path { qself: None, path },
                    ..
                }),
            ) => {
                let segs: Vec<String> =
                    path.segments.iter().map(|s| s.ident.name.clone()).collect();
                let r = self.cx.index.resolve(
                    self.cx.module,
                    path.leading_colon,
                    &segs,
                    self.cx.self_ty,
                );
                self.cx
                    .index
                    .callee(&r)
                    .is_some_and(|f| f.kind == crate::index::FnKind::Spec)
            }
            _ => false,
        };
        if !allowed {
            let n = recv_res
                .or_else(|| arg_res.first().cloned())
                .unwrap_or_default();
            self.proof_op(&n);
        }
    }

    /// 式が領域・権限（受け手なら、権限を持つポインタも）を指すなら、その名前。`&` / `&mut` / 括弧は外す。
    fn ghost_resource(&self, e: &ast::Expr, receiver: bool) -> Option<String> {
        use ast::ExprKind;
        match &e.kind {
            ExprKind::Paren(x) | ExprKind::Ref { expr: x, .. } => self.ghost_resource(x, receiver),
            ExprKind::Perm(x) => Some(format!("{}@", x.name)),
            ExprKind::Path { qself: None, path } => {
                let x = path.as_ident()?;
                let region =
                    self.regions.contains(&x.name) || self.region_modes.contains_key(&x.name);
                let held = self.env.contains_key(&x.name) || self.known.contains(&x.name);
                (region || receiver && held).then(|| x.name.clone())
            }
            // `owns` の消去フィールド（クレートのどれかの構造体の `owns` にある名前なら領域とみなす）。
            ExprKind::Erased { field, .. } => {
                let owns =
                    self.cx.index.structs.values().any(|s| {
                        s.erased_kind(&field.name) == Some(crate::index::ErasedKind::Owns)
                    });
                owns.then(|| format!("@{}", field.name))
            }
            _ => None,
        }
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
                        let prev = std::mem::replace(&mut self.in_assert_by, true);
                        self.block(b);
                        self.in_assert_by = prev;
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
                let prev = std::mem::replace(&mut self.in_assert_by, true);
                self.block(by);
                self.in_assert_by = prev;
            }
            ProofStmt::SpecLet(l) => {
                self.spec_let_shadow(l);
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
