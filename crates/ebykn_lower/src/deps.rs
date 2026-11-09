//! 項目の依存（仕様 §9.1 の 2 の呼び出しグラフ）。`verify` が、本体のない補題（`admit`）に届く項目を
//! `verified` にしないために使う（#149 のレビューの B3）。
//!
//! 書き下しの途中で、いま書き下している項目（[`crate::Cx::current`]）から、次への辺を記録する。
//!
//! - 解決できた呼び出し（`f(..)`、`Type::m(..)`。証明の中と契約の中を含む）：その項目。
//! - 解決できない呼び出しとメソッドの呼び出し（`x.m(..)`）：最後の名前が同じすべての項目（受け手の
//!   型を決めないので、多めに数える）。
//! - `use lemmas g;`：`g` の要素の補題。モジュールの直下に書けば、そのモジュール（とその下）のすべての
//!   項目から。`g` を決められなければ、すべての `admit` に届くとみなす。
//!
//! 辺を多めに数えると `unverified` が増えるだけで、`verified` を誤って出すことはない。

use std::collections::{BTreeMap, BTreeSet};

use ebykn_check::index::Resolved;
use ebykn_syntax::ast::{self, Expr, ExprKind};

use crate::Cx;

/// 依存先。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dep {
    /// 項目（`k::m::f`。`verify` の項目の名前と同じ形）。
    Item(String),
    /// 最後の名前がこれのすべての項目。
    Name(String),
    /// すべての `admit`。
    AnyAdmit,
}

/// `lemma group` の要素：（グループを置いたモジュール、グループの名前）→ 要素の補題の名前（`k::m::l`）。
/// 要素を決められなければ `None`。
pub type Groups = BTreeMap<(Vec<String>, String), Vec<Option<String>>>;

impl Cx<'_> {
    fn display(&self, path: &[String]) -> String {
        std::iter::once(self.crate_name.to_string())
            .chain(path.iter().cloned())
            .collect::<Vec<_>>()
            .join("::")
    }

    fn note(&self, d: Dep) {
        if let Some(cur) = &self.current {
            self.report
                .borrow_mut()
                .deps
                .entry(cur.clone())
                .or_default()
                .insert(d);
        }
    }

    /// 呼び出し `func(..)` の辺。
    pub(crate) fn note_call(&self, func: &Expr) {
        let ExprKind::Path { qself, path } = &func.kind else {
            return;
        };
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.name.clone()).collect();
        let Some(last) = segs.last().cloned() else {
            return;
        };
        if qself.is_none() {
            match self.index.resolve(
                &self.module,
                path.leading_colon,
                &segs,
                self.self_ty.as_ref(),
            ) {
                Resolved::Fn(p) => return self.note(Dep::Item(self.display(&p))),
                Resolved::Method { ty, name } => {
                    let mut p = ty.clone();
                    p.push(name);
                    return self.note(Dep::Item(self.display(&p)));
                }
                // prelude の関数（`Ptr::new` など）はクレートの項目でない。
                Resolved::Prelude(_) | Resolved::PreludeType(_) => return,
                _ => {}
            }
        }
        self.note(Dep::Name(last));
    }

    /// メソッドの呼び出し `x.m(..)` の辺。
    pub(crate) fn note_method(&self, method: &ast::Ident) {
        self.note(Dep::Name(method.name.clone()));
    }

    /// `use lemmas g;` の辺。`module_level` ならモジュールのすべての項目から。
    pub(crate) fn note_use_lemmas(&self, p: &ast::Path, module_level: bool) {
        let deps: Vec<Dep> = match self.group_members(p) {
            Some(ms) => ms
                .iter()
                .map(|m| m.clone().map(Dep::Item).unwrap_or(Dep::AnyAdmit))
                .collect(),
            None => vec![Dep::AnyAdmit],
        };
        if module_level {
            let prefix = self.display(&self.module);
            let mut r = self.report.borrow_mut();
            for d in deps {
                r.module_deps.push((prefix.clone(), d));
            }
        } else {
            for d in deps {
                self.note(d);
            }
        }
    }

    /// `use lemmas` のパスのグループの要素。
    fn group_members(&self, p: &ast::Path) -> Option<&Vec<Option<String>>> {
        if p.leading_colon {
            return None;
        }
        let segs: Vec<String> = p.segments.iter().map(|s| s.ident.name.clone()).collect();
        let (name, dirs) = segs.split_last()?;
        let mut module = self.module.clone();
        for (i, s) in dirs.iter().enumerate() {
            match s.as_str() {
                "crate" if i == 0 => module.clear(),
                "self" if i == 0 => {}
                "super" => {
                    module.pop()?;
                }
                other => module.push(other.to_string()),
            }
        }
        self.groups.get(&(module, name.clone()))
    }
}

/// 項目 `item` から辺をたどって届く `admit` の項目（一つ）。`items` はすべての項目の名前。
pub fn reaches_admit(
    item: &str,
    deps: &BTreeMap<String, BTreeSet<Dep>>,
    module_deps: &[(String, Dep)],
    items: &[String],
    admitted: &[String],
) -> Option<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![item.to_string()];
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if cur != item && admitted.contains(&cur) {
            return Some(cur);
        }
        let in_module = module_deps
            .iter()
            .filter(|(m, _)| cur == *m || cur.starts_with(&format!("{m}::")))
            .map(|(_, d)| d);
        for d in deps.get(&cur).into_iter().flatten().chain(in_module) {
            match d {
                Dep::Item(n) => stack.push(n.clone()),
                // 状態の一覧の項目に加えて、辺の始点（入れ子の項目など、一覧にない項目）も探す。
                Dep::Name(n) => stack.extend(
                    items
                        .iter()
                        .chain(deps.keys())
                        .chain(admitted.iter())
                        .filter(|i| i.rsplit("::").next() == Some(n.as_str()))
                        .cloned(),
                ),
                Dep::AnyAdmit => {
                    if let Some(a) = admitted.iter().find(|a| *a != item) {
                        return Some(a.clone());
                    }
                }
            }
        }
    }
    None
}
