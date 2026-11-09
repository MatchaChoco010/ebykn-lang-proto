//! 消去フィールドのパターン `@x` の束縛を、書き下しが束縛し直す前の位置で使うもの（#152）。
//!
//! Verus は `Tracked(x)` / `Ghost(x)` のパターンを実行文脈の `let` の文でしか受けないので、書き下しは
//! ほかの位置（match の腕、`if let` / `while let`、`for`、クロージャと関数の引数）の `@x` を別の名前に
//! 束縛し、腕・本体の先頭で `x` に束縛し直す（`ebykn_lower` の `erased_pat.rs`）。その前に評価される
//! 位置（match のガード、`&&` で続く条件、`for` のループ句、関数とクロージャの契約）では `x` がまだない
//! ので、そこで `x` を使うものは E0003 にする。
//!
//! 使っているかは、名前が式の中に現れるかで決める（[`mentions`]）。内側で同じ名前を束縛し直したものや、
//! マクロの引数の中のトークンも「使う」に数える（多めに拒否する。黙って通さない）。

use ebykn_syntax::ast::{
    self, Block, Expr, ExprKind, FieldInit, Ident, Macro, ProofStmt, Region, RegionArg,
    RegionIndex, Stmt, StmtKind, Trigger,
};
use ebykn_syntax::diag::{CheckMsg, Message, Unimpl};
use ebykn_syntax::span::Span;
use proc_macro2::{TokenStream, TokenTree};

use super::Walker;

impl Walker<'_> {
    /// `pats` の `@x` の名前を `exprs` が使っていれば、最初の位置に `what`（E0003）。
    pub(super) fn erased_before_rebind<'e>(
        &mut self,
        pats: &[&ast::Pat],
        exprs: impl IntoIterator<Item = &'e Expr>,
        what: Unimpl,
    ) {
        let names: Vec<&str> = pats
            .iter()
            .flat_map(|p| p.erased_idents())
            .map(|i| i.name.as_str())
            .collect();
        if names.is_empty() {
            return;
        }
        if let Some(span) = exprs.into_iter().find_map(|e| mentions(e, &names)) {
            self.diags
                .push(span, Message::Check(CheckMsg::Unimplemented(what)));
        }
    }

    /// `if` / `while` の条件：`&&` で続く条件が、前の `let` の `@x` を使うもの。
    pub(super) fn erased_in_cond(&mut self, cond: &Expr) {
        let ops = cond.and_chain();
        for (i, e) in ops.iter().enumerate() {
            if let ExprKind::Let { pat, .. } = &e.kind {
                self.erased_before_rebind(
                    &[pat],
                    ops[i + 1..].iter().copied(),
                    Unimpl::ErasedBindInCond,
                );
            }
        }
    }

    /// 関数・クロージャの引数の `@x` を、その契約の式が使うもの。
    pub(super) fn erased_in_contract(&mut self, pats: &[&ast::Pat], contract: &ast::Contract) {
        self.erased_before_rebind(pats, contract_exprs(contract), Unimpl::ErasedBindInContract);
    }
}

/// 契約の句の式（`requires` / `ensures` / `returns` / `decreases`）。
fn contract_exprs(c: &ast::Contract) -> impl Iterator<Item = &Expr> {
    c.clauses.iter().flat_map(|c| match &c.kind {
        ast::ClauseKind::Requires(es)
        | ast::ClauseKind::Ensures(es)
        | ast::ClauseKind::Decreases(es) => es.iter().collect::<Vec<_>>(),
        ast::ClauseKind::Returns(e) => vec![e],
        ast::ClauseKind::Uses(_)
        | ast::ClauseKind::Gives(_)
        | ast::ClauseKind::NoPanic
        | ast::ClauseKind::By(_) => Vec::new(),
    })
}

/// `e` の中で `names` のどれかが名前として現れる最初の位置。欄の名前（`x.f`、`x@f`、リテラルの `f: e`）と
/// メソッドの名前は数えない。
pub(crate) fn mentions(e: &Expr, names: &[&str]) -> Option<Span> {
    let mut m = Mentions { names, found: None };
    m.expr(e);
    m.found
}

struct Mentions<'n> {
    names: &'n [&'n str],
    found: Option<Span>,
}

impl Mentions<'_> {
    fn ident(&mut self, i: &Ident) {
        if self.found.is_none() && self.names.contains(&i.name.as_str()) {
            self.found = Some(i.span);
        }
    }

    fn tokens(&mut self, ts: &TokenStream, span: Span) {
        for t in ts.clone() {
            match t {
                TokenTree::Group(g) => self.tokens(&g.stream(), span),
                TokenTree::Ident(i) => {
                    let s = i.to_string();
                    let s = s.strip_prefix("r#").unwrap_or(&s);
                    if self.found.is_none() && self.names.contains(&s) {
                        self.found = Some(span);
                    }
                }
                TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }

    fn exprs<'e>(&mut self, es: impl IntoIterator<Item = &'e Expr>) {
        for e in es {
            self.expr(e);
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn local(&mut self, l: &ast::Local) {
        self.exprs(&l.init);
        if let Some(b) = &l.else_ {
            self.block(b);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let(l) => self.local(l),
            StmtKind::Expr { expr, .. } => self.expr(expr),
            StmtKind::Take(ri) | StmtKind::Put(ri) => self.region_index(ri),
            StmtKind::Leak(i) => self.ident(i),
            StmtKind::SpecAssign { target, value } => self.exprs([target, value]),
            StmtKind::Proof(p) => match p {
                ProofStmt::Block(b) => self.block(b),
                ProofStmt::Assert { expr, by } => {
                    self.expr(expr);
                    match by {
                        Some(ast::AssertBy::Block(b)) => self.block(b),
                        Some(ast::AssertBy::Prover { requires, .. }) => self.exprs(requires),
                        None => {}
                    }
                }
                ProofStmt::AssertForall {
                    binders,
                    triggers,
                    body,
                    by,
                } => {
                    self.binders(binders);
                    self.triggers(triggers);
                    self.expr(body);
                    self.block(by);
                }
                ProofStmt::SpecLet(l) => self.local(l),
                ProofStmt::Reveal { fuel, .. } => self.exprs(fuel),
                ProofStmt::Hide(_) => {}
            },
            // 関数の本体の中の項目は、外の局所の名前を見ない。
            StmtKind::Item(_) | StmtKind::Empty => {}
        }
    }

    fn binders(&mut self, bs: &[ast::Binder]) {
        for b in bs {
            if let ast::BinderKind::In(e) = &b.kind {
                self.expr(e);
            }
        }
    }

    fn triggers(&mut self, ts: &[Trigger]) {
        for t in ts {
            if let Trigger::Exprs { exprs, .. } = t {
                self.exprs(exprs);
            }
        }
    }

    fn region(&mut self, r: &Region) {
        match r {
            Region::Var(i) => self.ident(i),
            Region::SelfField { .. } => {}
        }
    }

    fn region_index(&mut self, ri: &RegionIndex) {
        self.region(&ri.region);
        self.expr(&ri.index);
    }

    fn region_args(&mut self, us: &[RegionArg]) {
        for u in us {
            self.region(&u.region);
        }
    }

    fn mac(&mut self, m: &Macro) {
        // 引数を式として読むマクロ（`seq!` など）も、トークンで見る（トークンは式を含む）。
        self.tokens(&m.tokens, m.span);
    }

    fn fields(&mut self, fs: &[FieldInit]) {
        for f in fs {
            self.expr(&f.expr);
        }
    }

    fn expr(&mut self, e: &Expr) {
        if self.found.is_some() {
            return;
        }
        match &e.kind {
            ExprKind::Array(es) | ExprKind::Tuple(es) => self.exprs(es),
            ExprKind::Repeat { expr, len } => self.exprs([&**expr, &**len]),
            ExprKind::Assign { lhs, rhs } | ExprKind::Binary { lhs, rhs, .. } => {
                self.exprs([&**lhs, &**rhs])
            }
            ExprKind::Unary { expr, .. }
            | ExprKind::Ref { expr, .. }
            | ExprKind::Cast { expr, .. }
            | ExprKind::Paren(expr)
            | ExprKind::Try(expr)
            | ExprKind::Field { base: expr, .. }
            | ExprKind::Erased { base: expr, .. } => self.expr(expr),
            ExprKind::Block { block, .. } | ExprKind::Unsafe(block) | ExprKind::Const(block) => {
                self.block(block)
            }
            ExprKind::Break { expr, .. } | ExprKind::Return(expr) => {
                self.exprs(expr.iter().map(|x| &**x))
            }
            ExprKind::Continue { .. } | ExprKind::Lit(_) | ExprKind::Infer => {}
            ExprKind::Call { func, args, uses } => {
                self.expr(func);
                self.exprs(args);
                self.region_args(uses);
            }
            ExprKind::MethodCall {
                receiver,
                args,
                uses,
                ..
            } => {
                self.expr(receiver);
                self.exprs(args);
                self.region_args(uses);
            }
            ExprKind::Closure(c) => {
                self.exprs(contract_exprs(&c.contract));
                self.expr(&c.body);
            }
            ExprKind::Index { base, index } => self.exprs([&**base, &**index]),
            ExprKind::If { cond, then, else_ } => {
                self.expr(cond);
                self.block(then);
                self.exprs(else_.iter().map(|x| &**x));
            }
            ExprKind::Let { expr, .. } => self.expr(expr),
            ExprKind::Loop(l) => {
                match &l.kind {
                    ast::LoopKind::While(c) => self.expr(c),
                    ast::LoopKind::For { iter, .. } => self.expr(iter),
                    ast::LoopKind::Loop => {}
                }
                for c in &l.clauses {
                    self.exprs(&c.exprs);
                }
                self.block(&l.body);
            }
            ExprKind::Match { expr, arms } => {
                self.expr(expr);
                for a in arms {
                    self.exprs(&a.guard);
                    self.expr(&a.body);
                }
            }
            ExprKind::Macro(m) => self.mac(m),
            ExprKind::Path { qself: None, path } => {
                if let Some(i) = path.as_ident() {
                    self.ident(i);
                }
            }
            ExprKind::Path { qself: Some(_), .. } => {}
            ExprKind::Range { start, end, .. } => {
                self.exprs(start.iter().chain(end.iter()).map(|x| &**x))
            }
            ExprKind::Struct(lit) => {
                self.fields(&lit.fields);
                if let Some(Some(b)) = &lit.rest {
                    self.expr(b);
                }
            }
            ExprKind::Rust(tk) => self.tokens(&tk.tokens, tk.span),
            ExprKind::Quant {
                binders,
                triggers,
                body,
                ..
            } => {
                self.binders(binders);
                self.triggers(triggers);
                self.expr(body);
            }
            ExprKind::AndOr { exprs, .. } => self.exprs(exprs),
            ExprKind::Perm(i) => self.ident(i),
            ExprKind::Borrow { region, body, .. } => {
                self.region_index(region);
                self.block(body);
            }
        }
    }
}
