//! `Ptr<T>` の `T` がゼロサイズ型か（E0517。#12、`docs/current-spec.md` §7.1）。
//!
//! ゼロサイズ型の確保はどれも同じアドレスになり、`Ptr` では互いを区別できない。prelude は
//! `Ptr::new` / `Ptr::empty` の単相化の時点でコンパイルエラーにするが、検証（`ebykn verify`）は
//! 単相化しないので通ってしまう。そこで `.eyk` の上で確実にゼロサイズ型と分かる `T` をここで拒否する。
//!
//! 判定するのは次の位置の `T`：型の位置の `Ptr<T>`（別名の右辺、`Region<Ptr<T>>` の中を含む）、
//! パスのターボフィッシュ（`Ptr::<T>::new`）、型を書かない `Ptr::new(e)` の引数 `e`（`()`、
//! unit struct の値、ゼロサイズ型の構造体のリテラル、ゼロサイズ型の列挙のフィールドのない列挙子）。
//!
//! 確実にゼロサイズ型と分かる型は次のものだけ（それ以外は判定しない。判定しなかった型は、
//! ゼロサイズ型ならビルドで止まる）：
//! - `()` と、ゼロサイズ型だけの tuple
//! - 長さが `0` の配列と、ゼロサイズ型の配列
//! - std の `PhantomData<..>`（`std::marker::` / `core::marker::` を前に付けるか、裸の名前を
//!   モジュールの `use` で `std::marker` / `core::marker` から取り込んだもの）
//! - クレートの列挙で、`repr` がなく、列挙子がないか、列挙子が一つでフィールドがすべてゼロサイズ型の
//!   もの（`enum M { A }`）。`#[cfg]` の付いた列挙子があれば判定しない
//! - クレートの構造体で、実行時のフィールドがすべてゼロサイズ型のもの（フィールドのない構造体、
//!   unit struct、`owns` / `spec` だけの構造体を含む。消去されるフィールドは実行時に 0 バイト）。
//!   型の引数は使う側の型を代入して判定する（`Wrap<()>`）
//!
//! ジェネリックな `T`（型の引数）は判定しない。ジェネリックな関数や構造体の中の `Ptr<T>` を
//! ゼロサイズ型で使う場合は、ビルドで止まる。

use std::collections::BTreeMap;

use ebykn_syntax::ast::{
    self, Block, Expr, ExprKind, FieldPat, GenericArg, GenericArgs, ItemKind, Pat, PatKind, Path,
    Stmt, StmtKind, TraitItemKind, Type, TypeKind,
};
use ebykn_syntax::diag::{CheckMsg, Message};
use ebykn_syntax::span::Span;

use crate::Diags;
use crate::index::{self, EnumInfo, Import, Index, ItemPath, PreludeFn, Resolved, type_params};
use crate::project::Project;

/// 型の判定をたどる深さの上限（再帰する型の別名で止まるため）。
const MAX_DEPTH: usize = 32;

/// すべての名前を覆う印（名前には使えない文字列）。
const ALL: &str = "*";

/// 全モジュールの `Ptr<T>` を検査する。
pub fn check(project: &Project, index: &Index, diags: &mut Diags) {
    project.for_each_module(|module, file, items| {
        let mut w = Walker {
            index,
            module,
            text: &project.file(file).text,
            diags: &mut *diags,
            shadow: Vec::new(),
            self_ty: Vec::new(),
        };
        w.items(items);
    });
}

struct Walker<'a, 'd> {
    index: &'a Index,
    module: &'a [String],
    text: &'a str,
    diags: &'d mut Diags,
    /// 型の名前を覆う名前（型の引数、本体の中の項目と `use`）。覆われた名前はクレートの型に
    /// 解決しない。[`ALL`] はすべての名前を覆う（本体の中の glob の `use`）。
    shadow: Vec<String>,
    /// `impl` の Self 型。
    self_ty: Vec<Type>,
}

impl Walker<'_, '_> {
    fn env(&self) -> Env {
        self.shadow.iter().map(|n| (n.clone(), false)).collect()
    }

    fn zst(&self, ty: &Type) -> bool {
        let mut sizer = Sizer {
            index: self.index,
            visiting: Vec::new(),
        };
        sizer.is_zst(self.module, ty, &self.env(), self.self_ty.last(), 0)
    }

    /// 名前 `first` で始まるパスを、クレートと prelude の名前として引いてよいか。
    fn visible(&self, first: &str) -> bool {
        !self.shadow.iter().any(|n| n == first || n == ALL)
    }

    fn report(&mut self, span: Span) {
        let written = self
            .text
            .get(span.lo..span.hi)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        self.diags
            .push(span, Message::Check(CheckMsg::PtrZeroSized { ty: written }));
    }

    fn with_generics(&mut self, g: &ast::Generics, f: impl FnOnce(&mut Self)) {
        let n = self.shadow.len();
        self.shadow.extend(type_params(g));
        for p in &g.params {
            match p {
                ast::GenericParam::Type {
                    default: Some(t), ..
                } => self.ty(t),
                ast::GenericParam::Const { ty, default, .. } => {
                    self.ty(ty);
                    if let Some(e) = default {
                        self.expr(e);
                    }
                }
                _ => {}
            }
        }
        f(self);
        self.shadow.truncate(n);
    }

    fn items(&mut self, items: &[ast::Item]) {
        for item in items {
            self.item(item);
        }
    }

    fn item(&mut self, item: &ast::Item) {
        match &item.kind {
            ItemKind::Fn(f) => self.func(&f.sig, &f.contract, |w| match &f.body {
                Some(ast::FnBody::Block(b)) => w.block(b),
                Some(ast::FnBody::Pure(e)) => w.expr(e),
                None => {}
            }),
            ItemKind::SpecFn(f) => self.func(&f.sig, &f.contract, |w| {
                if let Some(e) = &f.body {
                    w.expr(e);
                }
            }),
            ItemKind::LemmaFn(f) => self.func(&f.sig, &f.contract, |_| {}),
            ItemKind::Struct(s) => self.with_generics(&s.generics, |w| {
                w.fields(&s.fields);
                for sec in [&s.owns, &s.spec].into_iter().flatten() {
                    for f in &sec.fields {
                        w.ty(&f.ty);
                    }
                }
            }),
            ItemKind::Enum(e) => self.with_generics(&e.generics, |w| {
                for v in &e.variants {
                    w.fields(&v.fields);
                    if let Some(d) = &v.discriminant {
                        w.expr(d);
                    }
                }
            }),
            ItemKind::Trait(t) => self.with_generics(&t.generics, |w| {
                for it in &t.items {
                    match &it.kind {
                        TraitItemKind::Fn(f) => w.func(&f.sig, &f.contract, |w| match &f.body {
                            Some(ast::FnBody::Block(b)) => w.block(b),
                            Some(ast::FnBody::Pure(e)) => w.expr(e),
                            None => {}
                        }),
                        TraitItemKind::SpecFn(f) => w.func(&f.sig, &f.contract, |w| {
                            if let Some(e) = &f.body {
                                w.expr(e);
                            }
                        }),
                        TraitItemKind::LemmaFn(f) => w.func(&f.sig, &f.contract, |_| {}),
                        TraitItemKind::Rust(_) => {}
                    }
                }
            }),
            ItemKind::Impl(imp) => self.with_generics(&imp.generics, |w| {
                w.ty(&imp.self_ty);
                if let Some(p) = &imp.trait_ {
                    w.path(p);
                }
                w.self_ty.push(imp.self_ty.clone());
                w.items(&imp.items);
                w.self_ty.pop();
            }),
            ItemKind::ImplSpec(imp) => self.with_generics(&imp.generics, |w| {
                w.path(&imp.trait_);
                if let Some(t) = &imp.self_ty {
                    w.ty(t);
                    w.self_ty.push(t.clone());
                }
                w.items(&imp.items);
                if imp.self_ty.is_some() {
                    w.self_ty.pop();
                }
            }),
            ItemKind::TypeAlias(t) => self.with_generics(&t.generics, |w| w.ty(&t.ty)),
            ItemKind::Subset(t) => self.with_generics(&t.generics, |w| {
                w.ty(&t.base);
                w.expr(&t.pred);
            }),
            ItemKind::Const(c) => self.with_generics(&c.generics, |w| {
                w.ty(&c.ty);
                if let Some(e) = &c.expr {
                    w.expr(e);
                }
            }),
            ItemKind::Static(s) => {
                self.ty(&s.ty);
                self.expr(&s.expr);
            }
            // インラインの `mod` の中は [`Project::for_each_module`] が別のモジュールとしてたどる。
            ItemKind::Mod(_) | ItemKind::Use(_) | ItemKind::Rust(_) => {}
        }
    }

    fn fields(&mut self, fields: &ast::Fields) {
        match fields {
            ast::Fields::Named(fs) | ast::Fields::Tuple(fs) => {
                for f in fs {
                    self.ty(&f.ty);
                }
            }
            ast::Fields::Unit => {}
        }
    }

    fn func(
        &mut self,
        sig: &ast::Signature,
        contract: &ast::Contract,
        body: impl FnOnce(&mut Self),
    ) {
        self.with_generics(&sig.generics, |w| {
            for p in &sig.params {
                match p {
                    ast::Param::Receiver { ty: Some(t), .. } => w.ty(t),
                    ast::Param::Receiver { ty: None, .. } => {}
                    ast::Param::Typed { pat, ty, .. } => {
                        w.pat(pat);
                        w.ty(ty);
                    }
                }
            }
            if let Some(r) = &sig.ret {
                w.ty(&r.ty);
            }
            w.contract(contract);
            body(w);
        });
    }

    fn contract(&mut self, c: &ast::Contract) {
        for clause in &c.clauses {
            match &clause.kind {
                ast::ClauseKind::Uses(us) => {
                    for u in us {
                        if let ast::UsesItem::Named { ty, .. } = u {
                            self.ty(ty);
                        }
                    }
                }
                ast::ClauseKind::Gives(_) | ast::ClauseKind::NoPanic => {}
                ast::ClauseKind::Requires(es)
                | ast::ClauseKind::Ensures(es)
                | ast::ClauseKind::Decreases(es) => {
                    for e in es {
                        self.expr(e);
                    }
                }
                ast::ClauseKind::Returns(e) => self.expr(e),
            }
        }
    }

    fn ty(&mut self, ty: &Type) {
        match &ty.kind {
            TypeKind::Path { qself, path } => {
                if let Some(q) = qself {
                    self.ty(&q.ty);
                }
                self.path(path);
            }
            TypeKind::Ref { elem, .. } | TypeKind::Slice(elem) | TypeKind::Paren(elem) => {
                self.ty(elem)
            }
            TypeKind::Tuple(ts) => {
                for t in ts {
                    self.ty(t);
                }
            }
            TypeKind::Array { elem, len } => {
                self.ty(elem);
                self.expr(len);
            }
            TypeKind::Never | TypeKind::Infer | TypeKind::Rust(_) => {}
        }
    }

    /// パスの型の引数をたどり、`Ptr<T>` / `Ptr::<T>` の `T` を判定する。
    fn path(&mut self, path: &Path) {
        for seg in &path.segments {
            match &seg.args {
                GenericArgs::None => {}
                GenericArgs::Angle { args, .. } => {
                    for a in args {
                        match a {
                            GenericArg::Type(t) | GenericArg::AssocType { ty: t, .. } => self.ty(t),
                            GenericArg::Const(e) => self.expr(e),
                            GenericArg::Lifetime(_) | GenericArg::Rust(_) => {}
                        }
                    }
                }
                GenericArgs::Paren { inputs, output, .. } => {
                    for t in inputs {
                        self.ty(t);
                    }
                    if let Some(t) = output {
                        self.ty(t);
                    }
                }
            }
        }
        for i in 0..path.segments.len() {
            let GenericArgs::Angle { args, .. } = &path.segments[i].args else {
                continue;
            };
            if !self.is_ptr(path, i + 1) {
                continue;
            }
            if let Some(GenericArg::Type(t)) = args.first()
                && self.zst(t)
            {
                self.report(t.span);
            }
        }
    }

    /// パスの先頭の `n` 個の名前が prelude の `Ptr` を指すか（型の別名はたどらない）。
    fn is_ptr(&self, path: &Path, n: usize) -> bool {
        let segs: Vec<String> = path.segments[..n]
            .iter()
            .map(|s| s.ident.name.clone())
            .collect();
        if !self.visible(&segs[0]) {
            return false;
        }
        self.index
            .resolve_exact(self.module, path.leading_colon, &segs, None)
            == Resolved::PreludeType("Ptr")
    }

    /// 型を書かない `Ptr::new(e)` の `e` がゼロサイズ型の値か。
    fn alloc_arg(&mut self, func: &Expr, args: &[Expr]) {
        let ExprKind::Path { qself: None, path } = &func.kind else {
            return;
        };
        let [.., owner, last] = path.segments.as_slice() else {
            return;
        };
        if !last.ident.is("new")
            || !matches!(owner.args, GenericArgs::None)
            || !self.is_ptr(path, path.segments.len() - 1)
        {
            return;
        }
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        let r = self
            .index
            .resolve_exact(self.module, path.leading_colon, &segs, None);
        if r != Resolved::Prelude(PreludeFn::Alloc { cell: false }) {
            return;
        }
        if let [arg] = args
            && let Some(span) = self.zst_value(arg)
        {
            self.report(span);
        }
    }

    /// 値の式がゼロサイズ型と確実に分かれば、その型を書いた位置（`()`、unit struct の名前、
    /// 構造体のリテラルのパス）。
    fn zst_value(&self, e: &Expr) -> Option<Span> {
        match &e.kind {
            ExprKind::Paren(inner) => self.zst_value(inner),
            ExprKind::Tuple(es) => es
                .iter()
                .all(|x| self.zst_value(x).is_some())
                .then_some(e.span),
            ExprKind::Path { qself: None, path } => {
                if let Some(s) = self.crate_struct(path) {
                    return (s.unit && self.zst(&path_type(path))).then_some(path.span);
                }
                self.enum_unit_value(path)
            }
            ExprKind::Struct(lit) if lit.qself.is_none() => {
                self.crate_struct(&lit.path)?;
                self.zst(&path_type(&lit.path)).then_some(lit.path.span)
            }
            _ => None,
        }
    }

    /// `M::A`（列挙 `M` のフィールドのない列挙子）で、`M` がゼロサイズ型なら `M` の位置。
    fn enum_unit_value(&self, path: &Path) -> Option<Span> {
        let [prefix @ .., last] = path.segments.as_slice() else {
            return None;
        };
        let first = prefix.first()?;
        if !self.visible(&first.ident.name) || !matches!(last.args, GenericArgs::None) {
            return None;
        }
        let segs: Vec<String> = prefix.iter().map(|s| s.ident.name.clone()).collect();
        let Resolved::Type(p) =
            self.index
                .resolve_exact(self.module, path.leading_colon, &segs, None)
        else {
            return None;
        };
        let e = self.index.enums.get(&p)?;
        if !e
            .variants
            .iter()
            .any(|(n, _, unit)| *unit && last.ident.is(n))
        {
            return None;
        }
        let ty_path = Path {
            leading_colon: path.leading_colon,
            segments: prefix.to_vec(),
            span: first.ident.span.to(prefix.last()?.ident.span),
        };
        self.zst(&path_type(&ty_path)).then_some(ty_path.span)
    }

    fn crate_struct(&self, path: &Path) -> Option<&crate::index::StructInfo> {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        if !self.visible(&segs[0]) {
            return None;
        }
        match self
            .index
            .resolve_exact(self.module, path.leading_colon, &segs, None)
        {
            Resolved::Type(p) => self.index.structs.get(&p),
            _ => None,
        }
    }

    fn block(&mut self, b: &Block) {
        // 本体の中の項目の名前は、その本体の中でモジュールの名前を覆う。
        let n = self.shadow.len();
        for s in &b.stmts {
            if let StmtKind::Item(item) = &s.kind {
                self.shadow.extend(item_names(item));
            }
        }
        for s in &b.stmts {
            self.stmt(s);
        }
        self.shadow.truncate(n);
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let(l) => {
                self.pat(&l.pat);
                if let Some(t) = &l.ty {
                    self.ty(t);
                }
                if let Some(e) = &l.init {
                    self.expr(e);
                }
                if let Some(b) = &l.else_ {
                    self.block(b);
                }
            }
            StmtKind::Item(item) => self.item(item),
            StmtKind::Expr { expr, .. } => self.expr(expr),
            StmtKind::Empty | StmtKind::Leak(_) => {}
            StmtKind::Take(r) | StmtKind::Put(r) => self.expr(&r.index),
            StmtKind::SpecAssign { target, value } => {
                self.expr(target);
                self.expr(value);
            }
        }
    }

    fn exprs(&mut self, es: &[Expr]) {
        for e in es {
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
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
            | ExprKind::Try(expr) => self.expr(expr),
            ExprKind::Block { block, .. } | ExprKind::Unsafe(block) | ExprKind::Const(block) => {
                self.block(block)
            }
            ExprKind::Break { expr, .. } | ExprKind::Return(expr) => {
                if let Some(x) = expr {
                    self.expr(x);
                }
            }
            ExprKind::Continue { .. }
            | ExprKind::Lit(_)
            | ExprKind::Infer
            | ExprKind::Rust(_)
            | ExprKind::Perm(_) => {}
            ExprKind::Call { func, args } => {
                self.alloc_arg(func, args);
                self.expr(func);
                self.exprs(args);
            }
            ExprKind::MethodCall {
                receiver,
                turbofish,
                args,
                ..
            } => {
                self.expr(receiver);
                for a in turbofish.iter().flatten() {
                    match a {
                        GenericArg::Type(t) | GenericArg::AssocType { ty: t, .. } => self.ty(t),
                        GenericArg::Const(e) => self.expr(e),
                        GenericArg::Lifetime(_) | GenericArg::Rust(_) => {}
                    }
                }
                self.exprs(args);
            }
            ExprKind::Cast { expr, ty } => {
                self.expr(expr);
                self.ty(ty);
            }
            ExprKind::Closure(c) => {
                for p in &c.params {
                    self.pat(p);
                }
                if let Some(r) = &c.ret {
                    self.ty(&r.ty);
                }
                self.contract(&c.contract);
                self.expr(&c.body);
            }
            ExprKind::Field { base, .. } | ExprKind::Erased { base, .. } => self.expr(base),
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::If { cond, then, else_ } => {
                self.expr(cond);
                self.block(then);
                if let Some(x) = else_ {
                    self.expr(x);
                }
            }
            ExprKind::Let { pat, expr } => {
                self.pat(pat);
                self.expr(expr);
            }
            ExprKind::Loop(l) => {
                match &l.kind {
                    ast::LoopKind::While(c) => self.expr(c),
                    ast::LoopKind::For { pat, iter } => {
                        self.pat(pat);
                        self.expr(iter);
                    }
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
                    self.pat(&a.pat);
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    self.expr(&a.body);
                }
            }
            ExprKind::Macro(m) => self.mac(m),
            ExprKind::Path { qself, path } => {
                if let Some(q) = qself {
                    self.ty(&q.ty);
                }
                self.path(path);
            }
            ExprKind::Range { start, end, .. } => {
                for x in [start, end].into_iter().flatten() {
                    self.expr(x);
                }
            }
            ExprKind::Struct(lit) => {
                if let Some(q) = &lit.qself {
                    self.ty(&q.ty);
                }
                self.path(&lit.path);
                for f in &lit.fields {
                    self.expr(&f.expr);
                }
                if let Some(Some(b)) = &lit.rest {
                    self.expr(b);
                }
            }
            ExprKind::Quant { binders, body, .. } => {
                for b in binders {
                    match &b.kind {
                        ast::BinderKind::Typed(t) => self.ty(t),
                        ast::BinderKind::In(x) => self.expr(x),
                        ast::BinderKind::Bare => {}
                    }
                }
                self.expr(body);
            }
            ExprKind::AndOr { exprs, .. } => self.exprs(exprs),
            ExprKind::Borrow { region, body, .. } => {
                self.expr(&region.index);
                self.block(body);
            }
        }
    }

    fn mac(&mut self, m: &ast::Macro) {
        match &m.args {
            ast::MacroArgs::Tokens => {}
            ast::MacroArgs::Exprs(es) => self.exprs(es),
            ast::MacroArgs::Map(kvs) => {
                for (k, v) in kvs {
                    self.expr(k);
                    self.expr(v);
                }
            }
        }
    }

    fn pat(&mut self, p: &Pat) {
        match &p.kind {
            PatKind::Ident { sub, .. } => {
                if let Some(s) = sub {
                    self.pat(s);
                }
            }
            PatKind::Wild | PatKind::Rest => {}
            PatKind::Lit(e) => self.expr(e),
            PatKind::Range { start, end, .. } => {
                for x in [start, end].into_iter().flatten() {
                    self.expr(x);
                }
            }
            PatKind::Path { qself, path } => {
                if let Some(q) = qself {
                    self.ty(&q.ty);
                }
                self.path(path);
            }
            PatKind::Tuple(ps) | PatKind::Or(ps) | PatKind::Slice(ps) => {
                for x in ps {
                    self.pat(x);
                }
            }
            PatKind::TupleStruct { qself, path, elems } => {
                if let Some(q) = qself {
                    self.ty(&q.ty);
                }
                self.path(path);
                for x in elems {
                    self.pat(x);
                }
            }
            PatKind::Struct {
                qself,
                path,
                fields,
                ..
            } => {
                if let Some(q) = qself {
                    self.ty(&q.ty);
                }
                self.path(path);
                for f in fields {
                    if let FieldPat::Field { pat, .. } = f {
                        self.pat(pat);
                    }
                }
            }
            PatKind::Ref { pat, .. } | PatKind::Paren(pat) => self.pat(pat),
            PatKind::Macro(m) => self.mac(m),
            PatKind::Type { pat, ty } => {
                self.pat(pat);
                self.ty(ty);
            }
        }
    }
}

/// 本体の中の項目が、型の名前空間に置く名前。glob の `use` は [`ALL`]。
fn item_names(item: &ast::Item) -> Vec<String> {
    let ident = match &item.kind {
        ItemKind::Struct(s) => &s.ident,
        ItemKind::Enum(e) => &e.ident,
        ItemKind::Trait(t) => &t.ident,
        ItemKind::TypeAlias(t) => &t.ident,
        ItemKind::Subset(t) => &t.ident,
        ItemKind::Mod(m) => &m.ident,
        ItemKind::Use(t) => {
            return index::parse_use(&t.tokens)
                .into_iter()
                .map(|i| match i {
                    Import::Name { alias, .. } => alias,
                    Import::Glob { .. } => ALL.to_string(),
                })
                .collect();
        }
        _ => return Vec::new(),
    };
    vec![ident.name.clone()]
}

/// 値のパス（`S` / `m::S`）を、同じ綴りの型として読む。
fn path_type(path: &Path) -> Type {
    Type {
        kind: TypeKind::Path {
            qself: None,
            path: path.clone(),
        },
        span: path.span,
    }
}

/// 型の引数の名前と、その引数がゼロサイズ型か。
type Env = BTreeMap<String, bool>;

/// 型がゼロサイズ型と確実に分かるかを判定する。
struct Sizer<'a> {
    index: &'a Index,
    /// 判定の途中の構造体。循環する定義（大きさが無限で rustc が拒否する）はゼロサイズ型としない。
    visiting: Vec<ItemPath>,
}

impl Sizer<'_> {
    fn is_zst(
        &mut self,
        module: &[String],
        ty: &Type,
        env: &Env,
        self_ty: Option<&Type>,
        depth: usize,
    ) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        match &ty.kind {
            TypeKind::Paren(t) => self.is_zst(module, t, env, self_ty, depth + 1),
            TypeKind::Tuple(ts) => ts
                .iter()
                .all(|t| self.is_zst(module, t, env, self_ty, depth + 1)),
            TypeKind::Array { elem, len } => {
                zero_len(len) || self.is_zst(module, elem, env, self_ty, depth + 1)
            }
            TypeKind::Path { qself: None, path } => {
                self.path_is_zst(module, path, env, self_ty, depth)
            }
            _ => false,
        }
    }

    fn path_is_zst(
        &mut self,
        module: &[String],
        path: &Path,
        env: &Env,
        self_ty: Option<&Type>,
        depth: usize,
    ) -> bool {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        if env.contains_key(ALL) {
            return false;
        }
        if let Some(v) = env.get(&segs[0]) {
            return segs.len() == 1 && matches!(path.segments[0].args, GenericArgs::None) && *v;
        }
        if segs == ["Self"] {
            return self_ty.is_some_and(|t| self.is_zst(module, t, env, None, depth + 1));
        }
        let r = self
            .index
            .resolve_exact(module, path.leading_colon, &segs, None);
        let args = type_args(path);
        match r {
            Resolved::Type(p) if !self.index.is_gated(&p) => {
                if let Some(a) = self.index.aliases.get(&p) {
                    let a = a.clone();
                    let Some(inner) = self.bind(module, &a.params, &args, env, self_ty, depth)
                    else {
                        return false;
                    };
                    return self.is_zst(&a.module, &a.ty, &inner, None, depth + 1);
                }
                let index = self.index;
                if let Some(e) = index.enums.get(&p) {
                    return self.enum_is_zst(module, &p, e, &args, env, self_ty, depth);
                }
                let Some(s) = index.structs.get(&p) else {
                    return false;
                };
                if self.visiting.contains(&p) {
                    return false;
                }
                let Some(inner) = self.bind(module, &s.params, &args, env, self_ty, depth) else {
                    return false;
                };
                self.visiting.push(p);
                let zst = s
                    .runtime
                    .iter()
                    .all(|t| self.is_zst(&s.module, t, &inner, None, depth + 1));
                self.visiting.pop();
                zst
            }
            Resolved::Unknown => self.is_phantom_data(module, &segs),
            _ => false,
        }
    }

    /// 列挙は、`repr` がなく、列挙子がない（値のない型）か、列挙子が一つでそのフィールドがすべて
    /// ゼロサイズ型なら、ゼロサイズ型（`enum M { A }`）。判別子（`A = 3`）は `repr` がなければ大きさを変えない。
    #[allow(clippy::too_many_arguments)]
    fn enum_is_zst(
        &mut self,
        module: &[String],
        p: &ItemPath,
        e: &EnumInfo,
        args: &[&Type],
        env: &Env,
        self_ty: Option<&Type>,
        depth: usize,
    ) -> bool {
        if e.opaque || self.visiting.contains(p) {
            return false;
        }
        let fields = match e.variants.as_slice() {
            [] => return true,
            [(_, fields, _)] => fields,
            _ => return false,
        };
        let Some(inner) = self.bind(module, &e.params, args, env, self_ty, depth) else {
            return false;
        };
        self.visiting.push(p.clone());
        let zst = fields
            .iter()
            .all(|t| self.is_zst(&e.module, t, &inner, None, depth + 1));
        self.visiting.pop();
        zst
    }

    /// std の `PhantomData` か。`std::marker::PhantomData` / `core::marker::PhantomData` と書くか、
    /// 裸の `PhantomData` をモジュールの `use` で `std::marker` / `core::marker` から取り込んだとき
    /// （`PhantomData` は std の prelude にないので、裸の名前は `use` がなければ解決しない）。
    fn is_phantom_data(&self, module: &[String], segs: &[String]) -> bool {
        let std_marker = |p: &[String]| {
            matches!(p.first().map(String::as_str), Some("std" | "core"))
                && p.get(1).map(String::as_str) == Some("marker")
        };
        match segs {
            [std, marker, name] => {
                std_marker(&[std.clone(), marker.clone()]) && name == "PhantomData"
            }
            [name] if name == "PhantomData" => {
                let Some(scope) = self.index.modules.get(module) else {
                    return false;
                };
                // 名前の `use` が glob より優先する。
                let named = scope.imports.iter().find_map(|i| match i {
                    Import::Name { alias, path } if alias == "PhantomData" => Some(path),
                    _ => None,
                });
                match named {
                    Some(path) => {
                        path.len() == 3 && std_marker(path) && path[2] == "PhantomData"
                    }
                    None => scope
                        .imports
                        .iter()
                        .any(|i| matches!(i, Import::Glob { path } if path.len() == 2 && std_marker(path))),
                }
            }
            _ => false,
        }
    }

    /// 定義の型の引数に、使う側の型の引数がゼロサイズ型かを結び付ける。数が合わなければ `None`。
    fn bind(
        &mut self,
        module: &[String],
        params: &[String],
        args: &[&Type],
        env: &Env,
        self_ty: Option<&Type>,
        depth: usize,
    ) -> Option<Env> {
        if params.len() != args.len() {
            return None;
        }
        Some(
            params
                .iter()
                .zip(args)
                .map(|(p, a)| (p.clone(), self.is_zst(module, a, env, self_ty, depth + 1)))
                .collect(),
        )
    }
}

/// パスの最後の名前の型の引数。
fn type_args(path: &Path) -> Vec<&Type> {
    match path.segments.last().map(|s| &s.args) {
        Some(GenericArgs::Angle { args, .. }) => args
            .iter()
            .filter_map(|g| match g {
                GenericArg::Type(t) => Some(t),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 配列の長さが `0` のリテラルか（`0`、`0usize`、`0_usize`）。
fn zero_len(len: &Expr) -> bool {
    let ExprKind::Lit(l) = &len.kind else {
        return false;
    };
    let digits = l.text.trim_end_matches("usize").replace('_', "");
    digits == "0"
}
