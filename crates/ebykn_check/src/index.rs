//! 索引と名前解決（仕様 §6.5）。
//!
//! 索引はクレートの全項目（`fn` の `uses` / `gives`、`struct` の `owns` / `spec` フィールド、
//! `impl Invariant for T`、`impl T` の Self 型とメソッド、`impl View for T`）と prelude の署名。
//! 名前は「現在のモジュール → `use` 宣言 → prelude」の順に引く。解決できない名前は外のもの
//! （そのまま Verus に渡す）。`interface.json`（`[ebykn-dependencies]`）は M5。

use std::collections::{BTreeMap, BTreeSet};

use ebykn_syntax::ast::{self, ItemKind, PermMode, TypeKind, UsesItem, Vis};
use ebykn_syntax::span::{FileId, Span};
use proc_macro2::{Delimiter, TokenStream, TokenTree};

/// クレートの中の項目のパス（クレートルートからの名前の列）。
pub type ItemPath = Vec<String>;

/// 消去フィールドの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErasedKind {
    Owns,
    Spec,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub path: ItemPath,
    /// 名前付きの実行時フィールド（tuple struct では空）。
    pub fields: Vec<(String, ast::Type)>,
    /// `owns { .. }` と `spec { .. }` のフィールド（書かれた順）。
    pub erased: Vec<(String, ErasedKind, ast::Type)>,
    pub module: ItemPath,
}

impl StructInfo {
    pub fn erased_kind(&self, name: &str) -> Option<ErasedKind> {
        self.erased
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, k, _)| *k)
    }

    pub fn erased_ty(&self, name: &str) -> Option<&ast::Type> {
        self.erased
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, _, t)| t)
    }
}

/// `uses` の一つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseInfo {
    /// `p@` / `&p@` / `&mut p@`。`param` は受け手を除いた引数の番号。`cell` は型が `Cell` か。
    Perm {
        name: String,
        param: usize,
        mode: PermMode,
        cell: bool,
    },
    /// `r: Region<..>` など。
    Named { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnKind {
    /// `fn`（純粋関数を含む）。
    Fn,
    Unchecked,
    Spec,
    Lemma,
}

/// 関数の署名のうち、検査と書き下しに要るもの。
#[derive(Debug, Clone)]
pub struct FnInfo {
    pub name: String,
    /// 受け手を除いた引数の名前（パターンが識別子でなければ `None`）。
    pub params: Vec<Option<String>>,
    pub has_receiver: bool,
    pub uses: Vec<UseInfo>,
    /// `gives` の名前。
    pub gives: Vec<String>,
    pub is_pub: bool,
    pub kind: FnKind,
}

impl FnInfo {
    pub fn has_perms(&self) -> bool {
        !self.uses.is_empty() || !self.gives.is_empty()
    }
}

/// `use` 宣言の一つ。
#[derive(Debug, Clone)]
pub enum Import {
    /// `use a::b as c;`（`alias` は `c`）。
    Name { alias: String, path: Vec<String> },
    /// `use a::b::*;`。
    Glob { path: Vec<String> },
}

/// 一つのモジュールの名前。
#[derive(Debug, Clone, Default)]
pub struct ModuleScope {
    /// このモジュールで定義した名前（`fn`・`struct`・`enum`・`mod`・`type`・`trait` など）。
    pub items: BTreeMap<String, ItemPath>,
    pub imports: Vec<Import>,
}

/// `impl Invariant for T`。
#[derive(Debug, Clone)]
pub struct InvariantInfo {
    pub file: FileId,
    pub span: Span,
}

/// prelude の関数（§7.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreludeFn {
    /// `Ptr::new` / `Ptr::empty` / `Cell::new` / `Cell::empty`：権限を返す。`cell` は `Cell` か。
    Alloc { cell: bool },
}

/// prelude が定義する型の名前。
pub const PRELUDE_TYPES: &[&str] = &[
    "Ptr",
    "Cell",
    "Region",
    "PointsTo",
    "CellPointsTo",
    "CellId",
    "Loc",
    "SeqExt",
    "View",
];

/// 名前を解決した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// クレートの中の自由関数。
    Fn(ItemPath),
    /// 型 `ty` の固有の impl のメソッド。
    Method {
        ty: ItemPath,
        name: String,
    },
    /// クレートの中の構造体・列挙・型の別名・trait など（関数でないもの）。
    Type(ItemPath),
    Module(ItemPath),
    Prelude(PreludeFn),
    /// prelude の型（`Ptr` など）。
    PreludeType(&'static str),
    /// 解決できない（外のクレート、std、ローカル変数など）。
    Unknown,
}

#[derive(Debug, Default)]
pub struct Index {
    pub structs: BTreeMap<ItemPath, StructInfo>,
    pub fns: BTreeMap<ItemPath, FnInfo>,
    /// 固有の impl のメソッド（型のパス → 名前 → 署名）。
    pub methods: BTreeMap<ItemPath, BTreeMap<String, FnInfo>>,
    pub invariants: BTreeMap<ItemPath, InvariantInfo>,
    pub views: BTreeSet<ItemPath>,
    pub modules: BTreeMap<ItemPath, ModuleScope>,
    /// 定義した名前の種類（関数か、型か、モジュールか）。
    pub kinds: BTreeMap<ItemPath, DefKind>,
    /// `uses` / `gives` を持つメソッドの名前（E0514）。
    pub perm_method_names: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Fn,
    Type,
    Module,
    Other,
}

/// 関数の署名から [`FnInfo`] を作る。
pub fn fn_info(vis: &Vis, sig: &ast::Signature, contract: &ast::Contract, kind: FnKind) -> FnInfo {
    let mut params = Vec::new();
    let mut has_receiver = false;
    let mut tys = Vec::new();
    for p in &sig.params {
        match p {
            ast::Param::Receiver { .. } => has_receiver = true,
            ast::Param::Typed { pat, ty, .. } => {
                params.push(pat_ident(pat).map(|i| i.name.clone()));
                tys.push(ty);
            }
        }
    }
    let mut uses = Vec::new();
    for u in contract.uses() {
        match u {
            UsesItem::Perm { mode, ident, .. } => {
                if let Some(i) = params
                    .iter()
                    .position(|p| p.as_deref() == Some(ident.name.as_str()))
                {
                    uses.push(UseInfo::Perm {
                        name: ident.name.clone(),
                        param: i,
                        mode: *mode,
                        cell: ptr_or_cell(tys[i]) == Some(true),
                    });
                }
            }
            UsesItem::Named { ident, .. } => uses.push(UseInfo::Named {
                name: ident.name.clone(),
            }),
        }
    }
    FnInfo {
        name: sig.ident.name.clone(),
        params,
        has_receiver,
        uses,
        gives: contract.gives().map(|g| g.name.clone()).collect(),
        is_pub: !matches!(vis, Vis::Inherited),
        kind,
    }
}

/// パターンが一つの識別子（`mut x` を含む）ならその識別子。
pub fn pat_ident(p: &ast::Pat) -> Option<&ast::Ident> {
    match &p.kind {
        ast::PatKind::Ident {
            ident, sub: None, ..
        } => Some(ident),
        ast::PatKind::Type { pat, .. } => pat_ident(pat),
        _ => None,
    }
}

/// 型が直接 `Ptr<..>` か `Cell<..>` なら（`&` を一つ外して）`Some(cell か)`。
pub fn ptr_or_cell(ty: &ast::Type) -> Option<bool> {
    let ty = match &ty.kind {
        TypeKind::Ref {
            mutable: false,
            elem,
            ..
        } => elem,
        _ => ty,
    };
    match last_segment(ty)? {
        "Ptr" => Some(false),
        "Cell" => Some(true),
        _ => None,
    }
}

/// 型が `Region<..>`（`&` / `&mut` を一つ外して）か。
pub fn is_region_type(ty: &ast::Type) -> bool {
    let ty = match &ty.kind {
        TypeKind::Ref { elem, .. } => elem,
        _ => ty,
    };
    last_segment(ty) == Some("Region")
}

/// 型が一つの名前（`ebykn_prelude::` を前に付けてよい）のパスなら、その最後の名前。
fn last_segment(ty: &ast::Type) -> Option<&str> {
    let TypeKind::Path { qself: None, path } = &ty.kind else {
        return None;
    };
    match path.segments.as_slice() {
        [s] if !path.leading_colon => Some(s.ident.name.as_str()),
        [p, s] if p.ident.is("ebykn_prelude") => Some(s.ident.name.as_str()),
        _ => None,
    }
}

impl Index {
    /// モジュール `module` の項目の索引を作る（[`crate::project::Project::for_each_module`] から呼ぶ）。
    pub fn add_module(&mut self, module: &[String], file: FileId, items: &[ast::Item]) {
        let scope = self.modules.entry(module.to_vec()).or_default();
        let path_of = |name: &str| {
            let mut p = module.to_vec();
            p.push(name.to_string());
            p
        };
        let mut defs = Vec::new();
        for item in items {
            let (name, kind) = match &item.kind {
                ItemKind::Fn(f) => (&f.sig.ident, DefKind::Fn),
                ItemKind::SpecFn(f) => (&f.sig.ident, DefKind::Fn),
                ItemKind::LemmaFn(f) => (&f.sig.ident, DefKind::Fn),
                ItemKind::Struct(s) => (&s.ident, DefKind::Type),
                ItemKind::Enum(e) => (&e.ident, DefKind::Type),
                ItemKind::Trait(t) => (&t.ident, DefKind::Type),
                ItemKind::TypeAlias(t) => (&t.ident, DefKind::Type),
                ItemKind::Subset(t) => (&t.ident, DefKind::Type),
                ItemKind::Mod(m) => (&m.ident, DefKind::Module),
                ItemKind::Const(c) => (&c.ident, DefKind::Other),
                ItemKind::Static(s) => (&s.ident, DefKind::Other),
                ItemKind::Use(t) => {
                    scope.imports.extend(parse_use(&t.tokens));
                    continue;
                }
                _ => continue,
            };
            let p = path_of(&name.name);
            scope.items.insert(name.name.clone(), p.clone());
            defs.push((p, kind, item));
        }
        for (p, kind, item) in defs {
            self.kinds.insert(p.clone(), kind);
            match &item.kind {
                ItemKind::Fn(f) => {
                    let k = if f.unchecked.is_some() {
                        FnKind::Unchecked
                    } else {
                        FnKind::Fn
                    };
                    self.fns
                        .insert(p, fn_info(&item.vis, &f.sig, &f.contract, k));
                }
                ItemKind::SpecFn(f) => {
                    self.fns
                        .insert(p, fn_info(&item.vis, &f.sig, &f.contract, FnKind::Spec));
                }
                ItemKind::LemmaFn(f) => {
                    self.fns
                        .insert(p, fn_info(&item.vis, &f.sig, &f.contract, FnKind::Lemma));
                }
                ItemKind::Struct(s) => {
                    let fields = match &s.fields {
                        ast::Fields::Named(fs) => fs
                            .iter()
                            .filter_map(|f| Some((f.ident.as_ref()?.name.clone(), f.ty.clone())))
                            .collect(),
                        _ => Vec::new(),
                    };
                    let mut erased = Vec::new();
                    for (sec, kind) in [(&s.owns, ErasedKind::Owns), (&s.spec, ErasedKind::Spec)] {
                        for f in sec.iter().flat_map(|e| &e.fields) {
                            if let Some(i) = &f.ident {
                                erased.push((i.name.clone(), kind, f.ty.clone()));
                            }
                        }
                    }
                    self.structs.insert(
                        p.clone(),
                        StructInfo {
                            path: p,
                            fields,
                            erased,
                            module: module.to_vec(),
                        },
                    );
                }
                _ => {}
            }
        }
        let _ = file;
    }

    /// 2 回目の走査：impl を型に結び付ける（型の名前の解決に全モジュールの名前が要る）。
    pub fn add_impls(&mut self, module: &[String], file: FileId, items: &[ast::Item]) {
        for item in items {
            let ItemKind::Impl(imp) = &item.kind else {
                continue;
            };
            let Some(ty) = self.type_path(module, &imp.self_ty, None) else {
                continue;
            };
            match imp.trait_.as_ref().and_then(|t| t.as_ident()) {
                Some(t) if t.is("Invariant") => {
                    self.invariants.insert(
                        ty,
                        InvariantInfo {
                            file,
                            span: item.span,
                        },
                    );
                }
                Some(t) if t.is("View") => {
                    self.views.insert(ty);
                }
                _ if imp.trait_.is_none() => {
                    for m in &imp.items {
                        let info = match &m.kind {
                            ItemKind::Fn(f) => fn_info(
                                &m.vis,
                                &f.sig,
                                &f.contract,
                                if f.unchecked.is_some() {
                                    FnKind::Unchecked
                                } else {
                                    FnKind::Fn
                                },
                            ),
                            ItemKind::SpecFn(f) => {
                                fn_info(&m.vis, &f.sig, &f.contract, FnKind::Spec)
                            }
                            ItemKind::LemmaFn(f) => {
                                fn_info(&m.vis, &f.sig, &f.contract, FnKind::Lemma)
                            }
                            _ => continue,
                        };
                        if info.has_perms() {
                            self.perm_method_names.insert(info.name.clone());
                        }
                        self.methods
                            .entry(ty.clone())
                            .or_default()
                            .insert(info.name.clone(), info);
                    }
                }
                _ => {}
            }
        }
    }

    /// 型のパスを解決する（`Self` は `self_ty`）。クレートの型でなければ `None`。
    pub fn type_path(
        &self,
        module: &[String],
        ty: &ast::Type,
        self_ty: Option<&ItemPath>,
    ) -> Option<ItemPath> {
        let TypeKind::Path { qself: None, path } = &ty.kind else {
            return None;
        };
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        if segs == ["Self"] {
            return self_ty.cloned();
        }
        match self.resolve(module, path.leading_colon, &segs, self_ty) {
            Resolved::Type(p) => Some(p),
            _ => None,
        }
    }

    /// パスを解決する。
    pub fn resolve(
        &self,
        module: &[String],
        leading_colon: bool,
        segs: &[String],
        self_ty: Option<&ItemPath>,
    ) -> Resolved {
        if leading_colon || segs.is_empty() {
            return Resolved::Unknown;
        }
        let (start, rest) = match segs[0].as_str() {
            "crate" => (Resolved::Module(Vec::new()), &segs[1..]),
            "self" if segs.len() > 1 => (Resolved::Module(module.to_vec()), &segs[1..]),
            "super" => {
                let mut m = module.to_vec();
                if m.pop().is_none() {
                    return Resolved::Unknown;
                }
                (Resolved::Module(m), &segs[1..])
            }
            "Self" => match self_ty {
                Some(t) => (Resolved::Type(t.clone()), &segs[1..]),
                None => return Resolved::Unknown,
            },
            first => (self.lookup(module, first, 0), &segs[1..]),
        };
        self.descend(start, rest)
    }

    fn descend(&self, mut cur: Resolved, rest: &[String]) -> Resolved {
        for (i, seg) in rest.iter().enumerate() {
            cur = match cur {
                Resolved::Module(m) => self.lookup_in(&m, seg),
                Resolved::Type(t) if i + 1 == rest.len() => {
                    if self.methods.get(&t).is_some_and(|ms| ms.contains_key(seg)) {
                        Resolved::Method {
                            ty: t,
                            name: seg.clone(),
                        }
                    } else {
                        Resolved::Unknown
                    }
                }
                Resolved::PreludeType(t) if i + 1 == rest.len() => match (t, seg.as_str()) {
                    ("Ptr", "new" | "empty") => Resolved::Prelude(PreludeFn::Alloc { cell: false }),
                    ("Cell", "new" | "empty") => Resolved::Prelude(PreludeFn::Alloc { cell: true }),
                    _ => Resolved::Unknown,
                },
                _ => return Resolved::Unknown,
            };
        }
        cur
    }

    /// モジュール `m` で定義した名前（`use` を見ない）。
    fn lookup_in(&self, m: &[String], name: &str) -> Resolved {
        match self.modules.get(m).and_then(|s| s.items.get(name)) {
            Some(p) => self.def(p),
            None => Resolved::Unknown,
        }
    }

    fn def(&self, p: &ItemPath) -> Resolved {
        match self.kinds.get(p) {
            Some(DefKind::Fn) => Resolved::Fn(p.clone()),
            Some(DefKind::Type) => Resolved::Type(p.clone()),
            Some(DefKind::Module) => Resolved::Module(p.clone()),
            _ => Resolved::Unknown,
        }
    }

    /// 名前を「現在のモジュール → `use` 宣言 → prelude」の順に引く。
    fn lookup(&self, module: &[String], name: &str, depth: usize) -> Resolved {
        if depth > 16 {
            return Resolved::Unknown;
        }
        let Some(scope) = self.modules.get(module) else {
            return Resolved::Unknown;
        };
        if let Some(p) = scope.items.get(name) {
            return self.def(p);
        }
        for imp in &scope.imports {
            if let Import::Name { alias, path } = imp
                && alias == name
            {
                return self.resolve_import(module, path, depth);
            }
        }
        for imp in &scope.imports {
            if let Import::Glob { path } = imp
                && let Resolved::Module(m) = self.resolve_import(module, path, depth)
                && let r @ (Resolved::Fn(_) | Resolved::Type(_) | Resolved::Module(_)) =
                    self.lookup_in(&m, name)
            {
                return r;
            }
        }
        if let Some(t) = PRELUDE_TYPES.iter().find(|t| **t == name) {
            return Resolved::PreludeType(t);
        }
        Resolved::Unknown
    }

    fn resolve_import(&self, module: &[String], path: &[String], depth: usize) -> Resolved {
        let Some(first) = path.first() else {
            return Resolved::Unknown;
        };
        let start = match first.as_str() {
            "crate" => Resolved::Module(Vec::new()),
            "self" => Resolved::Module(module.to_vec()),
            "super" => {
                let mut m = module.to_vec();
                if m.pop().is_none() {
                    return Resolved::Unknown;
                }
                Resolved::Module(m)
            }
            "ebykn_prelude" => {
                return match path.get(1).map(String::as_str) {
                    Some(t) if path.len() == 2 => PRELUDE_TYPES
                        .iter()
                        .find(|x| **x == t)
                        .map_or(Resolved::Unknown, |t| Resolved::PreludeType(t)),
                    _ => Resolved::Unknown,
                };
            }
            // 2018 以降の `use a::b;` の `a` は、このモジュールの名前か外のクレート。
            name => match self.modules.get(module).and_then(|s| s.items.get(name)) {
                Some(p) => self.def(p),
                None => return Resolved::Unknown,
            },
        };
        let _ = depth;
        self.descend(start, &path[1..])
    }

    /// 呼び出しの対象の署名。
    pub fn callee(&self, r: &Resolved) -> Option<&FnInfo> {
        match r {
            Resolved::Fn(p) => self.fns.get(p),
            Resolved::Method { ty, name } => self.methods.get(ty)?.get(name),
            _ => None,
        }
    }
}

/// `use` 宣言のトークン列（`pub`、`use`、`;` を含んでよい）を読む。読めない形は捨てる。
pub fn parse_use(tokens: &TokenStream) -> Vec<Import> {
    let mut tts: Vec<TokenTree> = tokens.clone().into_iter().collect();
    // 先頭の可視性と `use`、末尾の `;` を外す。
    while let Some(TokenTree::Ident(i)) = tts.first() {
        let s = i.to_string();
        if s == "pub" {
            tts.remove(0);
            if let Some(TokenTree::Group(g)) = tts.first()
                && g.delimiter() == Delimiter::Parenthesis
            {
                tts.remove(0);
            }
        } else if s == "use" {
            tts.remove(0);
            break;
        } else {
            break;
        }
    }
    if let Some(TokenTree::Punct(p)) = tts.last()
        && p.as_char() == ';'
    {
        tts.pop();
    }
    let mut out = Vec::new();
    use_tree(&tts, &[], &mut out);
    out
}

fn use_tree(tts: &[TokenTree], prefix: &[String], out: &mut Vec<Import>) {
    let mut i = 0;
    let mut path = prefix.to_vec();
    // 先頭の `::` は外のクレート。
    if is_path_sep(tts, 0) {
        return;
    }
    loop {
        match tts.get(i) {
            Some(TokenTree::Ident(id)) => {
                let name = id.to_string();
                let name = name.strip_prefix("r#").unwrap_or(&name).to_string();
                i += 1;
                if is_path_sep(tts, i) {
                    path.push(name);
                    i += 2;
                    continue;
                }
                // 末尾の名前（`as` が続くかもしれない）。
                let alias = match (tts.get(i), tts.get(i + 1)) {
                    (Some(TokenTree::Ident(a)), Some(TokenTree::Ident(b))) if a == "as" => {
                        b.to_string()
                    }
                    _ => name.clone(),
                };
                if name == "self" {
                    // `use a::{self};` は `a` を取り込む。
                    let alias = if alias == "self" {
                        path.last().cloned().unwrap_or_default()
                    } else {
                        alias
                    };
                    out.push(Import::Name { alias, path });
                } else {
                    path.push(name);
                    if alias != "_" {
                        out.push(Import::Name { alias, path });
                    }
                }
                return;
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '*' => {
                out.push(Import::Glob { path });
                return;
            }
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                for part in inner.split(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ','))
                {
                    if !part.is_empty() {
                        use_tree(part, &path, out);
                    }
                }
                return;
            }
            _ => return,
        }
    }
}

fn is_path_sep(tts: &[TokenTree], i: usize) -> bool {
    matches!(
        (tts.get(i), tts.get(i + 1)),
        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b))) if a.as_char() == ':' && b.as_char() == ':'
    )
}
