//! 項目の書き下し（仕様 §7.2、§7.3 の構造体と署名、§6.4 の展開）。

use std::collections::BTreeMap;

use ebykn_check::facts::SpanKey;
use ebykn_check::index::{ImplTrait, ptr_or_cell};
use ebykn_check::sig::{FnExpansion, RET_NAME, Refine, Subject, Wrap};
use ebykn_syntax::ast::{
    self, Fields, GenericArg, GenericArgs, GenericParam, ItemKind, Param, PermMode, TraitItemKind,
    TypeKind, UsesItem, Vis,
};

use crate::expr::{PermKind, ident, perm_var};
use crate::{Cx, Piece, attr};

/// 項目の列を一片ずつ書き下す。
pub(crate) fn items(cx: &Cx, items: &[ast::Item], indent: usize, out: &mut Vec<Piece>) {
    for item in items {
        item_pieces(cx, item, indent, out);
    }
}

fn tokens(text: String, item: Option<String>, indent: usize) -> Piece {
    Piece::Tokens { text, item, indent }
}

fn vis(v: &Vis) -> String {
    match v {
        Vis::Inherited => String::new(),
        Vis::Pub(_) => "pub ".into(),
        Vis::Restricted(t) => format!("{} ", t.tokens),
    }
}

fn attrs(a: &[ast::Attr]) -> String {
    a.iter().map(|x| attr(x) + " ").collect()
}

fn is_test(a: &[ast::Attr]) -> bool {
    a.iter().any(|x| x.is_builtin("test"))
}

fn item_pieces(cx: &Cx, item: &ast::Item, indent: usize, out: &mut Vec<Piece>) {
    match &item.kind {
        ItemKind::Mod(m) => {
            cx.note_macros(&item.attrs, || cx.item_path(&m.ident.name));
            let head = format!(
                "{}{}mod {}",
                attrs(&item.attrs),
                vis(&item.vis),
                ident(&m.ident)
            );
            match &m.content {
                None => out.push(tokens(format!("{head};"), None, indent)),
                Some((inner_attrs, inner)) => {
                    let pad = "    ".repeat(indent);
                    out.push(Piece::Raw(format!("{pad}{head} {{")));
                    for a in inner_attrs {
                        out.push(Piece::Raw(format!("{pad}    {}", attr(a))));
                    }
                    // インラインの `mod` にも暗黙の取り込みを付ける（§3.1。Rust のモジュールは親の `use` を見ない）。
                    for l in crate::HEADER_USES.lines() {
                        out.push(Piece::Raw(format!("{pad}    {l}")));
                    }
                    let mut c = cx.clone();
                    c.module.push(m.ident.name.clone());
                    items(&c, inner, indent + 1, out);
                    out.push(Piece::Raw(format!("{pad}}}")));
                }
            }
        }
        ItemKind::Impl(imp) => impl_pieces(cx, item, imp, indent, out),
        ItemKind::Trait(t) => trait_pieces(cx, item, t, indent, out),
        _ => {
            let (text, name) = item_text(cx, item);
            if !text.is_empty() {
                out.push(tokens(text, name, indent));
            }
        }
    }
}

impl Cx<'_> {
    /// 関数の本体の中の項目。
    pub(crate) fn nested_item(&self, item: &ast::Item) -> String {
        let mut c = self.clone();
        c.self_ty = None;
        c.self_struct = None;
        match &item.kind {
            ItemKind::Impl(imp) => {
                let mut pieces = Vec::new();
                impl_pieces(&c, item, imp, 0, &mut pieces);
                pieces
                    .into_iter()
                    .map(|p| match p {
                        Piece::Raw(s) => s,
                        Piece::Tokens { text, .. } => text,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => item_text(&c, item).0,
        }
    }
}

/// 項目（`mod` と `impl` 以外）の書き下しと、検証の結果を出す項目の名前。
fn item_text(cx: &Cx, item: &ast::Item) -> (String, Option<String>) {
    note_item_macros(cx, item);
    let a = attrs(&item.attrs);
    let v = vis(&item.vis);
    match &item.kind {
        ItemKind::Fn(f) => {
            let name = cx.item_path(&f.sig.ident.name);
            (fn_text(cx, item, f, false), Some(name))
        }
        ItemKind::SpecFn(f) => (
            spec_fn_text(cx, &a, &item.vis, f),
            Some(cx.item_path(&f.sig.ident.name)),
        ),
        ItemKind::LemmaFn(f) => {
            let name = cx.item_path(&f.sig.ident.name);
            (lemma_text(cx, &a, &v, f, &name), Some(name))
        }
        ItemKind::Struct(s) => (struct_text(cx, &a, &v, s), None),
        ItemKind::Enum(e) => {
            let vars: Vec<String> = e
                .variants
                .iter()
                .map(|x| {
                    format!(
                        "{}{}{}{}",
                        attrs(&x.attrs),
                        ident(&x.ident),
                        fields_text(cx, &x.fields, false),
                        x.discriminant
                            .as_ref()
                            .map(|d| format!(" = {}", cx.exec_expr_str(d)))
                            .unwrap_or_default()
                    )
                })
                .collect();
            (
                format!(
                    "{a}{v}enum {}{}{} {{ {} }}",
                    ident(&e.ident),
                    generics(cx, &e.generics),
                    where_clause(&e.where_clause),
                    vars.join(", ")
                ),
                None,
            )
        }
        // trait は [`trait_pieces`]。関数の本体の中の trait はここ（項目の行を持たない）。
        ItemKind::Trait(t) => {
            let mut pieces = Vec::new();
            trait_pieces(cx, item, t, 0, &mut pieces);
            let text = pieces
                .into_iter()
                .map(|p| match p {
                    Piece::Raw(s) => s,
                    Piece::Tokens { text, .. } => text,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (text, None)
        }
        ItemKind::TypeAlias(t) => (
            format!(
                "{a}{v}type {}{}{} = {};",
                ident(&t.ident),
                generics(cx, &t.generics),
                where_clause(&t.where_clause),
                cx.ty(&t.ty)
            ),
            None,
        ),
        ItemKind::Const(c) => (
            format!(
                "{a}{v}const {}{}: {}{};",
                ident(&c.ident),
                generics(cx, &c.generics),
                cx.ty(&c.ty),
                c.expr
                    .as_ref()
                    .map(|e| format!(" = {}", cx.exec_expr_str(e)))
                    .unwrap_or_default()
            ),
            None,
        ),
        ItemKind::Static(s) => (
            format!(
                "{a}{v}static {}{}: {} = {};",
                if s.mutable { "mut " } else { "" },
                ident(&s.ident),
                cx.ty(&s.ty),
                cx.exec_expr_str(&s.expr)
            ),
            None,
        ),
        // `use` と Rust の項目のトークン列は、可視性と属性を含む。
        ItemKind::Use(tk) | ItemKind::Rust(tk) => (tk.tokens.to_string(), None),
        ItemKind::LemmaGroup(_) => crate::eykp_only("lemma group"),
        ItemKind::UseLemmas(_) => crate::eykp_only("use lemmas"),
        // 部分集合型と `impl spec` は検査が E0003 にする。`mod` と `impl` は [`item_pieces`]。
        ItemKind::Subset(_) | ItemKind::ImplSpec(_) | ItemKind::Mod(_) | ItemKind::Impl(_) => {
            (String::new(), None)
        }
    }
}

/// 項目（`mod`・`impl`・`trait` 以外）の属性の手続きマクロを前提として記録する（#49）。
/// `mod`・`impl`・`trait` は [`item_pieces`]・[`impl_pieces`]・[`trait_pieces`] で記録する。
fn note_item_macros(cx: &Cx, item: &ast::Item) {
    let name = match &item.kind {
        ItemKind::Fn(f) => &f.sig.ident,
        ItemKind::SpecFn(f) => &f.sig.ident,
        ItemKind::LemmaFn(f) => &f.sig.ident,
        ItemKind::Struct(s) => &s.ident,
        ItemKind::Enum(e) => &e.ident,
        ItemKind::TypeAlias(t) => &t.ident,
        ItemKind::Subset(t) => &t.ident,
        ItemKind::Const(c) => &c.ident,
        ItemKind::Static(s) => &s.ident,
        // 属性は AST にもトークン列の先頭にもある。AST の属性を見る。`extern` ブロックの中の宣言は
        // AST を持たないので、トークン列から宣言ごとの属性を見る（PR #57 のレビューの N1）。
        ItemKind::Use(tk) | ItemKind::Rust(tk) => {
            cx.note_macros(&item.attrs, || rust_item_name(cx, &tk.tokens));
            for decl in foreign_items(&tk.tokens) {
                cx.note_macros_in_tokens(&decl, || rust_item_name(cx, &decl));
            }
            return;
        }
        ItemKind::Trait(_) | ItemKind::Impl(_) | ItemKind::ImplSpec(_) | ItemKind::Mod(_) => return,
        ItemKind::LemmaGroup(_) => crate::eykp_only("lemma group"),
        ItemKind::UseLemmas(_) => crate::eykp_only("use lemmas"),
    };
    cx.note_macros(&item.attrs, || cx.item_path(&name.name));
}

/// `extern` ブロック（`extern "C" { .. }`、`unsafe extern "C" { .. }`）なら、中の宣言ごとのトークン列。
fn foreign_items(ts: &proc_macro2::TokenStream) -> Vec<proc_macro2::TokenStream> {
    use proc_macro2::{Delimiter, TokenTree};
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    let is_extern = tts
        .iter()
        .any(|t| matches!(t, TokenTree::Ident(i) if i == "extern"))
        && !tts
            .iter()
            .any(|t| matches!(t, TokenTree::Ident(i) if i == "crate"));
    let Some(TokenTree::Group(g)) = tts.last() else {
        return Vec::new();
    };
    if !is_extern || g.delimiter() != Delimiter::Brace {
        return Vec::new();
    }
    let mut out = vec![proc_macro2::TokenStream::new()];
    for t in g.stream() {
        let end = matches!(&t, TokenTree::Punct(p) if p.as_char() == ';');
        out.last_mut().expect("空でない").extend([t]);
        if end {
            out.push(proc_macro2::TokenStream::new());
        }
    }
    out.retain(|d| !d.is_empty());
    out
}

/// トークン列のまま持つ項目の表示：属性と可視性を除いた先頭（`union U`、`use a::b`）。
fn rust_item_name(cx: &Cx, ts: &proc_macro2::TokenStream) -> String {
    use proc_macro2::TokenTree;
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    let mut i = 0;
    while matches!(tts.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
        i += 2;
    }
    if matches!(tts.get(i), Some(TokenTree::Ident(x)) if x == "pub") {
        i += 1;
        if matches!(tts.get(i), Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Parenthesis)
        {
            i += 1;
        }
    }
    let head: proc_macro2::TokenStream = tts[i.min(tts.len())..]
        .iter()
        .take_while(|t| {
            !matches!(t, TokenTree::Group(g) if g.delimiter() == proc_macro2::Delimiter::Brace)
                && !matches!(t, TokenTree::Punct(p) if p.as_char() == ';')
        })
        .cloned()
        .collect();
    let mut p = vec![cx.crate_name.to_string()];
    p.extend(cx.module.iter().cloned());
    format!("{} in {}", head, p.join("::"))
}

pub(crate) fn generics(cx: &Cx, g: &ast::Generics) -> String {
    if g.params.is_empty() {
        return String::new();
    }
    let ps: Vec<String> = g
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Lifetime { lifetime, bounds } => format!(
                "'{}{}",
                lifetime.name,
                bounds
                    .as_ref()
                    .map(|b| format!(": {}", b.tokens))
                    .unwrap_or_default()
            ),
            GenericParam::Type {
                ident: i,
                bounds,
                default,
            } => format!(
                "{}{}{}",
                ident(i),
                bounds
                    .as_ref()
                    .map(|b| format!(": {}", b.tokens))
                    .unwrap_or_default(),
                default
                    .as_ref()
                    .map(|d| format!(" = {}", cx.ty(d)))
                    .unwrap_or_default()
            ),
            GenericParam::Const {
                ident: i,
                ty,
                default,
            } => format!(
                "const {}: {}{}",
                ident(i),
                cx.ty(ty),
                default
                    .as_ref()
                    .map(|d| format!(" = {}", cx.exec_expr_str(d)))
                    .unwrap_or_default()
            ),
        })
        .collect();
    format!("<{}>", ps.join(", "))
}

/// impl の `T<A, B>` に付ける引数（`impl<A, B>` の名前）。
fn generic_names(g: &ast::Generics) -> String {
    if g.params.is_empty() {
        return String::new();
    }
    let ps: Vec<String> = g
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Lifetime { lifetime, .. } => format!("'{}", lifetime.name),
            GenericParam::Type { ident: i, .. } | GenericParam::Const { ident: i, .. } => ident(i),
        })
        .collect();
    format!("<{}>", ps.join(", "))
}

pub(crate) fn where_clause(w: &Option<ast::WhereClause>) -> String {
    match w {
        Some(w) if !w.predicates.is_empty() => format!(
            " where {}",
            w.predicates
                .iter()
                .map(|p| p.tokens.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => String::new(),
    }
}

fn fields_text(cx: &Cx, f: &Fields, named_braces: bool) -> String {
    match f {
        Fields::Named(fs) => {
            let parts: Vec<String> = fs.iter().map(|x| field_text(cx, x, None)).collect();
            if named_braces || !parts.is_empty() {
                format!(" {{ {} }}", parts.join(" "))
            } else {
                " {}".into()
            }
        }
        Fields::Tuple(fs) => format!(
            "({})",
            fs.iter()
                .map(|x| format!("{}{}{}", attrs(&x.attrs), vis(&x.vis), cx.ty(&x.ty)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Fields::Unit => String::new(),
    }
}

fn field_text(cx: &Cx, f: &ast::Field, wrap: Option<&str>) -> String {
    let ty = cx.ty(&f.ty);
    let ty = match wrap {
        Some(w) => format!("{w}<{ty}>"),
        None => ty,
    };
    format!(
        "{}{}{}: {ty},",
        attrs(&f.attrs),
        vis(&f.vis),
        f.ident.as_ref().map(ident).unwrap_or_default()
    )
}

/// 構造体（§7.3）：`owns` は `Tracked<..>`、`spec` は `Ghost<..>` のフィールドになる。
fn struct_text(cx: &Cx, a: &str, v: &str, s: &ast::Struct) -> String {
    let head = format!(
        "{a}{v}struct {}{}",
        ident(&s.ident),
        generics(cx, &s.generics)
    );
    let w = where_clause(&s.where_clause);
    match &s.fields {
        Fields::Named(fs) => {
            let mut parts: Vec<String> = fs.iter().map(|x| field_text(cx, x, None)).collect();
            for (sec, wrap) in [(&s.owns, "Tracked"), (&s.spec, "Ghost")] {
                for f in sec.iter().flat_map(|e| &e.fields) {
                    parts.push(field_text(cx, f, Some(wrap)));
                }
            }
            format!("{head}{w} {{ {} }}", parts.join(" "))
        }
        Fields::Tuple(_) => format!("{head}{}{w};", fields_text(cx, &s.fields, false)),
        Fields::Unit => format!("{head}{w};"),
    }
}

// ---- 関数 ----

/// 関数の文脈を作る：引数の権限と `gives`。`ensures` の `&mut` 引数は、利用者が `old` / `final` を
/// 書く（#10 の決定。裸のものは検査が E0004 にする）。
fn fn_cx<'a>(cx: &Cx<'a>, sig: &ast::Signature, contract: &ast::Contract) -> Cx<'a> {
    let mut c = cx.clone();
    c.perms = BTreeMap::new();
    c.rename = BTreeMap::new();
    c.gives = None;
    c.exits = Vec::new();
    c.try_kind = sig.ret.as_ref().and_then(|r| crate::expr::try_kind(&r.ty));
    for u in contract.uses() {
        if let UsesItem::Perm { mode, ident: i, .. } = u {
            let k = match mode {
                PermMode::Move => PermKind::Owned,
                PermMode::Shared => PermKind::Ref,
                PermMode::Mut => PermKind::MutRef,
            };
            c.perms.insert(i.name.clone(), k);
        }
    }
    if let Some(g) = contract.gives().next()
        && sig
            .ret
            .as_ref()
            .and_then(|r| r.name.as_ref())
            .is_some_and(|n| n.name == g.name)
    {
        c.gives = Some(g.name.clone());
        c.rename.insert(g.name.clone(), "__eyk_ret.0".into());
        c.rename.insert(perm_var(&g.name), "__eyk_ret.1@".into());
    }
    c
}

/// 引数の型 `Ptr<T>` / `&Ptr<T>` / `Cell<T>` / `&Cell<T>` の `T` と、`&` か、`Cell` か。
fn pointee(cx: &Cx, ty: &ast::Type) -> Option<(String, bool, bool)> {
    let cell = ptr_or_cell(ty)?;
    let (r, inner) = match &ty.kind {
        TypeKind::Ref { elem, .. } => (true, &**elem),
        _ => (false, ty),
    };
    let TypeKind::Path { path, .. } = &inner.kind else {
        return None;
    };
    let GenericArgs::Angle { args, .. } = &path.segments.last()?.args else {
        return None;
    };
    let t = args.iter().find_map(|a| match a {
        GenericArg::Type(t) => Some(cx.ty(t)),
        _ => None,
    })?;
    Some((t, r, cell))
}

fn param_ty<'s>(sig: &'s ast::Signature, name: &str) -> Option<&'s ast::Type> {
    sig.params.iter().find_map(|p| match p {
        Param::Typed { pat, ty, .. }
            if ebykn_check::index::pat_ident(pat).is_some_and(|i| i.name == name) =>
        {
            Some(ty)
        }
        _ => None,
    })
}

/// 署名と契約（`fn` から本体の前まで）。`c` は関数の文脈。
fn sig_text(
    c: &mut Cx,
    sig: &ast::Signature,
    contract: &ast::Contract,
    exp: &FnExpansion,
) -> String {
    let q = &sig.qualifiers;
    let mut head = String::new();
    if q.constness.is_some() {
        head.push_str("const ");
    }
    if q.asyncness.is_some() {
        head.push_str("async ");
    }
    if q.unsafety.is_some() {
        head.push_str("unsafe ");
    }
    if let Some(abi) = &q.abi {
        head.push_str(&format!("{} ", abi.tokens));
    }
    let mut params: Vec<String> = sig
        .params
        .iter()
        .map(|p| match p {
            Param::Receiver {
                attrs: at,
                reference,
                mutable,
                ty,
                ..
            } => {
                let mut s = attrs(at);
                if let Some(lt) = reference {
                    s.push('&');
                    if let Some(l) = lt {
                        s.push_str(&format!("'{} ", l.name));
                    }
                }
                if *mutable {
                    s.push_str("mut ");
                }
                s.push_str("self");
                if let Some(t) = ty {
                    s.push_str(&format!(": {}", c.ty(t)));
                }
                s
            }
            Param::Typed {
                attrs: at, pat, ty, ..
            } => format!("{}{}: {}", attrs(at), c.pat(pat), c.ty(ty)),
        })
        .collect();
    let mut requires = Vec::new();
    let mut ensures_extra = Vec::new();
    for u in contract.uses() {
        match u {
            UsesItem::Perm { mode, ident: i, .. } => {
                let p = ident(i);
                let v = perm_var(&i.name);
                let Some((t, by_ref, cell)) = param_ty(sig, &i.name).and_then(|ty| pointee(c, ty))
                else {
                    continue;
                };
                let pt = if cell {
                    format!("CellPointsTo<{t}>")
                } else {
                    format!("PointsTo<{t}>")
                };
                let pt = match mode {
                    PermMode::Move => pt,
                    PermMode::Shared => format!("&{pt}"),
                    PermMode::Mut => format!("&mut {pt}"),
                };
                params.push(format!("Tracked({v}): Tracked<{pt}>"));
                let target = if by_ref { format!("*{p}") } else { p.clone() };
                let rel = |v: &str| {
                    if cell {
                        format!("{v}.id() == {target}.id()")
                    } else {
                        format!("{v}.pptr() == {target}")
                    }
                };
                requires.push(rel(&v));
                if *mode == PermMode::Mut {
                    ensures_extra.push(rel(&format!("final({v})")));
                }
            }
            UsesItem::Named { ident: i, ty, .. } => {
                params.push(format!("Tracked({}): Tracked<{}>", ident(i), c.ty(ty)));
            }
        }
    }
    let ret = match &sig.ret {
        None => String::new(),
        Some(r) => {
            let t = c.ty(&r.ty);
            if c.gives.is_some() {
                let (pt, _, cell) = pointee(c, &r.ty).unwrap_or_default();
                let perm = if cell {
                    format!("CellPointsTo<{pt}>")
                } else {
                    format!("PointsTo<{pt}>")
                };
                ensures_extra.push(if cell {
                    "__eyk_ret.1@.id() == __eyk_ret.0.id()".into()
                } else {
                    "__eyk_ret.1@.pptr() == __eyk_ret.0".into()
                });
                format!(" -> (__eyk_ret: ({t}, Tracked<{perm}>))")
            } else {
                match &r.name {
                    Some(n) => format!(" -> ({}: {t})", ident(n)),
                    None if exp.name_ret => format!(" -> ({RET_NAME}: {t})"),
                    None => format!(" -> {t}"),
                }
            }
        }
    };
    // 契約。
    let lg = |c: &mut Cx, e: &ast::Expr| {
        let prev = std::mem::replace(&mut c.logic, true);
        let s = c.expr(e);
        c.logic = prev;
        s
    };
    for e in contract.requires() {
        requires.push(lg(c, e));
    }
    requires.extend(exp.requires.iter().map(refine));
    c.ensures = true;
    let mut ensures: Vec<String> = contract.ensures().map(|e| lg(c, e)).collect();
    c.ensures = false;
    ensures.extend(ensures_extra);
    ensures.extend(exp.ensures.iter().map(refine));
    let returns: Vec<String> = contract.returns().map(|e| lg(c, e)).collect();
    let decreases: Vec<String> = contract.decreases().map(|e| lg(c, e)).collect();
    let mut s = format!(
        "{head}fn {}{}({}){ret}{}",
        ident(&sig.ident),
        generics(c, &sig.generics),
        params.join(", "),
        where_clause(&sig.where_clause)
    );
    for (kw, es) in [
        ("requires", &requires),
        ("ensures", &ensures),
        ("returns", &returns),
        ("decreases", &decreases),
    ] {
        if !es.is_empty() {
            s.push_str(&format!(" {kw} {},", es.join(", ")));
        }
    }
    if contract.no_panic() {
        s.push_str(" no_unwind");
    }
    s
}

/// 足す契約 `P[x]`（§6.4）。
fn refine(r: &Refine) -> String {
    let s = match &r.subject {
        Subject::Var(x) => x.clone(),
        Subject::Old(x) => format!("old({x})"),
        Subject::Final(x) => format!("final({x})"),
    };
    match r.wrap {
        Wrap::None => format!("{s}.invariant()"),
        Wrap::Option => format!("({s} is Some ==> {s}.unwrap().invariant())"),
        Wrap::Result => format!("({s} is Ok ==> {s}.unwrap().invariant())"),
    }
}

/// 整数の基本型か（純粋関数の spec 側を `(e) as T` にする。`OPEN.md` の M0 の注意）。
fn int_type(t: &ast::Type) -> bool {
    matches!(&t.kind, TypeKind::Path { qself: None, path } if path.as_ident().is_some_and(|i| matches!(i.name.as_str(),
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize")))
}

/// `drop_fn` は std の `Drop` の impl の `fn drop`（#61）。Verus は `opens_invariants none` と
/// `no_unwind` のない `Drop` の実装を拒否し、利用者はどちらも `.eyk` に書けないので、書き下しが足す。
fn fn_text(cx: &Cx, item: &ast::Item, f: &ast::Fn, drop_fn: bool) -> String {
    let mut c = fn_cx(cx, &f.sig, &f.contract);
    let exp = cx
        .facts
        .expansions
        .get(&SpanKey::from(f.sig.ident.span))
        .cloned()
        .unwrap_or_default();
    let mut a = attrs(&item.attrs);
    // `item_path` は外の型への impl で `report` を借りるので、借りる前にパスを求める（PR #26 の
    // 5 回目の確認。`borrow_mut()` を持ったまま引数で `cx` のメソッドを呼ぶと panic する）。
    if is_test(&item.attrs) {
        a = format!("#[verifier::external] {a}");
        let path = cx.item_path(&f.sig.ident.name);
        cx.report.borrow_mut().external.push(path);
    }
    if f.unchecked.is_some() {
        a = format!("#[verifier::external_body] {a}");
        let path = cx.item_path(&f.sig.ident.name);
        cx.report.borrow_mut().unchecked.push(path);
    }
    let v = vis(&item.vis);
    let sig = |c: &mut Cx| {
        let s = sig_text(c, &f.sig, &f.contract, &exp);
        // `Drop` の `fn drop` の契約（`no_panic` を含む）は検査が E0003 にするので、`s` は句を持たない。
        if drop_fn {
            format!("{s} opens_invariants none no_unwind")
        } else {
            s
        }
    };
    match &f.body {
        None => format!("{a}{v}{};", sig(&mut c)),
        Some(ast::FnBody::Block(b)) => {
            let sig = sig(&mut c);
            let body = c.body_block(b);
            format!("{a}{v}{sig} {}", rebind_owned(&f.contract, body))
        }
        Some(ast::FnBody::Pure(e)) => pure_fn(&mut c, &a, &item.vis, f, e, &exp),
    }
}

/// 値で受けた領域（`uses r: Region<..>`）と移動で受けた権限（`uses p@`）は持っているので、変更
/// できるよう本体の先頭で `mut` に束縛し直す（#90、#102）。Verus は引数のパターン `Tracked(mut r)` を
/// 受け付けない。
fn rebind_owned(contract: &ast::Contract, body: String) -> String {
    let lets: String = contract
        .uses()
        .filter_map(|u| match u {
            UsesItem::Named { ident: i, ty, .. }
                if !matches!(ty.kind, ast::TypeKind::Ref { .. }) =>
            {
                Some(ident(i))
            }
            UsesItem::Perm {
                mode: PermMode::Move,
                ident: i,
                ..
            } => Some(perm_var(&i.name)),
            _ => None,
        })
        .map(|v| format!("let tracked mut {v} = {v}; "))
        .collect();
    if lets.is_empty() {
        return body;
    }
    let rest = body
        .strip_prefix('{')
        .expect("関数の本体の書き下しは `{` で始まる");
    format!("{{ {lets}{rest}")
}

/// 純粋関数 `fn f(..) -> T = e;`（§7.2）：`when_used_as_spec` の `fn` と `spec fn` の組。
fn pure_fn(c: &mut Cx, a: &str, v: &Vis, f: &ast::Fn, e: &ast::Expr, exp: &FnExpansion) -> String {
    let name = ident(&f.sig.ident);
    let spec_name = format!("__eyk_spec_{}", f.sig.ident.name);
    let ret = f.sig.ret.as_ref();
    let rname = ret
        .and_then(|r| r.name.as_ref())
        .map(ident)
        .unwrap_or_else(|| "__eyk_r".into());
    let rty = ret.map(|r| c.ty(&r.ty)).unwrap_or_else(|| "()".into());
    let mut args = Vec::new();
    let mut recv = None;
    for p in &f.sig.params {
        match p {
            Param::Receiver { .. } => recv = Some("self"),
            Param::Typed { pat, .. } => args.push(c.pat(pat)),
        }
    }
    let call = match recv {
        Some(r) => format!("{r}.{spec_name}({})", args.join(", ")),
        None => format!("{spec_name}({})", args.join(", ")),
    };
    // fn の側：名前付き戻り値で `ensures r == spec(..)` を足す。
    let mut sig_ast = f.sig.clone();
    if let Some(r) = &mut sig_ast.ret
        && r.name.is_none()
    {
        r.name = Some(ast::Ident {
            name: "__eyk_r".into(),
            raw: false,
            span: r.span,
        });
    }
    let sig = sig_text(c, &sig_ast, &f.contract, exp);
    let sig = if sig.contains(" ensures ") {
        sig.replacen(" ensures ", &format!(" ensures {rname} == {call}, "), 1)
    } else {
        // 句の位置（`requires` の後、`returns` などの前）に置く。
        let mut s = sig;
        let at = ["returns", "decreases", "no_unwind"]
            .iter()
            .filter_map(|k| {
                s.find(&format!(" {k} "))
                    .or_else(|| s.strip_suffix(&format!(" {k}")).map(|x| x.len()))
            })
            .min();
        let clause = format!(" ensures {rname} == {call},");
        match at {
            Some(i) => s.insert_str(i, &clause),
            None => s.push_str(&clause),
        }
        s
    };
    let exec_body = c.sub(false).expr(e);
    let spec_body = c.sub(true).expr(e);
    let spec_body = match ret {
        Some(r) if int_type(&r.ty) => format!("({spec_body}) as {rty}"),
        _ => spec_body,
    };
    let pubv = vis(v);
    let open = if matches!(v, Vis::Inherited) {
        ""
    } else {
        "open "
    };
    let params: Vec<String> = f
        .sig
        .params
        .iter()
        .map(|p| match p {
            Param::Receiver {
                reference, mutable, ..
            } => format!(
                "{}{}self",
                if reference.is_some() { "&" } else { "" },
                if *mutable && reference.is_some() {
                    "mut "
                } else {
                    ""
                }
            ),
            Param::Typed { pat, ty, .. } => format!("{}: {}", c.pat(pat), c.ty(ty)),
        })
        .collect();
    format!(
        "{a}#[verifier::when_used_as_spec({spec_name})] {pubv}{sig} {{ {exec_body} }} {pubv}{open}spec fn {spec_name}{}({}) -> {rty} {{ {spec_body} }}",
        generics(c, &f.sig.generics),
        params.join(", ")
    )
    .replacen(&format!("fn {name}"), &format!("fn {name}"), 1)
}

fn spec_fn_text(cx: &Cx, a: &str, v: &Vis, f: &ast::SpecFn) -> String {
    let mut c = fn_cx(cx, &f.sig, &f.contract);
    let sig = sig_text(&mut c, &f.sig, &f.contract, &FnExpansion::default());
    // `pub` でない `open spec fn` は Verus のエラーなので、`pub` でなければ `spec fn` とだけ書く。
    let kw = match (v, f.closed.is_some()) {
        (Vis::Inherited, _) => "spec ".to_string(),
        (_, true) => format!("{}closed spec ", vis(v)),
        (_, false) => format!("{}open spec ", vis(v)),
    };
    match &f.body {
        Some(e) => format!("{a}{kw}{sig} {{ {} }}", c.sub(true).expr(e)),
        None => format!("{a}{kw}{sig};"),
    }
}

fn lemma_text(cx: &Cx, a: &str, v: &str, f: &ast::LemmaFn, name: &str) -> String {
    if f.body.is_some() {
        crate::eykp_only("lemma fn の本体");
    }
    if f.contract
        .clauses
        .iter()
        .any(|c| matches!(c.kind, ast::ClauseKind::By(_)))
    {
        crate::eykp_only("by(..)");
    }
    let mut c = fn_cx(cx, &f.sig, &f.contract);
    let sig = sig_text(&mut c, &f.sig, &f.contract, &FnExpansion::default());
    if f.unchecked.is_some() {
        cx.report.borrow_mut().unchecked.push(name.to_string());
        return format!("{a}#[verifier::external_body] {v}proof {sig} {{ }}");
    }
    // 本体は `.eykp`（M3）。`.eykp` がなければ `admit()`（§7.2）。`verify` が一覧に出す。
    cx.report.borrow_mut().admitted.push(name.to_string());
    format!("{a}{v}proof {sig} {{ vstd::prelude::admit() }}")
}

// ---- trait ----

/// trait（契約のないもの。契約のある trait は検査が E0003 にする）。本体のある既定のメソッドは
/// 検証の結果を出す項目にする（`stack::Tr::m`）。
fn trait_pieces(cx: &Cx, item: &ast::Item, t: &ast::Trait, indent: usize, out: &mut Vec<Piece>) {
    cx.note_macros(&item.attrs, || cx.item_path(&t.ident.name));
    let pad = "    ".repeat(indent);
    out.push(Piece::Raw(format!(
        "{pad}{}{}{}{}trait {}{}{}{} {{",
        attrs(&item.attrs),
        vis(&item.vis),
        t.unsafety.map(|_| "unsafe ").unwrap_or_default(),
        t.auto.map(|_| "auto ").unwrap_or_default(),
        ident(&t.ident),
        generics(cx, &t.generics),
        t.supertraits
            .as_ref()
            .map(|s| format!(": {}", s.tokens))
            .unwrap_or_default(),
        where_clause(&t.where_clause),
    )));
    let mut c = cx.clone();
    let mut p = cx.module.clone();
    p.push(t.ident.name.clone());
    c.self_ty = Some(p);
    for ti in &t.items {
        match &ti.kind {
            TraitItemKind::Fn(f) => c.note_macros(&ti.attrs, || c.item_path(&f.sig.ident.name)),
            TraitItemKind::SpecFn(f) => c.note_macros(&ti.attrs, || c.item_path(&f.sig.ident.name)),
            TraitItemKind::LemmaFn(f) => {
                c.note_macros(&ti.attrs, || c.item_path(&f.sig.ident.name))
            }
            TraitItemKind::Rust(tk) => c.note_macros(&ti.attrs, || rust_item_name(&c, &tk.tokens)),
        }
        match &ti.kind {
            TraitItemKind::Fn(f) => {
                let mut fc = fn_cx(&c, &f.sig, &f.contract);
                let sig = sig_text(&mut fc, &f.sig, &f.contract, &FnExpansion::default());
                let (body, name) = match &f.body {
                    Some(ast::FnBody::Block(b)) => {
                        (fc.block(b), Some(c.item_path(&f.sig.ident.name)))
                    }
                    _ => (";".into(), None),
                };
                out.push(tokens(
                    format!("{}{sig} {body}", attrs(&ti.attrs)),
                    name,
                    indent + 1,
                ));
            }
            TraitItemKind::Rust(tk) => out.push(tokens(tk.tokens.to_string(), None, indent + 1)),
            // 契約のある trait は検査が E0003 にする（M4）。
            TraitItemKind::SpecFn(_) | TraitItemKind::LemmaFn(_) => {}
        }
    }
    out.push(Piece::Raw(format!("{pad}}}")));
    out.push(Piece::Raw(String::new()));
}

// ---- impl ----

fn impl_pieces(cx: &Cx, item: &ast::Item, imp: &ast::Impl, indent: usize, out: &mut Vec<Piece>) {
    let self_ty = cx.index.type_path(&cx.module, &imp.self_ty, None);
    let mut c = cx.clone();
    c.self_struct = self_ty.as_ref().and_then(|t| cx.index.structs.get(t));
    c.self_ty = self_ty.clone();
    let pad = "    ".repeat(indent);
    let g = generics(cx, &imp.generics);
    let w = where_clause(&imp.where_clause);
    let st = cx.ty(&imp.self_ty);
    cx.note_macros(&item.attrs, || match &imp.trait_ {
        Some(t) => format!("impl {} for {st}", cx.path(t)),
        None => format!("impl {st}"),
    });
    if self_ty.is_none() {
        c.foreign_impl = Some(match &imp.trait_ {
            Some(t) => format!("<{st} as {}>", cx.path(t)),
            None => format!("<{st}>"),
        });
    }
    match cx.index.impl_trait(&cx.module, imp) {
        ImplTrait::Invariant => {
            for m in &imp.items {
                note_item_macros(&c, m);
            }
            let Some(f) = ebykn_check::invariant_fn(imp) else {
                return;
            };
            let body = f
                .body
                .as_ref()
                .map(|e| c.sub(true).expr(e))
                .unwrap_or_default();
            let mut conj = vec![body];
            if let Some(t) = &self_ty
                && let Some(ns) = cx.facts.nested.get(t)
            {
                for (field, _, wrap) in ns {
                    conj.push(match wrap {
                        Wrap::Option => {
                            format!("(self.{field} is Some ==> self.{field}.unwrap().invariant())")
                        }
                        _ => format!("self.{field}.invariant()"),
                    });
                }
            }
            let body = if conj.len() == 1 {
                conj.remove(0)
            } else {
                conj.iter()
                    .map(|x| format!("({x})"))
                    .collect::<Vec<_>>()
                    .join(" && ")
            };
            let kw = if f.closed.is_some() { "closed" } else { "open" };
            out.push(Piece::Raw(format!("{pad}impl{g} {st}{w} {{")));
            out.push(tokens(
                format!("pub {kw} spec fn invariant(&self) -> bool {{ {body} }}"),
                Some(c.item_path("invariant")),
                indent + 1,
            ));
            out.push(Piece::Raw(format!("{pad}}}")));
            out.push(Piece::Raw(String::new()));
            let _ = generic_names;
        }
        kind => {
            let head = match (&imp.trait_, kind) {
                (Some(t), _) => format!(
                    "{}{}impl{g} {}{} for {st}{w} {{",
                    attrs(&item.attrs),
                    imp.unsafety.map(|_| "unsafe ").unwrap_or_default(),
                    if imp.negative { "!" } else { "" },
                    cx.path(t)
                ),
                (None, _) => format!(
                    "{}{}impl{g} {st}{w} {{",
                    attrs(&item.attrs),
                    imp.unsafety.map(|_| "unsafe ").unwrap_or_default()
                ),
            };
            out.push(Piece::Raw(format!("{pad}{head}")));
            for m in &imp.items {
                match &m.kind {
                    ItemKind::SpecFn(f) if kind == ImplTrait::View => {
                        note_item_macros(&c, m);
                        let kwv = if f.closed.is_some() { "closed" } else { "open" };
                        let mut fc = fn_cx(&c, &f.sig, &f.contract);
                        let sig = sig_text(&mut fc, &f.sig, &f.contract, &FnExpansion::default());
                        let body = f
                            .body
                            .as_ref()
                            .map(|e| fc.sub(true).expr(e))
                            .unwrap_or_default();
                        out.push(tokens(
                            format!("{}{kwv} spec {sig} {{ {body} }}", attrs(&m.attrs)),
                            Some(c.item_path(&f.sig.ident.name)),
                            indent + 1,
                        ));
                    }
                    ItemKind::Fn(f) if kind == ImplTrait::Drop && f.sig.ident.name == "drop" => {
                        out.push(tokens(
                            fn_text(&c, m, f, true),
                            Some(c.item_path(&f.sig.ident.name)),
                            indent + 1,
                        ));
                    }
                    _ => {
                        let (text, name) = item_text(&c, m);
                        if !text.is_empty() {
                            out.push(tokens(text, name, indent + 1));
                        }
                    }
                }
            }
            out.push(Piece::Raw(format!("{pad}}}")));
            out.push(Piece::Raw(String::new()));
        }
    }
}
