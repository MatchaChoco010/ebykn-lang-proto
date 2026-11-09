//! `ebykn` コマンドの中身（仕様 §9）。`main.rs` は引数を読んでここを呼ぶ。golden テスト
//! （`tests/golden.rs`）も同じ関数を使う。

use std::path::{Path, PathBuf};

use ebykn_check::project::{self, Project};
use ebykn_check::{Checked, sort_diags};
use ebykn_syntax::diag::{Diagnostic, Lang};

pub mod no_cheating;
pub mod verify;
pub mod verus;

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
    diags.extend(pin_mismatches(&project));
    let checked = ebykn_check::check(&project);
    diags.extend(checked.diags.iter().cloned());
    sort_diags(&mut diags);
    CheckReport {
        project,
        checked: Some(checked),
        diags,
    }
}

/// `Ebykn.toml` の `[verus]` が、この `ebykn` の固定した Verus と違えば E0002（仕様 §1.3）。
fn pin_mismatches(project: &Project) -> Vec<Diagnostic> {
    use ebykn_syntax::diag::{CheckMsg, Message};
    let pin = verus_pin();
    let m = &project.manifest;
    let file = project.file(ebykn_syntax::span::FileId(0));
    let mut out = Vec::new();
    for (key, written, pinned) in [
        ("release", &m.verus_release, &pin.release),
        ("commit", &m.verus_commit, &pin.commit),
    ] {
        // 書いていなければ、読み込みが E0002（鍵がない）を出している。
        if written.is_empty() || written == pinned {
            continue;
        }
        let span = match file.text.find(&format!("\"{written}\"")) {
            Some(i) => file.span(i, i + written.len() + 2),
            None => file.span(0, 0),
        };
        out.push(Diagnostic::new(
            span,
            Message::Check(CheckMsg::VerusPinMismatch {
                key,
                written: written.clone(),
                pinned: pinned.clone(),
            }),
        ));
    }
    out
}

/// `check` がまだ実行しない段階（仕様 §9 の `check` のうち、M2 の後半と M3 で足すもの）。
pub const CHECK_SKIPPED: &[&str] = &["self-contained check (§6.6)"];

impl CheckReport {
    /// `--json` の最後に出す 1 行：結果の件数と、実行しなかった段階。
    pub fn summary_json(&self) -> String {
        let errors = self.diags.iter().filter(|d| !d.code().is_warning()).count();
        let warnings = self.diags.len() - errors;
        let skipped = CHECK_SKIPPED
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"$message_type\":\"ebykn-summary\",\"command\":\"check\",\"success\":{},\"errors\":{errors},\"warnings\":{warnings},\"skipped\":[{skipped}]}}",
            errors == 0
        )
    }
}

/// 検査を通ったプロジェクトを書き下す（§7）。検査にエラーがあれば `None`。
pub fn lower_report(report: &CheckReport) -> Option<Result<ebykn_lower::Output, String>> {
    if report.has_errors() {
        return None;
    }
    let checked = report.checked.as_ref()?;
    Some(ebykn_lower::lower(&report.project, checked))
}

/// [`lower_report`] の、依存クレートがコードで使われる名前を与える版（`verify` が `cargo metadata`
/// で取った名前で、手続きマクロの前提の一覧を確定し直す。#49）。
pub fn lower_report_with_crates(
    report: &CheckReport,
    crates: &[String],
) -> Option<Result<ebykn_lower::Output, String>> {
    if report.has_errors() {
        return None;
    }
    let checked = report.checked.as_ref()?;
    Some(ebykn_lower::lower_with_crates(
        &report.project,
        checked,
        Some(crates),
    ))
}

/// 固定した Verus（ルートの `Cargo.toml` の `[workspace.metadata.verus]`）。`init` が `Ebykn.toml` に書く。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusPin {
    pub release: String,
    pub commit: String,
    pub version: String,
    pub toolchain: String,
    pub vstd: String,
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
        version: s("version")?,
        toolchain: s("toolchain")?,
        vstd: s("vstd")?,
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
    fn summary_lists_the_skipped_stages() {
        let (project, _) = project::single("pub fn f() {}");
        let r = CheckReport {
            project,
            checked: None,
            diags: Vec::new(),
        };
        let j = r.summary_json();
        assert!(j.contains("\"success\":true"), "{j}");
        assert!(
            j.contains("\"skipped\":[\"self-contained check (§6.6)\""),
            "{j}"
        );
    }

    #[test]
    fn rejects_bad_package_names() {
        let dir = std::env::temp_dir();
        assert!(init(&dir, "1abc").is_err());
        assert!(init(&dir, "a b").is_err());
        assert!(init(&dir, "").is_err());
    }
}
