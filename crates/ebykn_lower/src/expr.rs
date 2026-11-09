//! 式・文・パターン・型の書き下し（仕様 §7.3、§7.4）。文字列で組み立て、最後に
//! [`crate::print`] で整形する（`OPEN.md`）。

use std::collections::BTreeMap;

use ebykn_check::facts::{RegionRef, SpanKey};
use ebykn_check::index::ErasedKind;
use ebykn_syntax::ast::{
    self, AndOrKind, BinOp, BinderKind, Block, Expr, ExprKind, FieldPat, GenericArg, GenericArgs,
    LoopKind, Member, PatKind, PermMode, QuantKind, StmtKind, TypeKind, UnOp,
};

use crate::Cx;

/// 権限の変数 `__eyk_perm_p` が何を持つか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermKind {
    /// 権限そのもの（`take`、`Ptr::new`、`uses p@`、`borrow mut`）。
    Owned,
    /// `&PointsTo`（`uses &p@`、`borrow`）。
    Ref,
    /// `&mut PointsTo`（`uses &mut p@`）。
    MutRef,
}

pub fn perm_var(p: &str) -> String {
    format!("__eyk_perm_{p}")
}

impl Cx<'_> {
    fn perm_kind(&self, p: &str) -> PermKind {
        self.perms.get(p).copied().unwrap_or(PermKind::Owned)
    }

    /// `Tracked(&..)`（共有の借用を渡す形）。
    fn perm_shared(&self, p: &str) -> String {
        let v = perm_var(p);
        match self.perm_kind(p) {
            PermKind::Owned => format!("Tracked(&{v})"),
            PermKind::Ref | PermKind::MutRef => format!("Tracked(&*{v})"),
        }
    }

    /// `Tracked(&mut ..)`。
    fn perm_mut(&self, p: &str) -> String {
        let v = perm_var(p);
        match self.perm_kind(p) {
            PermKind::Owned => format!("Tracked(&mut {v})"),
            PermKind::Ref | PermKind::MutRef => format!("Tracked(&mut *{v})"),
        }
    }

    // ---- 型・パス・パターン ----

    pub fn ty(&self, t: &ast::Type) -> String {
        match &t.kind {
            TypeKind::Path { qself, path } => self.qpath(qself.as_ref(), path),
            TypeKind::Ref {
                lifetime,
                mutable,
                elem,
            } => format!(
                "&{}{}{}",
                lifetime
                    .as_ref()
                    .map(|l| format!("'{} ", l.name))
                    .unwrap_or_default(),
                if *mutable { "mut " } else { "" },
                self.ty(elem)
            ),
            TypeKind::Tuple(ts) => {
                let inner: Vec<String> = ts.iter().map(|t| self.ty(t)).collect();
                if inner.len() == 1 {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            }
            TypeKind::Array { elem, len } => {
                format!("[{}; {}]", self.ty(elem), self.exec_expr_str(len))
            }
            TypeKind::Slice(e) => format!("[{}]", self.ty(e)),
            TypeKind::Paren(e) => format!("({})", self.ty(e)),
            TypeKind::Never => "!".into(),
            TypeKind::Infer => "_".into(),
            TypeKind::Rust(tk) => tk.tokens.to_string(),
        }
    }

    pub fn path(&self, p: &ast::Path) -> String {
        let mut s = String::new();
        if p.leading_colon {
            s.push_str("::");
        }
        let segs: Vec<String> = p
            .segments
            .iter()
            .map(|seg| {
                let name = ident(&seg.ident);
                match &seg.args {
                    GenericArgs::None => name,
                    GenericArgs::Angle {
                        turbofish, args, ..
                    } => format!(
                        "{name}{}<{}>",
                        if *turbofish { "::" } else { "" },
                        args.iter()
                            .map(|a| self.generic_arg(a))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    GenericArgs::Paren { inputs, output, .. } => format!(
                        "{name}({}){}",
                        inputs
                            .iter()
                            .map(|t| self.ty(t))
                            .collect::<Vec<_>>()
                            .join(", "),
                        output
                            .as_ref()
                            .map(|o| format!(" -> {}", self.ty(o)))
                            .unwrap_or_default()
                    ),
                }
            })
            .collect();
        s.push_str(&segs.join("::"));
        s
    }

    fn qpath(&self, q: Option<&ast::QSelf>, p: &ast::Path) -> String {
        let Some(q) = q else {
            return self.path(p);
        };
        // `<T as Trait>::rest`：`position` は Trait のパスの長さ。
        let all = self.path(p);
        let segs: Vec<&str> = all.split("::").collect();
        let (tr, rest) = segs.split_at(q.position.min(segs.len()));
        if tr.is_empty() {
            format!("<{}>::{}", self.ty(&q.ty), rest.join("::"))
        } else {
            format!(
                "<{} as {}>::{}",
                self.ty(&q.ty),
                tr.join("::"),
                rest.join("::")
            )
        }
    }

    fn generic_arg(&self, a: &GenericArg) -> String {
        match a {
            GenericArg::Lifetime(l) => format!("'{}", l.name),
            GenericArg::Type(t) => self.ty(t),
            GenericArg::Const(e) => self.exec_expr_str(e),
            GenericArg::AssocType { ident: i, ty } => format!("{} = {}", ident(i), self.ty(ty)),
            GenericArg::Rust(tk) => tk.tokens.to_string(),
        }
    }

    pub fn pat(&self, p: &ast::Pat) -> String {
        match &p.kind {
            PatKind::Ident {
                by_ref,
                mutable,
                ident: i,
                sub,
            } => format!(
                "{}{}{}{}",
                if *by_ref { "ref " } else { "" },
                if *mutable { "mut " } else { "" },
                ident(i),
                sub.as_ref()
                    .map(|s| format!(" @ {}", self.pat(s)))
                    .unwrap_or_default()
            ),
            PatKind::Wild => "_".into(),
            PatKind::Rest => "..".into(),
            PatKind::Lit(e) => self.exec_expr_str(e),
            PatKind::Range { start, end, closed } => format!(
                "{}{}{}",
                start
                    .as_ref()
                    .map(|e| self.exec_expr_str(e))
                    .unwrap_or_default(),
                if *closed { "..=" } else { ".." },
                end.as_ref()
                    .map(|e| self.exec_expr_str(e))
                    .unwrap_or_default()
            ),
            PatKind::Path { qself, path } => self.qpath(qself.as_ref(), path),
            PatKind::Tuple(ps) => {
                let inner: Vec<String> = ps.iter().map(|q| self.pat(q)).collect();
                if inner.len() == 1 && !matches!(ps[0].kind, PatKind::Rest) {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            }
            PatKind::TupleStruct { qself, path, elems } => format!(
                "{}({})",
                self.qpath(qself.as_ref(), path),
                elems
                    .iter()
                    .map(|q| self.pat(q))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            PatKind::Struct {
                qself,
                path,
                fields,
                rest,
            } => {
                let mut parts = Vec::new();
                let mut erased_rest = false;
                for f in fields {
                    match f {
                        FieldPat::Field {
                            member,
                            pat,
                            shorthand,
                            ..
                        } => {
                            if *shorthand {
                                parts.push(self.pat(pat));
                            } else {
                                parts.push(format!("{}: {}", member_str(member), self.pat(pat)));
                            }
                        }
                        FieldPat::Erased { ident: i, .. } => {
                            let wrap = match self.facts.erased.get(&SpanKey::from(i.span)) {
                                Some(ErasedKind::Owns) => "Tracked",
                                _ => "Ghost",
                            };
                            parts.push(format!("{}: {wrap}({})", ident(i), ident(i)));
                        }
                        FieldPat::ErasedRest { .. } => erased_rest = true,
                    }
                }
                if *rest || erased_rest {
                    parts.push("..".into());
                }
                format!(
                    "{} {{ {} }}",
                    self.qpath(qself.as_ref(), path),
                    parts.join(", ")
                )
            }
            PatKind::Ref { mutable, pat } => {
                format!("&{}{}", if *mutable { "mut " } else { "" }, self.pat(pat))
            }
            PatKind::Or(ps) => ps
                .iter()
                .map(|q| self.pat(q))
                .collect::<Vec<_>>()
                .join(" | "),
            PatKind::Slice(ps) => format!(
                "[{}]",
                ps.iter()
                    .map(|q| self.pat(q))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            PatKind::Paren(q) => format!("({})", self.pat(q)),
            PatKind::Macro(m) => self.clone().mac(m),
            PatKind::Type { pat, ty } => format!("{}: {}", self.pat(pat), self.ty(ty)),
        }
    }

    // ---- 式 ----

    pub fn exec_expr_str(&self, e: &Expr) -> String {
        let mut c = self.sub(false);
        c.expr(e)
    }

    pub fn expr(&mut self, e: &Expr) -> String {
        let attrs: String = e.attrs.iter().map(|a| crate::attr(a) + " ").collect();
        attrs + &self.expr_kind(e)
    }

    fn exprs(&mut self, es: &[Expr]) -> String {
        es.iter()
            .map(|e| self.expr(e))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// 論理文脈で式を書き下す（`logic` を一時的に立てる）。
    fn in_logic(&mut self, e: &Expr) -> String {
        let prev = std::mem::replace(&mut self.logic, true);
        let s = self.expr(e);
        self.logic = prev;
        s
    }

    fn expr_kind(&mut self, e: &Expr) -> String {
        let key = SpanKey::from(e.span);
        match &e.kind {
            ExprKind::Array(es) => format!("[{}]", self.exprs(es)),
            ExprKind::Repeat { expr, len } => format!("[{}; {}]", self.expr(expr), self.expr(len)),
            ExprKind::Assign { lhs, rhs } => {
                let rhs = self.expr(rhs);
                self.assign(lhs, "=", rhs)
            }
            ExprKind::Binary { op, lhs, rhs, .. } if is_compound(*op) => {
                let rhs = self.expr(rhs);
                self.assign(lhs, binop(*op), rhs)
            }
            ExprKind::Binary { op, lhs, rhs, .. } => {
                format!("{} {} {}", self.expr(lhs), binop(*op), self.expr(rhs))
            }
            ExprKind::Unary { op, expr } => {
                if *op == UnOp::Deref
                    && let Some(x) = self.facts.derefs.get(&key).cloned()
                {
                    return self.deref_read(&x);
                }
                let s = self.expr(expr);
                match op {
                    UnOp::Deref => format!("*{s}"),
                    UnOp::Not => format!("!{s}"),
                    UnOp::Neg => format!("-{s}"),
                }
            }
            ExprKind::Ref { mutable, expr } => {
                if *mutable && ebykn_check::body::deref_root(expr).is_some() && !self.logic {
                    format!("&mut {}", self.place(expr))
                } else {
                    format!("&{}{}", if *mutable { "mut " } else { "" }, self.expr(expr))
                }
            }
            ExprKind::Block { label, block } => format!(
                "{}{}",
                label
                    .as_ref()
                    .map(|l| format!("'{}: ", l.name))
                    .unwrap_or_default(),
                self.block(block)
            ),
            ExprKind::Unsafe(b) => format!("unsafe {}", self.block(b)),
            ExprKind::Const(b) => format!("const {}", self.block(b)),
            ExprKind::Break { label, expr } => format!(
                "break{}{}",
                label
                    .as_ref()
                    .map(|l| format!(" '{}", l.name))
                    .unwrap_or_default(),
                expr.as_ref()
                    .map(|x| format!(" {}", self.expr(x)))
                    .unwrap_or_default()
            ),
            ExprKind::Continue { label } => format!(
                "continue{}",
                label
                    .as_ref()
                    .map(|l| format!(" '{}", l.name))
                    .unwrap_or_default()
            ),
            ExprKind::Return(x) => {
                let v = x.as_ref().map(|x| self.expr(x));
                match (&self.gives, v) {
                    (Some(g), Some(v)) => format!("return ({v}, Tracked({}))", perm_var(g)),
                    (_, Some(v)) => format!("return {v}"),
                    (_, None) => "return".into(),
                }
            }
            ExprKind::Call { func, args } => self.call(e, func, args),
            ExprKind::MethodCall {
                receiver,
                method,
                turbofish,
                args,
            } => self.method_call(e, receiver, method, turbofish.as_deref(), args),
            ExprKind::Cast { expr, ty } => format!("{} as {}", self.expr(expr), self.ty(ty)),
            ExprKind::Closure(c) => self.closure(c),
            ExprKind::Field { base, member } => {
                format!("{}.{}", self.expr(base), member_str(member))
            }
            ExprKind::Index { base, index } => {
                let idx = self.expr(index);
                if let Some(r) = self.facts.region_index.get(&key).cloned() {
                    return format!("{}.index({idx})", self.region_logic(&r));
                }
                format!("{}[{idx}]", self.expr(base))
            }
            ExprKind::If { cond, then, else_ } => {
                let mut s = format!("if {} {}", self.expr(cond), self.block(then));
                if let Some(e) = else_ {
                    s.push_str(" else ");
                    s.push_str(&self.expr(e));
                }
                s
            }
            ExprKind::Let { pat, expr } => format!("let {} = {}", self.pat(pat), self.expr(expr)),
            ExprKind::Loop(l) => self.loop_(l),
            ExprKind::Match { expr, arms } => {
                let scrut = self.expr(expr);
                let arms: Vec<String> = arms
                    .iter()
                    .map(|a| {
                        format!(
                            "{}{}{} => {},",
                            a.attrs
                                .iter()
                                .map(|x| crate::attr(x) + " ")
                                .collect::<String>(),
                            self.pat(&a.pat),
                            a.guard
                                .as_ref()
                                .map(|g| format!(" if {}", self.expr(g)))
                                .unwrap_or_default(),
                            self.expr(&a.body)
                        )
                    })
                    .collect();
                format!("match {scrut} {{ {} }}", arms.join(" "))
            }
            ExprKind::Lit(l) => l.text.clone(),
            ExprKind::Macro(m) => self.mac(m),
            ExprKind::Paren(x) => format!("({})", self.expr(x)),
            ExprKind::Path { qself, path } => self.path_expr(qself.as_ref(), path),
            ExprKind::Range { start, end, closed } => format!(
                "{}{}{}",
                start.as_ref().map(|x| self.expr(x)).unwrap_or_default(),
                if *closed { "..=" } else { ".." },
                end.as_ref().map(|x| self.expr(x)).unwrap_or_default()
            ),
            ExprKind::Struct(lit) => self.struct_lit(lit),
            ExprKind::Try(x) => format!("{}?", self.expr(x)),
            ExprKind::Tuple(es) => {
                if es.len() == 1 {
                    format!("({},)", self.expr(&es[0]))
                } else {
                    format!("({})", self.exprs(es))
                }
            }
            ExprKind::Infer => "_".into(),
            ExprKind::Rust(tk) => tk.tokens.to_string(),
            ExprKind::Quant {
                kind,
                binders,
                body,
            } => self.quant(*kind, binders, body),
            ExprKind::AndOr { kind, exprs } => {
                let op = match kind {
                    AndOrKind::And => " && ",
                    AndOrKind::Or => " || ",
                };
                let parts: Vec<String> = exprs
                    .iter()
                    .map(|x| format!("({})", self.in_logic(x)))
                    .collect();
                format!("({})", parts.join(op))
            }
            ExprKind::Perm(x) => self.perm_term(&x.name),
            ExprKind::Erased { base, field } => format!("{}.{}@", self.expr(base), ident(field)),
            ExprKind::Borrow {
                mutable,
                region,
                body,
            } => self.borrow(e, *mutable, region, body),
        }
    }

    /// `ensures` の中の、`old` / `final` の下にない `&mut` 引数を `final(..)` で包む（#10 の既定）。
    fn wrap_final(&self, name: &str, s: String) -> String {
        if self.ensures && !self.under_old_final && self.mut_params.contains(name) {
            format!("final({s})")
        } else {
            s
        }
    }

    fn path_expr(&mut self, q: Option<&ast::QSelf>, p: &ast::Path) -> String {
        if q.is_none()
            && let Some(i) = p.as_ident()
        {
            if self.ensures
                && let Some(r) = self.rename.get(&i.name)
            {
                return r.clone();
            }
            return self.wrap_final(&i.name, ident(i));
        }
        self.qpath(q, p)
    }

    /// 権限付きの `*X` の読み（§7.3）。
    fn deref_read(&self, x: &str) -> String {
        if self.logic {
            let v = self.perm_term(x);
            return format!("{v}.value()");
        }
        format!("*{x}.read({})", self.perm_shared(x))
    }

    /// 論理文脈の `p@`（`ensures` の `&mut` は `final`、`gives` の戻り値は名前を付け替える）。
    fn perm_term(&self, x: &str) -> String {
        let v = perm_var(x);
        if self.ensures
            && let Some(r) = self.rename.get(&v)
        {
            return r.clone();
        }
        self.wrap_final(&v, v.clone())
    }

    /// 代入（`=`、`+=` など）。左辺が権限付きの `*X` なら `write` / `borrow_mut` にする。
    fn assign(&mut self, lhs: &Expr, op: &str, rhs: String) -> String {
        if let ExprKind::Unary {
            op: UnOp::Deref, ..
        } = &lhs.kind
            && let Some(x) = self.facts.derefs.get(&SpanKey::from(lhs.span)).cloned()
            && op == "="
            && !self.logic
        {
            return format!("{x}.write({}, {rhs})", self.perm_mut(&x));
        }
        let l = if !self.logic && ebykn_check::body::deref_root(lhs).is_some() {
            self.place(lhs)
        } else {
            self.expr(lhs)
        };
        format!("{l} {op} {rhs}")
    }

    /// 書き込む場所（`(*X).f`、`*X` など）：根の `*X` を `X.borrow_mut(..)` にする。
    fn place(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Unary {
                op: UnOp::Deref, ..
            } => match self.facts.derefs.get(&SpanKey::from(e.span)).cloned() {
                Some(x) => format!("(*{x}.borrow_mut({}))", self.perm_mut(&x)),
                None => self.expr(e),
            },
            ExprKind::Paren(x) => {
                if let ExprKind::Unary {
                    op: UnOp::Deref, ..
                } = &x.kind
                    && let Some(p) = self.facts.derefs.get(&SpanKey::from(x.span)).cloned()
                {
                    return format!("{p}.borrow_mut({})", self.perm_mut(&p));
                }
                format!("({})", self.place(x))
            }
            ExprKind::Field { base, member } => {
                format!("{}.{}", self.place(base), member_str(member))
            }
            ExprKind::Index { base, index } => {
                let i = self.expr(index);
                format!("{}[{i}]", self.place(base))
            }
            _ => self.expr(e),
        }
    }

    fn extra_args(&self, span: ebykn_syntax::span::Span) -> Vec<String> {
        let Some(extra) = self.facts.calls.get(&SpanKey::from(span)) else {
            return Vec::new();
        };
        extra
            .iter()
            .map(|a| match a.mode {
                PermMode::Move => format!("Tracked({})", perm_var(&a.var)),
                PermMode::Mut => self.perm_mut(&a.var),
                PermMode::Shared => self.perm_shared(&a.var),
            })
            .collect()
    }

    fn call(&mut self, e: &Expr, func: &Expr, args: &[Expr]) -> String {
        // `old(e)` / `final(e)`：中は包まない。`old(*p)` は `old(p@).value()`。
        if let ExprKind::Path { qself: None, path } = &func.kind
            && let Some(f) = path.as_ident()
            && (f.is("old") || f.is("final"))
            && args.len() == 1
        {
            let prev = std::mem::replace(&mut self.under_old_final, true);
            let s = match &args[0].kind {
                ExprKind::Unary {
                    op: UnOp::Deref, ..
                } if self.facts.derefs.contains_key(&SpanKey::from(args[0].span)) => {
                    let x = self.facts.derefs[&SpanKey::from(args[0].span)].clone();
                    format!("{}({}).value()", f.name, perm_var(&x))
                }
                _ => format!("{}({})", f.name, self.expr(&args[0])),
            };
            self.under_old_final = prev;
            return s;
        }
        let f = self.expr(func);
        let mut a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
        a.extend(self.extra_args(e.span));
        format!("{f}({})", a.join(", "))
    }

    fn method_call(
        &mut self,
        e: &Expr,
        receiver: &Expr,
        method: &ast::Ident,
        turbofish: Option<&[GenericArg]>,
        args: &[Expr],
    ) -> String {
        let key = SpanKey::from(e.span);
        if let Some(x) = self.facts.frees.get(&key).cloned() {
            return format!("{x}.free(Tracked({}))", perm_var(&x));
        }
        let recv = self.expr(receiver);
        if self.logic && args.is_empty() && turbofish.is_none() {
            // `v.view()` ⇒ `v@`（§7.4）。`.first()` / `.last()` ⇒ `first_opt` / `last_opt`（#11 の既定）。
            match method.name.as_str() {
                "view" => return format!("{recv}@"),
                "first" => return format!("{recv}.first_opt()"),
                "last" => return format!("{recv}.last_opt()"),
                _ => {}
            }
        }
        let mut a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
        a.extend(self.extra_args(e.span));
        format!(
            "{recv}.{}{}({})",
            ident(method),
            turbofish
                .map(|t| format!(
                    "::<{}>",
                    t.iter()
                        .map(|g| self.generic_arg(g))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .unwrap_or_default(),
            a.join(", ")
        )
    }

    fn closure(&mut self, c: &ast::Closure) -> String {
        let params: Vec<String> = c.params.iter().map(|p| self.pat(p)).collect();
        let saved = (self.rename.clone(), self.mut_params.clone());
        for p in &c.params {
            if let Some(i) = ebykn_check::index::pat_ident(p) {
                self.rename.remove(&i.name);
                self.mut_params.remove(&i.name);
            }
        }
        let ret = c
            .ret
            .as_ref()
            .map(|r| match &r.name {
                Some(n) => format!(" -> ({}: {})", ident(n), self.ty(&r.ty)),
                None => format!(" -> {}", self.ty(&r.ty)),
            })
            .unwrap_or_default();
        let contract = self.clauses_simple(&c.contract);
        let s = format!(
            "{}|{}|{ret}{contract} {}",
            if c.capture { "move " } else { "" },
            params.join(", "),
            self.expr(&c.body)
        );
        (self.rename, self.mut_params) = saved;
        s
    }

    /// クロージャの契約（`requires` / `ensures` だけ）。
    fn clauses_simple(&mut self, c: &ast::Contract) -> String {
        let mut s = String::new();
        let req: Vec<String> = c.requires().map(|e| self.in_logic(e)).collect();
        let ens: Vec<String> = c.ensures().map(|e| self.in_logic(e)).collect();
        if !req.is_empty() {
            s.push_str(&format!(" requires {},", req.join(", ")));
        }
        if !ens.is_empty() {
            s.push_str(&format!(" ensures {},", ens.join(", ")));
        }
        s
    }

    fn loop_(&mut self, l: &ast::Loop) -> String {
        let label = l
            .label
            .as_ref()
            .map(|x| format!("'{}: ", x.name))
            .unwrap_or_default();
        let head = match &l.kind {
            LoopKind::While(c) => format!("while {}", self.expr(c)),
            LoopKind::For { pat, iter } => format!("for {} in {}", self.pat(pat), self.expr(iter)),
            LoopKind::Loop => "loop".into(),
        };
        let mut clauses = String::new();
        for c in &l.clauses {
            let kw = match c.kind {
                ast::LoopClauseKind::Invariant => "invariant",
                ast::LoopClauseKind::Decreases => "decreases",
            };
            let es: Vec<String> = c.exprs.iter().map(|e| self.in_logic(e)).collect();
            clauses.push_str(&format!(" {kw} {},", es.join(", ")));
        }
        format!("{label}{head}{clauses} {}", self.block(&l.body))
    }

    pub fn mac(&mut self, m: &ast::Macro) -> String {
        let path = self.path(&m.path);
        let (o, c) = match m.delimiter {
            ast::Delimiter::Paren => ("(", ")"),
            ast::Delimiter::Brace => ("{", "}"),
            ast::Delimiter::Bracket => ("[", "]"),
        };
        let inner = match &m.args {
            ast::MacroArgs::Tokens => m.tokens.to_string(),
            ast::MacroArgs::Exprs(es) => {
                let prev = std::mem::replace(&mut self.logic, true);
                let s = self.exprs(es);
                self.logic = prev;
                s
            }
            ast::MacroArgs::Map(kvs) => {
                let prev = std::mem::replace(&mut self.logic, true);
                let s = kvs
                    .iter()
                    .map(|(k, v)| format!("{} => {}", self.expr(k), self.expr(v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.logic = prev;
                s
            }
        };
        format!("{path}!{o}{inner}{c}")
    }

    fn struct_lit(&mut self, lit: &ast::StructLit) -> String {
        let mut parts = Vec::new();
        for f in &lit.fields {
            let attrs: String = f.attrs.iter().map(|a| crate::attr(a) + " ").collect();
            if f.erased {
                let wrap = match self.facts.erased.get(&SpanKey::from(f.span)) {
                    Some(ErasedKind::Owns) => "Tracked",
                    _ => "Ghost",
                };
                let v = self.in_logic(&f.expr);
                parts.push(format!("{attrs}{}: {wrap}({v})", member_str(&f.member)));
            } else if f.shorthand {
                parts.push(format!("{attrs}{}", self.expr(&f.expr)));
            } else {
                parts.push(format!(
                    "{attrs}{}: {}",
                    member_str(&f.member),
                    self.expr(&f.expr)
                ));
            }
        }
        match &lit.rest {
            Some(Some(b)) => parts.push(format!("..{}", self.expr(b))),
            Some(None) => parts.push("..".into()),
            None => {}
        }
        format!(
            "{} {{ {} }}",
            self.qpath(lit.qself.as_ref(), &lit.path),
            parts.join(", ")
        )
    }

    fn quant(&mut self, kind: QuantKind, binders: &[ast::Binder], body: &Expr) -> String {
        let prev = std::mem::replace(&mut self.logic, true);
        let mut names = Vec::new();
        let mut conds = Vec::new();
        // 束縛した名前は、外の `gives` の付け替えと `final` の包みを隠す（シャドウ）。
        let saved = (self.rename.clone(), self.mut_params.clone());
        for b in binders {
            let n = ident(&b.ident);
            match &b.kind {
                BinderKind::Typed(t) => names.push(format!("{n}: {}", self.ty(t))),
                BinderKind::Bare => names.push(n.clone()),
                BinderKind::In(r) => {
                    conds.push(self.membership(&n, r));
                    // 範囲の束縛子には型を付ける（端のリテラルの接尾辞、なければ `int`）。
                    match range_type(r) {
                        Some(t) => names.push(format!("{n}: {t}")),
                        None => names.push(n.clone()),
                    }
                }
            }
            self.rename.remove(&b.ident.name);
            self.mut_params.remove(&b.ident.name);
        }
        let body_s = self.expr(body);
        (self.rename, self.mut_params) = saved;
        self.logic = prev;
        let low = |e: &Expr, ops: &[BinOp]| matches!(&e.kind, ExprKind::Binary { op, .. } if ops.contains(op));
        let q = match kind {
            QuantKind::Forall => "forall",
            QuantKind::Exists => "exists",
        };
        if conds.is_empty() {
            return format!("({q}|{}| {body_s})", names.join(", "));
        }
        let cond = conds.join(" && ");
        let body_s = match kind {
            // `range ==> P`：`P` が `<==>` なら括弧が要る（`<==>` は `==>` より低い）。
            QuantKind::Forall if low(body, &[BinOp::Iff]) => format!("({body_s})"),
            QuantKind::Exists if low(body, &[BinOp::Or, BinOp::Implies, BinOp::Iff]) => {
                format!("({body_s})")
            }
            _ => body_s,
        };
        let op = match kind {
            QuantKind::Forall => "==>",
            QuantKind::Exists => "&&",
        };
        format!("({q}|{}| {cond} {op} {body_s})", names.join(", "))
    }

    /// `x in r` の展開（§7.4）。範囲は不等式、`Map`（型で分かるとき）は `dom().contains`、
    /// それ以外は `contains`。
    fn membership(&mut self, x: &str, r: &Expr) -> String {
        let atom = |c: &mut Self, e: &Expr| {
            let s = c.expr(e);
            if simple(e) { s } else { format!("({s})") }
        };
        match &r.kind {
            ExprKind::Range {
                start: Some(a),
                end: Some(b),
                closed,
            } => {
                let a = atom(self, a);
                let b = atom(self, b);
                format!("{a} <= {x} {} {b}", if *closed { "<=" } else { "<" })
            }
            ExprKind::Paren(inner) if matches!(inner.kind, ExprKind::Range { .. }) => {
                self.membership(x, inner)
            }
            _ => {
                let s = atom(self, r);
                if self.facts.map_binders.contains(&SpanKey::from(r.span)) {
                    format!("{s}.dom().contains({x})")
                } else {
                    format!("{s}.contains({x})")
                }
            }
        }
    }

    /// 論理文脈の領域（`self.r@` / 領域変数）。
    fn region_logic(&self, r: &RegionRef) -> String {
        match r {
            RegionRef::SelfField(f) => format!("{}.{f}@", self.wrap_final("self", "self".into())),
            RegionRef::Var(v) => v.clone(),
        }
    }

    /// 実行文脈の領域（変更 `borrow_mut()` / 読み `borrow()`）。
    fn region_exec(&self, r: &RegionRef, write: bool) -> String {
        match r {
            RegionRef::SelfField(f) => {
                format!("self.{f}.{}()", if write { "borrow_mut" } else { "borrow" })
            }
            RegionRef::Var(v) => v.clone(),
        }
    }

    fn borrow(&mut self, e: &Expr, mutable: bool, ri: &ast::RegionIndex, body: &Block) -> String {
        let Some(r) = self.facts.region_stmts.get(&SpanKey::from(e.span)).cloned() else {
            return "{ }".into();
        };
        let p = index_name(ri);
        let v = perm_var(&p);
        if mutable {
            let saved = self.perms.insert(p.clone(), PermKind::Owned);
            let b = self.block(body);
            restore(&mut self.perms, &p, saved);
            let rx = self.region_exec(&r, true);
            format!(
                "{{ let tracked {v} = {rx}.remove({p}); let __eyk_res = {b}; proof {{ {rx}.insert({v}); }} __eyk_res }}"
            )
        } else {
            let saved = self.perms.insert(p.clone(), PermKind::Ref);
            let b = self.block(body);
            restore(&mut self.perms, &p, saved);
            let rx = self.region_exec(&r, false);
            format!("{{ let tracked {v} = {rx}.borrow({p}); {b} }}")
        }
    }

    // ---- ブロックと文 ----

    pub fn block(&mut self, b: &Block) -> String {
        let saved = self.perms.clone();
        let mut parts = Vec::new();
        for s in &b.stmts {
            parts.push(self.stmt(s));
        }
        self.perms = saved;
        format!("{{ {} }}", parts.join(" "))
    }

    /// 関数の本体のブロック。末尾の式に `gives` の権限を付ける。
    pub fn body_block(&mut self, b: &Block) -> String {
        let Some(g) = self.gives.clone() else {
            return self.block(b);
        };
        let mut parts = Vec::new();
        let n = b.stmts.len();
        for (i, s) in b.stmts.iter().enumerate() {
            match &s.kind {
                StmtKind::Expr { expr, semi: false } if i + 1 == n => {
                    let v = self.expr(expr);
                    parts.push(format!("({v}, Tracked({}))", perm_var(&g)));
                }
                _ => parts.push(self.stmt(s)),
            }
        }
        format!("{{ {} }}", parts.join(" "))
    }

    fn stmt(&mut self, s: &ast::Stmt) -> String {
        let key = SpanKey::from(s.span);
        match &s.kind {
            StmtKind::Let(l) => {
                let attrs: String = l.attrs.iter().map(|a| crate::attr(a) + " ").collect();
                if let Some(x) = self.facts.perm_lets.get(&key).cloned() {
                    let init = l.init.as_ref().map(|e| self.expr(e)).unwrap_or_default();
                    self.perms.insert(x.clone(), PermKind::Owned);
                    // 型注釈は残す（`let p: Ptr<u8> = ..` の `u8` が値の型を決める）。
                    let ty =
                        l.ty.as_ref()
                            .map(|t| format!(": ({}, Tracked<_>)", self.ty(t)))
                            .unwrap_or_default();
                    return format!(
                        "{attrs}let ({}, Tracked({})){ty} = {init};",
                        self.pat(&l.pat),
                        perm_var(&x)
                    );
                }
                let region =
                    l.ty.as_ref()
                        .is_some_and(ebykn_check::index::is_region_type);
                let mut out = format!(
                    "{attrs}let {}{}",
                    if region { "tracked " } else { "" },
                    self.pat(&l.pat)
                );
                if let Some(t) = &l.ty {
                    out.push_str(&format!(": {}", self.ty(t)));
                }
                if let Some(i) = &l.init {
                    out.push_str(&format!(" = {}", self.expr(i)));
                }
                if let Some(e) = &l.else_ {
                    out.push_str(&format!(" else {}", self.block(e)));
                }
                out.push(';');
                out
            }
            StmtKind::Item(item) => self.nested_item(item),
            StmtKind::Expr { expr, semi } => {
                let s = self.expr(expr);
                if *semi { format!("{s};") } else { s }
            }
            StmtKind::Empty => ";".into(),
            StmtKind::Take(ri) => {
                let Some(r) = self.facts.region_stmts.get(&key).cloned() else {
                    return String::new();
                };
                let p = index_name(ri);
                self.perms.insert(p.clone(), PermKind::Owned);
                format!(
                    "let tracked {} = {}.remove({p});",
                    perm_var(&p),
                    self.region_exec(&r, true)
                )
            }
            StmtKind::Put(ri) => {
                let Some(r) = self.facts.region_stmts.get(&key).cloned() else {
                    return String::new();
                };
                let p = index_name(ri);
                format!(
                    "proof {{ {}.insert({}); }}",
                    self.region_exec(&r, true),
                    perm_var(&p)
                )
            }
            // `leak p@;` は何も出さない（§7.3）。
            StmtKind::Leak(_) => String::new(),
            StmtKind::SpecAssign { target, value } => {
                let ExprKind::Erased { base, field } = &target.kind else {
                    return String::new();
                };
                let b = self.expr(base);
                let v = self.in_logic(value);
                format!("proof {{ {b}.{} = Ghost({v}); }}", ident(field))
            }
        }
    }
}

/// 範囲 `a..b` の束縛子の型：端のリテラルの接尾辞（`3int`、`10u64`）、なければ `int`。範囲でなければ `None`。
fn range_type(r: &Expr) -> Option<String> {
    let r = match &r.kind {
        ExprKind::Paren(x) => &**x,
        _ => r,
    };
    let ExprKind::Range { start, end, .. } = &r.kind else {
        return None;
    };
    const TYPES: &[&str] = &[
        "int", "nat", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
        "i128", "isize",
    ];
    for e in [start, end].into_iter().flatten() {
        if let ExprKind::Lit(l) = &e.kind
            && let Some(t) = TYPES.iter().find(|t| l.text.ends_with(*t))
        {
            return Some(t.to_string());
        }
    }
    Some("int".into())
}

fn restore(m: &mut BTreeMap<String, PermKind>, k: &str, saved: Option<PermKind>) {
    match saved {
        Some(v) => {
            m.insert(k.to_string(), v);
        }
        None => {
            m.remove(k);
        }
    }
}

/// 領域添字の添字の名前（検査で識別子と確かめてある）。
fn index_name(ri: &ast::RegionIndex) -> String {
    match &ri.index.kind {
        ExprKind::Path { qself: None, path } => {
            path.as_ident().map(|i| i.name.clone()).unwrap_or_default()
        }
        _ => String::new(),
    }
}

pub fn ident(i: &ast::Ident) -> String {
    if i.raw {
        format!("r#{}", i.name)
    } else {
        i.name.clone()
    }
}

fn member_str(m: &Member) -> String {
    match m {
        Member::Named(i) => ident(i),
        Member::Unnamed { index, .. } => index.to_string(),
    }
}

/// 括弧なしで `<=` の両辺に置ける式か。
fn simple(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Lit(_)
            | ExprKind::Path { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Call { .. }
            | ExprKind::Field { .. }
            | ExprKind::Index { .. }
            | ExprKind::Paren(_)
            | ExprKind::Erased { .. }
            | ExprKind::Perm(_)
    )
}

fn is_compound(op: BinOp) -> bool {
    use BinOp::*;
    matches!(
        op,
        AddAssign
            | SubAssign
            | MulAssign
            | DivAssign
            | RemAssign
            | BitXorAssign
            | BitAndAssign
            | BitOrAssign
            | ShlAssign
            | ShrAssign
    )
}

fn binop(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Rem => "%",
        And => "&&",
        Or => "||",
        BitXor => "^",
        BitAnd => "&",
        BitOr => "|",
        Shl => "<<",
        Shr => ">>",
        Eq => "==",
        Lt => "<",
        Le => "<=",
        Ne => "!=",
        Ge => ">=",
        Gt => ">",
        AddAssign => "+=",
        SubAssign => "-=",
        MulAssign => "*=",
        DivAssign => "/=",
        RemAssign => "%=",
        BitXorAssign => "^=",
        BitAndAssign => "&=",
        BitOrAssign => "|=",
        ShlAssign => "<<=",
        ShrAssign => ">>=",
        Implies => "==>",
        Iff => "<==>",
    }
}
