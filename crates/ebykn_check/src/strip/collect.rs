//! AST から、剥がす範囲（`.eykp` の証明構文、仕様 §5.7）、ループの句、診断に使う項目と文の位置を集める。

use std::collections::HashSet;

use ebykn_syntax::ast::{
    self, ClauseKind, Expr, ExprKind, FnBody, ItemKind, LoopClauseKind, MacroArgs, StmtKind,
    TraitItemKind,
};
use ebykn_syntax::span::Span;

use super::tokens::Cut;

/// `.eyk` にある `spec fn` / `lemma fn` の宣言：（項目を置く場所、種類、名前）。場所は、ファイルの中の
/// インラインの `mod` のパスと、`impl` の見出し。
pub type Decls = HashSet<(String, &'static str, String)>;

/// ループの句の式一つ（同じ種類の句は連結して読むので、句ではなく式を単位にする。仕様 §5.2）。
#[derive(Debug, Clone)]
pub struct Elem {
    pub kind: LoopClauseKind,
    pub lo: usize,
    pub hi: usize,
}

/// ループ一つ。
#[derive(Debug, Clone)]
pub struct LoopSite {
    /// ループの式全体。
    pub span: Span,
    /// 句の並び（最初の句のキーワードから本体の `{` の前まで）。句がなければ `None`。
    pub region: Option<(usize, usize)>,
    /// 本体の `{` の位置。
    pub body_lo: usize,
    pub elems: Vec<Elem>,
}

/// 一つのファイルから集めたもの。
#[derive(Debug, Default)]
pub struct Shape {
    pub cuts: Vec<Cut>,
    pub loops: Vec<LoopSite>,
    /// 項目の位置と名前（`fn f` など）。入れ子の項目も入る。
    pub items: Vec<(Span, String)>,
    /// 文の位置。入れ子の文も入る。
    pub stmts: Vec<Span>,
    /// `.eyk` の側で集めた宣言。
    pub decls: Decls,
    /// `.eykp` だけの項目として剥がした、名前を定める項目（名前の位置、`spec fn ok` などの表示、名前）。
    pub proof_only: Vec<(Span, String, String)>,
}

/// `proof` は `.eykp` の側なら、対応する `.eyk` の宣言。
pub fn collect(text: &str, file: &ast::File, proof: Option<&Decls>) -> Shape {
    let mut w = W {
        text,
        proof,
        shape: Shape::default(),
    };
    w.items(&file.items, "");
    w.shape
}

struct W<'a> {
    text: &'a str,
    proof: Option<&'a Decls>,
    shape: Shape,
}

/// 空白をまとめた本文（項目の名前と、`impl` の見出しの比較に使う）。
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl W<'_> {
    fn slice(&self, s: Span) -> String {
        squash(self.text.get(s.lo..s.hi).unwrap_or_default())
    }

    fn proof_only(&mut self, id: &ast::Ident, kw: &str) {
        self.shape
            .proof_only
            .push((id.span, format!("{kw} {}", id.name), id.name.clone()));
    }

    fn cut(&mut self, s: Span, put: Option<&'static str>) {
        self.shape.cuts.push(Cut {
            lo: s.lo,
            hi: s.hi,
            put,
        });
    }

    fn items(&mut self, items: &[ast::Item], scope: &str) {
        for item in items {
            self.item(item, Some(scope));
        }
    }

    /// `by(mode)` の句（`.eykp` だけ）を剥がす。
    fn cut_by(&mut self, c: &ast::Contract) {
        if self.proof.is_none() {
            return;
        }
        for cl in &c.clauses {
            if matches!(cl.kind, ClauseKind::By(_)) {
                self.cut(cl.span, None);
            }
        }
    }

    /// `scope` は項目を置く場所（モジュールの直下か `impl` の中）。関数の本体の中の項目は `None` で、
    /// `.eykp` だけの項目として剥がさない（`.eyk` と比べる）。
    fn item(&mut self, item: &ast::Item, scope: Option<&str>) {
        let desc = self.describe(item);
        self.shape.items.push((item.span, desc));
        let proof = self.proof.is_some();
        let decls = self.proof;
        let declared = |kind: &'static str, name: &str| match (decls, scope) {
            (Some(d), Some(s)) => d.contains(&(s.to_string(), kind, name.to_string())),
            // 関数の本体の中は、名前で探さずに比べる。
            _ => true,
        };
        match &item.kind {
            ItemKind::Fn(f) => {
                self.cut_by(&f.contract);
                match &f.body {
                    Some(FnBody::Block(b)) => self.block(b),
                    Some(FnBody::Pure(e)) => self.expr(e),
                    None => {}
                }
            }
            ItemKind::SpecFn(f) => {
                if let (false, Some(s)) = (proof, scope) {
                    self.shape
                        .decls
                        .insert((s.to_string(), "spec", f.sig.ident.name.clone()));
                }
                // `.eykp` だけの `spec fn` は項目ごと剥がす。
                if proof && !declared("spec", &f.sig.ident.name) {
                    self.proof_only(&f.sig.ident, "spec fn");
                    self.cut(item.span, None);
                    return;
                }
                if let Some(e) = &f.body {
                    self.expr(e);
                }
            }
            ItemKind::LemmaFn(f) => {
                if let (false, Some(s)) = (proof, scope) {
                    self.shape
                        .decls
                        .insert((s.to_string(), "lemma", f.sig.ident.name.clone()));
                }
                if let Some(b) = &f.body {
                    if !declared("lemma", &f.sig.ident.name) {
                        // `.eyk` に宣言のない本体付きの `lemma fn` は項目ごと剥がす。
                        self.proof_only(&f.sig.ident, "lemma fn");
                        self.cut(item.span, None);
                        return;
                    }
                    // 宣言があれば本体だけを剥がし、宣言の `;` にする。
                    self.cut(b.span, Some(";"));
                }
                self.cut_by(&f.contract);
            }
            ItemKind::LemmaGroup(_) | ItemKind::UseLemmas(_) => {
                if let ItemKind::LemmaGroup(g) = &item.kind
                    && proof
                {
                    self.proof_only(&g.ident, "lemma group");
                }
                if proof {
                    self.cut(item.span, None);
                }
            }
            ItemKind::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    let s = match scope {
                        Some(s) => format!("{s}::{}", m.ident.name),
                        None => format!("{{fn}}::{}", m.ident.name),
                    };
                    self.items(inner, &s);
                }
            }
            ItemKind::Impl(imp) => {
                let head = format!(
                    "{}impl {}{}",
                    scope.map(|s| format!("{s} ")).unwrap_or_default(),
                    imp.trait_
                        .as_ref()
                        .map(|t| format!("{} for ", self.slice(t.span)))
                        .unwrap_or_default(),
                    self.slice(imp.self_ty.span)
                );
                for it in &imp.items {
                    self.item(it, scope.map(|_| head.as_str()));
                }
            }
            ItemKind::ImplSpec(imp) => {
                let head = format!(
                    "{}impl spec {}{}",
                    scope.map(|s| format!("{s} ")).unwrap_or_default(),
                    self.slice(imp.trait_.span),
                    imp.self_ty
                        .as_ref()
                        .map(|t| format!(" for {}", self.slice(t.span)))
                        .unwrap_or_default()
                );
                for it in &imp.items {
                    self.item(it, scope.map(|_| head.as_str()));
                }
            }
            ItemKind::Trait(t) => {
                for ti in &t.items {
                    let (name, c, body) = match &ti.kind {
                        TraitItemKind::Fn(f) => (
                            format!("fn {}", f.sig.ident.name),
                            Some(&f.contract),
                            match &f.body {
                                Some(FnBody::Block(b)) => Some(Ok(b)),
                                Some(FnBody::Pure(e)) => Some(Err(e)),
                                None => None,
                            },
                        ),
                        TraitItemKind::SpecFn(f) => (
                            format!("spec fn {}", f.sig.ident.name),
                            Some(&f.contract),
                            f.body.as_ref().map(Err),
                        ),
                        TraitItemKind::LemmaFn(f) => (
                            format!("lemma fn {}", f.sig.ident.name),
                            Some(&f.contract),
                            None,
                        ),
                        TraitItemKind::Rust(_) => (self.slice(ti.span), None, None),
                    };
                    self.shape.items.push((ti.span, name));
                    if let Some(c) = c {
                        self.cut_by(c);
                    }
                    match body {
                        Some(Ok(b)) => self.block(b),
                        Some(Err(e)) => self.expr(e),
                        None => {}
                    }
                }
            }
            ItemKind::Const(c) => {
                if let Some(e) = &c.expr {
                    self.expr(e);
                }
            }
            ItemKind::Static(s) => self.expr(&s.expr),
            ItemKind::Struct(_)
            | ItemKind::Enum(_)
            | ItemKind::TypeAlias(_)
            | ItemKind::Subset(_)
            | ItemKind::Use(_)
            | ItemKind::ForeignMod(_)
            | ItemKind::Rust(_) => {}
        }
    }

    /// 診断に出す項目の名前。
    fn describe(&self, item: &ast::Item) -> String {
        let named = |kw: &str, id: &ast::Ident| format!("{kw} {}", id.name);
        match &item.kind {
            ItemKind::Fn(f) => named("fn", &f.sig.ident),
            ItemKind::SpecFn(f) => named("spec fn", &f.sig.ident),
            ItemKind::LemmaFn(f) => named("lemma fn", &f.sig.ident),
            ItemKind::Struct(s) => named("struct", &s.ident),
            ItemKind::Enum(e) => named("enum", &e.ident),
            ItemKind::Trait(t) => named("trait", &t.ident),
            ItemKind::Impl(imp) => match &imp.trait_ {
                Some(t) => format!(
                    "impl {} for {}",
                    self.slice(t.span),
                    self.slice(imp.self_ty.span)
                ),
                None => format!("impl {}", self.slice(imp.self_ty.span)),
            },
            ItemKind::ImplSpec(imp) => format!("impl spec {}", self.slice(imp.trait_.span)),
            ItemKind::TypeAlias(t) => named("type", &t.ident),
            ItemKind::Subset(t) => named("type", &t.ident),
            ItemKind::Mod(m) => named("mod", &m.ident),
            ItemKind::Const(c) => named("const", &c.ident),
            ItemKind::Static(s) => named("static", &s.ident),
            ItemKind::LemmaGroup(g) => named("lemma group", &g.ident),
            ItemKind::UseLemmas(p) => format!("use lemmas {}", self.slice(p.span)),
            ItemKind::Use(tk) | ItemKind::ForeignMod(tk) | ItemKind::Rust(tk) => {
                let s = self.slice(tk.span);
                match s.char_indices().nth(40) {
                    Some((i, _)) => format!("{} ..", &s[..i]),
                    None => s,
                }
            }
        }
    }

    fn block(&mut self, b: &ast::Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &ast::Stmt) {
        self.shape.stmts.push(s.span);
        let proof = self.proof.is_some();
        match &s.kind {
            StmtKind::Let(l) => {
                if let Some(e) = &l.init {
                    self.expr(e);
                }
                if let Some(b) = &l.else_ {
                    self.block(b);
                }
            }
            StmtKind::Item(it) => {
                if proof && matches!(it.kind, ItemKind::UseLemmas(_) | ItemKind::LemmaGroup(_)) {
                    self.cut(s.span, None);
                } else {
                    self.item(it, None);
                }
            }
            StmtKind::Expr { expr, .. } => self.expr(expr),
            StmtKind::Take(r) | StmtKind::Put(r) => self.expr(&r.index),
            StmtKind::SpecAssign { target, value } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::Proof(_) => {
                if proof {
                    self.cut(s.span, None);
                }
            }
            StmtKind::Leak(_) | StmtKind::Empty => {}
        }
    }

    fn exprs<'e>(&mut self, es: impl IntoIterator<Item = &'e Expr>) {
        for e in es {
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Array(v) | ExprKind::Tuple(v) => self.exprs(v),
            ExprKind::AndOr { exprs, .. } => self.exprs(exprs),
            ExprKind::Repeat { expr, len } => self.exprs([&**expr, &**len]),
            ExprKind::Assign { lhs, rhs } | ExprKind::Binary { lhs, rhs, .. } => {
                self.exprs([&**lhs, &**rhs])
            }
            ExprKind::Unary { expr, .. }
            | ExprKind::Ref { expr, .. }
            | ExprKind::Cast { expr, .. }
            | ExprKind::Paren(expr)
            | ExprKind::Try(expr)
            | ExprKind::Let { expr, .. } => self.expr(expr),
            ExprKind::Field { base, .. } | ExprKind::Erased { base, .. } => self.expr(base),
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
            ExprKind::Closure(c) => self.expr(&c.body),
            ExprKind::Index { base, index } => self.exprs([&**base, &**index]),
            ExprKind::If { cond, then, else_ } => {
                self.expr(cond);
                self.block(then);
                if let Some(x) = else_ {
                    self.expr(x);
                }
            }
            ExprKind::Loop(l) => {
                let region = l
                    .clauses
                    .first()
                    .map(|c| (c.keyword.lo.min(c.span.lo), l.body.span.lo));
                let elems = l
                    .clauses
                    .iter()
                    .flat_map(|c| {
                        c.exprs.iter().map(|x| Elem {
                            kind: c.kind,
                            lo: x.span.lo,
                            hi: x.span.hi,
                        })
                    })
                    .collect();
                self.shape.loops.push(LoopSite {
                    span: e.span,
                    region,
                    body_lo: l.body.span.lo,
                    elems,
                });
                match &l.kind {
                    ast::LoopKind::While(c) => self.expr(c),
                    ast::LoopKind::For { iter, .. } => self.expr(iter),
                    ast::LoopKind::Loop => {}
                }
                self.block(&l.body);
            }
            ExprKind::Match { expr, arms } => {
                self.expr(expr);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    self.expr(&a.body);
                }
            }
            ExprKind::Macro(m) => match &m.args {
                MacroArgs::Exprs(v) => self.exprs(v),
                MacroArgs::Map(kv) => {
                    for (k, v) in kv {
                        self.exprs([k, v]);
                    }
                }
                MacroArgs::Tokens => {}
            },
            ExprKind::Range { start, end, .. } => {
                for x in [start, end].into_iter().flatten() {
                    self.expr(x);
                }
            }
            ExprKind::Struct(s) => {
                for f in &s.fields {
                    self.expr(&f.expr);
                }
                if let Some(Some(r)) = &s.rest {
                    self.expr(r);
                }
            }
            ExprKind::Quant { binders, body, .. } => {
                for b in binders {
                    if let ast::BinderKind::In(x) = &b.kind {
                        self.expr(x);
                    }
                }
                self.expr(body);
            }
            ExprKind::Borrow { region, body, .. } => {
                self.expr(&region.index);
                self.block(body);
            }
            ExprKind::Continue { .. }
            | ExprKind::Lit(_)
            | ExprKind::Path { .. }
            | ExprKind::Infer
            | ExprKind::Rust(_)
            | ExprKind::Perm(_) => {}
        }
    }
}
