//! 式・文・パターン・型の書き下し（仕様 §7.3、§7.4）。文字列で組み立て、最後に
//! [`crate::print`] で整形する（`OPEN.md`）。

use std::collections::BTreeMap;

use ebykn_check::facts::{ExtraArg, RegionRef, SpanKey};
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

/// 抜ける文の行き先と、開いている `borrow mut`（[`Cx::exits`](crate::Cx)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitScope {
    /// ループ（`is_loop`）か、ラベル付きのブロック。
    Frame {
        label: Option<String>,
        is_loop: bool,
    },
    /// `borrow mut` のブロック。抜けるときに置く、権限を領域に戻す文（`r.insert(__eyk_perm_p);`）。
    BorrowMut { insert: String },
}

/// 関数の戻り値の型の形（`?` の失敗の値の作り方）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryKind {
    Option,
    Result,
}

/// 戻り値の型の最後の名前が `Option` / `Result` なら、その形。
pub fn try_kind(t: &ast::Type) -> Option<TryKind> {
    let TypeKind::Path { qself: None, path } = &t.kind else {
        return None;
    };
    match path.segments.last()?.ident.name.as_str() {
        "Option" => Some(TryKind::Option),
        "Result" => Some(TryKind::Result),
        _ => None,
    }
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
        crate::attrs(&p.attrs) + &self.pat_kind(p)
    }

    fn pat_kind(&self, p: &ast::Pat) -> String {
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
                            attrs,
                            member,
                            pat,
                            shorthand,
                            ..
                        } => {
                            let a = crate::attrs(attrs);
                            if *shorthand {
                                parts.push(format!("{a}{}", self.pat(pat)));
                            } else {
                                parts.push(format!("{a}{}: {}", member_str(member), self.pat(pat)));
                            }
                        }
                        FieldPat::Erased { attrs, ident: i } => {
                            let wrap = match self.facts.erased.get(&SpanKey::from(i.span)) {
                                Some(ErasedKind::Owns) => "Tracked",
                                _ => "Ghost",
                            };
                            parts.push(format!(
                                "{}{}: {wrap}({})",
                                crate::attrs(attrs),
                                ident(i),
                                ident(i)
                            ));
                        }
                        // `@..` は Rust の `..` に畳む。`..` の前には属性を書けない（Verus が
                        // 「expected identifier」で拒む）。パターンに付けて意味を持つ属性は
                        // `cfg` / `cfg_attr(.., cfg(..))` だけで、それは検査が E0003 にする（#95）。
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
            ExprKind::Block { label: None, block } => self.block(block),
            ExprKind::Block {
                label: Some(l),
                block,
            } => {
                self.exits.push(ExitScope::Frame {
                    label: Some(l.name.clone()),
                    is_loop: false,
                });
                let b = self.block(block);
                self.exits.pop();
                format!("'{}: {b}", l.name)
            }
            ExprKind::Unsafe(b) => format!("unsafe {}", self.block(b)),
            ExprKind::Const(b) => format!("const {}", self.block(b)),
            ExprKind::Break { label, expr } => {
                let l = label.as_ref().map(|l| l.name.as_str());
                let ins = self.jump_inserts(l);
                let v = expr.as_ref().map(|x| self.expr(x));
                let l = l.map(|l| format!(" '{l}")).unwrap_or_default();
                with_inserts(&ins, v, |v| {
                    format!("break{l}{}", v.map(|v| format!(" {v}")).unwrap_or_default())
                })
            }
            ExprKind::Continue { label } => {
                let l = label.as_ref().map(|l| l.name.as_str());
                let ins = self.jump_inserts(l);
                let l = l.map(|l| format!(" '{l}")).unwrap_or_default();
                with_inserts(&ins, None, |_| format!("continue{l}"))
            }
            ExprKind::Return(x) => {
                let ins = self.return_inserts();
                let v = x.as_ref().map(|x| self.expr(x));
                with_inserts(&ins, v, |v| self.ret(v))
            }
            ExprKind::Call { func, args, uses } => self.call(e, func, args, uses),
            ExprKind::MethodCall {
                receiver,
                method,
                turbofish,
                args,
                uses,
            } => self.method_call(e, receiver, method, turbofish.as_deref(), args, uses),
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
            ExprKind::Try(x) => self.try_(x),
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
                triggers,
                body,
            } => {
                if !triggers.is_empty() {
                    crate::eykp_only("#![trigger ..]");
                }
                self.quant(*kind, binders, body)
            }
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

    fn path_expr(&mut self, q: Option<&ast::QSelf>, p: &ast::Path) -> String {
        if q.is_none()
            && let Some(i) = p.as_ident()
        {
            if self.ensures
                && let Some(r) = self.rename.get(&i.name)
            {
                return r.clone();
            }
            return ident(i);
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

    /// 論理文脈の `p@`（`ensures` の `gives` の戻り値は名前を付け替える）。
    fn perm_term(&self, x: &str) -> String {
        let v = perm_var(x);
        if self.ensures
            && let Some(r) = self.rename.get(&v)
        {
            return r.clone();
        }
        v
    }

    /// 代入（`=`、`+=` など）。左辺が権限付きの `*X` なら `write` / `borrow_mut` にする。
    ///
    /// 文の属性（`#[allow(..)]` など）は、構文解析器が左辺に付ける。`write` / `borrow_mut` の形でも落とさずに
    /// 先頭に書く（PR #59 のレビュー。cfg は検査が E0003 にする）。
    fn assign(&mut self, lhs: &Expr, op: &str, rhs: String) -> String {
        let pre: String = lhs.attrs.iter().map(|a| crate::attr(a) + " ").collect();
        if let ExprKind::Unary {
            op: UnOp::Deref, ..
        } = &lhs.kind
            && let Some(x) = self.facts.derefs.get(&SpanKey::from(lhs.span)).cloned()
            && op == "="
            && !self.logic
        {
            return format!("{pre}{x}.write({}, {rhs})", self.perm_mut(&x));
        }
        if !self.logic && ebykn_check::body::deref_root(lhs).is_some() {
            let l = self.place(lhs);
            return format!("{pre}{l} {op} {rhs}");
        }
        // `expr` は式の属性を自分で書く。
        let l = self.expr(lhs);
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

    /// 呼び出しの末尾に足す消去される引数（callee の `uses` の宣言順）。検査が並べなかった領域も、
    /// 書いた順で最後に足す（黙って落とさない。受け取らない関数なら Verus が拒む）。
    fn extra_args(&self, span: ebykn_syntax::span::Span, uses: &[ast::RegionArg]) -> Vec<String> {
        let extra = self
            .facts
            .calls
            .get(&SpanKey::from(span))
            .cloned()
            .unwrap_or_default();
        let mut placed = vec![false; uses.len()];
        let mut out: Vec<String> = extra
            .iter()
            .filter_map(|a| match a {
                ExtraArg::Perm { var, mode } => Some(match mode {
                    PermMode::Move => format!("Tracked({})", perm_var(var)),
                    PermMode::Mut => self.perm_mut(var),
                    PermMode::Shared => self.perm_shared(var),
                }),
                ExtraArg::Region(i) => {
                    let u = uses.get(*i)?;
                    placed[*i] = true;
                    Some(self.region_arg(u))
                }
            })
            .collect();
        out.extend(
            uses.iter()
                .zip(&placed)
                .filter(|(_, p)| !**p)
                .map(|(u, _)| self.region_arg(u)),
        );
        out
    }

    /// 呼び出しで渡す領域（#19 の既定 P1）：`&mut r` ⇒ `Tracked(&mut r)`（`r` が `uses` で受けた参照なら
    /// `Tracked(&mut *r)`）、`&mut self@x` ⇒ `Tracked(self.x.borrow_mut())`、移動の `r` ⇒ `Tracked(r)`。
    fn region_arg(&self, u: &ast::RegionArg) -> String {
        let by_ref = self.facts.region_arg_refs.contains(&SpanKey::from(u.span));
        let inner = match (&u.region, u.mode) {
            (ast::Region::Var(v), PermMode::Move) => ident(v),
            (ast::Region::Var(v), PermMode::Shared) if by_ref => format!("&*{}", ident(v)),
            (ast::Region::Var(v), PermMode::Shared) => format!("&{}", ident(v)),
            (ast::Region::Var(v), PermMode::Mut) if by_ref => format!("&mut *{}", ident(v)),
            (ast::Region::Var(v), PermMode::Mut) => format!("&mut {}", ident(v)),
            (ast::Region::SelfField { field, .. }, PermMode::Shared) => {
                format!("self.{}.borrow()", ident(field))
            }
            (ast::Region::SelfField { field, .. }, PermMode::Mut) => {
                format!("self.{}.borrow_mut()", ident(field))
            }
            // ここに届くのは `unchecked fn` の中だけ（ほかは検査が E0519 で止める）。`self.x` は
            // `Tracked<Region<..>>` で、受け取る側の引数の型そのものなので、欄をそのまま渡す。
            (ast::Region::SelfField { field, .. }, PermMode::Move) => {
                return format!("self.{}", ident(field));
            }
        };
        format!("Tracked({inner})")
    }

    fn call(&mut self, e: &Expr, func: &Expr, args: &[Expr], uses: &[ast::RegionArg]) -> String {
        if let ExprKind::Path { qself: None, path } = &func.kind
            && let Some(f) = path.as_ident()
            && (f.is("old") || f.is("final"))
            && let [arg] = args
            && uses.is_empty()
        {
            // `old(*p)` / `final(*p)` は `old(p@).value()` / `final(p@).value()`。
            if let ExprKind::Unary {
                op: UnOp::Deref, ..
            } = &arg.kind
                && let Some(x) = self.facts.derefs.get(&SpanKey::from(arg.span)).cloned()
            {
                return format!("{}({}).value()", f.name, perm_var(&x));
            }
            // `old(x@f)` / `final(x@f)` は `old(x).f@` / `final(x).f@`（`old` / `final` は `&mut` の
            // 引数そのものにしか付けられない。`self@f` は `self.f@` なので、外に出す）。
            if matches!(arg.kind, ExprKind::Erased { .. }) {
                let mut fields = Vec::new();
                let mut root = arg;
                while let ExprKind::Erased { base, field } = &root.kind {
                    fields.push(format!(".{}@", ident(field)));
                    root = base;
                }
                fields.reverse();
                return format!("{}({}){}", f.name, self.expr(root), fields.concat());
            }
        }
        let f = self.expr(func);
        let mut a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
        a.extend(self.extra_args(e.span, uses));
        format!("{f}({})", a.join(", "))
    }

    fn method_call(
        &mut self,
        e: &Expr,
        receiver: &Expr,
        method: &ast::Ident,
        turbofish: Option<&[GenericArg]>,
        args: &[Expr],
        uses: &[ast::RegionArg],
    ) -> String {
        let key = SpanKey::from(e.span);
        if let Some(x) = self.facts.frees.get(&key).cloned() {
            return format!("{x}.free(Tracked({}))", perm_var(&x));
        }
        let recv = self.expr(receiver);
        if self.logic && args.is_empty() && turbofish.is_none() {
            // `v.view()` ⇒ `v@`（§7.4）。`.first()` / `.last()` は vstd のまま（#11 の決定：案 B）。
            if method.name == "view" {
                return format!("{recv}@");
            }
        }
        let mut a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
        a.extend(self.extra_args(e.span, uses));
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
        let saved = self.rename.clone();
        for p in &c.params {
            if let Some(i) = ebykn_check::index::pat_ident(p) {
                self.rename.remove(&i.name);
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
        // クロージャの中の `return` / `break` は外のブロックを抜けない。
        let exits = std::mem::take(&mut self.exits);
        let body = self.expr(&c.body);
        self.exits = exits;
        let s = format!(
            "{}|{}|{ret}{contract} {body}",
            if c.capture { "move " } else { "" },
            params.join(", "),
        );
        self.rename = saved;
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
        self.exits.push(ExitScope::Frame {
            label: l.label.as_ref().map(|x| x.name.clone()),
            is_loop: true,
        });
        let body = self.block(&l.body);
        self.exits.pop();
        format!("{label}{head}{clauses} {body}")
    }

    // ---- ブロックを抜ける文（#21 の既定 D） ----

    /// `break` / `continue` の前に置く、権限を戻す文（内側の `borrow mut` から順に）。行き先は
    /// ラベルが同じ枠、ラベルがなければいちばん内側のループ（`ebykn_check` の §6.2 と同じ）。
    /// 行き先より内側で開いた `borrow mut` だけを戻す。
    fn jump_inserts(&self, label: Option<&str>) -> Vec<String> {
        let target = self.exits.iter().rposition(|x| match (x, label) {
            (ExitScope::Frame { label: f, .. }, Some(l)) => f.as_deref() == Some(l),
            (ExitScope::Frame { is_loop, .. }, None) => *is_loop,
            (ExitScope::BorrowMut { .. }, _) => false,
        });
        match target {
            Some(t) => borrow_inserts(&self.exits[t + 1..]),
            None => Vec::new(),
        }
    }

    /// `return` / `?` の前に置く、権限を戻す文（開いているすべての `borrow mut`）。
    fn return_inserts(&self) -> Vec<String> {
        borrow_inserts(&self.exits)
    }

    /// `return v`（`gives` があれば権限を付ける）。
    fn ret(&self, v: Option<String>) -> String {
        match (&self.gives, v) {
            (Some(g), Some(v)) => format!("return ({v}, Tracked({}))", perm_var(g)),
            (_, Some(v)) => format!("return {v}"),
            (_, None) => "return".into(),
        }
    }

    /// `e?`。`borrow mut` の中では、失敗の経路で権限を戻してから返すよう、明示の `match` に展開する
    /// （形は関数の戻り値の型の `Option` / `Result`。どちらでもなければ `ebykn_check` が拒否している）。
    fn try_(&mut self, x: &Expr) -> String {
        let ins = self.return_inserts();
        let v = self.expr(x);
        if ins.is_empty() {
            return format!("{v}?");
        }
        let (ok, err, fail) = match self.try_kind {
            Some(TryKind::Option) => ("Some(__eyk_ok)", "None", "None"),
            Some(TryKind::Result) | None => (
                "Ok(__eyk_ok)",
                "Err(__eyk_err)",
                "Err(::core::convert::From::from(__eyk_err))",
            ),
        };
        let fail = with_inserts(&ins, None, |_| self.ret(Some(fail.into())));
        format!("match {v} {{ {ok} => __eyk_ok, {err} => {fail} }}")
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
        // 束縛した名前は、外の `gives` の付け替えを隠す（シャドウ）。
        let saved = self.rename.clone();
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
        }
        let body_s = self.expr(body);
        self.rename = saved;
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
            RegionRef::SelfField(f) => format!("self.{f}@"),
            RegionRef::Var(v) => v.clone(),
            RegionRef::At { old, region } => {
                let at = if *old { "old" } else { "final" };
                match &**region {
                    RegionRef::SelfField(f) => format!("{at}(self).{f}@"),
                    r => format!("{at}({})", self.region_logic(r)),
                }
            }
        }
    }

    /// 実行文脈の領域（変更 `borrow_mut()` / 読み `borrow()`）。
    fn region_exec(&self, r: &RegionRef, write: bool) -> String {
        match r {
            RegionRef::SelfField(f) => {
                format!("self.{f}.{}()", if write { "borrow_mut" } else { "borrow" })
            }
            RegionRef::Var(v) => v.clone(),
            // `old` / `final` を付けた領域は論理文脈の添字にしか現れない（`take` などの文は
            // `self@x` か領域変数だけを受け付ける）。
            RegionRef::At { region, .. } => self.region_exec(region, write),
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
            let rx = self.region_exec(&r, true);
            self.exits.push(ExitScope::BorrowMut {
                insert: format!("{rx}.insert({v});"),
            });
            let b = self.block(body);
            self.exits.pop();
            restore(&mut self.perms, &p, saved);
            format!(
                "{{ let tracked {v} = {rx}.remove({p}); let __eyk_res = {b}; #[allow(unreachable_code)] let __eyk_res = {{ proof {{ {rx}.insert({v}); }} __eyk_res }}; __eyk_res }}"
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
            StmtKind::Proof(_) => crate::eykp_only("証明の文"),
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

/// 枠の並びのうち `borrow mut` の戻す文を、内側から順に。
fn borrow_inserts(scopes: &[ExitScope]) -> Vec<String> {
    scopes
        .iter()
        .rev()
        .filter_map(|x| match x {
            ExitScope::BorrowMut { insert } => Some(insert.clone()),
            ExitScope::Frame { .. } => None,
        })
        .collect()
}

/// 抜ける文 `jump` の前に、権限を戻す文 `ins` を置く。値があれば先に評価して束縛する（値の式が
/// 借りている権限を読むことがある）。
fn with_inserts(
    ins: &[String],
    v: Option<String>,
    jump: impl FnOnce(Option<String>) -> String,
) -> String {
    if ins.is_empty() {
        return jump(v);
    }
    let proof = format!("proof {{ {} }}", ins.join(" "));
    match v {
        Some(v) => format!(
            "{{ let __eyk_exit = {v}; {proof} {} }}",
            jump(Some("__eyk_exit".into()))
        ),
        None => format!("{{ {proof} {} }}", jump(None)),
    }
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
        ExtEq => "=~=",
    }
}
