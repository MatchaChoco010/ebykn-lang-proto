//! 消去フィールドを `.` で書いたものの検査（E0404。#99）。
//!
//! 消去フィールド（`owns` / `spec`）は `x@f` で書く。`x.f`、構造体リテラルの `f: e`、パターンの `f` は、
//! 書き下しの後の `Tracked` / `Ghost` のフィールドに直接届き、領域の権限を捨てたり差し替えたりできる。
//! 書き下しは消去フィールドを `.eyk` から書けない名前（`__eyk_erased_f`）にするので、ここで見逃した形も
//! Verus のビルドで止まる。ここでは、行動につながる診断を出すために、型の分かる受け手と、型が
//! 分からず、名前が消去フィールドと同じで、クレートのどの構造体もその名前の通常のフィールドを
//! 持たない受け手を拒否する。

use ebykn_syntax::ast::{self, Expr, ExprKind, FieldPat, Member, UnOp};
use ebykn_syntax::diag::{CheckMsg, DotForm};
use ebykn_syntax::span::Span;

use crate::index::{StructInfo, ptr_or_cell};

use super::Walker;

/// `.` の前の値の型。
enum Recv<'a> {
    /// クレートの構造体。
    Struct(&'a StructInfo),
    /// クレートの構造体ではない型（Rust の型、型の引数など）。
    Other,
    /// 分からない。
    Unknown,
}

/// 式の型（分かるものだけ）。
enum Ty {
    /// `self` の型（`impl` の Self）。
    SelfTy,
    /// 書かれた型と、それを読むモジュール。
    Written(ast::Type, Vec<String>),
}

impl<'a> Walker<'a> {
    /// `e.f` の `f` が消去フィールドなら E0404。
    pub(super) fn field_dot(&mut self, base: &Expr, member: &Member) {
        let Member::Named(f) = member else {
            return;
        };
        let recv = self.recv(self.ty_of(base), &f.name);
        let Some((ty, sure)) = self.decide(recv, &f.name) else {
            return;
        };
        self.push(
            f.span,
            CheckMsg::ErasedFieldDot {
                ty: struct_name(ty),
                field: f.name.clone(),
                form: DotForm::Access,
                sure,
            },
        );
    }

    /// 構造体リテラル・パターンの `f: e` / `f` の `f` が消去フィールドなら E0404。診断にしたら `true`
    /// （E0402 の欠落としては数えない）。
    pub(super) fn named_erased(
        &mut self,
        info: Option<&StructInfo>,
        member: &Member,
        form: DotForm,
        span: Span,
    ) -> bool {
        let (Some(s), Member::Named(f)) = (info, member) else {
            return false;
        };
        if s.erased_kind(&f.name).is_none() {
            return false;
        }
        self.push(
            span,
            CheckMsg::ErasedFieldDot {
                ty: struct_name(s),
                field: f.name.clone(),
                form,
                sure: true,
            },
        );
        true
    }

    /// パターンの `f` / `f: p`（`FieldPat::Field`）が消去フィールドの名前なら E0404。
    pub(super) fn pat_field(&mut self, info: Option<&StructInfo>, f: &FieldPat) -> Option<String> {
        let FieldPat::Field { member, span, .. } = f else {
            return None;
        };
        self.named_erased(info, member, DotForm::Pattern, *span)
            .then(|| member_name(member))
    }

    /// 中を読まないマクロの引数（`println!` など）の `.f`（メソッドの呼び出しでないもの）を、式の `e.f` と
    /// 同じ規則で E0404 にする。受け手の型は、`.` の直前が裸の名前（`self` か、型を書いた変数）のときだけ
    /// 見る。それ以外は型が分からないものとして扱う。
    pub(super) fn field_dot_tokens(&mut self, ts: &proc_macro2::TokenStream, span: Span) {
        use proc_macro2::{Delimiter, TokenTree};
        let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
        let dot =
            |t: Option<&TokenTree>| matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == '.');
        for (i, tt) in tts.iter().enumerate() {
            if let TokenTree::Group(g) = tt {
                self.field_dot_tokens(&g.stream(), span);
                continue;
            }
            // `..f`（範囲）の `.` は見ない。
            if !dot(Some(tt)) || dot(i.checked_sub(1).and_then(|j| tts.get(j))) {
                continue;
            }
            let Some(TokenTree::Ident(f)) = tts.get(i + 1) else {
                continue;
            };
            let call = matches!(tts.get(i + 2), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
                || matches!(tts.get(i + 2), Some(TokenTree::Punct(p)) if p.as_char() == ':');
            let name = f.to_string();
            let name = name.strip_prefix("r#").unwrap_or(&name).to_string();
            if call {
                continue;
            }
            // `x.f` の `x`（`a.x.f` / `a::x.f` の `x` は除く）。
            let head = match (i.checked_sub(1).and_then(|j| tts.get(j)), i.checked_sub(2)) {
                (Some(TokenTree::Ident(x)), j) => {
                    let before = j.and_then(|j| tts.get(j));
                    let joined = matches!(before, Some(TokenTree::Punct(p)) if matches!(p.as_char(), '.' | ':'));
                    (!joined).then(|| x.to_string())
                }
                _ => None,
            };
            let ty = head.and_then(|x| self.ident_ty(x.strip_prefix("r#").unwrap_or(&x)));
            let recv = self.recv(ty, &name);
            if let Some((s, sure)) = self.decide(recv, &name) {
                self.push(
                    span,
                    CheckMsg::ErasedFieldDot {
                        ty: struct_name(s),
                        field: name,
                        form: DotForm::Access,
                        sure,
                    },
                );
            }
        }
    }

    /// `.name` を E0404 にするなら、診断に出す構造体と、受け手の型で確かめたか。
    fn decide(&self, recv: Recv<'a>, name: &str) -> Option<(&'a StructInfo, bool)> {
        match recv {
            Recv::Struct(s) if s.erased_kind(name).is_some() => Some((s, true)),
            Recv::Struct(s) if s.fields.iter().any(|(n, _)| n == name) => None,
            Recv::Other => None,
            // そのフィールドを持たない構造体（`Deref` で別の型に届きうる）は、型が分からないのと同じ。
            Recv::Struct(_) | Recv::Unknown => self.unknown_owner(name).map(|s| (s, false)),
        }
    }

    /// 型の分からない受け手の `.name` を拒否するときの、`name` を消去フィールドに持つ構造体（パスの
    /// 順で最初のもの）。クレートのどれかの構造体が `name` を通常のフィールドに持てば、その値かも
    /// しれないので拒否しない（消去フィールドに届く形なら、書き下しの名前のためにビルドで止まる）。
    fn unknown_owner(&self, name: &str) -> Option<&'a StructInfo> {
        let structs = &self.cx.index.structs;
        if structs
            .values()
            .any(|s| s.fields.iter().any(|(n, _)| n == name))
        {
            return None;
        }
        structs.values().find(|s| s.erased_kind(name).is_some())
    }

    /// `.name` の前の値の型。クレートの構造体でない型が、`name` を消去フィールドに持つ構造体か型の
    /// 引数を中に書いていれば（`Box<S>`、`type A = S;` の `A`、`T` など。参照外しで届きうる）、分からない
    /// とする。
    fn recv(&self, ty: Option<Ty>, name: &str) -> Recv<'a> {
        let Some(ty) = ty else {
            return Recv::Unknown;
        };
        match ty {
            // クレートの構造体でない型の `impl`（`impl Tr for Box<S>`）と trait の既定のメソッドの `self`。
            Ty::SelfTy => self.self_struct().map_or(Recv::Unknown, Recv::Struct),
            Ty::Written(t, module) => match self.struct_of(&t, &module) {
                Some(s) => Recv::Struct(s),
                None if self.mentions(&t, &module, name, 0) => Recv::Unknown,
                None => Recv::Other,
            },
        }
    }

    /// 型が、スコープにある型の引数から始まるパス（`T`、`T::Target`）か。
    fn is_type_param(&self, t: &ast::Type) -> bool {
        matches!(&t.kind, ast::TypeKind::Path { qself: None, path }
            if !path.leading_colon
                && path.segments.first().is_some_and(|s| self.cx.type_params.contains(&s.ident.name)))
    }

    /// 型 `t` が、`name` を消去フィールドに持つクレートの構造体を、型の引数か型の別名の先に書いているか。
    fn mentions(&self, t: &ast::Type, module: &[String], name: &str, depth: usize) -> bool {
        // 型の別名の循環は Rust が拒否するが、ここでは深さで止める。
        if depth > 16 {
            return true;
        }
        let index = self.cx.index;
        if self.is_type_param(t) {
            return true;
        }
        match &t.kind {
            // `<T as Tr>::A` は読まない。
            ast::TypeKind::Path { qself: Some(_), .. } => true,
            ast::TypeKind::Path { path, .. } => {
                if let Some(p) = index.type_path(module, t, self.cx.self_ty) {
                    if index
                        .structs
                        .get(&p)
                        .is_some_and(|s| s.erased_kind(name).is_some())
                    {
                        return true;
                    }
                    if let Some(a) = index.aliases.get(&p)
                        && self.mentions(&a.ty, &a.module, name, depth + 1)
                    {
                        return true;
                    }
                }
                path.segments.iter().any(|seg| match &seg.args {
                    ast::GenericArgs::Angle { args, .. } => args.iter().any(|a| match a {
                        ast::GenericArg::Type(x) | ast::GenericArg::AssocType { ty: x, .. } => {
                            self.mentions(x, module, name, depth + 1)
                        }
                        _ => false,
                    }),
                    _ => false,
                })
            }
            ast::TypeKind::Ref { elem, .. }
            | ast::TypeKind::Paren(elem)
            | ast::TypeKind::Slice(elem)
            | ast::TypeKind::Array { elem, .. } => self.mentions(elem, module, name, depth + 1),
            ast::TypeKind::Tuple(ts) => {
                ts.iter().any(|x| self.mentions(x, module, name, depth + 1))
            }
            // `_` と、中を読まない `dyn Trait` など。型が分からないとする。
            ast::TypeKind::Rust(_) | ast::TypeKind::Infer => true,
            ast::TypeKind::Never => false,
        }
    }

    /// 型の参照（`&` / `&mut`）を外した先のクレートの構造体。
    fn struct_of(&self, t: &ast::Type, module: &[String]) -> Option<&'a StructInfo> {
        let t = peel_refs(t);
        if self.is_type_param(t) {
            return None;
        }
        let p = self.cx.index.type_path(module, t, self.cx.self_ty)?;
        self.cx.index.structs.get(&p)
    }

    /// 名前の値の型（`self` と、型を書いた変数と引数）。
    fn ident_ty(&self, name: &str) -> Option<Ty> {
        if name == "self" {
            return self.cx.has_self.then_some(Ty::SelfTy);
        }
        if !self.locals.contains(name) {
            return None;
        }
        let t = self.var_types.get(name)?;
        Some(Ty::Written(t.clone(), self.cx.module.to_vec()))
    }

    /// 式の型。`self`、型を書いた変数と引数、`*e`（参照と `Ptr` / `Cell` の指す値）、`old(e)` /
    /// `final(e)`、構造体のフィールド `e.g` を見る。
    fn ty_of(&self, e: &Expr) -> Option<Ty> {
        match &e.kind {
            ExprKind::Paren(x) => self.ty_of(x),
            ExprKind::Path { qself: None, path } => self.ident_ty(&path.as_ident()?.name),
            ExprKind::Unary {
                op: UnOp::Deref,
                expr,
            } => match self.ty_of(expr)? {
                Ty::SelfTy => Some(Ty::SelfTy),
                Ty::Written(t, m) => pointee(&t).map(|t| Ty::Written(t.clone(), m)),
            },
            ExprKind::Call { func, args, .. } if args.len() == 1 => {
                let ExprKind::Path { qself: None, path } = &func.kind else {
                    return None;
                };
                let f = path.as_ident()?;
                (f.is("old") || f.is("final"))
                    .then(|| self.ty_of(&args[0]))
                    .flatten()
            }
            ExprKind::Field {
                base,
                member: Member::Named(g),
            } => {
                let s = match self.ty_of(base)? {
                    Ty::SelfTy => self.self_struct()?,
                    Ty::Written(t, m) => self.struct_of(&t, &m)?,
                };
                // 型の引数を使うフィールドの型は、使う側の型を代入しないと読めない。
                if !s.params.is_empty() {
                    return None;
                }
                let (_, t) = s.fields.iter().find(|(n, _)| *n == g.name)?;
                Some(Ty::Written(t.clone(), s.module.clone()))
            }
            _ => None,
        }
    }
}

fn peel_refs(t: &ast::Type) -> &ast::Type {
    match &t.kind {
        ast::TypeKind::Ref { elem, .. } | ast::TypeKind::Paren(elem) => peel_refs(elem),
        _ => t,
    }
}

/// `*e` の型：参照の先、`Ptr<T>` / `Cell<T>` の `T`。
fn pointee(t: &ast::Type) -> Option<&ast::Type> {
    match &t.kind {
        ast::TypeKind::Ref { elem, .. } => Some(elem),
        ast::TypeKind::Paren(x) => pointee(x),
        ast::TypeKind::Path { qself: None, path } if ptr_or_cell(t).is_some() => {
            match &path.segments.last()?.args {
                ast::GenericArgs::Angle { args, .. } => match args.as_slice() {
                    [ast::GenericArg::Type(x)] => Some(x),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}

fn struct_name(s: &StructInfo) -> String {
    s.path.last().cloned().unwrap_or_default()
}

pub(super) fn member_name(m: &Member) -> String {
    match m {
        Member::Named(i) => i.name.clone(),
        Member::Unnamed { index, .. } => index.to_string(),
    }
}
