//! `ensures` の中の裸の `&mut` 引数（#10 の決定：案 B）。
//!
//! Verus と同じく、`ensures` で `&mut` 引数を `old(..)` も `final(..)` も付けずに書くことを
//! 許さない。どちらの値か（入口か、呼び出しの後か）を書かせる。対象は、`&mut self`
//! （`self: &mut Self` を含む）、`x: &mut T`、`uses r: &mut Region<..>` の `r` と、
//! `uses &mut p@` の権限（`p@` と、権限を通した読み `*p`）。`requires` の裸の `x` は入口の値
//! なので見ない（Verus と同じ）。§6.4 の展開が足す `ensures P[final(x)]` は利用者の式では
//! ないので、ここには来ない。
//!
//! 量化子・クロージャの引数、`match` の腕、`let` のパターンで束縛した同じ名前は外の引数を
//! 隠すので、報告しない。中を読まないマクロの引数（トークン列）は見ない。そこに裸の `&mut`
//! 引数があれば Verus が同じ理由で拒む（黙っては誤らない）。

use std::collections::BTreeSet;

use ebykn_syntax::ast::{
    self, Block, ExprKind, FieldPat, Param, PatKind, PermMode, StmtKind, TypeKind, UnOp, UsesItem,
};
use ebykn_syntax::diag::{CheckMsg, Diagnostic, Message};

use crate::body::ident_of;
use crate::index::pat_ident;

/// 関数の `ensures` を検査し、裸の `&mut` 引数を E0004 にする。
pub fn check(sig: &ast::Signature, contract: &ast::Contract, out: &mut Vec<Diagnostic>) {
    let mut v = Visitor {
        muts: BTreeSet::new(),
        perms: BTreeSet::new(),
        out,
    };
    for p in &sig.params {
        match p {
            Param::Receiver {
                reference: Some(_),
                mutable: true,
                ..
            } => {
                v.muts.insert("self".into());
            }
            Param::Receiver { ty: Some(ty), .. } if is_mut_ref(ty) => {
                v.muts.insert("self".into());
            }
            Param::Typed { pat, ty, .. } if is_mut_ref(ty) => {
                if let Some(i) = pat_ident(pat) {
                    v.muts.insert(i.name.clone());
                }
            }
            _ => {}
        }
    }
    for u in contract.uses() {
        match u {
            UsesItem::Named { ident, ty, .. } if is_mut_ref(ty) => {
                v.muts.insert(ident.name.clone());
            }
            UsesItem::Perm {
                mode: PermMode::Mut,
                ident,
                ..
            } => {
                v.perms.insert(ident.name.clone());
            }
            _ => {}
        }
    }
    if v.muts.is_empty() && v.perms.is_empty() {
        return;
    }
    for e in contract.ensures() {
        v.expr(e);
    }
}

fn is_mut_ref(ty: &ast::Type) -> bool {
    matches!(ty.kind, TypeKind::Ref { mutable: true, .. })
}

struct Visitor<'o> {
    /// `&mut` の引数（`self` を含む）と `uses r: &mut Region<..>` の `r`。
    muts: BTreeSet<String>,
    /// `uses &mut p@` の `p`。
    perms: BTreeSet<String>,
    out: &'o mut Vec<Diagnostic>,
}

impl Visitor<'_> {
    fn report(&mut self, span: ebykn_syntax::span::Span, written: String, perm: Option<String>) {
        self.out.push(Diagnostic::new(
            span,
            Message::Check(CheckMsg::BareMutInEnsures { written, perm }),
        ));
    }

    /// `f` の中だけ、`names` を隠す。
    fn shadowed(&mut self, names: &BTreeSet<String>, f: impl FnOnce(&mut Self)) {
        let saved = (self.muts.clone(), self.perms.clone());
        for n in names {
            self.muts.remove(n);
            self.perms.remove(n);
        }
        f(self);
        (self.muts, self.perms) = saved;
    }

    fn exprs(&mut self, es: &[ast::Expr]) {
        for e in es {
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &ast::Expr) {
        match &e.kind {
            // `old(e)` / `final(e)` の下は、どちらの値かが書いてある。
            ExprKind::Call { func, args }
                if args.len() == 1
                    && matches!(&func.kind, ExprKind::Path { qself: None, path }
                        if path.as_ident().is_some_and(|i| i.is("old") || i.is("final"))) => {}
            ExprKind::Path { qself: None, path } => {
                if let Some(i) = path.as_ident()
                    && self.muts.contains(&i.name)
                {
                    self.report(e.span, i.name.clone(), None);
                }
            }
            ExprKind::Perm(x) => {
                if self.perms.contains(&x.name) {
                    self.report(e.span, format!("{}@", x.name), Some(x.name.clone()));
                }
            }
            ExprKind::Unary {
                op: UnOp::Deref,
                expr,
            } if ident_of(expr).is_some_and(|x| self.perms.contains(&x.name)) => {
                let x = ident_of(expr).expect("ident").name.clone();
                self.report(e.span, format!("*{x}"), Some(x));
            }
            ExprKind::Array(es) | ExprKind::Tuple(es) => self.exprs(es),
            ExprKind::Repeat { expr, len } => {
                self.expr(expr);
                self.expr(len);
            }
            ExprKind::Assign { lhs, rhs } | ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Unary { expr, .. }
            | ExprKind::Ref { expr, .. }
            | ExprKind::Paren(expr)
            | ExprKind::Cast { expr, .. }
            | ExprKind::Try(expr)
            | ExprKind::Field { base: expr, .. }
            | ExprKind::Erased { base: expr, .. } => self.expr(expr),
            ExprKind::Block { block, .. } | ExprKind::Unsafe(block) | ExprKind::Const(block) => {
                self.block(block)
            }
            ExprKind::Break { expr, .. } | ExprKind::Return(expr) => {
                if let Some(x) = expr {
                    self.expr(x);
                }
            }
            ExprKind::Call { func, args } => {
                self.expr(func);
                self.exprs(args);
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.expr(receiver);
                self.exprs(args);
            }
            ExprKind::Closure(c) => {
                let mut names = BTreeSet::new();
                for p in &c.params {
                    pat_names(p, &mut names);
                }
                self.shadowed(&names, |v| v.expr(&c.body));
            }
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::If { cond, then, else_ } => {
                self.expr(cond);
                let mut names = BTreeSet::new();
                let_names(cond, &mut names);
                self.shadowed(&names, |v| v.block(then));
                if let Some(x) = else_ {
                    self.expr(x);
                }
            }
            ExprKind::Let { expr, .. } => self.expr(expr),
            ExprKind::Loop(l) => {
                match &l.kind {
                    ast::LoopKind::While(c) => self.expr(c),
                    ast::LoopKind::For { iter, .. } => self.expr(iter),
                    ast::LoopKind::Loop => {}
                }
                let mut names = BTreeSet::new();
                if let ast::LoopKind::For { pat, .. } = &l.kind {
                    pat_names(pat, &mut names);
                }
                self.shadowed(&names, |v| v.block(&l.body));
            }
            ExprKind::Match { expr, arms } => {
                self.expr(expr);
                for a in arms {
                    let mut names = BTreeSet::new();
                    pat_names(&a.pat, &mut names);
                    self.shadowed(&names, |v| {
                        if let Some(g) = &a.guard {
                            v.expr(g);
                        }
                        v.expr(&a.body);
                    });
                }
            }
            ExprKind::Macro(m) => match &m.args {
                ast::MacroArgs::Exprs(es) => self.exprs(es),
                ast::MacroArgs::Map(kvs) => {
                    for (k, x) in kvs {
                        self.expr(k);
                        self.expr(x);
                    }
                }
                ast::MacroArgs::Tokens => {}
            },
            ExprKind::Range { start, end, .. } => {
                for x in [start, end].into_iter().flatten() {
                    self.expr(x);
                }
            }
            ExprKind::Struct(lit) => {
                for f in &lit.fields {
                    self.expr(&f.expr);
                }
                if let Some(Some(b)) = &lit.rest {
                    self.expr(b);
                }
            }
            ExprKind::Quant { binders, body, .. } => {
                let mut names = BTreeSet::new();
                for b in binders {
                    if let ast::BinderKind::In(x) = &b.kind {
                        self.expr(x);
                    }
                    names.insert(b.ident.name.clone());
                }
                self.shadowed(&names, |v| v.expr(body));
            }
            ExprKind::AndOr { exprs, .. } => self.exprs(exprs),
            ExprKind::Borrow { region, body, .. } => {
                if let ast::Region::SelfField { self_span, .. } = &region.region
                    && self.muts.contains("self")
                {
                    self.report(*self_span, "self".into(), None);
                }
                if let ast::Region::Var(r) = &region.region
                    && self.muts.contains(&r.name)
                {
                    self.report(r.span, r.name.clone(), None);
                }
                self.expr(&region.index);
                self.block(body);
            }
            ExprKind::Path { .. }
            | ExprKind::Continue { .. }
            | ExprKind::Lit(_)
            | ExprKind::Infer
            | ExprKind::Rust(_) => {}
        }
    }

    fn block(&mut self, b: &Block) {
        let saved = (self.muts.clone(), self.perms.clone());
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Let(l) => {
                    if let Some(x) = &l.init {
                        self.expr(x);
                    }
                    if let Some(e) = &l.else_ {
                        self.block(e);
                    }
                    let mut names = BTreeSet::new();
                    pat_names(&l.pat, &mut names);
                    for n in names {
                        self.muts.remove(&n);
                        self.perms.remove(&n);
                    }
                }
                StmtKind::Expr { expr, .. } => self.expr(expr),
                StmtKind::SpecAssign { target, value } => {
                    self.expr(target);
                    self.expr(value);
                }
                StmtKind::Item(_)
                | StmtKind::Empty
                | StmtKind::Take(_)
                | StmtKind::Put(_)
                | StmtKind::Leak(_) => {}
            }
        }
        (self.muts, self.perms) = saved;
    }
}

/// `if let` / `while let` の条件（`&&` でつないだものを含む）が束縛する名前。
fn let_names(e: &ast::Expr, out: &mut BTreeSet<String>) {
    match &e.kind {
        ExprKind::Let { pat, .. } => pat_names(pat, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            let_names(lhs, out);
            let_names(rhs, out);
        }
        ExprKind::Paren(x) => let_names(x, out),
        _ => {}
    }
}

/// パターンが束縛する名前。
fn pat_names(p: &ast::Pat, out: &mut BTreeSet<String>) {
    match &p.kind {
        PatKind::Ident { ident, sub, .. } => {
            out.insert(ident.name.clone());
            if let Some(s) = sub {
                pat_names(s, out);
            }
        }
        PatKind::Tuple(ps) | PatKind::Or(ps) | PatKind::Slice(ps) => {
            for q in ps {
                pat_names(q, out);
            }
        }
        PatKind::TupleStruct { elems, .. } => {
            for q in elems {
                pat_names(q, out);
            }
        }
        PatKind::Ref { pat, .. } | PatKind::Paren(pat) | PatKind::Type { pat, .. } => {
            pat_names(pat, out)
        }
        PatKind::Struct { fields, .. } => {
            for f in fields {
                match f {
                    FieldPat::Field { pat, .. } => pat_names(pat, out),
                    FieldPat::Erased { ident, .. } => {
                        out.insert(ident.name.clone());
                    }
                    FieldPat::ErasedRest { .. } => {}
                }
            }
        }
        PatKind::Wild
        | PatKind::Rest
        | PatKind::Lit(_)
        | PatKind::Range { .. }
        | PatKind::Path { .. }
        | PatKind::Macro(_) => {}
    }
}
