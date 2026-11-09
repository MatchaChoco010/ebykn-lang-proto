//! 契約の式の中の、`&mut` 引数と権限の参照（#10 の決定：案 B）。
//!
//! 1. 事後条件（`ensures` と `returns`）で `&mut` 引数を `old(..)` も `final(..)` も付けずに書く
//!    ことを、Verus と同じく許さない（`E0004`）。どちらの値か（入口か、呼び出しの後か）を書かせる。
//!    対象は、`&mut self`（`self: &mut Self` を含む）、`x: &mut T`、`uses r: &mut Region<..>` の
//!    `r` と、`uses &mut p@` の権限（`p@` と、権限を通した読み `*p`）。`requires` の裸の `x` は
//!    入口の値なので見ない（Verus と同じ）。§6.4 の展開が足す `ensures P[final(x)]` は利用者の式
//!    ではないので、ここには来ない。量化子・クロージャの引数、`match` の腕、`let` のパターンで
//!    束縛し直した `&mut` 引数の名前は、その中では束縛した値を指すので報告しない。
//! 2. 権限を持つ名前 `p`（`uses` の権限と、事後条件の `gives` の権限）を量化子などで束縛し直した
//!    中の `p@` / `*p` は、どの句でも `E0507` にする。書き下しは束縛に従わず、`p@` / `*p` をいつも
//!    外側の引数の権限にするので、束縛した `p` の権限とは読めない（`old` / `final` の下でも同じ）。
//!
//! `old(e)` / `final(e)` の `e` は、`&mut` の引数、`uses &mut p@` の `p@` / `*p`、それらの消去
//! フィールド（`x@f`）のどれかでなければ `E0004` にする（Verus の `old` / `final` は `&mut` の引数
//! そのものにしか付けられず、`old(self.len)` は生成コードの型エラーになる。`old(self).len` と書く）。
//!
//! 中を読まないマクロの引数（トークン列）も、`&mut` 引数の名前を探して `E0004` にする（権限の
//! `p@` / `*p` は、トークン列の中にあれば本体の検査（[`crate::body`]）が `E0003` にする）。引数として
//! 参照を丸ごと渡す形（`matches!(g(x), 3)`）は、裸のまま Verus に渡ると入口の値と読まれ、黙って
//! 誤るため。トークン列の中の束縛（クロージャの引数など）は読まないので、そこで束縛し直した名前も
//! 引数として報告する（その形はマクロの外で書く）。`old(..)` の括弧の中では `&mut` 引数を
//! 報告しない。Verus はマクロの引数の中の `final` を受け付けないので、`final(..)` は `E0004` に
//! し、条件をマクロの外で書くよう案内する。`.` / `::` の直後の名前（フィールドやパスの途中）と、
//! 直後に `:` が続く名前（パスの先頭とフィールドの名前）は見ない。位置はマクロ全体にする。

use std::collections::BTreeSet;

use ebykn_syntax::ast::{
    self, Block, ExprKind, FieldPat, Param, PatKind, PermMode, StmtKind, TypeKind, UnOp, UsesItem,
};
use ebykn_syntax::diag::{CheckMsg, Diagnostic, Message};
use ebykn_syntax::span::Span;

use crate::body::ident_of;
use crate::index::pat_ident;

/// 関数の契約を検査する。
pub fn check(sig: &ast::Signature, contract: &ast::Contract, out: &mut Vec<Diagnostic>) {
    let mut muts = BTreeSet::new();
    let mut mut_perms = BTreeSet::new();
    let mut perms = BTreeSet::new();
    for p in &sig.params {
        match p {
            Param::Receiver {
                reference: Some(_),
                mutable: true,
                ..
            } => {
                muts.insert("self".to_string());
            }
            Param::Receiver { ty: Some(ty), .. } if is_mut_ref(ty) => {
                muts.insert("self".to_string());
            }
            Param::Typed { pat, ty, .. } if is_mut_ref(ty) => {
                if let Some(i) = pat_ident(pat) {
                    muts.insert(i.name.clone());
                }
            }
            _ => {}
        }
    }
    for u in contract.uses() {
        match u {
            UsesItem::Named { ident, ty, .. } if is_mut_ref(ty) => {
                muts.insert(ident.name.clone());
            }
            UsesItem::Perm { mode, ident, .. } => {
                perms.insert(ident.name.clone());
                if *mode == PermMode::Mut {
                    mut_perms.insert(ident.name.clone());
                }
            }
            _ => {}
        }
    }
    let gives: BTreeSet<String> = contract.gives().map(|g| g.name.clone()).collect();
    for c in &contract.clauses {
        let (es, post): (&[ast::Expr], bool) = match &c.kind {
            ast::ClauseKind::Ensures(es) => (es, true),
            ast::ClauseKind::Returns(e) => (std::slice::from_ref(e), true),
            ast::ClauseKind::Requires(es) | ast::ClauseKind::Decreases(es) => (es, false),
            _ => continue,
        };
        let mut v = Visitor {
            post,
            muts: muts.clone(),
            mut_perms: mut_perms.clone(),
            perms: if post {
                perms.union(&gives).cloned().collect()
            } else {
                perms.clone()
            },
            rebound: BTreeSet::new(),
            at: false,
            out,
        };
        v.exprs(es);
    }
}

fn is_mut_ref(ty: &ast::Type) -> bool {
    matches!(ty.kind, TypeKind::Ref { mutable: true, .. })
}

struct Visitor<'o> {
    /// 事後条件（`ensures` / `returns`）か。
    post: bool,
    /// 事後条件で `old` / `final` が要る `&mut` の引数（`self` を含む）と `uses r: &mut Region<..>`
    /// の `r`。束縛し直すと外す。
    muts: BTreeSet<String>,
    /// 事後条件で `old` / `final` が要る `uses &mut p@` の `p`。
    mut_perms: BTreeSet<String>,
    /// 権限を持つ名前（束縛し直しても外さない）。
    perms: BTreeSet<String>,
    /// 束縛し直した、権限を持つ名前。
    rebound: BTreeSet<String>,
    /// `old(..)` / `final(..)` の下か。
    at: bool,
    out: &'o mut Vec<Diagnostic>,
}

/// `old(e)` / `final(e)` なら `e`。
fn old_or_final(e: &ast::Expr) -> Option<&ast::Expr> {
    match &e.kind {
        ExprKind::Call { func, args, .. } if args.len() == 1 => match &func.kind {
            ExprKind::Path { qself: None, path }
                if path
                    .as_ident()
                    .is_some_and(|i| i.is("old") || i.is("final")) =>
            {
                Some(&args[0])
            }
            _ => None,
        },
        _ => None,
    }
}

impl Visitor<'_> {
    fn push(&mut self, span: Span, m: CheckMsg) {
        self.out.push(Diagnostic::new(span, Message::Check(m)));
    }

    fn bare(&mut self, span: Span, written: String, perm: Option<String>) {
        self.push(span, CheckMsg::BareMutInEnsures { written, perm });
    }

    /// `p@` / `*p` の参照（`written` は書いた形）。
    fn perm_ref(&mut self, span: Span, p: &str, written: String) {
        if self.rebound.contains(p) {
            self.push(
                span,
                CheckMsg::PermOfRebound {
                    written,
                    name: p.to_string(),
                },
            );
        } else if self.post && !self.at && self.mut_perms.contains(p) {
            self.bare(span, written, Some(p.to_string()));
        }
    }

    /// `f` の中だけ、`names` を束縛し直したものとして扱う。
    fn shadowed(&mut self, names: &BTreeSet<String>, f: impl FnOnce(&mut Self)) {
        let saved = (self.muts.clone(), self.rebound.clone());
        self.rebind(names);
        f(self);
        (self.muts, self.rebound) = saved;
    }

    fn rebind(&mut self, names: &BTreeSet<String>) {
        for n in names {
            self.muts.remove(n);
            if self.perms.contains(n) {
                self.rebound.insert(n.clone());
            }
        }
    }

    /// `old(e)` / `final(e)` の `e` として Verus に渡せる形か（`old` / `final` は `&mut` の引数
    /// そのものにしか付けられない。`*p`・`p@` は権限の引数、`x@f` は `old(x).f@` に書き下す）。
    fn at_arg_ok(&self, e: &ast::Expr) -> bool {
        match &e.kind {
            ExprKind::Paren(x) | ExprKind::Erased { base: x, .. } => self.at_arg_ok(x),
            ExprKind::Path { qself: None, path } => {
                path.as_ident().is_some_and(|i| self.muts.contains(&i.name))
            }
            // 束縛し直した `p` の `p@` / `*p` は、中をたどって E0507 にする。
            ExprKind::Perm(p) => self.mut_perms.contains(&p.name),
            ExprKind::Unary {
                op: UnOp::Deref,
                expr,
            } => ident_of(expr).is_some_and(|p| self.mut_perms.contains(&p.name)),
            _ => false,
        }
    }

    fn exprs(&mut self, es: &[ast::Expr]) {
        for e in es {
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &ast::Expr) {
        if let Some(arg) = old_or_final(e) {
            // `old(e)` / `final(e)` の下は、どちらの値かが書いてある。`e` は `&mut` の引数、
            // `uses &mut p@` の `p@` / `*p`、それらの消去フィールド（`self@f`）のどれか。
            if !self.at_arg_ok(arg) {
                self.push(e.span, CheckMsg::OldFinalArg);
                return;
            }
            let prev = std::mem::replace(&mut self.at, true);
            self.expr(arg);
            self.at = prev;
            return;
        }
        match &e.kind {
            ExprKind::Path { qself: None, path } => {
                if let Some(i) = path.as_ident()
                    && self.post
                    && !self.at
                    && self.muts.contains(&i.name)
                {
                    self.bare(e.span, i.name.clone(), None);
                }
            }
            ExprKind::Perm(x) => {
                if self.perms.contains(&x.name) {
                    self.perm_ref(e.span, &x.name, format!("{}@", x.name));
                }
            }
            ExprKind::Unary {
                op: UnOp::Deref,
                expr,
            } if ident_of(expr).is_some_and(|x| self.perms.contains(&x.name)) => {
                let x = ident_of(expr).expect("ident").name.clone();
                self.perm_ref(e.span, &x, format!("*{x}"));
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
            ExprKind::Call { func, args, .. } => {
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
                ast::MacroArgs::Tokens => self.tokens(m.tokens.clone(), m.span),
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
                if self.post && !self.at {
                    if let ast::Region::SelfField { self_span, .. } = &region.region
                        && self.muts.contains("self")
                    {
                        self.bare(*self_span, "self".into(), None);
                    }
                    if let ast::Region::Var(r) = &region.region
                        && self.muts.contains(&r.name)
                    {
                        self.bare(r.span, r.name.clone(), None);
                    }
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
        let saved = (self.muts.clone(), self.rebound.clone());
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
                    self.rebind(&names);
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
        (self.muts, self.rebound) = saved;
    }

    /// 中を読まないマクロの引数。位置はマクロ全体にする。
    fn tokens(&mut self, ts: proc_macro2::TokenStream, span: Span) {
        use proc_macro2::{Delimiter, TokenTree};
        let tts: Vec<TokenTree> = ts.into_iter().collect();
        let punct = |t: Option<&TokenTree>, c: char| matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == c);
        let mut i = 0;
        while i < tts.len() {
            match &tts[i] {
                TokenTree::Group(g) => self.tokens(g.stream(), span),
                TokenTree::Ident(id) => {
                    let name = id.to_string();
                    // `.` / `::` の直後はフィールドやパスの途中。`S { f: x }` の `x` は見る。
                    let after_dot = i > 0
                        && (punct(tts.get(i - 1), '.')
                            || (i > 1 && punct(tts.get(i - 1), ':') && punct(tts.get(i - 2), ':')));
                    // 直後に `:` が続く名前は、パスの先頭（`x::y`）かフィールドの名前（`S { x: .. }`）。
                    let path_head = punct(tts.get(i + 1), ':');
                    let call = matches!(tts.get(i + 1), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis);
                    if name == "final" && call {
                        // Verus はマクロの引数の中の `final` を受け付けない。
                        self.push(span, CheckMsg::FinalInMacro);
                        i += 2;
                        continue;
                    }
                    if name == "old" && call {
                        // `old(..)` の中：`&mut` 引数は書いてよい。
                        let prev = std::mem::replace(&mut self.at, true);
                        if let Some(TokenTree::Group(g)) = tts.get(i + 1) {
                            self.tokens(g.stream(), span);
                        }
                        self.at = prev;
                        i += 2;
                        continue;
                    }
                    if !after_dot
                        && !path_head
                        && self.post
                        && !self.at
                        && self.muts.contains(&name)
                    {
                        self.push(span, CheckMsg::BareMutInMacro { written: name });
                    }
                }
                _ => {}
            }
            i += 1;
        }
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
