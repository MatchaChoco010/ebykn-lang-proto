//! `ebykn` コマンドの中身（仕様 §9）。`main.rs` は引数を読んでここを呼ぶ。golden テスト
//! （`tests/golden.rs`）も同じ関数を使う。

use std::path::{Path, PathBuf};

use ebykn_check::project::{self, Project};
use ebykn_check::{Checked, sort_diags};
use ebykn_syntax::diag::{Diagnostic, Lang};

/// `ebykn check` の §6 までの結果。
pub struct CheckReport {
    pub project: Project,
    pub checked: Option<Checked>,
    /// 位置の順に並べた診断（読み込み・構文・検査のすべて）。
    pub diags: Vec<Diagnostic>,
}

impl CheckReport {
    /// エラー（警告でない診断）があるか。
    pub fn has_errors(&self) -> bool {
        self.diags.iter().any(|d| !d.code().is_warning())
    }

    /// `ファイル:行:列: コード: メッセージ` の行。
    pub fn render(&self, lang: Lang) -> Vec<String> {
        self.diags
            .iter()
            .map(|d| d.render(self.project.file(d.span.file), lang))
            .collect()
    }

    /// rustc 互換の JSON の行。
    pub fn json(&self, lang: Lang) -> Vec<String> {
        self.diags
            .iter()
            .map(|d| d.to_json(self.project.file(d.span.file), lang))
            .collect()
    }
}

/// プロジェクト `root` を読み、§6 の検査をする。構文エラーがあっても、読めた項目は検査する。
pub fn check_project(root: &Path) -> CheckReport {
    let (project, mut diags) = project::load(root);
    let checked = ebykn_check::check(&project);
    diags.extend(checked.diags.iter().cloned());
    sort_diags(&mut diags);
    CheckReport {
        project,
        checked: Some(checked),
        diags,
    }
}

/// 固定した Verus（ルートの `Cargo.toml` の `[workspace.metadata.verus]`）。`init` が `Ebykn.toml` に書く。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusPin {
    pub release: String,
    pub commit: String,
}

const WORKSPACE_TOML: &str = include_str!("../../../Cargo.toml");

/// `ebykn` をビルドしたときの固定値。
pub fn verus_pin() -> VerusPin {
    read_pin(WORKSPACE_TOML).expect("ルートの Cargo.toml の [workspace.metadata.verus] を読める")
}

fn read_pin(text: &str) -> Option<VerusPin> {
    use toml::de::{DeTable, DeValue};
    fn table<'a, 'i>(t: &'a DeTable<'i>, k: &str) -> Option<&'a DeTable<'i>> {
        match t.get(k)?.get_ref() {
            DeValue::Table(x) => Some(x),
            _ => None,
        }
    }
    let root = DeTable::parse(text).ok()?.into_inner();
    let verus = table(table(table(&root, "workspace")?, "metadata")?, "verus")?;
    let s = |k: &str| match verus.get(k)?.get_ref() {
        DeValue::String(s) => Some(s.to_string()),
        _ => None,
    };
    Some(VerusPin {
        release: s("release")?,
        commit: s("commit")?,
    })
}

/// `ebykn init <name>`（仕様 §9）：`dir/<name>/` に雛形を作る。
pub fn init(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let ok = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok {
        return Err(format!(
            "`{name}` cannot be a package name; use ASCII letters, digits, `_` and `-`, starting with a letter or `_`"
        ));
    }
    let root = dir.join(name);
    if root.exists() {
        return Err(format!("`{}` already exists", root.display()));
    }
    let pin = verus_pin();
    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[verus]\nrelease = \"{}\"\ncommit = \"{}\"\n\n[dependencies]\n",
        pin.release, pin.commit
    );
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    };
    write(project::MANIFEST, &manifest)?;
    write("src/lib.eyk", INIT_LIB)?;
    write(".gitignore", "/build/\n")?;
    Ok(root)
}

/// `init` が作る `src/lib.eyk`。
pub const INIT_LIB: &str = "\
/// `x` の 2 倍。
pub fn double(x: u32) -> (r: u32)
    requires x <= 1000,
    ensures r == 2 * x,
{
    2 * x
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_workspace_pin() {
        let pin = verus_pin();
        assert!(pin.release.starts_with("release/"), "{pin:?}");
        assert_eq!(pin.commit.len(), 40);
    }

    #[test]
    fn rejects_bad_package_names() {
        let dir = std::env::temp_dir();
        assert!(init(&dir, "1abc").is_err());
        assert!(init(&dir, "a b").is_err());
        assert!(init(&dir, "").is_err());
    }
}
