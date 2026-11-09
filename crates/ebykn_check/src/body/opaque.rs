//! 検査が中を読まないトークン（std のマクロの引数と、`.await` などの Rust のトークンのまま持つ式）の
//! 検査。§6.2 の検査も書き下しもトークンの中を見ないので、権限に関わる名前か脱出が一つでもあれば
//! E0003 にする（#21、#74）。読まないトークンは [`Walker::opaque_tokens`] を通して検査する。

use ebykn_syntax::ast;
use ebykn_syntax::diag::{CheckMsg, Message, Unimpl};
use ebykn_syntax::span::Span;

use super::Walker;
use std::collections::BTreeSet;

use crate::index::{PERM_TYPES, Resolved, alias_unresolved, perm_resolved};

/// 検査が中を読まないトークンの出どころ（[`Walker::opaque_tokens`]）。
#[derive(Clone, Copy)]
pub(super) enum Opaque {
    /// std のマクロの引数。
    MacroArgs,
    /// Rust のトークンのまま持つ式（`.await`、async ブロックなど）。
    RustExpr,
}

impl Walker<'_> {
    /// 検査が中を読まないトークン（std のマクロの引数と、`.await` などの Rust のトークンのまま持つ式）の
    /// 中の、権限の操作と脱出（`return` / `break` / `continue` / `?`）を探し、見つけたら E0003。
    /// §6.2 の検査も書き下しも中を見ないので、どちらも見逃すと黙って通る（#21、#74）。読まない
    /// トークンはここを通して検査する。
    pub(super) fn opaque_tokens(
        &mut self,
        span: Span,
        tokens: &proc_macro2::TokenStream,
        kind: Opaque,
    ) {
        let names = self.cx.index.perm_token_names();
        if self.perm_in_tokens(tokens.clone(), names) {
            let u = match kind {
                Opaque::MacroArgs => Unimpl::PermInMacro,
                Opaque::RustExpr => Unimpl::PermInRustExpr,
            };
            self.diags
                .push(span, Message::Check(CheckMsg::Unimplemented(u)));
        }
        if jump_in_tokens(tokens.clone()) {
            let u = match kind {
                Opaque::MacroArgs => Unimpl::JumpInMacro,
                Opaque::RustExpr => Unimpl::JumpInRustExpr,
            };
            self.diags
                .push(span, Message::Check(CheckMsg::Unimplemented(u)));
        }
    }

    /// トークンの中の権限の操作を探す。綴りを列挙して追わず、権限に関わる名前が一つでも現れたら
    /// 見つけたとする（読めない形は大きく失敗させる。#74 のレビュー）。見る名前は、prelude の権限の型
    /// （`Ptr`、`Cell`、`Region`、`PointsTo`、`CellPointsTo`）、`uses` / `gives` を持つ関数・メソッドの名前
    /// （`use` の別名を解決したものを含む）と、権限の型に行き着く別名、Γ・R・一度でも権限を持った名前、
    /// `self@`、領域添字が続く `take` / `put` / `borrow`、`leak X@`、局所の `use` / `type`、`uses`、`<T as Tr>::`。`(` が続くかどうかは見ない。`.` の後の
    /// 名前（フィールドとメソッド）は、権限を持つメソッドの名前のときだけ数える。
    fn perm_in_tokens(&self, ts: proc_macro2::TokenStream, names: &BTreeSet<String>) -> bool {
        use proc_macro2::{Delimiter, TokenTree};
        let tts: Vec<TokenTree> = ts.into_iter().collect();
        let id = |t: Option<&TokenTree>| match t {
            // `r#Ptr` も `Ptr` と同じ名前（トークンの外では構文解析が `r#` を外す）。
            Some(TokenTree::Ident(i)) => {
                let s = i.to_string();
                Some(s.strip_prefix("r#").map(str::to_string).unwrap_or(s))
            }
            _ => None,
        };
        let pc = |t: Option<&TokenTree>, c: char| matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == c);
        let grp = |t: Option<&TokenTree>, d: Delimiter| matches!(t, Some(TokenTree::Group(g)) if g.delimiter() == d);
        let held = |n: &str| {
            self.env.contains_key(n)
                || self.regions.contains(n)
                || self.known.contains(n)
                || self
                    .closure_outer
                    .as_ref()
                    .is_some_and(|(e, r)| e.contains_key(n) || r.contains(n))
        };
        // 領域添字（`x[..]`、`self@x[..]`）が `tts[j]` から始まるか。
        let region_index = |j: usize| {
            (id(tts.get(j)).is_some() && grp(tts.get(j + 1), Delimiter::Bracket))
                || (id(tts.get(j)).as_deref() == Some("self") && pc(tts.get(j + 1), '@'))
        };
        for (i, tt) in tts.iter().enumerate() {
            // 属性（`#[..]`、`#![..]`）の中のパスは、式のパスではない（属性そのものは別の検査が見る）。
            let attr = matches!(tt, TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket)
                && i >= 1
                && (pc(tts.get(i - 1), '#')
                    || (pc(tts.get(i - 1), '!') && i >= 2 && pc(tts.get(i - 2), '#')));
            if let TokenTree::Group(g) = tt
                && !attr
                && self.perm_in_tokens(g.stream(), names)
            {
                return true;
            }
            let Some(n) = id(Some(tt)) else {
                continue;
            };
            let prev = i.checked_sub(1).and_then(|j| tts.get(j));
            let next = tts.get(i + 1);
            if pc(prev, '.') {
                // フィールドかメソッド。`X.free()` は `X` の側（下の Γ の名前）で数える。
                if self.cx.index.perm_method_names.contains(&n) {
                    return true;
                }
                continue;
            }
            let found = PERM_TYPES.contains(&n.as_str())
                || names.contains(&n)
                || self.perm_fn_name(&n)
                || held(&n)
                || match n.as_str() {
                    // 局所の別名と、呼び出しで領域を渡す `f(x, uses &mut r)`（#19。`uses` は予約語）。
                    "use" | "type" | "uses" => true,
                    "self" => pc(next, '@'),
                    // 型に相対的なパス `Self::A::f`、`T::A::f`（関連型を通る。トークンの外の
                    // `type_relative` と同じ）。
                    _ if (n == "Self" || self.cx.type_params.contains(&n))
                        && pc(tts.get(i + 1), ':')
                        && pc(tts.get(i + 2), ':')
                        && id(tts.get(i + 3)).is_some()
                        && pc(tts.get(i + 4), ':')
                        && pc(tts.get(i + 5), ':') =>
                    {
                        true
                    }
                    "take" | "put" => region_index(i + 1),
                    "borrow" => {
                        region_index(i + 1)
                            || (id(next).as_deref() == Some("mut") && region_index(i + 2))
                    }
                    "leak" => id(next).is_some() && pc(tts.get(i + 2), '@'),
                    // トレイトを通した修飾パス `<T as Tr>::..`。関連型などを通ると権限の型に行き着くかを
                    // 引けないので、安全側に倒す（トークンの外の `perm_fn_path` と同じ）。
                    "as" => tts[i + 1..]
                        .iter()
                        .position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '>'))
                        .is_some_and(|k| {
                            pc(tts.get(i + 2 + k), ':') && pc(tts.get(i + 3 + k), ':')
                        }),
                    _ => false,
                };
            if found {
                return true;
            }
            // パスの先頭から読み、解決できず外とも言えないパスなら、権限に関わるとみなす（トークンの外の
            // `unresolvable_path` と同じ）。先頭の `::` は、その前が名前でも `>` でもないとき。
            let leading_colon = pc(prev, ':')
                && i >= 2
                && pc(tts.get(i - 2), ':')
                && !(i >= 3 && (id(tts.get(i - 3)).is_some() || pc(tts.get(i - 3), '>')));
            if !pc(prev, ':') || leading_colon {
                let mut segs = vec![n.clone()];
                let mut j = i + 1;
                while pc(tts.get(j), ':') && pc(tts.get(j + 1), ':') {
                    j += 2;
                    if let Some(s) = id(tts.get(j)) {
                        segs.push(s);
                        j += 1;
                    } else if pc(tts.get(j), '<') {
                        // ターボフィッシュ `::<..>` を飛ばす。
                        let mut depth = 0usize;
                        while let Some(t) = tts.get(j) {
                            if pc(Some(t), '<') {
                                depth += 1;
                            } else if pc(Some(t), '>') {
                                depth = depth.saturating_sub(1);
                                if depth == 0 {
                                    j += 1;
                                    break;
                                }
                            }
                            j += 1;
                        }
                    } else {
                        break;
                    }
                }
                if self.unresolvable_segs(leading_colon, &segs, false) {
                    return true;
                }
            }
        }
        false
    }

    /// `n` が `uses` / `gives` を持つ関数の名前か、名前を解決するとそうなる（`use` の別名、`Ptr::new` など）。
    fn perm_fn_name(&self, n: &str) -> bool {
        // `new` / `empty` は prelude の関数の名前で、それだけでは権限の操作にならない（`Vec::new()`）。
        if !matches!(n, "new" | "empty") && self.cx.index.perm_fn_names().contains(n) {
            return true;
        }
        let r = self
            .cx
            .index
            .resolve(self.cx.module, false, &[n.to_string()], self.cx.self_ty);
        perm_resolved(self.cx.index, &r)
    }

    /// 権限を持つ関数のパス（`uses` / `gives` を持つ関数、`Ptr::new` などの権限を作る関数）か。
    /// 修飾パス（`<Ptr<Node>>::new`）は型のパスの後ろに続けて引き、引けなければ（`<T as Tr>::f`、関連型を
    /// 通したもの、クレートの外のもの）権限を持つとみなす（安全側）。
    pub(super) fn perm_fn_path(&self, qself: Option<&ast::QSelf>, path: &ast::Path) -> bool {
        if qself.is_none() && self.type_relative(path) {
            return true;
        }
        let mut segs: Vec<String> = Vec::new();
        if let Some(q) = qself {
            // `<T as Tr>::f`：トレイトの後ろが一つの名前なら、トレイトのメソッド（契約を持てないので権限を
            // 受け渡さない。`S5::new()` やトレイトの `Self::f` と同じ）。2 つ以上なら関連型を通るので止める。
            if q.position > 0 && path.segments.len() == q.position + 1 {
                return false;
            }
            match &q.ty.kind {
                ast::TypeKind::Path {
                    qself: None,
                    path: tp,
                } if q.position == 0 => {
                    segs.extend(tp.segments.iter().map(|s| s.ident.name.clone()));
                }
                _ => return true,
            }
        }
        segs.extend(path.segments.iter().map(|s| s.ident.name.clone()));
        let r = self.cx.index.resolve(
            self.cx.module,
            path.leading_colon && qself.is_none(),
            &segs,
            self.cx.self_ty,
        );
        if qself.is_some() && matches!(r, Resolved::Unknown) {
            return true;
        }
        perm_resolved(self.cx.index, &r)
    }
}

impl Walker<'_> {
    /// 解決できないパスで、外と言えないもの（[`crate::index::Index::unresolvable_path`]）。
    pub(super) fn unresolvable_path(&self, path: &ast::Path) -> bool {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        self.unresolvable_segs(path.leading_colon, &segs, true)
    }

    /// `strict` は呼び出しの頭と値の位置（通すのは肯定の事実があるときだけ）。読まないトークンの中では
    /// `false`（ローカル変数を区別できないので、どこにも見つからない一つの名前を通す）。
    fn unresolvable_segs(&self, leading_colon: bool, segs: &[String], strict: bool) -> bool {
        if !leading_colon && let Some(first) = segs.first() {
            // ローカル変数（引数・`let`・パターン・クロージャの引数・量化子の束縛）の一つの名前と、契約の
            // 組み込みの `old` / `final`。
            if segs.len() == 1
                && (self.locals.contains(first)
                    || self.regions.contains(first)
                    || self.env.contains_key(first)
                    || matches!(first.as_str(), "old" | "final"))
            {
                return false;
            }
            // 本体の中の `use` 以外の項目（`fn`・型など）を根に持つパス。権限に行き着く本体の中の項目は、
            // 本体の中の項目の検査が拒む。
            if self.local.items.contains(first) {
                return false;
            }
            // 本体の中の `use` の別名と、本体の中のクレートの glob から来る名前は、行き先のパスに置き換えて
            // 同じ検査をかける（別名を根拠に通さない）。
            if let Some(sub) = self.local_substitute(segs) {
                return self.blocked_path(&sub, strict);
            }
        }
        self.cx.index.unresolvable_path(
            self.cx.module,
            leading_colon,
            segs,
            self.cx.self_ty,
            &self.cx.type_params,
            strict,
            self.local.ext_glob,
        )
    }

    /// 本体の中の `use` の別名（`mm` → `crate::m`）か、本体の中のクレートの glob（`use crate::m::*;`）から
    /// 来る根の名前を、行き先のパスに置き換える。
    fn local_substitute(&self, segs: &[String]) -> Option<Vec<String>> {
        let first = segs.first()?;
        if let Some(p) = self.local.aliases.get(first) {
            let mut sub = p.clone();
            sub.extend(segs[1..].iter().cloned());
            return Some(sub);
        }
        self.local.globs.iter().find_map(|g| {
            let mut head = g.clone();
            head.push(first.clone());
            // 解決できなくても、glob の行き先に定義があるかもしれない名前は、glob から来たとみなす（外の glob
            // の例外に落とさない）。
            (self
                .cx
                .index
                .resolve_exact(self.cx.module, false, &head, self.cx.self_ty)
                != Resolved::Unknown
                || self.cx.index.may_define(self.cx.module, &head))
            .then(|| {
                let mut sub = g.clone();
                sub.extend(segs.iter().cloned());
                sub
            })
        })
    }

    /// 置き換えた先のパスを止めるか：関連型などを通る、権限を受け渡す関数を指す（本体の中の別名を通した
    /// 呼び出しは §6.2 の検査が引けない）、解決できず外とも言えない、のどれか。
    fn blocked_path(&self, sub: &[String], strict: bool) -> bool {
        self.type_relative_segs(false, sub)
            || perm_resolved(
                self.cx.index,
                &self
                    .cx
                    .index
                    .resolve(self.cx.module, false, sub, self.cx.self_ty),
            )
            || self.cx.index.unresolvable_path(
                self.cx.module,
                false,
                sub,
                self.cx.self_ty,
                &self.cx.type_params,
                strict,
                false,
            )
    }

    /// 本体の中の項目の名前を集める（ブロックに入るときに呼ぶ。項目はブロックのどこからでも見える）。
    pub(super) fn add_local_item(&mut self, item: &ast::Item) {
        use ast::ItemKind as K;
        let name = match &item.kind {
            K::Fn(f) => Some(&f.sig.ident),
            K::SpecFn(f) => Some(&f.sig.ident),
            K::LemmaFn(f) => Some(&f.sig.ident),
            K::Struct(x) => Some(&x.ident),
            K::Enum(x) => Some(&x.ident),
            K::Trait(x) => Some(&x.ident),
            K::TypeAlias(x) => Some(&x.ident),
            K::Subset(x) => Some(&x.ident),
            K::Mod(x) => Some(&x.ident),
            K::Const(x) => Some(&x.ident),
            K::Static(x) => Some(&x.ident),
            K::Use(t) => {
                for u in crate::index::parse_use(&t.tokens) {
                    match u {
                        crate::index::Import::Name { alias, path } => {
                            self.local.aliases.insert(alias, path);
                        }
                        // 行き先が外と分かる glob だけ（`use std::collections::*;`）。クレートの中の glob
                        // （`use crate::Color::*;`）は、そこから来る名前を索引で引ける。
                        crate::index::Import::Glob { path } => {
                            if self.cx.index.is_external_path(self.cx.module, &path) {
                                self.local.ext_glob = true;
                            } else {
                                self.local.globs.push(path);
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(n) = name {
            self.local.items.insert(n.name.clone());
        }
    }

    /// 関連型・型の引数・行き先を引けない型の別名を通るパスか（`Self::A::f`、`T::A::f`、
    /// `type X = <Node as Tr>::Out;` の `X::new`、`type PX<T> = T;` の `PX::<Ptr<N>>::new`）。索引は関連型を
    /// 解決しないので、そこから先の関数が権限を受け渡すかを引けない（#75 の再確認）。引けないパスを全部
    /// 止めると `Vec::new` まで止まるので、この 3 つを通るものに限る。
    pub(super) fn type_relative(&self, path: &ast::Path) -> bool {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        self.type_relative_segs(path.leading_colon, &segs)
    }

    fn type_relative_segs(&self, leading_colon: bool, segs: &[String]) -> bool {
        if segs.len() >= 3 && (segs[0] == "Self" || self.cx.type_params.contains(&segs[0])) {
            return true;
        }
        (1..segs.len()).any(|k| {
            match self.cx.index.resolve_exact(
                self.cx.module,
                leading_colon,
                &segs[..k],
                self.cx.self_ty,
            ) {
                Resolved::Type(p) => alias_unresolved(self.cx.index, &p, 0),
                _ => false,
            }
        })
    }
}

/// 中を読まないマクロの引数の中の `return` / `break` / `continue` / `?`（グループの中も見る）。クロージャの
/// 中かどうかは読み分けない（#21 の既定 D。読めない形は大きく失敗させる）。
fn jump_in_tokens(ts: proc_macro2::TokenStream) -> bool {
    use proc_macro2::TokenTree;
    ts.into_iter().any(|t| match t {
        TokenTree::Ident(i) => matches!(i.to_string().as_str(), "return" | "break" | "continue"),
        TokenTree::Punct(p) => p.as_char() == '?',
        TokenTree::Group(g) => jump_in_tokens(g.stream()),
        TokenTree::Literal(_) => false,
    })
}
