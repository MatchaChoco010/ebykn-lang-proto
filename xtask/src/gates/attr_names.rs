//! `check-attr-names`：組み込みの属性の名前を、raw の綴り（`r#cfg`）を見逃す形で比べない（#122）。
//!
//! rustc は `#[r#cfg]` を `#[cfg]` とまったく同じに読む。`Ident::is` は raw の綴りを別の名前として扱う
//! ので、属性の名前の比較に使うと `#[r#cfg(..)]` が検査を抜ける。トークンの識別子を文字列と `==` で
//! 比べても、`r#cfg` は `"cfg"` と等しくない。属性の名前は `Ident::is_name` / `Attr::is_builtin` /
//! `macros::token_is_name` で比べる。
//!
//! 検査は字句的に行う。`crates/` の下の追跡されている `.rs` から、次の形を探して失敗にする。名前は
//! 組み込みの属性の表（`crates/ebykn_syntax/src/macros.rs` の `BUILTIN_ATTRS` と `PRELUDE_ATTR_MACROS`）の
//! もので、表を読めなければ失敗にする。
//!
//! - `.is("名前")`
//! - 同じ行の `Ident(x)` で束縛した `x` と `"名前"` の `==`（`TokenTree::Ident(i) => i == "repr"`）

use super::{Repo, Report};

/// 組み込みの属性の表を持つファイル。
pub const TABLE: &str = "crates/ebykn_syntax/src/macros.rs";
/// 表の配列の名前。
const ARRAYS: &[&str] = &["BUILTIN_ATTRS", "PRELUDE_ATTR_MACROS"];

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    let names = match repo.read(TABLE).and_then(|src| table(&src)) {
        Ok(n) => n,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    let mut checked = 0;
    for f in &repo.files {
        if !(f.starts_with("crates/") && f.ends_with(".rs")) {
            continue;
        }
        checked += 1;
        let src = match repo.read(f) {
            Ok(s) => s,
            Err(e) => {
                r.errors.push(e);
                continue;
            }
        };
        for (i, line) in src.lines().enumerate() {
            for name in &names {
                if line.contains(&format!(".is(\"{name}\")")) {
                    r.errors.push(format!(
                        "{f}:{}: 属性の名前 `{name}` を `Ident::is` で比べています。raw の綴り（`r#{name}`）を見逃すので、`Ident::is_name` か `Attr::is_builtin` で比べてください",
                        i + 1
                    ));
                }
                for var in ident_bindings(line) {
                    if line.contains(&format!("{var} == \"{name}\"")) {
                        r.errors.push(format!(
                            "{f}:{}: トークンの識別子 `{var}` を属性の名前 `\"{name}\"` と `==` で比べています。raw の綴り（`r#{name}`）を見逃すので、`macros::token_is_name` で比べてください",
                            i + 1
                        ));
                    }
                }
            }
        }
    }
    r.notes.push(format!(
        "属性の名前 {} 個（{TABLE} の {}）で、{checked} ファイルを検査",
        names.len(),
        ARRAYS.join("・")
    ));
    r
}

/// 表の配列の文字列（`pub const BUILTIN_ATTRS: &[&str] = &[ .. ];` の中の `"..."`）。
fn table(src: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for a in ARRAYS {
        let head = format!("pub const {a}: &[&str] = &[");
        let start = src
            .find(&head)
            .ok_or_else(|| format!("{TABLE}: `{head}` がありません"))?
            + head.len();
        let end = src[start..]
            .find("];")
            .ok_or_else(|| format!("{TABLE}: `{a}` の配列が閉じていません"))?;
        let body = &src[start..start + end];
        let mut n = 0;
        for line in body.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with("//") {
                continue;
            }
            let lits: Vec<&str> = t
                .split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .collect();
            for lit in lits {
                let name = lit
                    .strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                    .ok_or_else(|| format!("{TABLE}: `{a}` の要素を読めません: {lit}"))?;
                out.push(name.to_string());
                n += 1;
            }
        }
        if n == 0 {
            return Err(format!("{TABLE}: `{a}` が空です"));
        }
    }
    Ok(out)
}

/// 行の中の `Ident(x)` の `x`（識別子だけ）。
fn ident_bindings(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find("Ident(") {
        let after = &rest[i + "Ident(".len()..];
        if let Some(close) = after.find(')') {
            let var = after[..close].trim();
            if !var.is_empty() && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push(var);
            }
        }
        rest = after;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const INDEX: &str = "crates/ebykn_check/src/index.rs";

    fn with_table(t: &mut TempRepo) {
        t.write(TABLE, &real(TABLE));
    }

    #[test]
    fn passes_for_the_real_sources() {
        let mut t = TempRepo::new();
        with_table(&mut t);
        t.write(INDEX, &real(INDEX));
        let r = t.run(run);
        assert_passes(&r);
        assert!(
            r.notes.iter().any(|n| n.contains("2 ファイルを検査")),
            "{:?}",
            r.notes
        );
    }

    #[test]
    fn fails_when_an_attribute_name_is_compared_with_is() {
        let mut t = TempRepo::new();
        with_table(&mut t);
        let broken = mutate(
            &real(INDEX),
            "seg.ident.is_name(\"repr\")",
            "seg.ident.is(\"repr\")",
        );
        t.write(INDEX, &broken);
        assert_fails(&t.run(run), "`repr` を `Ident::is` で比べています");
    }

    #[test]
    fn fails_when_a_token_ident_is_compared_with_an_attribute_name() {
        let mut t = TempRepo::new();
        with_table(&mut t);
        let broken = mutate(
            &real(INDEX),
            "ebykn_syntax::macros::token_is_name(&i, \"cfg\")",
            "i == \"cfg\"",
        );
        t.write(INDEX, &broken);
        assert_fails(
            &t.run(run),
            "`i` を属性の名前 `\"cfg\"` と `==` で比べています",
        );
    }

    #[test]
    fn fails_when_the_table_cannot_be_read() {
        let mut t = TempRepo::new();
        let broken = mutate(
            &real(TABLE),
            "pub const BUILTIN_ATTRS",
            "pub const BUILTIN_ATTRIBUTES",
        );
        t.write(TABLE, &broken);
        assert_fails(
            &t.run(run),
            "`pub const BUILTIN_ATTRS: &[&str] = &[` がありません",
        );
        let mut t = TempRepo::new();
        t.write(INDEX, &real(INDEX));
        assert_fails(&t.run(run), "読めません");
    }

    #[test]
    fn reads_the_table() {
        let names = table(&real(TABLE)).unwrap();
        for want in ["cfg", "cfg_attr", "repr", "test", "derive", "doc"] {
            assert!(names.iter().any(|n| n == want), "{want}: {names:?}");
        }
        assert!(!names.iter().any(|n| n == "feature"), "{names:?}");
    }

    #[test]
    fn finds_ident_bindings() {
        assert_eq!(
            ident_bindings("TokenTree::Ident(i) => i == \"repr\", Some(TokenTree::Ident(x)) if x"),
            ["i", "x"]
        );
        assert!(ident_bindings("Ident(&x)").is_empty());
    }
}
