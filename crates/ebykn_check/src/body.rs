//! 関数本体の検査：権限スコープと線形性（仕様 §6.2）、領域添字の認識（§6.3）、
//! 消去フィールドの規則（E0402、E0513、E0515、E0516）、メソッド呼び出しの解決（E0514）。
//!
//! 環境 Γ（名前 → 状態）と領域変数の集合 R を持って文を順にたどる。読み方で迷った点は
//! `OPEN.md` に書いた（分岐の比べ方、`borrow` の借用の扱い、`gives` の戻し方など）。

use std::collections::{BTreeMap, BTreeSet};

use ebykn_syntax::ast::{
    self, Block, Expr, ExprKind, FieldPat, LoopKind, Member, PatKind, PermMode, Region,
    RegionIndex, StmtKind, UnOp,
};
use ebykn_syntax::diag::{CheckMsg, LoopPoint, Message, Mismatch, Op, PermState, Unimpl};
use ebykn_syntax::span::Span;

use crate::Diags;
use crate::facts::{ExtraArg, Facts, RegionRef};
use crate::index::{
    ErasedKind, FnInfo, Index, ItemPath, PreludeFn, Resolved, StructInfo, UseInfo, is_region_type,
    pat_ident,
};

/// Γ の一つの項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Perm {
    state: PermState,
    /// `borrow` ブロックで借りたものか（E0509）。
    block: bool,
}

type Env = BTreeMap<String, Perm>;

/// 二つの Γ の食い違い（名前の順）。
fn diff(a: &Env, b: &Env) -> Vec<Mismatch> {
    let names: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    names
        .into_iter()
        .filter_map(|n| {
            let sa = a.get(n).map_or(PermState::Absent, |p| p.state);
            let sb = b.get(n).map_or(PermState::Absent, |p| p.state);
            (sa != sb).then(|| Mismatch {
                name: n.clone(),
                a: sa,
                b: sb,
            })
        })
        .collect()
}

/// ループ・ラベル付きブロックの枠。
struct Frame {
    label: Option<String>,
    /// ループか（ラベル付きブロックでないか）。
    is_loop: bool,
    breaks: Vec<(Env, Span)>,
    continues: Vec<(Env, Span)>,
}

/// 関数の文脈。
pub struct FnCx<'a> {
    pub index: &'a Index,
    pub module: &'a [String],
    pub self_ty: Option<&'a ItemPath>,
    pub has_self: bool,
    /// 権限の規則（E0503〜E0512）を検査するか（`unchecked fn` と論理の本体では検査しない）。
    pub linear: bool,
    pub gives: Vec<String>,
}

pub struct Walker<'a> {
    cx: FnCx<'a>,
    diags: &'a mut Diags,
    facts: &'a mut Facts,
    env: Env,
    regions: BTreeSet<String>,
    /// 一度でも権限を持った名前（解放後の `X.free()` を E0504 にする）。
    known: BTreeSet<String>,
    /// 型を書いた変数（引数と型注釈付きの `let`）。
    var_types: BTreeMap<String, ast::Type>,
    frames: Vec<Frame>,
    /// クロージャの中なら、外の Γ と R。
    closure_outer: Option<(Env, BTreeSet<String>)>,
    /// 論理文脈（契約・`spec fn` の本体・`seq!` の引数など）か。
    logic: bool,
}

impl<'a> Walker<'a> {
    pub fn new(cx: FnCx<'a>, diags: &'a mut Diags, facts: &'a mut Facts) -> Self {
        Walker {
            cx,
            diags,
            facts,
            env: Env::new(),
            regions: BTreeSet::new(),
            known: BTreeSet::new(),
            var_types: BTreeMap::new(),
            frames: Vec::new(),
            closure_outer: None,
            logic: false,
        }
    }

    /// 引数と `uses` から入口の Γ と R を作る（仕様 §6.2 の「導入」）。
    pub fn enter(&mut self, sig: &ast::Signature, contract: &ast::Contract) {
        for p in &sig.params {
            if let ast::Param::Typed { pat, ty, .. } = p
                && let Some(i) = pat_ident(pat)
            {
                self.var_types.insert(i.name.clone(), ty.clone());
            }
        }
        for u in contract.uses() {
            match u {
                ast::UsesItem::Perm { mode, ident, .. } => {
                    let state = match mode {
                        PermMode::Move => PermState::Owned,
                        PermMode::Shared => PermState::Shared,
                        PermMode::Mut => PermState::Mut,
                    };
                    self.env.insert(
                        ident.name.clone(),
                        Perm {
                            state,
                            block: false,
                        },
                    );
                    self.known.insert(ident.name.clone());
                }
                ast::UsesItem::Named { ident, .. } => {
                    self.regions.insert(ident.name.clone());
                }
            }
        }
    }

    /// 契約の式（論理文脈）。
    pub fn contract(&mut self, contract: &ast::Contract) {
        let prev = std::mem::replace(&mut self.logic, true);
        for c in &contract.clauses {
            match &c.kind {
                ast::ClauseKind::Requires(es)
                | ast::ClauseKind::Ensures(es)
                | ast::ClauseKind::Decreases(es) => {
                    for e in es {
                        self.expr(e);
                    }
                }
                ast::ClauseKind::Returns(e) => {
                    self.expr(e);
                }
                _ => {}
            }
        }
        self.logic = prev;
    }

    /// 実行の本体。末尾で Γ を検査する（E0505）。
    pub fn body(&mut self, block: &Block) {
        let diverges = self.block(block);
        if !diverges {
            self.exit(block.span);
        }
    }

    /// 純粋関数の本体 `= e;`。
    pub fn pure_body(&mut self, e: &Expr) {
        let diverges = self.expr(e);
        if !diverges {
            self.exit(e.span);
        }
    }

    /// 論理の本体（`spec fn` の本体、`impl Invariant` の述語）。
    pub fn logic_expr(&mut self, e: &Expr) {
        let prev = std::mem::replace(&mut self.logic, true);
        self.expr(e);
        self.logic = prev;
    }

    fn push(&mut self, span: Span, m: CheckMsg) {
        let linear = matches!(
            m.code().as_str(),
            "E0503"
                | "E0504"
                | "E0505"
                | "E0506"
                | "E0507"
                | "E0508"
                | "E0509"
                | "E0510"
                | "E0511"
                | "E0512"
        );
        if linear && !self.cx.linear {
            return;
        }
        self.diags.push(span, Message::Check(m));
    }

    /// `return` / 本体の末尾 / `?`：所有している権限は `gives` の名前だけ（E0505）。
    fn exit(&mut self, span: Span) {
        if self.logic || self.closure_outer.is_some() {
            return;
        }
        let left: Vec<String> = self
            .env
            .iter()
            .filter(|(n, p)| p.state == PermState::Owned && !self.cx.gives.contains(n))
            .map(|(n, _)| n.clone())
            .collect();
        if !left.is_empty() {
            self.push(span, CheckMsg::PermLeft { names: left });
        }
        for g in self.cx.gives.clone() {
            let state = self.env.get(&g).map_or(PermState::Absent, |p| p.state);
            if state != PermState::Owned {
                self.push(
                    span,
                    CheckMsg::PermNotOwned {
                        name: g,
                        op: Op::Give,
                        state,
                    },
                );
            }
        }
    }

    /// クロージャの中で外の権限・領域を使ったら E0512。使ったなら `true`。
    fn closure_use(&mut self, name: &str, span: Span) -> bool {
        if let Some((env, regions)) = &self.closure_outer
            && (env.contains_key(name) || regions.contains(name))
        {
            self.push(
                span,
                CheckMsg::PermInClosure {
                    name: name.to_string(),
                },
            );
            return true;
        }
        false
    }

    /// 権限を消費する（`free`、`put`、移動、`leak`）。
    fn consume(&mut self, name: &str, op: Op, span: Span) {
        if self.closure_use(name, span) {
            return;
        }
        match self.env.get(name).copied() {
            Some(Perm {
                state: PermState::Owned,
                ..
            }) => {
                self.env.remove(name);
            }
            Some(Perm { block: true, .. }) => self.push(
                span,
                CheckMsg::ConsumeBorrowed {
                    name: name.to_string(),
                    op,
                },
            ),
            Some(p) => self.push(
                span,
                CheckMsg::PermNotOwned {
                    name: name.to_string(),
                    op,
                    state: p.state,
                },
            ),
            None => self.push(
                span,
                CheckMsg::PermNotOwned {
                    name: name.to_string(),
                    op,
                    state: PermState::Absent,
                },
            ),
        }
    }

    /// 権限を貸す（`uses &mut p@` / `uses &p@` の引数）。
    fn lend(&mut self, name: &str, mutable: bool, span: Span) {
        if self.closure_use(name, span) {
            return;
        }
        let op = if mutable {
            Op::MutBorrow
        } else {
            Op::SharedBorrow
        };
        match self.env.get(name).copied() {
            None => self.push(
                span,
                CheckMsg::PermNotOwned {
                    name: name.to_string(),
                    op,
                    state: PermState::Absent,
                },
            ),
            Some(p) if mutable && p.state == PermState::Shared => self.push(
                span,
                CheckMsg::WriteShared {
                    name: name.to_string(),
                },
            ),
            Some(_) => {}
        }
    }

    /// 権限を導入する（`take`、`Ptr::new`、`gives`）。既にあれば E0503。
    fn introduce(&mut self, name: &str, state: PermState, block: bool, span: Span) -> bool {
        if self.env.contains_key(name) {
            self.push(
                span,
                CheckMsg::PermAlreadyHeld {
                    name: name.to_string(),
                },
            );
            return false;
        }
        self.env.insert(name.to_string(), Perm { state, block });
        self.known.insert(name.to_string());
        true
    }

    /// 領域添字の領域を決める（仕様 §5.4、§6.3）。
    fn region(&mut self, ri: &RegionIndex, keyword: &'static str) -> Option<RegionRef> {
        match &ri.region {
            Region::SelfField { self_span, field } => {
                if !self.cx.has_self {
                    self.push(*self_span, CheckMsg::SelfErasedWithoutSelf);
                    return None;
                }
                if self.closure_use_region_self(field, ri.span) {
                    return None;
                }
                match self.self_struct().and_then(|s| s.erased_kind(&field.name)) {
                    Some(ErasedKind::Owns) => Some(RegionRef::SelfField(field.name.clone())),
                    Some(ErasedKind::Spec) => {
                        self.push(
                            ri.span,
                            CheckMsg::SpecFieldAsRegion {
                                field: field.name.clone(),
                            },
                        );
                        None
                    }
                    None => {
                        self.push(
                            field.span,
                            CheckMsg::NotARegion {
                                name: field.name.clone(),
                                keyword,
                            },
                        );
                        None
                    }
                }
            }
            Region::Var(v) => {
                if self.closure_use(&v.name, ri.span) {
                    return None;
                }
                if self.regions.contains(&v.name) {
                    Some(RegionRef::Var(v.name.clone()))
                } else {
                    self.push(
                        v.span,
                        CheckMsg::NotARegion {
                            name: v.name.clone(),
                            keyword,
                        },
                    );
                    None
                }
            }
        }
    }

    /// クロージャの中の `self@r[..]`（`r` が `owns`）は E0512。
    fn closure_use_region_self(&mut self, field: &ast::Ident, span: Span) -> bool {
        if self.closure_outer.is_some()
            && self.self_struct().and_then(|s| s.erased_kind(&field.name)) == Some(ErasedKind::Owns)
        {
            self.push(
                span,
                CheckMsg::PermInClosure {
                    name: format!("self@{}", field.name),
                },
            );
            return true;
        }
        false
    }

    fn self_struct(&self) -> Option<&'a StructInfo> {
        self.cx.index.structs.get(self.cx.self_ty?)
    }

    /// 領域添字の添字（識別子でなければ E0502）。
    fn index_ident(&mut self, ri: &RegionIndex) -> Option<String> {
        match &ri.index.kind {
            ExprKind::Path { qself: None, path } if path.as_ident().is_some() => {
                Some(path.as_ident()?.name.clone())
            }
            _ => {
                self.push(ri.index.span, CheckMsg::RegionIndexNotIdent);
                None
            }
        }
    }

    // ---- ブロックと文 ----

    /// ブロック。発散するなら `true`。
    fn block(&mut self, b: &Block) -> bool {
        let saved = (
            self.regions.clone(),
            self.known.clone(),
            self.var_types.clone(),
        );
        let mut diverges = false;
        for s in &b.stmts {
            if self.stmt(s) {
                diverges = true;
            }
        }
        (self.regions, self.known, self.var_types) = saved;
        diverges
    }

    fn stmt(&mut self, s: &ast::Stmt) -> bool {
        match &s.kind {
            StmtKind::Let(l) => self.local(l, s.span),
            StmtKind::Item(item) => {
                crate::check_items(
                    self.cx.index,
                    self.cx.module,
                    std::slice::from_ref(&**item),
                    self.diags,
                    self.facts,
                );
                false
            }
            StmtKind::Empty => false,
            StmtKind::Expr { expr, .. } => self.expr(expr),
            StmtKind::Take(ri) => {
                if let Some(r) = self.region(ri, "take") {
                    self.facts.region_stmts.insert(s.span.into(), r);
                    if let Some(x) = self.index_ident(ri) {
                        self.introduce(&x, PermState::Owned, false, ri.index.span);
                    }
                }
                false
            }
            StmtKind::Put(ri) => {
                if let Some(r) = self.region(ri, "put") {
                    self.facts.region_stmts.insert(s.span.into(), r);
                    if let Some(x) = self.index_ident(ri) {
                        self.consume(&x, Op::Put, ri.index.span);
                    }
                }
                false
            }
            StmtKind::Leak(x) => {
                self.consume(&x.name, Op::Leak, x.span);
                false
            }
            StmtKind::SpecAssign { target, value } => {
                self.spec_assign(target);
                let prev = std::mem::replace(&mut self.logic, true);
                self.expr(value);
                self.logic = prev;
                false
            }
        }
    }

    /// `x@f = e;` の左辺：`self` のない関数なら E0515、`owns` フィールドなら E0516。
    fn spec_assign(&mut self, target: &Expr) {
        let ExprKind::Erased { base, field } = &target.kind else {
            return;
        };
        let info = match &base.kind {
            ExprKind::Path { qself: None, path }
                if path.as_ident().is_some_and(|i| i.is("self")) =>
            {
                if !self.cx.has_self {
                    self.push(base.span, CheckMsg::SelfErasedWithoutSelf);
                    return;
                }
                self.self_struct()
            }
            ExprKind::Path { qself: None, path } => path
                .as_ident()
                .and_then(|i| self.var_types.get(&i.name))
                .and_then(|t| self.struct_of_type(t)),
            _ => None,
        };
        if info.and_then(|s| s.erased_kind(&field.name)) == Some(ErasedKind::Owns) {
            self.push(
                field.span,
                CheckMsg::AssignOwnsField {
                    field: field.name.clone(),
                },
            );
        }
    }

    fn struct_of_type(&self, ty: &ast::Type) -> Option<&'a StructInfo> {
        let ty = match &ty.kind {
            ast::TypeKind::Ref { elem, .. } => elem,
            _ => ty,
        };
        let p = self
            .cx
            .index
            .type_path(self.cx.module, ty, self.cx.self_ty)?;
        self.cx.index.structs.get(&p)
    }

    fn local(&mut self, l: &ast::Local, span: Span) -> bool {
        let mut diverges = false;
        if let Some(init) = &l.init {
            diverges = self.expr(init);
        }
        if let Some(else_) = &l.else_ {
            // `let-else` の `else` は発散する（Rust の規則）。その Γ は比べない。
            let saved = self.env.clone();
            self.block(else_);
            self.env = saved;
        }
        let alloc = l.init.as_ref().and_then(|e| self.perm_source(e));
        match (pat_ident(&l.pat), alloc) {
            (Some(x), Some(_)) => {
                if self.env.contains_key(&x.name) {
                    self.push(
                        x.span,
                        CheckMsg::ShadowHeld {
                            name: x.name.clone(),
                        },
                    );
                } else {
                    self.introduce(&x.name, PermState::Owned, false, x.span);
                    self.facts.perm_lets.insert(span.into(), x.name.clone());
                }
            }
            _ => self.bind(&l.pat),
        }
        if let (Some(x), Some(ty)) = (pat_ident(&l.pat), &l.ty) {
            self.var_types.insert(x.name.clone(), ty.clone());
            if is_region_type(ty) {
                self.regions.insert(x.name.clone());
            }
        }
        diverges
    }

    /// 権限を返す呼び出し（`Ptr::new(..)` や `gives` を持つ関数）か。
    fn perm_source(&self, e: &Expr) -> Option<()> {
        let ExprKind::Call { func, .. } = &e.kind else {
            return None;
        };
        let ExprKind::Path { qself: None, path } = &func.kind else {
            return None;
        };
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        let r = self
            .cx
            .index
            .resolve(self.cx.module, path.leading_colon, &segs, self.cx.self_ty);
        match &r {
            Resolved::Prelude(PreludeFn::Alloc { .. }) => Some(()),
            _ => self
                .cx
                .index
                .callee(&r)
                .filter(|f| !f.gives.is_empty())
                .map(|_| ()),
        }
    }

    /// パターンの束縛：Γ にある名前なら E0507。消去フィールドのパターンを検査する。
    fn bind(&mut self, p: &ast::Pat) {
        match &p.kind {
            PatKind::Ident { ident, sub, .. } => {
                if self.env.contains_key(&ident.name) {
                    self.push(
                        ident.span,
                        CheckMsg::ShadowHeld {
                            name: ident.name.clone(),
                        },
                    );
                }
                self.known.remove(&ident.name);
                self.regions.remove(&ident.name);
                self.var_types.remove(&ident.name);
                if let Some(s) = sub {
                    self.bind(s);
                }
            }
            PatKind::Tuple(ps) | PatKind::Or(ps) | PatKind::Slice(ps) => {
                for q in ps {
                    self.bind(q);
                }
            }
            PatKind::TupleStruct { elems, .. } => {
                for q in elems {
                    self.bind(q);
                }
            }
            PatKind::Ref { pat, .. } | PatKind::Paren(pat) => self.bind(pat),
            PatKind::Type { pat, .. } => self.bind(pat),
            PatKind::Struct {
                qself: None,
                path,
                fields,
                ..
            } => {
                let info = self.struct_of_path(path);
                let mut named = BTreeSet::new();
                let mut rest = false;
                for f in fields {
                    match f {
                        FieldPat::Field { pat, .. } => self.bind(pat),
                        FieldPat::Erased { ident, .. } => {
                            named.insert(ident.name.clone());
                            match info.and_then(|s| s.erased_kind(&ident.name)) {
                                Some(k) => {
                                    self.facts.erased.insert(ident.span.into(), k);
                                    self.bind_ident(ident);
                                    if k == ErasedKind::Owns {
                                        self.regions.insert(ident.name.clone());
                                    }
                                }
                                None => self.push(
                                    ident.span,
                                    CheckMsg::NotErasedField {
                                        ty: path_text(path),
                                        field: ident.name.clone(),
                                    },
                                ),
                            }
                        }
                        FieldPat::ErasedRest { .. } => rest = true,
                    }
                }
                if let Some(s) = info
                    && !rest
                {
                    for (name, kind, _) in &s.erased {
                        if *kind == ErasedKind::Owns && !named.contains(name) {
                            self.push(
                                p.span,
                                CheckMsg::MissingErasedField {
                                    ty: path_text(path),
                                    field: name.clone(),
                                    pattern: true,
                                },
                            );
                        }
                    }
                }
            }
            PatKind::Struct { fields, .. } => {
                for f in fields {
                    if let FieldPat::Field { pat, .. } = f {
                        self.bind(pat);
                    }
                }
            }
            _ => {}
        }
    }

    fn bind_ident(&mut self, ident: &ast::Ident) {
        if self.env.contains_key(&ident.name) {
            self.push(
                ident.span,
                CheckMsg::ShadowHeld {
                    name: ident.name.clone(),
                },
            );
        }
        self.known.remove(&ident.name);
        self.var_types.remove(&ident.name);
    }

    fn struct_of_path(&self, path: &ast::Path) -> Option<&'a StructInfo> {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        let p = if segs == ["Self"] {
            self.cx.self_ty?.clone()
        } else {
            match self
                .cx
                .index
                .resolve(self.cx.module, path.leading_colon, &segs, self.cx.self_ty)
            {
                Resolved::Type(p) => p,
                _ => return None,
            }
        };
        self.cx.index.structs.get(&p)
    }

    // ---- 式 ----

    /// 式。発散するなら `true`。
    fn expr(&mut self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Array(es) | ExprKind::Tuple(es) => self.exprs(es),
            ExprKind::Repeat { expr, len } => self.expr(expr) | self.expr(len),
            ExprKind::Assign { lhs, rhs } => {
                let d = self.expr(rhs);
                self.place(lhs, true);
                d
            }
            ExprKind::Binary { op, lhs, rhs, .. } => {
                if is_compound_assign(*op) {
                    let d = self.expr(rhs);
                    self.place(lhs, false);
                    d
                } else {
                    self.expr(lhs) | self.expr(rhs)
                }
            }
            ExprKind::Unary { op, expr } => {
                if *op == UnOp::Deref
                    && let Some(x) = ident_of(expr)
                {
                    self.deref(x, e.span);
                    return false;
                }
                self.expr(expr)
            }
            ExprKind::Ref { expr, .. } | ExprKind::Paren(expr) | ExprKind::Cast { expr, .. } => {
                self.expr(expr)
            }
            ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::Block { label, block } => match label {
                Some(l) => self.labeled_block(&l.name, block),
                None => self.block(block),
            },
            ExprKind::Unsafe(b) | ExprKind::Const(b) => self.block(b),
            ExprKind::Break { label, expr } => {
                if let Some(x) = expr {
                    self.expr(x);
                }
                self.jump(label.as_ref().map(|l| l.name.as_str()), false, e.span);
                true
            }
            ExprKind::Continue { label } => {
                self.jump(label.as_ref().map(|l| l.name.as_str()), true, e.span);
                true
            }
            ExprKind::Return(x) => {
                if let Some(x) = x {
                    self.expr(x);
                }
                self.exit(e.span);
                true
            }
            ExprKind::Try(x) => {
                let d = self.expr(x);
                // `?` の早期 return は `return` と同じ扱い（仕様 §6.2）。
                self.exit(e.span);
                d
            }
            ExprKind::Call { func, args } => self.call(e, func, args),
            ExprKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => self.method_call(e, receiver, method, args),
            ExprKind::Closure(c) => {
                self.closure(c);
                false
            }
            ExprKind::Index { base, index } => {
                let d = self.expr(index);
                if let Some(r) = self.logical_region(base) {
                    self.facts.region_index.insert(e.span.into(), r);
                    return d;
                }
                d | self.expr(base)
            }
            ExprKind::If { cond, then, else_ } => self.if_expr(e, cond, then, else_.as_deref()),
            ExprKind::Let { pat, expr } => {
                let d = self.expr(expr);
                self.bind(pat);
                d
            }
            ExprKind::Loop(l) => self.loop_expr(e, l),
            ExprKind::Match { expr, arms } => self.match_expr(e, expr, arms),
            ExprKind::Macro(m) => self.macro_call(m),
            ExprKind::Range { start, end, .. } => {
                let mut d = false;
                for x in [start, end].into_iter().flatten() {
                    d |= self.expr(x);
                }
                d
            }
            ExprKind::Struct(lit) => self.struct_lit(e, lit),
            ExprKind::Quant { binders, body, .. } => {
                let prev = std::mem::replace(&mut self.logic, true);
                for b in binders {
                    if let ast::BinderKind::In(x) = &b.kind {
                        self.expr(x);
                    }
                }
                self.expr(body);
                self.logic = prev;
                false
            }
            ExprKind::AndOr { exprs, .. } => {
                let prev = std::mem::replace(&mut self.logic, true);
                self.exprs(exprs);
                self.logic = prev;
                false
            }
            ExprKind::Perm(x) => {
                self.closure_use(&x.name, x.span);
                false
            }
            ExprKind::Erased { base, .. } => {
                if is_self(base) {
                    if !self.cx.has_self {
                        self.push(base.span, CheckMsg::SelfErasedWithoutSelf);
                    }
                    false
                } else {
                    self.expr(base)
                }
            }
            ExprKind::Borrow {
                mutable,
                region,
                body,
            } => self.borrow(e, *mutable, region, body),
            ExprKind::Path { .. } | ExprKind::Lit(_) | ExprKind::Infer | ExprKind::Rust(_) => false,
        }
    }

    fn exprs(&mut self, es: &[Expr]) -> bool {
        let mut d = false;
        for e in es {
            d |= self.expr(e);
        }
        d
    }

    /// `*X`。`X ∈ Γ` なら権限付きの読みとして記録する。
    fn deref(&mut self, x: &ast::Ident, span: Span) {
        if self.closure_use(&x.name, span) {
            return;
        }
        if self.env.contains_key(&x.name) {
            self.facts.derefs.insert(span.into(), x.name.clone());
        }
    }

    /// 代入の左辺。`assign` は `=`（`+=` などでなければ）。
    fn place(&mut self, lhs: &Expr, assign: bool) {
        match &lhs.kind {
            ExprKind::Path { qself: None, path } if path.as_ident().is_some() => {
                let x = path.as_ident().map(|i| i.name.clone()).unwrap_or_default();
                if self.closure_use(&x, lhs.span) {
                    return;
                }
                if self.env.contains_key(&x) && !self.logic {
                    self.push(lhs.span, CheckMsg::AssignHeld { name: x });
                }
            }
            ExprKind::Erased { .. } => {
                self.spec_assign(lhs);
            }
            _ => {
                // `*X = e`、`(*X).f = e`、`(*X).f.g[i] = e`。
                if let Some((x, deref_span)) = deref_root(lhs) {
                    if self.closure_use(&x.name, deref_span) {
                        return;
                    }
                    if let Some(p) = self.env.get(&x.name).copied() {
                        if p.state == PermState::Shared {
                            self.push(
                                lhs.span,
                                CheckMsg::WriteShared {
                                    name: x.name.clone(),
                                },
                            );
                        }
                        self.facts.derefs.insert(deref_span.into(), x.name.clone());
                    }
                    self.place_indices(lhs);
                    return;
                }
                let _ = assign;
                self.expr(lhs);
            }
        }
    }

    /// 左辺の中の添字の式をたどる。
    fn place_indices(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Field { base, .. } | ExprKind::Paren(base) => self.place_indices(base),
            ExprKind::Index { base, index } => {
                self.expr(index);
                self.place_indices(base);
            }
            _ => {}
        }
    }

    /// 論理の領域添字 `r[p]` の `r`（`self@x` の `x` が `owns`、または領域変数）。
    fn logical_region(&mut self, base: &Expr) -> Option<RegionRef> {
        match &base.kind {
            ExprKind::Erased { base: b, field } if is_self(b) => {
                if !self.cx.has_self {
                    self.push(b.span, CheckMsg::SelfErasedWithoutSelf);
                    return None;
                }
                if self.closure_use_region_self(field, base.span) {
                    return None;
                }
                (self.self_struct()?.erased_kind(&field.name)? == ErasedKind::Owns)
                    .then(|| RegionRef::SelfField(field.name.clone()))
            }
            ExprKind::Path { qself: None, path } => {
                let i = path.as_ident()?;
                if self.closure_use(&i.name, base.span) {
                    return None;
                }
                self.regions
                    .contains(&i.name)
                    .then(|| RegionRef::Var(i.name.clone()))
            }
            _ => None,
        }
    }

    fn call(&mut self, e: &Expr, func: &Expr, args: &[Expr]) -> bool {
        let mut d = false;
        let resolved = match &func.kind {
            ExprKind::Path { qself: None, path } => {
                let segs: Vec<String> =
                    path.segments.iter().map(|s| s.ident.name.clone()).collect();
                Some(self.cx.index.resolve(
                    self.cx.module,
                    path.leading_colon,
                    &segs,
                    self.cx.self_ty,
                ))
            }
            _ => {
                d |= self.expr(func);
                None
            }
        };
        d |= self.exprs(args);
        if let Some(info) = resolved.as_ref().and_then(|r| self.cx.index.callee(r)) {
            let info = info.clone();
            // `Type::method(recv, ..)` の形では、最初の引数が受け手。
            let args = if info.has_receiver {
                args.get(1..).unwrap_or_default()
            } else {
                args
            };
            self.apply_uses(e.span, &info, args);
        }
        if let Some(Resolved::Prelude(_)) = resolved {
            // `Ptr::new` などは `let` で受ける（[`Self::local`]）。
        }
        d
    }

    fn method_call(
        &mut self,
        e: &Expr,
        receiver: &Expr,
        method: &ast::Ident,
        args: &[Expr],
    ) -> bool {
        let mut d = self.exprs(args);
        if is_self(receiver) {
            // `self.f(..)`（`impl T` の中）は `T` の impl で引く。
            if let Some(info) = self
                .cx
                .self_ty
                .and_then(|t| self.cx.index.methods.get(t))
                .and_then(|ms| ms.get(&method.name))
            {
                let info = info.clone();
                self.apply_uses(e.span, &info, args);
                return d;
            }
        } else if let Some(x) = ident_of(receiver) {
            let held = self.env.contains_key(&x.name)
                || self.known.contains(&x.name)
                || self
                    .closure_outer
                    .as_ref()
                    .is_some_and(|(env, _)| env.contains_key(&x.name));
            if method.is("free") && args.is_empty() && held {
                if !self.logic {
                    self.consume(&x.name, Op::Free, e.span);
                    self.facts.frees.insert(e.span.into(), x.name.clone());
                }
                return d;
            }
            if self.env.contains_key(&x.name) {
                // `x ∈ Γ` なら prelude の `Ptr` / `Cell` のメソッド（仕様 §6.5）。
                return d;
            }
        } else {
            d |= self.expr(receiver);
        }
        if self.cx.index.perm_method_names.contains(&method.name) {
            self.push(
                method.span,
                CheckMsg::MethodCallUses {
                    method: method.name.clone(),
                },
            );
        }
        d
    }

    /// 呼び出しの引数に `uses` を当てる（仕様 §6.2 の「消費」）。
    fn apply_uses(&mut self, span: Span, info: &FnInfo, args: &[Expr]) {
        let mut extra = Vec::new();
        for u in &info.uses {
            match u {
                UseInfo::Perm {
                    name, param, mode, ..
                } => {
                    let Some(x) = args.get(*param).and_then(ident_of) else {
                        let at = args.get(*param).map_or(span, |a| a.span);
                        self.push(
                            at,
                            CheckMsg::UsesArgNotVar {
                                param: name.clone(),
                            },
                        );
                        continue;
                    };
                    if !self.logic {
                        match mode {
                            PermMode::Move => self.consume(&x.name, Op::Move, x.span),
                            PermMode::Mut => self.lend(&x.name, true, x.span),
                            PermMode::Shared => self.lend(&x.name, false, x.span),
                        }
                    }
                    extra.push(ExtraArg {
                        var: x.name.clone(),
                        mode: *mode,
                    });
                }
                UseInfo::Named { .. } => {
                    self.diags.push(
                        span,
                        Message::Check(CheckMsg::Unimplemented(Unimpl::NamedUsesCall)),
                    );
                }
            }
        }
        if !extra.is_empty() {
            self.facts.calls.insert(span.into(), extra);
        }
    }

    fn closure(&mut self, c: &ast::Closure) {
        // クロージャの中では Γ と R を空にする（仕様 §6.2、E0512）。
        let prev_outer = self.closure_outer.clone();
        let outer = match prev_outer.clone() {
            Some((mut env, mut regions)) => {
                env.extend(self.env.clone());
                regions.extend(self.regions.clone());
                (env, regions)
            }
            None => (self.env.clone(), self.regions.clone()),
        };
        let saved_env = std::mem::take(&mut self.env);
        let saved_regions = std::mem::take(&mut self.regions);
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_known = self.known.clone();
        let saved_types = self.var_types.clone();
        self.closure_outer = Some(outer);
        for p in &c.params {
            self.bind(p);
        }
        let prev = std::mem::replace(&mut self.logic, true);
        self.contract(&c.contract);
        self.logic = prev;
        self.expr(&c.body);
        self.env = saved_env;
        self.regions = saved_regions;
        self.frames = saved_frames;
        self.known = saved_known;
        self.var_types = saved_types;
        self.closure_outer = prev_outer;
    }

    fn borrow(&mut self, e: &Expr, mutable: bool, ri: &RegionIndex, body: &Block) -> bool {
        let keyword = if mutable { "borrow mut" } else { "borrow" };
        let region = self.region(ri, keyword);
        let x = region.as_ref().and_then(|_| self.index_ident(ri));
        if let Some(r) = region {
            self.facts.region_stmts.insert(e.span.into(), r);
        }
        let introduced = match &x {
            Some(x) => {
                let state = if mutable {
                    PermState::Mut
                } else {
                    PermState::Shared
                };
                self.introduce(x, state, true, ri.index.span)
            }
            None => false,
        };
        let d = self.block(body);
        if introduced && let Some(x) = x {
            self.env.remove(&x);
        }
        d
    }

    fn if_expr(&mut self, e: &Expr, cond: &Expr, then: &Block, else_: Option<&Expr>) -> bool {
        self.expr(cond);
        let start = self.env.clone();
        let d1 = self.block(then);
        let env1 = std::mem::replace(&mut self.env, start.clone());
        let (env2, d2) = match else_ {
            Some(x) => {
                let d = self.expr(x);
                (std::mem::take(&mut self.env), d)
            }
            None => (start, false),
        };
        self.merge(e.span, vec![(env1, d1), (env2, d2)])
    }

    fn match_expr(&mut self, e: &Expr, scrutinee: &Expr, arms: &[ast::Arm]) -> bool {
        self.expr(scrutinee);
        let start = self.env.clone();
        let mut ends = Vec::new();
        for arm in arms {
            self.env = start.clone();
            let saved = (
                self.regions.clone(),
                self.known.clone(),
                self.var_types.clone(),
            );
            self.bind(&arm.pat);
            if let Some(g) = &arm.guard {
                self.expr(g);
            }
            let d = self.expr(&arm.body);
            (self.regions, self.known, self.var_types) = saved;
            ends.push((std::mem::take(&mut self.env), d));
        }
        if ends.is_empty() {
            // 腕のない match は発散する。
            self.env = start;
            return true;
        }
        self.merge(e.span, ends)
    }

    /// 分岐の終端の Γ を比べる（E0510）。発散した分岐は除く。全部発散したら `true`。
    fn merge(&mut self, span: Span, ends: Vec<(Env, bool)>) -> bool {
        let mut live = ends.into_iter().filter(|(_, d)| !*d).map(|(env, _)| env);
        let Some(first) = live.next() else {
            return true;
        };
        // 食い違ったら、全分岐で一致する項目だけを残して続ける（同じ誤りを何度も出さない）。
        let mut common = first.clone();
        for other in live {
            let diffs = diff(&first, &other);
            if !diffs.is_empty() {
                self.push(span, CheckMsg::BranchMismatch { diffs });
            }
            common.retain(|n, p| other.get(n) == Some(p));
        }
        self.env = common;
        false
    }

    fn loop_expr(&mut self, e: &Expr, l: &ast::Loop) -> bool {
        let saved = (
            self.regions.clone(),
            self.known.clone(),
            self.var_types.clone(),
        );
        if let LoopKind::For { iter, .. } = &l.kind {
            self.expr(iter);
        }
        let entry = self.env.clone();
        {
            let prev = std::mem::replace(&mut self.logic, true);
            for c in &l.clauses {
                self.exprs(&c.exprs);
            }
            self.logic = prev;
        }
        match &l.kind {
            LoopKind::While(cond) => {
                self.expr(cond);
            }
            LoopKind::For { pat, .. } => self.bind(pat),
            LoopKind::Loop => {}
        }
        self.frames.push(Frame {
            label: l.label.as_ref().map(|x| x.name.clone()),
            is_loop: true,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        let d = self.block(&l.body);
        let frame = self.frames.pop().expect("frame");
        (self.regions, self.known, self.var_types) = saved;
        if !d {
            let diffs = diff(&entry, &self.env);
            if !diffs.is_empty() {
                self.push(
                    l.body.span,
                    CheckMsg::LoopMismatch {
                        diffs,
                        at: LoopPoint::End,
                    },
                );
            }
        }
        for (env, span) in &frame.continues {
            let diffs = diff(&entry, env);
            if !diffs.is_empty() {
                self.push(
                    *span,
                    CheckMsg::LoopMismatch {
                        diffs,
                        at: LoopPoint::Continue,
                    },
                );
            }
        }
        let after = match &l.kind {
            // `while` / `for` は条件で抜けるので、ループの後の Γ は入口と同じ。
            LoopKind::While(_) | LoopKind::For { .. } => entry,
            LoopKind::Loop => match frame.breaks.first() {
                Some((env, _)) => env.clone(),
                None => {
                    self.env = entry;
                    let _ = e;
                    return true;
                }
            },
        };
        for (env, span) in &frame.breaks {
            let diffs = diff(&after, env);
            if !diffs.is_empty() {
                self.push(
                    *span,
                    CheckMsg::LoopMismatch {
                        diffs,
                        at: LoopPoint::Break,
                    },
                );
            }
        }
        self.env = after;
        false
    }

    fn labeled_block(&mut self, label: &str, b: &Block) -> bool {
        self.frames.push(Frame {
            label: Some(label.to_string()),
            is_loop: false,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        let d = self.block(b);
        let frame = self.frames.pop().expect("frame");
        let mut ends: Vec<(Env, bool)> = Vec::new();
        ends.push((std::mem::take(&mut self.env), d));
        ends.extend(frame.breaks.into_iter().map(|(env, _)| (env, false)));
        self.merge(b.span, ends)
    }

    /// `break` / `continue`：Γ を対象の枠に記録する。
    fn jump(&mut self, label: Option<&str>, is_continue: bool, span: Span) {
        let env = self.env.clone();
        let frame = match label {
            Some(l) => self
                .frames
                .iter_mut()
                .rev()
                .find(|f| f.label.as_deref() == Some(l)),
            None => self.frames.iter_mut().rev().find(|f| f.is_loop),
        };
        if let Some(f) = frame {
            if is_continue {
                f.continues.push((env, span));
            } else {
                f.breaks.push((env, span));
            }
        }
    }

    fn macro_call(&mut self, m: &ast::Macro) -> bool {
        match &m.args {
            ast::MacroArgs::Exprs(es) => {
                let prev = std::mem::replace(&mut self.logic, true);
                self.exprs(es);
                self.logic = prev;
            }
            ast::MacroArgs::Map(kvs) => {
                let prev = std::mem::replace(&mut self.logic, true);
                for (k, v) in kvs {
                    self.expr(k);
                    self.expr(v);
                }
                self.logic = prev;
            }
            ast::MacroArgs::Tokens => {}
        }
        // 発散するマクロ（Rust と同じ）。
        matches!(
            m.path.segments.last().map(|s| s.ident.name.as_str()),
            Some("panic" | "unreachable" | "todo" | "unimplemented")
        )
    }

    fn struct_lit(&mut self, e: &Expr, lit: &ast::StructLit) -> bool {
        let info = if lit.qself.is_none() {
            self.struct_of_path(&lit.path)
        } else {
            None
        };
        let mut d = false;
        let mut given = BTreeSet::new();
        for f in &lit.fields {
            if f.erased {
                let name = match &f.member {
                    Member::Named(i) => i.name.clone(),
                    Member::Unnamed { index, .. } => index.to_string(),
                };
                match info.and_then(|s| s.erased_kind(&name)) {
                    Some(k) => {
                        self.facts.erased.insert(f.span.into(), k);
                    }
                    None => self.push(
                        f.span,
                        CheckMsg::NotErasedField {
                            ty: path_text(&lit.path),
                            field: name.clone(),
                        },
                    ),
                }
                given.insert(name);
                // 消去フィールドの値は仕様の世界（`Tracked(..)` / `Ghost(..)` に入る）。
                let prev = std::mem::replace(&mut self.logic, true);
                self.expr(&f.expr);
                self.logic = prev;
            } else {
                d |= self.expr(&f.expr);
            }
        }
        if let Some(Some(base)) = &lit.rest {
            d |= self.expr(base);
        } else if let Some(s) = info {
            for (name, _, _) in &s.erased {
                if !given.contains(name) {
                    self.push(
                        e.span,
                        CheckMsg::MissingErasedField {
                            ty: path_text(&lit.path),
                            field: name.clone(),
                            pattern: false,
                        },
                    );
                }
            }
        }
        d
    }
}

fn is_compound_assign(op: ast::BinOp) -> bool {
    use ast::BinOp::*;
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

fn is_self(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Path { qself: None, path } if path.as_ident().is_some_and(|i| i.is("self")))
}

/// 式が一つの識別子（`self` を除く）なら、その識別子。
pub fn ident_of(e: &Expr) -> Option<&ast::Ident> {
    match &e.kind {
        ExprKind::Path { qself: None, path } => path.as_ident().filter(|i| !i.is("self")),
        ExprKind::Paren(x) => ident_of(x),
        _ => None,
    }
}

/// `*X`、`(*X).f`、`(*X).f.g[i]` の `X` と `*X` の位置。
pub fn deref_root(e: &Expr) -> Option<(&ast::Ident, Span)> {
    match &e.kind {
        ExprKind::Unary {
            op: UnOp::Deref,
            expr,
        } => ident_of(expr).map(|x| (x, e.span)),
        ExprKind::Paren(x) => deref_root(x),
        ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => deref_root(base),
        _ => None,
    }
}

fn path_text(p: &ast::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.name.as_str())
        .collect::<Vec<_>>()
        .join("::")
}
