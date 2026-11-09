//! 静的検査（仕様 §6）とプロジェクトの読み込み（仕様 §3）。
//!
//! 入口は [`project::load`] と [`check`]。§6.1（剥がし一致、[`strip`]）と §6.2〜§6.5（線形性、領域添字、
//! 絞り込みの署名展開、索引と名前解決）を実装している。§6.2〜§6.5 が見るのは `.eyk` だけ（`.eykp` の
//! 本体の検査は #67）。§6.6（自己完結）は M3、部分集合型・trait の契約は M4、性質ファイルは M5 で、
//! どれも書かれていれば `E0003`（未実装）にする。

pub mod body;
pub mod ensures;
pub mod facts;
pub mod index;
pub mod names;
pub mod project;
pub mod sig;
pub mod strip;
pub mod zst;

use ebykn_syntax::ast::{self, ItemKind, Param, TraitItemKind};
use ebykn_syntax::diag::{CheckMsg, Diagnostic, Message, Unimpl};
use ebykn_syntax::span::Span;

use body::{FnCx, Walker};
use facts::Facts;
use index::{ImplTrait, Index, ItemPath};
use project::Project;
use sig::SigCx;

/// 名前が `Invariant` / `View`（[`ebykn_syntax::reserved`] の Ebykn のトレイト）か。
pub fn reserved_trait(name: &str) -> bool {
    ebykn_syntax::reserved::kind_of(name) == Some(ebykn_syntax::reserved::ReservedKind::EbyknTrait)
}

/// 診断を集めるもの。
#[derive(Debug, Default)]
pub struct Diags {
    pub list: Vec<Diagnostic>,
}

impl Diags {
    pub fn push(&mut self, span: Span, m: Message) {
        self.list.push(Diagnostic::new(span, m));
    }
}

/// 検査の結果。
#[derive(Debug)]
pub struct Checked {
    pub index: Index,
    pub facts: Facts,
    /// 位置の順（ファイル、バイト位置）に並べた診断。
    pub diags: Vec<Diagnostic>,
}

/// 読み込んだプロジェクトを検査する。
pub fn check(project: &Project) -> Checked {
    let mut index = Index::default();
    project.for_each_module(|m, f, items| index.add_module(m, f, items));
    // モジュールのファイルの先頭の `#![cfg(..)]`。
    for m in &project.modules {
        if let Some(ast) = project.ast(m.file)
            && index::has_cfg(&ast.attrs)
            && !m.module.is_empty()
        {
            index.gated.insert(m.module.clone());
        }
    }
    project.for_each_module(|m, f, items| index.add_impls(m, f, items));
    let mut facts = Facts::default();
    for ty in index.invariants.keys() {
        let nested = sig::nested(&index, ty);
        if !nested.is_empty() {
            facts.nested.insert(ty.clone(), nested);
        }
    }
    let mut diags = Diags::default();
    for span in &index.duplicates {
        diags.push(
            *span,
            Message::Check(CheckMsg::Unimplemented(Unimpl::CfgDuplicate)),
        );
    }
    project.for_each_module(|m, _, items| check_items(&index, m, items, &mut diags, &mut facts));
    names::check(project, &index, &mut diags);
    zst::check(project, &index, &mut diags);
    strip::check(project, &mut diags);
    let mut list = diags.list;
    sort_diags(&mut list);
    Checked {
        index,
        facts,
        diags: list,
    }
}

/// 診断をファイル・位置の順に並べ、重複を除く。
pub fn sort_diags(list: &mut Vec<Diagnostic>) {
    list.sort_by_key(|d| (d.span.file, d.span.lo, d.span.hi, d.code().as_str()));
    list.dedup();
}

/// モジュール（または関数の中）の項目を検査する。
pub fn check_items(
    index: &Index,
    module: &[String],
    items: &[ast::Item],
    diags: &mut Diags,
    facts: &mut Facts,
) {
    for item in items {
        match &item.kind {
            ItemKind::Fn(f) => {
                check_fn(index, module, None, &[], false, &item.vis, f, diags, facts)
            }
            ItemKind::SpecFn(f) => check_spec_fn(index, module, None, &[], f, diags, facts),
            ItemKind::LemmaFn(f) => check_lemma_fn(index, module, None, &[], f, diags, facts),
            ItemKind::Impl(imp) => check_impl(index, module, imp, item.span, diags, facts),
            ItemKind::Trait(t) => check_trait(index, module, t, diags, facts),
            ItemKind::ImplSpec(_) => diags.push(
                item.span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::ImplSpec)),
            ),
            ItemKind::Subset(_) => diags.push(
                item.span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::SubsetType)),
            ),
            ItemKind::Const(c) => {
                if let Some(e) = &c.expr {
                    plain_expr(index, module, e, diags, facts);
                }
            }
            ItemKind::Static(s) => plain_expr(index, module, &s.expr, diags, facts),
            // Ebykn の検査のない Rust の項目。インラインの `mod` の中は
            // [`project::Project::for_each_module`] がたどる。
            ItemKind::Struct(_)
            | ItemKind::Enum(_)
            | ItemKind::TypeAlias(_)
            | ItemKind::Mod(_)
            | ItemKind::Use(_)
            | ItemKind::LemmaGroup(_)
            | ItemKind::UseLemmas(_)
            | ItemKind::Rust(_) => {}
        }
    }
}

/// 関数の本体の中の項目。索引に入らないので、Ebykn の項目（`impl Invariant` / `impl View`、
/// `owns` / `spec` を持つ構造体、インラインの `mod`）は `E0003` にする。
pub fn check_nested_items(
    index: &Index,
    module: &[String],
    items: &[ast::Item],
    diags: &mut Diags,
    facts: &mut Facts,
) {
    for item in items {
        let ebykn = match &item.kind {
            ItemKind::Impl(imp) => !matches!(
                index.impl_trait(module, imp),
                ImplTrait::Inherent | ImplTrait::Drop | ImplTrait::Other
            ),
            ItemKind::Struct(s) => s.owns.is_some() || s.spec.is_some(),
            ItemKind::Mod(_) => true,
            _ => false,
        };
        let assumption = match &item.kind {
            ItemKind::Fn(f) => f.unchecked.is_some(),
            ItemKind::LemmaFn(_) => true,
            _ => false,
        };
        if assumption {
            diags.push(
                item.span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::NestedAssumption)),
            );
        } else if ebykn {
            diags.push(
                item.span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::NestedItem)),
            );
        } else {
            check_items(index, module, std::slice::from_ref(item), diags, facts);
        }
    }
}

/// 関数の外の式（`const` / `static` の初期値）。
fn plain_expr(index: &Index, module: &[String], e: &ast::Expr, d: &mut Diags, f: &mut Facts) {
    let cx = FnCx {
        index,
        module,
        self_ty: None,
        has_self: false,
        type_params: Vec::new(),
        linear: true,
        gives: Vec::new(),
    };
    Walker::new(cx, d, f).pure_body(e);
}

/// 型の引数の名前（外の `impl` / `trait` のものと、関数のもの）。
fn type_params(outer: &[String], g: &ast::Generics) -> Vec<String> {
    let own = g.params.iter().filter_map(|p| match p {
        ast::GenericParam::Type { ident, .. } => Some(ident.name.clone()),
        _ => None,
    });
    outer.iter().cloned().chain(own).collect()
}

fn has_receiver(sig: &ast::Signature) -> bool {
    sig.params
        .iter()
        .any(|p| matches!(p, Param::Receiver { .. }))
}

#[allow(clippy::too_many_arguments)]
fn check_fn(
    index: &Index,
    module: &[String],
    self_ty: Option<&ItemPath>,
    outer: &[String],
    own_impl: bool,
    vis: &ast::Vis,
    f: &ast::Fn,
    diags: &mut Diags,
    facts: &mut Facts,
) {
    sig::check_perm_targets(&f.sig, &f.contract, &mut diags.list);
    ensures::check(&f.sig, &f.contract, &mut diags.list);
    let scx = SigCx {
        index,
        module,
        self_ty,
        own_impl,
        is_pub: !matches!(vis, ast::Vis::Inherited),
    };
    let exp = sig::expand(&scx, &f.sig, &mut diags.list);
    if !exp.is_empty() || exp.name_ret {
        facts.expansions.insert(f.sig.ident.span.into(), exp);
    }
    let cx = FnCx {
        index,
        module,
        self_ty,
        has_self: has_receiver(&f.sig),
        type_params: type_params(outer, &f.sig.generics),
        // `unchecked fn` の本体は検査しない（仕様 §6.2）。
        linear: f.unchecked.is_none(),
        // 名前付き戻り値でない `gives` は E0501 にしたので、Γ の検査には使わない。
        gives: f
            .contract
            .gives()
            .filter(|g| {
                f.sig.ret.as_ref().is_some_and(|r| {
                    r.name.as_ref().is_some_and(|n| n.name == g.name)
                        && matches!(r.ty.kind, ast::TypeKind::Path { .. })
                        && index::ptr_or_cell(&r.ty).is_some()
                })
            })
            .map(|g| g.name.clone())
            .collect(),
    };
    let mut w = Walker::new(cx, diags, facts);
    w.enter(&f.sig, &f.contract);
    w.contract(&f.contract);
    match &f.body {
        Some(ast::FnBody::Block(b)) => w.body(b),
        Some(ast::FnBody::Pure(e)) => w.pure_body(e),
        None => {}
    }
}

fn logic_cx<'a>(
    index: &'a Index,
    module: &'a [String],
    self_ty: Option<&'a ItemPath>,
    outer: &[String],
    sig: &ast::Signature,
) -> FnCx<'a> {
    FnCx {
        index,
        module,
        self_ty,
        has_self: has_receiver(sig),
        type_params: type_params(outer, &sig.generics),
        linear: false,
        gives: Vec::new(),
    }
}

fn check_spec_fn(
    index: &Index,
    module: &[String],
    self_ty: Option<&ItemPath>,
    outer: &[String],
    f: &ast::SpecFn,
    diags: &mut Diags,
    facts: &mut Facts,
) {
    let mut w = Walker::new(
        logic_cx(index, module, self_ty, outer, &f.sig),
        diags,
        facts,
    );
    w.enter(&f.sig, &f.contract);
    w.contract(&f.contract);
    if let Some(e) = &f.body {
        w.logic_expr(e);
    }
}

fn check_lemma_fn(
    index: &Index,
    module: &[String],
    self_ty: Option<&ItemPath>,
    outer: &[String],
    f: &ast::LemmaFn,
    diags: &mut Diags,
    facts: &mut Facts,
) {
    sig::check_perm_targets(&f.sig, &f.contract, &mut diags.list);
    ensures::check(&f.sig, &f.contract, &mut diags.list);
    let mut w = Walker::new(
        logic_cx(index, module, self_ty, outer, &f.sig),
        diags,
        facts,
    );
    w.enter(&f.sig, &f.contract);
    w.contract(&f.contract);
    if let Some(b) = &f.body {
        w.logic_block(b);
    }
}

fn check_impl(
    index: &Index,
    module: &[String],
    imp: &ast::Impl,
    span: Span,
    diags: &mut Diags,
    facts: &mut Facts,
) {
    let self_ty = index.type_path(module, &imp.self_ty, None);
    let self_ty = self_ty.as_ref();
    let outer = type_params(&[], &imp.generics);
    let unimpl = |diags: &mut Diags, span: Span, u: Unimpl| {
        diags.push(span, Message::Check(CheckMsg::Unimplemented(u)));
    };
    let kind = index.impl_trait(module, imp);
    match kind {
        ImplTrait::Invariant | ImplTrait::View if self_ty.is_none() => {
            // クレートの外の型への `impl Invariant` / `impl View` は性質ファイル（M5）。
            unimpl(diags, span, Unimpl::ExternalTypeSpec);
            return;
        }
        ImplTrait::Invariant => {
            match invariant_fn(imp) {
                Some(f) => check_spec_fn(index, module, self_ty, &outer, f, diags, facts),
                None => diags.push(span, Message::Check(CheckMsg::InvariantImplShape)),
            }
            return;
        }
        ImplTrait::View => {
            for item in &imp.items {
                match &item.kind {
                    ItemKind::SpecFn(f) => {
                        check_spec_fn(index, module, self_ty, &outer, f, diags, facts)
                    }
                    ItemKind::TypeAlias(_) => {}
                    _ => diags.push(item.span, Message::Check(CheckMsg::ViewImplShape)),
                }
            }
            return;
        }
        ImplTrait::Misspelled => {
            if let Some(t) = &imp.trait_ {
                diags.push(
                    t.span,
                    Message::Check(CheckMsg::EbyknTraitSpelling {
                        written: format!("impl {}", path_text(t)),
                    }),
                );
            }
            return;
        }
        ImplTrait::Inherent | ImplTrait::Drop | ImplTrait::Other => {}
    }
    let own_impl = kind == ImplTrait::Inherent;
    for item in &imp.items {
        match &item.kind {
            ItemKind::Fn(f) => {
                if !own_impl {
                    // trait の impl のメソッドの契約は trait の契約（M4）と一緒に扱う。
                    let mut probe = Vec::new();
                    let scx = SigCx {
                        index,
                        module,
                        self_ty,
                        own_impl,
                        is_pub: true,
                    };
                    let exp = sig::expand(&scx, &f.sig, &mut probe);
                    if f.contract.is_empty() && !exp.is_empty() && kind == ImplTrait::Drop {
                        unimpl(diags, item.span, Unimpl::InvariantDrop);
                        continue;
                    }
                    if !f.contract.is_empty() || !exp.is_empty() {
                        unimpl(diags, item.span, Unimpl::ContractTrait);
                        continue;
                    }
                }
                check_fn(
                    index, module, self_ty, &outer, own_impl, &item.vis, f, diags, facts,
                );
            }
            ItemKind::SpecFn(f) => check_spec_fn(index, module, self_ty, &outer, f, diags, facts),
            ItemKind::LemmaFn(f) => check_lemma_fn(index, module, self_ty, &outer, f, diags, facts),
            ItemKind::Const(c) => {
                if let Some(e) = &c.expr {
                    plain_expr(index, module, e, diags, facts);
                }
            }
            ItemKind::Subset(_) => unimpl(diags, item.span, Unimpl::SubsetType),
            ItemKind::ImplSpec(_) => unimpl(diags, item.span, Unimpl::ImplSpec),
            // 関連型と、項目の位置のマクロなど Rust の項目。`lemma group` は構文解析器が
            // モジュールの外では診断にしている。
            ItemKind::TypeAlias(_) | ItemKind::Rust(_) | ItemKind::LemmaGroup(_) => {}
            // impl の中に書けない項目（構文解析器は作らないが、来たら黙って通さない）。
            ItemKind::Struct(_)
            | ItemKind::Enum(_)
            | ItemKind::Trait(_)
            | ItemKind::Impl(_)
            | ItemKind::Mod(_)
            | ItemKind::Static(_)
            | ItemKind::Use(_)
            | ItemKind::UseLemmas(_) => diags.push(
                item.span,
                Message::NotAllowedHere {
                    written: "this item in `impl`".into(),
                },
            ),
        }
    }
}

/// `impl Invariant for T` の中身が `spec fn invariant(&self) -> bool = ..;` 一つだけなら、その関数。
pub fn invariant_fn(imp: &ast::Impl) -> Option<&ast::SpecFn> {
    let [item] = imp.items.as_slice() else {
        return None;
    };
    let ItemKind::SpecFn(f) = &item.kind else {
        return None;
    };
    let receiver_ok = matches!(
        f.sig.params.as_slice(),
        [Param::Receiver {
            reference: Some(_),
            mutable: false,
            ty: None,
            ..
        }]
    );
    let ret_ok = f.sig.ret.as_ref().is_some_and(|r| {
        r.name.is_none()
            && matches!(&r.ty.kind, ast::TypeKind::Path { qself: None, path } if path.as_ident().is_some_and(|i| i.is("bool")))
    });
    (f.sig.ident.is("invariant")
        && receiver_ok
        && ret_ok
        && f.body.is_some()
        && f.contract.is_empty()
        && f.sig.generics.params.is_empty())
    .then_some(f)
}

fn check_trait(
    index: &Index,
    module: &[String],
    t: &ast::Trait,
    diags: &mut Diags,
    facts: &mut Facts,
) {
    let outer = type_params(&[], &t.generics);
    for item in &t.items {
        match &item.kind {
            TraitItemKind::SpecFn(_) | TraitItemKind::LemmaFn(_) => diags.push(
                item.span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::ContractTrait)),
            ),
            TraitItemKind::Fn(f) if !f.contract.is_empty() => diags.push(
                item.span,
                Message::Check(CheckMsg::Unimplemented(Unimpl::ContractTrait)),
            ),
            TraitItemKind::Fn(f) => {
                check_fn(
                    index,
                    module,
                    None,
                    &outer,
                    false,
                    &ast::Vis::Inherited,
                    f,
                    diags,
                    facts,
                );
            }
            TraitItemKind::Rust(_) => {}
        }
    }
}

/// パスの書き方（ジェネリクスを含む）。
fn path_text(p: &ast::Path) -> String {
    let mut out = String::new();
    if p.leading_colon {
        out.push_str("::");
    }
    let segs: Vec<String> = p
        .segments
        .iter()
        .map(|s| match &s.args {
            ast::GenericArgs::None => s.ident.name.clone(),
            _ => format!("{}<..>", s.ident.name),
        })
        .collect();
    out.push_str(&segs.join("::"));
    out
}
