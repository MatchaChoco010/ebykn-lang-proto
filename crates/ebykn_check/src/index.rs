//! 索引と名前解決（仕様 §6.5）。
//!
//! 索引はクレートの全項目（`fn` の `uses` / `gives`、`struct` の `owns` / `spec` フィールド、
//! `impl Invariant for T`、`impl T` の Self 型とメソッド、`impl View for T`）と prelude の署名。
//! 名前は「現在のモジュール → `use` 宣言 → prelude」の順に引く。解決できない名前は外のもの
//! （そのまま Verus に渡す）。`interface.json`（`[ebykn-dependencies]`）は M5。

use std::cell::{OnceCell, RefCell};
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
    /// 実行時のフィールドの型（名前付きと tuple struct のもの。書かれた順）。
    pub runtime: Vec<ast::Type>,
    /// 型の引数の名前（ライフタイムと定数は除く）。
    pub params: Vec<String>,
    /// unit struct（`struct S;`）か。
    pub unit: bool,
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

/// 列挙（E0517 のゼロサイズ型の判定に使う）。
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub module: ItemPath,
    /// 型の引数の名前（ライフタイムと定数は除く）。
    pub params: Vec<String>,
    /// 列挙子の名前、実行時のフィールドの型、フィールドのない列挙子（`A`）か。
    pub variants: Vec<(String, Vec<ast::Type>, bool)>,
    /// `#[repr(..)]`（`cfg_attr` の中を含む）か、`#[cfg]` の付いた列挙子があるか。
    /// どちらも大きさや列挙子の数が定義から決まらないので、ゼロサイズ型と判定しない。
    pub opaque: bool,
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
    /// `r: Region<..>` など。`mode` は型の形（`Region<..>` は移動、`&Region<..>` は共有、
    /// `&mut Region<..>` は可変）。
    Named { name: String, mode: PermMode },
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

/// std の `Drop` の判定でパスを引いた結果（[`Index::find_ext`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ext {
    Module(ItemPath),
    /// クレートの定義（モジュールでないもの）。
    Crate,
    /// 外のクレートか std の prelude のパス（`use` の別名をたどった後）。
    External(Vec<String>),
    /// モジュールの中に見つからない。
    NotFound,
    /// 深さの守りで打ち切った（分からない。外とも見つからないとも言わない）。
    Cut,
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
    "Invariant",
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

/// prelude の権限の型の名前（読まないトークンの中に現れたら権限の操作とみなす）。
pub const PERM_TYPES: &[&str] = &["Ptr", "Cell", "Region", "PointsTo", "CellPointsTo"];

/// 解決した名前が、権限の型・権限を作る関数・`uses` / `gives` を持つ関数か。
pub fn perm_resolved(index: &Index, r: &Resolved) -> bool {
    matches!(r, Resolved::Prelude(PreludeFn::Alloc { .. }))
        || matches!(r, Resolved::PreludeType(t) if PERM_TYPES.contains(t))
        || index.callee(r).is_some_and(FnInfo::has_perms)
}

/// 型の別名の行き先を引けないか（右辺が修飾パス `<T as Tr>::A`、別名の型の引数 `T`、`T::A`。別名の鎖も
/// たどる）。
pub fn alias_unresolved(index: &Index, p: &[String], depth: usize) -> bool {
    let Some(a) = index.aliases.get(p) else {
        return false;
    };
    if depth > 16 {
        return true;
    }
    match &a.ty.kind {
        TypeKind::Path { qself: Some(_), .. } => true,
        TypeKind::Path { qself: None, path } => {
            let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
            if segs.first().is_some_and(|f| a.params.contains(f)) {
                return true;
            }
            match index.resolve_exact(&a.module, path.leading_colon, &segs, None) {
                Resolved::Type(q) => alias_unresolved(index, &q, depth + 1),
                _ => false,
            }
        }
        _ => false,
    }
}

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
    /// クレートの中の値（`const`・`static`・列挙子）。関数でも型でもない。
    Value(ItemPath),
    /// 解決できない（外のクレート、std、ローカル変数など）。
    Unknown,
}

#[derive(Debug, Default)]
pub struct Index {
    pub structs: BTreeMap<ItemPath, StructInfo>,
    pub enums: BTreeMap<ItemPath, EnumInfo>,
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
    /// 型の別名（`type A<T> = S;`）：別名のパス → 定義。
    pub aliases: BTreeMap<ItemPath, Alias>,
    /// `#[cfg(..)]`（`cfg_attr` の中を含む）が付いた項目とモジュール。索引は cfg を評価しないので、
    /// これらが残るかは分からない（`OPEN.md`）。
    pub gated: BTreeSet<ItemPath>,
    /// `#[cfg(..)]` が付いたメソッドか、それを含む impl（型のパス、名前）。
    pub gated_methods: BTreeSet<(ItemPath, String)>,
    /// 同じモジュール・同じ名前空間に同じ名前の定義が二つ以上ある位置（cfg で分けた定義など）。
    pub duplicates: Vec<Span>,
    /// 依存クレートがコードで使われる名前（`Ebykn.toml` の `[dependencies]`。[`Index::is_external_root`]）。
    pub dependencies: BTreeSet<String>,
    /// [`Index::perm_fn_names`] と [`Index::perm_token_names`] の結果（索引ができたあとに一度だけ作る）。
    perm_fn_names_cache: OnceCell<BTreeSet<String>>,
    perm_token_names_cache: OnceCell<BTreeSet<String>>,
    /// [`Index::find_ext`] が一つの問い合わせの中でたどった glob の（モジュール、名前、根か）。glob が
    /// 互いに取り込み合っても、同じところを二度たどらない。
    ext_seen: RefCell<BTreeSet<(ItemPath, String, bool)>>,
    /// 検査している本体の中の項目を囲む本体の項目（[`LocalScope`]）。入れ子の本体ごとに積む。
    pub nested_scopes: NestedScopes,
}

/// [`Index::nested_scopes`] の型。
pub type NestedScopes = RefCell<Vec<LocalScope>>;

/// 関数の本体の中の項目（索引に入らない）。ブロックに入るときに集め、ブロックの終わりで戻し、入れ子の項目の
/// 本体に引き継ぐ（[`crate::body::Walker`]）。
#[derive(Debug, Clone, Default)]
pub struct LocalScope {
    /// `use` 以外の項目の名前（`fn`・型・定数など）。
    pub items: BTreeSet<String>,
    /// 行き先が外と分かる glob の `use`（`use std::collections::*;`）があるか。
    pub ext_glob: bool,
    /// クレートの中の glob の `use` の行き先（`use crate::Color::*;` の `[crate, Color]`）。
    pub globs: Vec<Vec<String>>,
    /// `use` の別名と、その行き先のパス（`use crate::m as mm;` の `mm` → `[crate, m]`）。
    pub aliases: BTreeMap<String, Vec<String>>,
}

/// 外のクレートと std とみなす根の名前（[`Index::is_external_root`]）。std の prelude の名前と、`.eyk` が
/// 裸の名前で書く vstd の名前（§3.1）、組み込みの型。
const STD_ROOTS: &[&str] = &["std", "core", "alloc"];
const STD_PRELUDE_NAMES: &[&str] = &[
    "Vec",
    "String",
    "Box",
    "Option",
    "Some",
    "None",
    "Result",
    "Ok",
    "Err",
    "ToString",
    "ToOwned",
    "Default",
    "Iterator",
    "IntoIterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "Extend",
    "FromIterator",
    "Clone",
    "Copy",
    "Send",
    "Sync",
    "Sized",
    "Unpin",
    "Drop",
    "Fn",
    "FnMut",
    "FnOnce",
    "From",
    "Into",
    "TryFrom",
    "TryInto",
    "AsRef",
    "AsMut",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Debug",
    "drop",
    "Seq",
    "Set",
    "Map",
    "int",
    "nat",
    "u8",
    "u16",
    "u32",
    "u64",
    "u128",
    "usize",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "isize",
    "f32",
    "f64",
    "bool",
    "char",
    "str",
    "size_of",
    "size_of_val",
    "align_of",
    "align_of_val",
    "Future",
    "IntoFuture",
];

/// 型の別名の定義。
#[derive(Debug, Clone)]
pub struct Alias {
    /// 定義したモジュール（右辺の名前はここで解決する）。
    pub module: ItemPath,
    /// 型の引数の名前（ライフタイムと定数は除く）。
    pub params: Vec<String>,
    pub ty: ast::Type,
}

/// `impl` の種類。トレイトのパスは索引で解決して決める（`ebykn_prelude::Invariant`、改名を含む）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplTrait {
    /// 固有の impl（`impl T { }`）。
    Inherent,
    /// `impl Invariant for T`。
    Invariant,
    /// `impl View for T`。
    View,
    /// std の `Drop`（`impl Drop for T`、`std::ops::Drop`・`core::ops::Drop` と、その `use` の別名）。
    /// 書き下しが `fn drop` に `opens_invariants none` と `no_unwind` を足す（#61）。
    Drop,
    /// それ以外のトレイト。
    Other,
    /// 最後の名前が `Invariant` / `View` で、`impl Invariant for T` の 1 つの書き方でないもの
    /// （パス付き、`::`、`<>`、改名、`mod` 経由）。クレートで定義したトレイトでなければ E0001。
    Misspelled,
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
            UsesItem::Named { ident, ty, .. } => uses.push(UseInfo::Named {
                name: ident.name.clone(),
                mode: region_mode(ty),
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

/// 型の引数の名前（ライフタイムと定数は除く）。
pub fn type_params(g: &ast::Generics) -> Vec<String> {
    g.params
        .iter()
        .filter_map(|g| match g {
            ast::GenericParam::Type { ident, .. } => Some(ident.name.clone()),
            _ => None,
        })
        .collect()
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

/// 領域の型の渡し方（`Region<..>` は移動、`&Region<..>` は共有、`&mut Region<..>` は可変）。
pub fn region_mode(ty: &ast::Type) -> PermMode {
    match &ty.kind {
        TypeKind::Ref { mutable: true, .. } => PermMode::Mut,
        TypeKind::Ref { .. } => PermMode::Shared,
        _ => PermMode::Move,
    }
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
        let mut rust_defs = Vec::new();
        // 名前空間（型とモジュール、値）ごとに、すでに見た名前。
        let mut seen: BTreeSet<(String, bool)> = BTreeSet::new();
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
                ItemKind::LemmaGroup(g) => (&g.ident, DefKind::Other),
                ItemKind::Use(t) => {
                    scope.imports.extend(parse_use(&t.tokens));
                    continue;
                }
                ItemKind::Rust(t) => {
                    for (n, kind) in rust_item_names(&t.tokens) {
                        let p = path_of(&n);
                        scope.items.insert(n, p.clone());
                        rust_defs.push((p, kind));
                    }
                    continue;
                }
                _ => continue,
            };
            let p = path_of(&name.name);
            let value_ns = matches!(kind, DefKind::Fn | DefKind::Other);
            if !seen.insert((name.name.clone(), value_ns)) {
                self.duplicates.push(item.span);
            }
            let inner_cfg = matches!(&item.kind, ItemKind::Mod(m) if m.content.as_ref().is_some_and(|(a, _)| has_cfg(a)));
            if has_cfg(&item.attrs) || inner_cfg {
                self.gated.insert(p.clone());
            }
            scope.items.insert(name.name.clone(), p.clone());
            defs.push((p, kind, item));
        }
        for (p, kind) in rust_defs {
            self.kinds.insert(p, kind);
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
                ItemKind::TypeAlias(t) => {
                    let params = type_params(&t.generics);
                    self.aliases.insert(
                        p,
                        Alias {
                            module: module.to_vec(),
                            params,
                            ty: t.ty.clone(),
                        },
                    );
                }
                ItemKind::Enum(e) => {
                    let variants = e
                        .variants
                        .iter()
                        .map(|v| {
                            let tys = match &v.fields {
                                ast::Fields::Named(fs) | ast::Fields::Tuple(fs) => {
                                    fs.iter().map(|f| f.ty.clone()).collect()
                                }
                                ast::Fields::Unit => Vec::new(),
                            };
                            (
                                v.ident.name.clone(),
                                tys,
                                matches!(v.fields, ast::Fields::Unit),
                            )
                        })
                        .collect();
                    let opaque =
                        has_repr(&item.attrs) || e.variants.iter().any(|v| has_cfg(&v.attrs));
                    self.enums.insert(
                        p,
                        EnumInfo {
                            module: module.to_vec(),
                            params: type_params(&e.generics),
                            variants,
                            opaque,
                        },
                    );
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
                    let runtime = match &s.fields {
                        ast::Fields::Named(fs) | ast::Fields::Tuple(fs) => {
                            fs.iter().map(|f| f.ty.clone()).collect()
                        }
                        ast::Fields::Unit => Vec::new(),
                    };
                    self.structs.insert(
                        p.clone(),
                        StructInfo {
                            path: p,
                            fields,
                            erased,
                            module: module.to_vec(),
                            runtime,
                            params: type_params(&s.generics),
                            unit: matches!(s.fields, ast::Fields::Unit),
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
            match self.impl_trait(module, imp) {
                ImplTrait::Invariant => {
                    self.invariants.insert(
                        ty,
                        InvariantInfo {
                            file,
                            span: item.span,
                        },
                    );
                }
                ImplTrait::View => {
                    self.views.insert(ty);
                }
                ImplTrait::Inherent => {
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
                        if has_cfg(&item.attrs) || has_cfg(&m.attrs) {
                            self.gated_methods.insert((ty.clone(), info.name.clone()));
                        }
                        let ms = self.methods.entry(ty.clone()).or_default();
                        if ms.contains_key(&info.name) {
                            self.duplicates.push(m.span);
                        }
                        ms.insert(info.name.clone(), info);
                    }
                }
                ImplTrait::Drop | ImplTrait::Other | ImplTrait::Misspelled => {}
            }
        }
    }

    /// `impl` のトレイトを決める。Ebykn のトレイトとして受理するのは、裸の名前・ジェネリクスなしの
    /// `impl Invariant for T` / `impl View for T` だけ（設計ノート §3.2「同じ概念を複数の形で
    /// 書けるようにしない」）。クレートで定義した同じ名前のトレイトは Rust のトレイト。
    pub fn impl_trait(&self, module: &[String], imp: &ast::Impl) -> ImplTrait {
        let Some(t) = &imp.trait_ else {
            return ImplTrait::Inherent;
        };
        let segs: Vec<String> = t.segments.iter().map(|s| s.ident.name.clone()).collect();
        let bare = !t.leading_colon
            && t.segments.len() == 1
            && matches!(t.segments[0].args, ast::GenericArgs::None);
        let last = segs.last().map(String::as_str).unwrap_or_default();
        let ebykn = crate::reserved_trait(last);
        match self.resolve(module, t.leading_colon, &segs, None) {
            Resolved::PreludeType("Invariant") if bare && last == "Invariant" => {
                ImplTrait::Invariant
            }
            Resolved::PreludeType("View") if bare && last == "View" => ImplTrait::View,
            Resolved::PreludeType("Invariant" | "View") => ImplTrait::Misspelled,
            // cfg で消えうるトレイトは、クレートの定義として数えない（消えれば prelude に解決する）。
            Resolved::Type(p) if !self.is_gated(&p) => ImplTrait::Other,
            _ if ebykn => ImplTrait::Misspelled,
            Resolved::Unknown if self.is_std_drop(module, t) => ImplTrait::Drop,
            _ => ImplTrait::Other,
        }
    }

    /// トレイトのパスが std の `Drop` か。`use` の別名・glob の `use`・`pub use` の再公開をたどって外のパスに
    /// なり、それが `Drop`（std の prelude）、`std::ops::Drop` / `core::ops::Drop`、
    /// `std::prelude::<版>::Drop` のどれかなら std の `Drop`。
    fn is_std_drop(&self, module: &[String], t: &ast::Path) -> bool {
        if !t
            .segments
            .iter()
            .all(|s| matches!(s.args, ast::GenericArgs::None))
        {
            return false;
        }
        let segs: Vec<String> = t.segments.iter().map(|s| s.ident.name.clone()).collect();
        // `::a::b` は外のクレートのパス。
        let found = if t.leading_colon {
            Ext::External(segs)
        } else {
            self.find_ext(module, &segs, true, 0)
        };
        let Ext::External(p) = found else {
            return false;
        };
        let p: Vec<&str> = p.iter().map(String::as_str).collect();
        match p.as_slice() {
            ["std" | "core", "ops", "Drop"] => true,
            [
                "std" | "core",
                "prelude",
                "v1" | "rust_2015" | "rust_2018" | "rust_2021" | "rust_2024",
                "Drop",
            ] => true,
            // `::Drop` は外のクレートの名前で、prelude の `Drop` ではない。
            ["Drop"] => !t.leading_colon,
            _ => false,
        }
    }

    /// パス `segs` をモジュール `m` から引く（std の `Drop` の判定に使う）。`use` の別名、glob の `use`
    /// （その先の `use` も）、`crate` / `self` / `super` をたどる。`root` は、パスの先頭（どこにもなければ
    /// 外のクレートか std の prelude の名前）か。モジュールの中をたどる途中なら、見つからないのは `NotFound`。
    fn find_ext(&self, m: &[String], segs: &[String], root: bool, depth: usize) -> Ext {
        if depth == 0 {
            self.ext_seen.borrow_mut().clear();
        }
        let Some(first) = segs.first() else {
            return Ext::Module(m.to_vec());
        };
        if depth > 32 {
            return Ext::Cut;
        }
        let rest = &segs[1..];
        let into = |r: Ext| match r {
            Ext::Module(p) if !rest.is_empty() => self.find_ext(&p, rest, false, depth + 1),
            Ext::External(mut p) => {
                p.extend(rest.iter().cloned());
                Ext::External(p)
            }
            r => r,
        };
        match first.as_str() {
            "crate" if root => return into(Ext::Module(Vec::new())),
            "self" if root => return into(Ext::Module(m.to_vec())),
            "super" => {
                let mut parent = m.to_vec();
                if parent.pop().is_none() {
                    return Ext::NotFound;
                }
                let chain = rest.first().is_some_and(|s| s == "super");
                return if rest.is_empty() {
                    Ext::Module(parent)
                } else {
                    self.find_ext(&parent, rest, chain, depth + 1)
                };
            }
            // `Self` と prelude（`ebykn_prelude`）は std の `Drop` ではない。
            "Self" | "ebykn_prelude" => return Ext::Crate,
            _ => {}
        }
        let Some(scope) = self.modules.get(m) else {
            return Ext::NotFound;
        };
        if let Some(p) = scope.items.get(first) {
            // 型の別名は右辺のパスをたどる（`type V = Vec<u64>;` の `V::new` は std の `Vec::new`）。
            if let Some(a) = self.aliases.get(p)
                && let TypeKind::Path { qself: None, path } = &a.ty.kind
                && !path.leading_colon
                && !path
                    .segments
                    .first()
                    .is_some_and(|f| a.params.contains(&f.ident.name))
            {
                let segs: Vec<String> =
                    path.segments.iter().map(|s| s.ident.name.clone()).collect();
                return into(self.find_ext(&a.module, &segs, true, depth + 1));
            }
            return into(match self.kinds.get(p) {
                Some(DefKind::Module) => Ext::Module(p.clone()),
                _ => Ext::Crate,
            });
        }
        for imp in &scope.imports {
            if let Import::Name { alias, path } = imp
                && alias == first
            {
                return into(self.find_ext(m, path, true, depth + 1));
            }
        }
        for imp in &scope.imports {
            let Import::Glob { path } = imp else {
                continue;
            };
            // 同じ glob のパスを引いている途中なら飛ばす（glob のパスの根を引くと、同じ glob にまた来る循環）。
            // 引き終えたら印を外す。
            let key = (m.to_vec(), format!("glob {}", path.join("::")), true);
            if !self.ext_seen.borrow_mut().insert(key.clone()) {
                continue;
            }
            let found = self.find_ext(m, path, true, depth + 1);
            self.ext_seen.borrow_mut().remove(&key);
            // glob のパス自身を引いた結果も、打ち切り（`Cut`）は捨てずにそのまま返す（分からない）。
            let g = match found {
                Ext::Module(g) => g,
                Ext::Cut => return Ext::Cut,
                _ => continue,
            };
            // 一つの問い合わせの中で、同じ glob の先を二度たどらない（互いに取り込む glob で指数的に
            // 増えないように）。
            if !self
                .ext_seen
                .borrow_mut()
                .insert((g.clone(), first.clone(), false))
            {
                continue;
            }
            match self.find_ext(&g, std::slice::from_ref(first), false, depth + 1) {
                Ext::NotFound => {}
                r => return into(r),
            }
        }
        if root && PRELUDE_TYPES.contains(&first.as_str()) {
            // prelude の型（`Ptr` など）は外のクレートでも std でもない。
            Ext::Crate
        } else if root {
            into(Ext::External(vec![first.clone()]))
        } else {
            Ext::NotFound
        }
    }

    /// 型のパスを解決する（`Self` は `self_ty`）。型の別名はたどる。クレートの型でなければ `None`。
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

    /// 型が型の別名なら、右辺の型（型の引数を代入したもの）と、右辺を解決するモジュール。
    pub fn expand_alias(
        &self,
        module: &[String],
        ty: &ast::Type,
        self_ty: Option<&ItemPath>,
    ) -> Option<(ItemPath, ast::Type)> {
        let TypeKind::Path { qself: None, path } = &ty.kind else {
            return None;
        };
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        let Resolved::Type(p) =
            self.resolve_with(module, path.leading_colon, &segs, self_ty, false, 0)
        else {
            return None;
        };
        let a = self.aliases.get(&p)?;
        let args: Vec<&ast::Type> = match path.segments.last().map(|s| &s.args) {
            Some(ast::GenericArgs::Angle { args, .. }) => args
                .iter()
                .filter_map(|g| match g {
                    ast::GenericArg::Type(t) => Some(t),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let map: BTreeMap<&str, &ast::Type> = a
            .params
            .iter()
            .map(String::as_str)
            .zip(args.iter().copied())
            .collect();
        Some((a.module.clone(), subst(&a.ty, &map)))
    }

    /// 型の別名をたどる（右辺がパスの型のとき）。
    fn follow(&self, r: Resolved, depth: usize) -> Resolved {
        let Resolved::Type(p) = &r else {
            return r;
        };
        let Some(a) = self.aliases.get(p) else {
            return r;
        };
        // 行き先を引けない別名は別名のまま返す（`use mm::X as Y;` の `Y` を `mm::X` と引けるように。
        // [`alias_unresolved`]）。
        if alias_unresolved(self, p, 0) {
            return r;
        }
        if depth > 16 {
            return Resolved::Unknown;
        }
        let TypeKind::Path { qself: None, path } = &a.ty.kind else {
            return Resolved::Unknown;
        };
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        self.resolve_with(&a.module, path.leading_colon, &segs, None, true, depth + 1)
    }

    /// 権限を作る・受け渡す関数の名前（prelude の `new` / `empty` と、クレートの `uses` / `gives` を持つ
    /// 関数とメソッドの名前）。索引ができたあとに一度だけ作る。
    pub fn perm_fn_names(&self) -> &BTreeSet<String> {
        self.perm_fn_names_cache.get_or_init(|| {
            let mut out: BTreeSet<String> =
                ["new", "empty"].iter().map(|s| s.to_string()).collect();
            for (p, f) in &self.fns {
                if f.has_perms()
                    && let Some(n) = p.last()
                {
                    out.insert(n.clone());
                }
            }
            for ms in self.methods.values() {
                for (n, f) in ms {
                    if f.has_perms() {
                        out.insert(n.clone());
                    }
                }
            }
            out
        })
    }

    /// クレートのどこかで、権限の型・権限を作る関数・`uses` / `gives` を持つ関数に行き着く名前と、行き先を
    /// 引けない型の別名の名前（読まないトークンの検査が数える。モジュールの直下の `type` と `use` の別名を、
    /// 鎖をたどって解決する）。トークンの中ではパスを読めないので、どのモジュールの名前かによらず数える。
    /// 索引ができたあとに一度だけ作る。
    pub fn perm_token_names(&self) -> &BTreeSet<String> {
        self.perm_token_names_cache.get_or_init(|| {
            let mut out = BTreeSet::new();
            for (m, scope) in &self.modules {
                let names = scope
                    .items
                    .keys()
                    .cloned()
                    .chain(scope.imports.iter().filter_map(|i| match i {
                        Import::Name { alias, .. } => Some(alias.clone()),
                        Import::Glob { .. } => None,
                    }));
                for n in names {
                    let r = self.resolve(m, false, std::slice::from_ref(&n), None);
                    // 行き先を引けない別名を取り込んだ名前（`use mm::X as Y;`）も数える。
                    let unresolved_alias =
                        matches!(&r, Resolved::Type(p) if alias_unresolved(self, p, 0));
                    if perm_resolved(self, &r) || unresolved_alias {
                        out.insert(n);
                    }
                }
            }
            for p in self.aliases.keys() {
                if alias_unresolved(self, p, 0)
                    && let Some(n) = p.last()
                {
                    out.insert(n.clone());
                }
            }
            out
        })
    }

    /// 根の名前が、外のクレートか std のもの（std / core / alloc、依存クレート、std の prelude の名前、
    /// 裸で書く vstd の名前、組み込みの型）か。
    pub fn is_external_root(&self, name: &str) -> bool {
        STD_ROOTS.contains(&name)
            || STD_PRELUDE_NAMES.contains(&name)
            || self.dependencies.contains(name)
    }

    /// モジュールに外の glob の `use`（`use std::collections::*;`）があるか。あれば、そのモジュールの知らない
    /// 根の名前は、そこから来た外の名前かもしれない。
    fn has_external_glob(&self, module: &[String]) -> bool {
        self.modules.get(module).is_some_and(|scope| {
            scope.imports.iter().any(|i| match i {
                Import::Glob { path } => self.is_external_path(module, path),
                Import::Name { .. } => false,
            })
        })
    }

    /// パスの名前を、クレートの中で定義しているかもしれないか（見つからないと言えないもの。打ち切りを含む）。
    pub fn may_define(&self, module: &[String], segs: &[String]) -> bool {
        !matches!(self.find_ext(module, segs, true, 0), Ext::NotFound)
    }

    /// パスの行き先が外（根が [`Index::is_external_root`] の名前）と分かるか（`use` の別名もたどる）。
    pub fn is_external_path(&self, module: &[String], segs: &[String]) -> bool {
        matches!(self.find_ext(module, segs, true, 0), Ext::External(p)
            if p.first().is_some_and(|r| self.is_external_root(r)))
    }

    /// 解決できないパスで、外（std と依存クレート）と言えないもの。権限の検査はその先の関数が権限を
    /// 受け渡すかを引けないので、呼び出しの頭・値・読まないトークンの中では E0003 にする（#75 の再確認。
    /// 綴りを一つずつ追わず、外と言える根の許可リストで決める）。
    /// - 外とみなすのは、根が [`Index::is_external_root`] の名前のときと、根がどこにも見つからない
    ///   一つの名前（ローカル変数、本体の中の項目）のときと、知らない根をそのモジュールの外の glob が
    ///   持ち込んだかもしれないときだけ。
    /// - 前半がクレートの型（型の別名でない）か prelude の型で、最後の名前を引けないもの（トレイトの
    ///   メソッド `S5::new`）は止めない。トレイトのメソッドは契約を持てないので権限を受け渡さない。
    #[allow(clippy::too_many_arguments)]
    pub fn unresolvable_path(
        &self,
        module: &[String],
        leading_colon: bool,
        segs: &[String],
        self_ty: Option<&ItemPath>,
        type_params: &[String],
        strict: bool,
        body_ext_glob: bool,
    ) -> bool {
        let Some(first) = segs.first() else {
            return false;
        };
        if self.resolve(module, leading_colon, segs, self_ty) != Resolved::Unknown {
            return false;
        }
        if segs.len() >= 2 {
            match self.resolve(module, leading_colon, &segs[..segs.len() - 1], self_ty) {
                Resolved::Type(t) if !self.aliases.contains_key(&t) => return false,
                Resolved::PreludeType(_) => return false,
                _ => {}
            }
        }
        if segs.len() == 1 && matches!(first.as_str(), "self" | "Self" | "super" | "crate") {
            return false;
        }
        if type_params.contains(first) {
            return false;
        }
        // 先頭の `::` は外のクレートの名前。
        if leading_colon {
            return !self.is_external_root(first);
        }
        // 通すのは、外と言える肯定の事実があるときだけ（根が許可リストの名前、行き先が外の glob）。深さで
        // 打ち切った（`Cut`）、クレートの中に見つかった、どこにも見つからない、はどれも分からないとして止める。
        // `strict` でない（読まないトークンの中）ときだけ、どこにも見つからない一つの名前を通す（ローカル変数
        // かもしれないが、トークンの中では区別できない）。
        match self.find_ext(module, segs, true, 0) {
            Ext::External(p) => {
                !(p.first().is_some_and(|r| self.is_external_root(r))
                    || (!strict && segs.len() == 1 && p.len() == 1)
                    // 根の名前そのものがモジュールのどこにもなく（`use` の別名でもなく）、外の glob（モジュールか
                    // 本体の中）があれば、そこから来た外の名前かもしれない。
                    || (p.first() == segs.first()
                        && (body_ext_glob || self.has_external_glob(module))))
            }
            _ => true,
        }
    }

    /// パスを解決する。型の別名はたどらない（`type P = Ptr<N>;` の `P` は別名のまま）。
    pub fn resolve_exact(
        &self,
        module: &[String],
        leading_colon: bool,
        segs: &[String],
        self_ty: Option<&ItemPath>,
    ) -> Resolved {
        self.resolve_with(module, leading_colon, segs, self_ty, false, 0)
    }

    /// パスを解決する。型の別名はたどる（`type P = Ptr<N>;` の `P::new` は prelude の `Ptr::new`）。
    pub fn resolve(
        &self,
        module: &[String],
        leading_colon: bool,
        segs: &[String],
        self_ty: Option<&ItemPath>,
    ) -> Resolved {
        self.resolve_with(module, leading_colon, segs, self_ty, true, 0)
    }

    fn resolve_with(
        &self,
        module: &[String],
        leading_colon: bool,
        segs: &[String],
        self_ty: Option<&ItemPath>,
        follow: bool,
        depth: usize,
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
            // 全 `.eyk` に暗黙に `use ebykn_prelude::*;` が付く（§3.1）。パスでも書ける。
            "ebykn_prelude" if segs.len() > 1 => {
                match PRELUDE_TYPES.iter().find(|t| **t == segs[1]) {
                    Some(t) => (Resolved::PreludeType(t), &segs[2..]),
                    None => return Resolved::Unknown,
                }
            }
            first => (self.lookup(module, first, 0), &segs[1..]),
        };
        self.descend(start, rest, follow, depth)
    }

    fn descend(&self, mut cur: Resolved, rest: &[String], follow: bool, depth: usize) -> Resolved {
        for (i, seg) in rest.iter().enumerate() {
            if follow {
                cur = self.follow(cur, depth);
            }
            cur = match cur {
                // 途中の `super`（`super::super::f`）は親のモジュール。
                Resolved::Module(mut m) if seg == "super" => {
                    if m.pop().is_none() {
                        return Resolved::Unknown;
                    }
                    Resolved::Module(m)
                }
                Resolved::Module(m) => self.lookup_in(&m, seg),
                Resolved::Type(t) if i + 1 == rest.len() => {
                    if self.methods.get(&t).is_some_and(|ms| ms.contains_key(seg)) {
                        Resolved::Method {
                            ty: t,
                            name: seg.clone(),
                        }
                    } else if self
                        .enums
                        .get(&t)
                        .is_some_and(|e| e.variants.iter().any(|(v, _, _)| v == seg))
                    {
                        let mut p = t;
                        p.push(seg.clone());
                        Resolved::Value(p)
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
        if follow {
            cur = self.follow(cur, depth);
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
            Some(DefKind::Other) => Resolved::Value(p.clone()),
            None => Resolved::Unknown,
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
                && let r @ (Resolved::Fn(_)
                | Resolved::Type(_)
                | Resolved::Module(_)
                | Resolved::Value(_)) = self.lookup_in(&m, name)
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
        self.descend(start, &path[1..], true, depth)
    }

    /// 項目か、それを囲むモジュールのどれかが `#[cfg(..)]` を持つか。
    pub fn is_gated(&self, p: &ItemPath) -> bool {
        (1..=p.len()).any(|n| self.gated.contains(&p[..n]))
    }

    /// 解決した結果が、cfg で消えないクレートの中の定義（関数・型・モジュール・メソッド）か。
    pub fn is_live_def(&self, r: &Resolved) -> bool {
        match r {
            Resolved::Fn(p) | Resolved::Type(p) | Resolved::Module(p) => !self.is_gated(p),
            Resolved::Method { ty, name } => {
                !self.is_gated(ty) && !self.gated_methods.contains(&(ty.clone(), name.clone()))
            }
            _ => false,
        }
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
                        ident_name(b)
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

/// トークンの識別子の名前。`r#` を外す（構文木の識別子と同じに比べる。`use crate::mk as r#k;` の `k`）。
pub fn ident_name(i: &proc_macro2::Ident) -> String {
    let s = i.to_string();
    s.strip_prefix("r#").map(str::to_string).unwrap_or(s)
}

/// 索引の項目にならない Rust の項目（トークンのまま持つ `union`、`extern` ブロックの `fn` / `static`）が
/// 定義する名前。索引が名前を取りこぼさないように、モジュールの名前に入れる（引けない型の上のメソッドを
/// 安全側の規則が見るため）。
fn rust_item_names(tokens: &TokenStream) -> Vec<(String, DefKind)> {
    let tts: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut out = Vec::new();
    for (i, t) in tts.iter().enumerate() {
        let TokenTree::Ident(id) = t else {
            continue;
        };
        match (ident_name(id).as_str(), tts.get(i + 1)) {
            ("union", Some(TokenTree::Ident(n))) => out.push((ident_name(n), DefKind::Type)),
            ("extern", _) => {
                for g in tts[i + 1..].iter() {
                    if let TokenTree::Group(g) = g
                        && g.delimiter() == Delimiter::Brace
                    {
                        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                        for (j, u) in inner.iter().enumerate() {
                            if let (TokenTree::Ident(k), Some(TokenTree::Ident(n))) =
                                (u, inner.get(j + 1))
                            {
                                match ident_name(k).as_str() {
                                    "fn" => out.push((ident_name(n), DefKind::Fn)),
                                    "static" => out.push((ident_name(n), DefKind::Other)),
                                    _ => {}
                                }
                            }
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn is_path_sep(tts: &[TokenTree], i: usize) -> bool {
    matches!(
        (tts.get(i), tts.get(i + 1)),
        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b))) if a.as_char() == ':' && b.as_char() == ':'
    )
}

/// 型の中の型の引数（一つの名前のパス）を代入する。
fn subst(ty: &ast::Type, map: &BTreeMap<&str, &ast::Type>) -> ast::Type {
    if map.is_empty() {
        return ty.clone();
    }
    let mut t = ty.clone();
    match &mut t.kind {
        TypeKind::Path { qself: None, path } => {
            if let Some(i) = path.as_ident()
                && let Some(r) = map.get(i.name.as_str())
            {
                return (*r).clone();
            }
            for seg in &mut path.segments {
                if let ast::GenericArgs::Angle { args, .. } = &mut seg.args {
                    for a in args {
                        if let ast::GenericArg::Type(x) = a {
                            *x = subst(x, map);
                        }
                    }
                }
            }
        }
        TypeKind::Ref { elem, .. }
        | TypeKind::Array { elem, .. }
        | TypeKind::Slice(elem)
        | TypeKind::Paren(elem) => **elem = subst(elem, map),
        TypeKind::Tuple(ts) => {
            for x in ts {
                *x = subst(x, map);
            }
        }
        _ => {}
    }
    t
}

/// `#[repr(..)]` か、`cfg_attr` の中の `repr`（属性の最後の名前で見る。安全側）。
pub fn has_repr(attrs: &[ast::Attr]) -> bool {
    fn mentions_repr(ts: &TokenStream) -> bool {
        ts.clone().into_iter().any(|t| match t {
            TokenTree::Ident(i) => ebykn_syntax::macros::token_is_name(&i, "repr"),
            TokenTree::Group(g) => mentions_repr(&g.stream()),
            _ => false,
        })
    }
    attrs.iter().any(|a| match a.path.segments.last() {
        Some(seg) if seg.ident.is_name("repr") => true,
        Some(seg) if seg.ident.is_name("cfg_attr") => mentions_repr(&a.args),
        _ => false,
    })
}

/// 属性に `#[cfg(..)]` があるか（[`ast::has_cfg`]）。
pub fn has_cfg(attrs: &[ast::Attr]) -> bool {
    ast::has_cfg(attrs)
}
