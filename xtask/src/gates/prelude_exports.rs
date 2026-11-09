//! `check-prelude-exports`：`ebykn_prelude` が vstd を glob で再公開していない（#16 の既定、`OPEN.md`）。
//!
//! `.eyk` には暗黙に `use ebykn_prelude::*;` が付く（仕様 §3.1）。prelude が `pub use vstd::prelude::*;`
//! などをすると、`.eyk` から `admit` や `requires` などの Verus の組み込みに届く。そこで、
//! `crates/ebykn_prelude/src/` の `pub use`（`pub(crate)` などを含む）が vstd・verus_builtin の項目を
//! 指すときは、`ALLOWED` に挙げた名前だけを許す。glob、改名、モジュールそのもの、`pub extern crate` は
//! 失敗にする。読めない `pub use`（`;` がない、木の形が読めない）も失敗にする。

use super::{Repo, Report};

const DIR: &str = "crates/ebykn_prelude/src/";

/// vstd から再公開してよい名前。
pub const ALLOWED: &[&str] = &["vstd::view::View", "vstd::cell::CellId"];

/// 外のクレートのうち、再公開を調べるもの。
const CRATES: &[&str] = &[
    "vstd",
    "verus_builtin",
    "verus_builtin_macros",
    "builtin",
    "verus_state_machines_macros",
];

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    let files: Vec<&String> = repo
        .files
        .iter()
        .filter(|f| f.starts_with(DIR) && f.ends_with(".rs"))
        .collect();
    if files.is_empty() {
        r.errors.push(format!("{DIR} に .rs がありません"));
        return r;
    }
    let mut checked = 0;
    for f in files {
        let text = match repo.read(f) {
            Ok(t) => t,
            Err(e) => {
                r.errors.push(e);
                continue;
            }
        };
        match exports(&text) {
            Err(e) => r.errors.push(format!("{f}: {e}")),
            Ok(uses) => {
                // `use vstd::prelude as vp;` のような別名（公開しないものを含む）を先にたどる。
                let aliases: Vec<(String, String)> = uses
                    .iter()
                    .filter(|u| !u.glob)
                    .map(|u| (u.name(), u.path.clone()))
                    .collect();
                for u in uses.iter().filter(|u| u.public) {
                    checked += 1;
                    let path = resolve(&u.path, &aliases);
                    let root = path
                        .trim_start_matches("::")
                        .split("::")
                        .next()
                        .unwrap_or("");
                    // 公開する glob は、どこを指しても失敗にする（prelude は名前を挙げて再公開する）。
                    if u.glob {
                        r.errors.push(format!(
                            "{f}: `{path}::*` を glob で再公開しています。prelude は名前を挙げて再公開します（.eyk から vstd に届くため）"
                        ));
                        continue;
                    }
                    if !CRATES.contains(&root) {
                        continue;
                    }
                    let renamed = u.alias.is_some();
                    let glob = false;
                    if glob || renamed || !ALLOWED.contains(&path.trim_start_matches("::")) {
                        r.errors.push(format!(
                            "{f}: `{path}{}` を再公開しています。.eyk から vstd に届くので、許すのは {} だけです（xtask/src/gates/prelude_exports.rs の ALLOWED）",
                            if glob { "::*" } else { "" },
                            ALLOWED.join(", ")
                        ));
                    }
                }
            }
        }
    }
    r.notes.push(format!("pub use の葉 {checked} 個を検査"));
    r
}

/// コメントを除く（`//` から行末、`/* */`）。文字列の中は考えない（prelude の `use` には現れない）。
fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("//") {
            rest = r.find('\n').map_or("", |i| &r[i..]);
        } else if let Some(r) = rest.strip_prefix("/*") {
            rest = r.find("*/").map_or("", |i| &r[i + 2..]);
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// `use` の葉。
struct Use {
    path: String,
    glob: bool,
    alias: Option<String>,
    public: bool,
}

impl Use {
    /// この `use` が入れる名前。
    fn name(&self) -> String {
        match &self.alias {
            Some(a) => a.clone(),
            None => self
                .path
                .rsplit("::")
                .next()
                .unwrap_or_default()
                .to_string(),
        }
    }
}

/// パスの先頭を、別名をたどって元のパスにする（たどれる限り、最大 8 回）。
fn resolve(path: &str, aliases: &[(String, String)]) -> String {
    let mut p = path.to_string();
    for _ in 0..8 {
        let (head, rest) = match p.split_once("::") {
            Some((h, r)) => (h.to_string(), Some(r.to_string())),
            None => (p.clone(), None),
        };
        let Some((_, to)) = aliases.iter().find(|(a, to)| *a == head && *to != p) else {
            break;
        };
        p = match rest {
            Some(r) => format!("{to}::{r}"),
            None => to.clone(),
        };
    }
    p
}

/// すべての `use`（公開しないものを含む）の葉と `extern crate` を集める。
fn exports(text: &str) -> Result<Vec<Use>, String> {
    let s = strip_comments(text);
    let words: Vec<&str> = tokenize(&s);
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let (public, mut j) = if words[i] == "pub" {
            let mut j = i + 1;
            if words.get(j) == Some(&"(") {
                while j < words.len() && words[j] != ")" {
                    j += 1;
                }
                j += 1;
            }
            (true, j)
        } else {
            (false, i)
        };
        // `use` は文の先頭（前が `;` `}` `{` `]` か、ファイルの先頭か、`pub ..` の後）だけを見る。
        let at_start = public
            || i == 0
            || matches!(words.get(i.wrapping_sub(1)), Some(&(";" | "}" | "{" | "]")));
        match words.get(j) {
            Some(&"use") if at_start => {
                let end = words[j..]
                    .iter()
                    .position(|w| *w == ";")
                    .ok_or("`use` を読めません（`;` で終わっていません）")?;
                let tree = &words[j + 1..j + end];
                let mut leaves_out = Vec::new();
                leaves(tree, String::new(), &mut leaves_out)?;
                for (path, glob, alias) in leaves_out {
                    out.push(Use {
                        path,
                        glob,
                        alias,
                        public,
                    });
                }
                i = j + end;
            }
            Some(&"extern") if words.get(j + 1) == Some(&"crate") => {
                let name = words.get(j + 2).copied().unwrap_or_default();
                let alias = (words.get(j + 3) == Some(&"as"))
                    .then(|| words.get(j + 4).map(|a| a.to_string()))
                    .flatten();
                out.push(Use {
                    path: name.to_string(),
                    glob: false,
                    alias: alias.or_else(|| public.then(|| name.to_string())),
                    public,
                });
                j += 3;
                i = j;
            }
            _ => i = j.max(i + 1),
        }
    }
    Ok(out)
}

fn tokenize(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c == ':' && b.get(i + 1) == Some(&b':') {
            out.push("::");
            i += 2;
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < b.len() && ((b[i] as char).is_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(&s[start..i]);
        } else {
            out.push(&s[i..i + c.len_utf8()]);
            i += c.len_utf8();
        }
    }
    out
}

/// `use` の木の葉を集める。読めない形は失敗にする。
fn leaves(
    t: &[&str],
    prefix: String,
    out: &mut Vec<(String, bool, Option<String>)>,
) -> Result<(), String> {
    let join = |p: &str, x: &str| {
        if p.is_empty() || p == "::" {
            format!("{p}{x}")
        } else {
            format!("{p}::{x}")
        }
    };
    match t {
        [] => Err("`use` の木が空です".into()),
        ["::", rest @ ..] if prefix.is_empty() => leaves(rest, "::".into(), out),
        ["*"] => {
            out.push((prefix, true, None));
            Ok(())
        }
        ["{", inner @ .., "}"] => {
            let mut depth = 0;
            let mut start = 0;
            for (k, w) in inner.iter().enumerate() {
                match *w {
                    "{" => depth += 1,
                    "}" => depth -= 1,
                    "," if depth == 0 => {
                        if k > start {
                            leaves(&inner[start..k], prefix.clone(), out)?;
                        }
                        start = k + 1;
                    }
                    _ => {}
                }
            }
            if start < inner.len() {
                leaves(&inner[start..], prefix, out)?;
            }
            Ok(())
        }
        [name] if name.chars().all(|c| c.is_alphanumeric() || c == '_') => {
            let p = if *name == "self" {
                prefix
            } else {
                join(&prefix, name)
            };
            out.push((p, false, None));
            Ok(())
        }
        [name, "as", alias] => {
            let p = if *name == "self" {
                prefix
            } else {
                join(&prefix, name)
            };
            out.push((p, false, Some(alias.to_string())));
            Ok(())
        }
        [name, "::", rest @ ..] => leaves(rest, join(&prefix, name), out),
        _ => Err(format!("`use` の木を読めません: {}", t.join(" "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const LIB: &str = "crates/ebykn_prelude/src/lib.rs";
    const CELL: &str = "crates/ebykn_prelude/src/cell.rs";

    fn repo(lib: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write(LIB, lib);
        t.write(CELL, &real(CELL));
        t
    }

    #[test]
    fn passes_on_the_real_prelude() {
        assert_passes(&repo(&real(LIB)).run(run));
    }

    #[test]
    fn fails_on_a_glob_of_vstd() {
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::prelude::*;",
        );
        assert_fails(&repo(&broken).run(run), "`vstd::prelude::*`");
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use ::vstd::{prelude::*, view::View};",
        );
        assert_fails(&repo(&broken).run(run), "`::vstd::prelude::*`");
    }

    #[test]
    fn fails_on_names_outside_the_list() {
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::prelude::admit;",
        );
        assert_fails(&repo(&broken).run(run), "`vstd::prelude::admit`");
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub(crate) use verus_builtin::requires;",
        );
        assert_fails(&repo(&broken).run(run), "`verus_builtin::requires`");
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd as v;",
        );
        assert_fails(&repo(&broken).run(run), "`vstd`");
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::view::{View as V};",
        );
        assert_fails(&repo(&broken).run(run), "`vstd::view::View`");
    }

    #[test]
    fn fails_on_extern_crate_and_unreadable_uses() {
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub extern crate vstd;",
        );
        assert_fails(&repo(&broken).run(run), "`vstd`");
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::view::View",
        );
        assert_fails(&repo(&broken).run(run), "読めません");
    }

    #[test]
    fn fails_on_reexports_through_aliases() {
        // 別名を通した glob と、別名を通した vstd の項目（PR #26 のレビュー）。
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::view::View;\nuse vstd::prelude as vp;\npub use vp::*;",
        );
        assert_fails(&repo(&broken).run(run), "glob で再公開");
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::view::View;\nuse vstd::prelude as vp;\npub use vp::admit;",
        );
        assert_fails(&repo(&broken).run(run), "`vstd::prelude::admit`");
        let broken = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::view::View;\nextern crate vstd as v2;\npub use v2::prelude::requires;",
        );
        assert_fails(&repo(&broken).run(run), "`vstd::prelude::requires`");
        // 公開しない glob の use（verus! のため）は通す。
        let ok = mutate(
            &real(LIB),
            "pub use vstd::view::View;",
            "pub use vstd::view::View;\nuse vstd::prelude::*;",
        );
        assert_passes(&repo(&ok).run(run));
    }

    #[test]
    fn fails_without_prelude_sources() {
        assert_fails(&TempRepo::new().run(run), ".rs がありません");
    }
}
