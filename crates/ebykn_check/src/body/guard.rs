//! 証明の文の中で、領域・権限を変える道を塞ぐ（E0520。#149 のレビューの B4・B5）。肯定の事実だけを通す。
//!
//! 資源とは、領域の変数、権限 `p@`、`owns` の消去フィールド（受け手の位置では、権限を持つポインタも）。
//! 式の部分木のどこかに資源が出れば、その式は資源を含む。資源を含む式で許すのは、次の 2 つだけ。
//!
//! - `()` / `&` / `&mut` / `*` を外すとちょうど資源になる受け手に、prelude の spec の関数
//!   （[`PRELUDE_SPEC_FNS`]）を呼ぶ形（`r.contains(p)`、`(*r).len()`、`p@.value()`）。結果は spec の値で、
//!   資源を含まない（`r.dom().len()` も通る）。
//! - 索引で解決したクレートの `spec fn` に渡す形。結果は資源を含まない。
//!
//! ほか（どんな呼び出し、代入、マクロ、ブロックの値、`if` / `match` の値など）に資源を含む式が来たら
//! E0520 にする。資源を含む場所への代入（`*r = ..`）、prelude の型（`use .. as` と型の別名をたどって
//! 決める）の spec でない関連関数（`Region::empty()`、`R::remove(r, p)`）、マクロの引数と Rust の
//! トークンのままの式に出る資源の名前も E0520。

use ebykn_syntax::ast::{self, Expr, ExprKind, MacroArgs, StmtKind};
use proc_macro2::{TokenStream, TokenTree};

use super::Walker;
use super::proof::PRELUDE_SPEC_FNS;

impl Walker<'_> {
    /// 証明の文を一つ見る（[`Walker::proof_stmt_in_logic`] から。`proof_ops` が `Some` のとき）。
    pub(super) fn guard_proof_stmt(&mut self, p: &ast::ProofStmt) {
        use ast::{AssertBy, ProofStmt};
        match p {
            ProofStmt::Block(b) => self.guard_block(b),
            ProofStmt::Assert { expr, by } => {
                self.guard_value(expr);
                match by {
                    Some(AssertBy::Block(b)) => self.guard_block(b),
                    Some(AssertBy::Prover { requires, .. }) => {
                        for r in requires {
                            self.guard_value(r);
                        }
                    }
                    None => {}
                }
            }
            ProofStmt::AssertForall {
                binders, body, by, ..
            } => {
                for b in binders {
                    if let ast::BinderKind::In(x) = &b.kind {
                        self.guard_range(x);
                    }
                }
                self.guard_value(body);
                self.guard_block(by);
            }
            ProofStmt::SpecLet(l) => {
                if let Some(i) = &l.init {
                    self.guard_value(i);
                }
            }
            ProofStmt::Reveal { fuel, .. } => {
                if let Some(f) = fuel {
                    self.guard_value(f);
                }
            }
            ProofStmt::Hide(_) => {}
        }
    }

    fn guard_block(&mut self, b: &ast::Block) {
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Let(l) => {
                    if let Some(i) = &l.init {
                        self.guard_value(i);
                    }
                    if let Some(e) = &l.else_ {
                        self.guard_block(e);
                    }
                }
                StmtKind::Expr { expr, .. } => {
                    // 文の値は捨てるので、資源を含む値でもよい（中の操作は見る）。
                    self.taint(expr);
                }
                StmtKind::Take(r) | StmtKind::Put(r) => self.guard_value(&r.index),
                StmtKind::SpecAssign { value, .. } => self.guard_value(value),
                StmtKind::Proof(p) => self.guard_proof_stmt(p),
                StmtKind::Item(_) | StmtKind::Leak(_) | StmtKind::Empty => {}
            }
        }
    }

    /// 量化子の `x in r` の `r`：資源そのもの（`r.contains(x)` に展開する spec の読み出し）は通す。
    fn guard_range(&mut self, e: &Expr) {
        if self.direct_resource(e).is_none() {
            self.guard_value(e);
        }
    }

    /// 値として使う式：資源を含めば E0520。
    fn guard_value(&mut self, e: &Expr) {
        if let Some(name) = self.taint(e) {
            self.proof_op(&name);
        }
    }

    /// 式を見て、資源を含むならその名前を返す。許さない形はここで E0520 にする。
    fn taint(&mut self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Perm(x) => Some(format!("{}@", x.name)),
            ExprKind::Path { qself: None, path } => {
                let x = path.as_ident()?;
                let held = self.env.contains_key(&x.name) || self.known.contains(&x.name);
                (self.is_region(&x.name) || self.guard_ptrs && held).then(|| x.name.clone())
            }
            ExprKind::Erased { base, field } => {
                if self.is_owns(&field.name) {
                    return Some(format!("@{}", field.name));
                }
                self.taint(base)
            }
            // 包むだけの形は、中の資源をそのまま運ぶ。
            ExprKind::Paren(x)
            | ExprKind::Ref { expr: x, .. }
            | ExprKind::Unary {
                op: ast::UnOp::Deref,
                expr: x,
            } => self.taint(x),
            ExprKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let direct = self.direct_resource(receiver);
                let args_t: Vec<String> = args.iter().filter_map(|a| self.taint(a)).collect();
                // 受け手の位置では、ブロックや `if` の値に包んだ権限を持つポインタも資源とみなす。
                let recv_t = if direct.is_some() {
                    None
                } else {
                    let prev = std::mem::replace(&mut self.guard_ptrs, true);
                    let t = self.taint(receiver);
                    self.guard_ptrs = prev;
                    t
                };
                let spec = PRELUDE_SPEC_FNS.contains(&method.name.as_str());
                match (direct, recv_t) {
                    // 資源の受け手に spec の関数：結果は spec の値。
                    (Some(_), _) if spec && args_t.is_empty() => None,
                    (Some(r), _) | (None, Some(r)) => {
                        self.proof_op(&r);
                        None
                    }
                    (None, None) => {
                        if let Some(a) = args_t.into_iter().next() {
                            self.proof_op(&a);
                        }
                        None
                    }
                }
            }
            // `old(x)` / `final(x)` は包むだけ。
            ExprKind::Call { func, args, .. } if is_old_final(func) && args.len() == 1 => {
                self.taint(&args[0])
            }
            // 領域を渡す `uses` は、§6.2 の検査（[`Walker::proof_op`] の呼び出し）が数える。
            ExprKind::Call { func, args, .. } => {
                let args_t: Vec<String> = args.iter().filter_map(|a| self.taint(a)).collect();
                let prelude_other = self.prelude_non_spec(func);
                if let Some(name) = prelude_other {
                    let n = args_t.first().cloned().unwrap_or(name);
                    self.proof_op(&n);
                    return None;
                }
                if let Some(t) = self.taint(func) {
                    self.proof_op(&t);
                    return None;
                }
                if let Some(a) = args_t.into_iter().next()
                    && !self.is_crate_spec(func)
                {
                    self.proof_op(&a);
                }
                None
            }
            ExprKind::Assign { lhs, rhs } => {
                let l = self.taint(lhs);
                let r = self.taint(rhs);
                if let Some(n) = l.or(r) {
                    self.proof_op(&n);
                }
                None
            }
            ExprKind::Binary { op, lhs, rhs, .. } => {
                let l = self.taint(lhs);
                let r = self.taint(rhs);
                if let Some(n) = l.or(r) {
                    // 複合代入と、資源どうしの比較などの値の計算は、どれも許さない形。
                    let _ = op;
                    self.proof_op(&n);
                }
                None
            }
            ExprKind::Index { base, index } => {
                // 論理の `r[p]`（領域の添字）は spec の読み出し。
                let b = self.taint(base);
                let i = self.taint(index);
                match (b, i) {
                    (Some(_), None) if self.direct_resource(base).is_some() => None,
                    (b, i) => {
                        if let Some(n) = b.or(i) {
                            self.proof_op(&n);
                        }
                        None
                    }
                }
            }
            ExprKind::Field { base, .. } => {
                if let Some(n) = self.taint(base) {
                    self.proof_op(&n);
                }
                None
            }
            ExprKind::Block { block, .. } | ExprKind::Unsafe(block) | ExprKind::Const(block) => {
                self.block_value(block)
            }
            ExprKind::If { cond, then, else_ } => {
                self.guard_value(cond);
                let a = self.block_value(then);
                let b = else_.as_ref().and_then(|x| self.taint(x));
                a.or(b)
            }
            ExprKind::Match { expr, arms } => {
                let mut t = self.taint(expr);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.guard_value(g);
                    }
                    t = t.or(self.taint(&arm.body));
                }
                t
            }
            ExprKind::Loop(l) => {
                match &l.kind {
                    ast::LoopKind::While(c) => self.guard_value(c),
                    ast::LoopKind::For { iter, .. } => self.guard_value(iter),
                    ast::LoopKind::Loop => {}
                }
                self.guard_block(&l.body);
                None
            }
            ExprKind::Macro(m) => {
                match &m.args {
                    MacroArgs::Tokens => self.guard_tokens(&m.tokens),
                    MacroArgs::Exprs(v) => {
                        for x in v {
                            self.guard_value(x);
                        }
                    }
                    MacroArgs::Map(kv) => {
                        for (k, v) in kv {
                            self.guard_value(k);
                            self.guard_value(v);
                        }
                    }
                }
                None
            }
            ExprKind::Rust(tk) => {
                self.guard_tokens(&tk.tokens);
                None
            }
            ExprKind::Closure(c) => {
                self.guard_value(&c.body);
                None
            }
            ExprKind::Quant { binders, body, .. } => {
                for b in binders {
                    if let ast::BinderKind::In(x) = &b.kind {
                        self.guard_range(x);
                    }
                }
                self.guard_value(body);
                None
            }
            ExprKind::Borrow { region, body, .. } => {
                // 読む借用（`borrow mut` は E0520 の操作として [`Walker::introduce`] が数える）。
                self.guard_value(&region.index);
                self.guard_block(body);
                None
            }
            ExprKind::Array(v) | ExprKind::Tuple(v) | ExprKind::AndOr { exprs: v, .. } => {
                for x in v {
                    self.guard_value(x);
                }
                None
            }
            ExprKind::Struct(s) => {
                for f in &s.fields {
                    self.guard_value(&f.expr);
                }
                if let Some(Some(r)) = &s.rest {
                    self.guard_value(r);
                }
                None
            }
            ExprKind::Unary { expr, .. }
            | ExprKind::Cast { expr, .. }
            | ExprKind::Try(expr)
            | ExprKind::Let { expr, .. }
            | ExprKind::Repeat { expr, .. } => {
                self.guard_value(expr);
                None
            }
            ExprKind::Return(x) | ExprKind::Break { expr: x, .. } => {
                if let Some(x) = x {
                    self.guard_value(x);
                }
                None
            }
            ExprKind::Range { start, end, .. } => {
                for x in [start, end].into_iter().flatten() {
                    self.guard_value(x);
                }
                None
            }
            ExprKind::Path { .. }
            | ExprKind::Lit(_)
            | ExprKind::Continue { .. }
            | ExprKind::Infer => None,
        }
    }

    /// ブロックの文を見て、値（末尾の式）が資源を含むならその名前。
    fn block_value(&mut self, b: &ast::Block) -> Option<String> {
        let (last, init) = match b.stmts.split_last() {
            Some((
                ast::Stmt {
                    kind: StmtKind::Expr { expr, semi: false },
                    ..
                },
                init,
            )) => (Some(expr), init),
            _ => (None, b.stmts.as_slice()),
        };
        self.guard_block(&ast::Block {
            stmts: init.to_vec(),
            span: b.span,
        });
        last.and_then(|e| self.taint(e))
    }

    /// `()` / `&` / `&mut` / `*` を外すとちょうど資源になる受け手（受け手の位置なので、権限を持つ
    /// ポインタも資源とみなす）。
    fn direct_resource(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Paren(x)
            | ExprKind::Ref { expr: x, .. }
            | ExprKind::Unary {
                op: ast::UnOp::Deref,
                expr: x,
            } => self.direct_resource(x),
            ExprKind::Perm(x) => Some(format!("{}@", x.name)),
            ExprKind::Path { qself: None, path } => {
                let x = path.as_ident()?;
                let held = self.env.contains_key(&x.name) || self.known.contains(&x.name);
                (self.is_region(&x.name) || held).then(|| x.name.clone())
            }
            ExprKind::Erased { field, .. } if self.is_owns(&field.name) => {
                Some(format!("@{}", field.name))
            }
            ExprKind::Call { func, args, .. } if is_old_final(func) && args.len() == 1 => {
                self.direct_resource(&args[0])
            }
            _ => None,
        }
    }

    fn is_region(&self, name: &str) -> bool {
        self.regions.contains(name) || self.region_modes.contains_key(name)
    }

    /// クレートのどれかの構造体の `owns` にある名前（型を決めずに、多めに数える）。
    fn is_owns(&self, field: &str) -> bool {
        self.cx
            .index
            .structs
            .values()
            .any(|s| s.erased_kind(field) == Some(crate::index::ErasedKind::Owns))
    }

    /// 呼び出しの頭が、索引で解決したクレートの `spec fn` か。
    fn is_crate_spec(&self, func: &Expr) -> bool {
        let ExprKind::Path { qself: None, path } = &func.kind else {
            return false;
        };
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        let r = self
            .cx
            .index
            .resolve(self.cx.module, path.leading_colon, &segs, self.cx.self_ty);
        self.cx
            .index
            .callee(&r)
            .is_some_and(|f| f.kind == crate::index::FnKind::Spec)
    }

    /// 呼び出しの頭が prelude の型の spec でない関連関数（`Region::empty`、`R::remove`）なら、その名前。
    /// 型は索引で解決する（`use .. as` と型の別名をたどる）。
    fn prelude_non_spec(&self, func: &Expr) -> Option<String> {
        let ExprKind::Path { qself: None, path } = &func.kind else {
            return None;
        };
        let (last, prefix) = path.segments.split_last()?;
        if prefix.is_empty() || PRELUDE_SPEC_FNS.contains(&last.ident.name.as_str()) {
            return None;
        }
        let segs: Vec<String> = prefix.iter().map(|s| s.ident.name.clone()).collect();
        let full: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        let ty = self
            .cx
            .index
            .resolve(self.cx.module, path.leading_colon, &segs, self.cx.self_ty);
        let whole =
            self.cx
                .index
                .resolve(self.cx.module, path.leading_colon, &full, self.cx.self_ty);
        let prelude = matches!(ty, crate::index::Resolved::PreludeType(_))
            || matches!(whole, crate::index::Resolved::Prelude(_));
        prelude.then(|| full.join("::"))
    }

    /// マクロの引数と Rust のトークンのままの式の中の資源の名前：領域の変数、`x @` の権限、`@ f` の
    /// `owns` のフィールド。
    fn guard_tokens(&mut self, ts: &TokenStream) {
        let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
        for (i, t) in tts.iter().enumerate() {
            match t {
                TokenTree::Group(g) => self.guard_tokens(&g.stream()),
                TokenTree::Ident(id) => {
                    let n = id.to_string();
                    let n = n.strip_prefix("r#").unwrap_or(&n).to_string();
                    let at_after =
                        matches!(tts.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '@');
                    let at_before = i > 0
                        && matches!(tts.get(i - 1), Some(TokenTree::Punct(p)) if p.as_char() == '@');
                    let held = self.env.contains_key(&n) || self.known.contains(&n);
                    let dot_after =
                        matches!(tts.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '.');
                    if self.is_region(&n)
                        || (at_after || dot_after) && held
                        || at_before && self.is_owns(&n)
                    {
                        self.proof_op(&n);
                    }
                }
                _ => {}
            }
        }
    }
}

/// `old` / `final`（裸の名前）か。
fn is_old_final(func: &Expr) -> bool {
    matches!(&func.kind, ExprKind::Path { qself: None, path }
        if path.as_ident().is_some_and(|i| i.is("old") || i.is("final")))
}
