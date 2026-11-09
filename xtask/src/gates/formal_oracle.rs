//! `formal-oracle`：§6.2 の参照モデル（`formal/linearity`）から判定の表を作り直し、
//! コミットされた表（`formal/oracle/`）と一致することを確かめる。
//!
//! 表を作るのはモデルのクレートの例 `oracle`（`examples/oracle.rs`）（Verus なしの cargo でビルドする）。バイナリは、
//! わざと壊した検査のどれかが表のどのケースでも正しい検査と食い違わなければ失敗する。
//! モデルのクレートは vstd をビルドするので `gates`（pre-commit）には入れず、CI の Verus のジョブで走らせる。
//!
//! 次を失敗にする：作り直した表とコミットされた表の違い、モデルが作るのに追跡されていない表、
//! `formal/oracle/` にあるのにモデルが作らないファイル、モデルが何も作らないこと。

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Repo, Report};
use crate::verus::clean_cargo;

/// コミットされた表の置き場。
pub const DIR: &str = "formal/oracle/";
/// 表を作るモデルのクレート。
pub const MODEL: &str = "formal/linearity";

/// モデルの `oracle` を `out` に走らせる。
fn generate(root: &Path, out: &Path) -> Result<Vec<(String, String)>, String> {
    let _ = std::fs::remove_dir_all(out);
    let mut cmd = Command::new("cargo");
    clean_cargo(&mut cmd)
        .args([
            "run",
            "--quiet",
            "--locked",
            "--example",
            "oracle",
            "--target-dir",
        ])
        .arg(root.join("target/verus-plain"))
        .arg("--")
        .arg(out)
        .current_dir(root.join(MODEL));
    eprintln!("$ (cd {MODEL} && {cmd:?})");
    let status = cmd
        .status()
        .map_err(|e| format!("cargo を起動できません: {e}"))?;
    if !status.success() {
        return Err(format!("{MODEL} の oracle が失敗しました（{status}）"));
    }
    let mut files = Vec::new();
    let entries = std::fs::read_dir(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", out.display()))?;
        let name = entry.file_name().into_string().map_err(|n| {
            format!(
                "{}: ファイル名が UTF-8 ではありません: {n:?}",
                out.display()
            )
        })?;
        let text = std::fs::read_to_string(entry.path())
            .map_err(|e| format!("{}: 読めません: {e}", entry.path().display()))?;
        files.push((name, text));
    }
    files.sort();
    Ok(files)
}

fn out_dir(root: &Path) -> PathBuf {
    root.join("target/formal-oracle")
}

pub fn run(repo: &Repo) -> Report {
    match generate(&repo.root, &out_dir(&repo.root)) {
        Ok(files) => compare(repo, &files),
        Err(e) => Report {
            errors: vec![e],
            notes: Vec::new(),
        },
    }
}

/// 作り直した表 `generated`（ファイル名と中身）とコミットされた表を比べる。
pub fn compare(repo: &Repo, generated: &[(String, String)]) -> Report {
    let mut r = Report::default();
    if generated.is_empty() {
        r.errors
            .push(format!("{MODEL} の oracle が何も作りませんでした"));
    }
    for (name, text) in generated {
        let rel = format!("{DIR}{name}");
        if !repo.has(&rel) {
            r.errors.push(format!(
                "{rel} がありません。`cargo xtask formal-oracle-write` で作ってコミットしてください"
            ));
            continue;
        }
        match repo.read(&rel) {
            Err(e) => r.errors.push(e),
            Ok(committed) if committed == *text => {
                let rows = text.lines().count();
                r.notes.push(format!("{rel}: {rows} 行が一致"));
            }
            Ok(committed) => {
                let line = committed
                    .lines()
                    .zip(text.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| committed.lines().count().min(text.lines().count()))
                    + 1;
                r.errors.push(format!(
                    "{rel}:{line}: モデルから作り直した表と違います。モデルを変えたなら `cargo xtask formal-oracle-write` で作り直してください"
                ));
            }
        }
    }
    for f in repo.files.iter().filter(|f| f.starts_with(DIR)) {
        let name = &f[DIR.len()..];
        if !generated.iter().any(|(n, _)| n == name) {
            r.errors
                .push(format!("{f}: モデルが作らないファイルです。消してください"));
        }
    }
    r
}

/// `formal-oracle-write`：表を作り直して `formal/oracle/` に書く。
pub fn write(root: &Path) -> bool {
    let out = out_dir(root);
    match generate(root, &out) {
        Err(e) => {
            eprintln!("error: formal-oracle-write: {e}");
            false
        }
        Ok(files) => {
            let dir = root.join(DIR);
            let _ = std::fs::remove_dir_all(&dir);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("error: formal-oracle-write: {}: {e}", dir.display());
                return false;
            }
            for (name, text) in &files {
                if let Err(e) = std::fs::write(dir.join(name), text) {
                    eprintln!("error: formal-oracle-write: {name}: {e}");
                    return false;
                }
                eprintln!("ok: formal-oracle-write: {DIR}{name}");
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const TSV: &str = "formal/oracle/linearity.tsv";
    const EYK: &str = "formal/oracle/linearity.eyk";
    const MUTANTS: &str = "formal/oracle/linearity-mutants.tsv";

    fn committed() -> Vec<(String, String)> {
        [TSV, EYK, MUTANTS]
            .iter()
            .map(|f| (f[DIR.len()..].to_string(), real(f)))
            .collect()
    }

    fn repo_with(files: &[(String, String)]) -> TempRepo {
        let mut t = TempRepo::new();
        for (name, text) in files {
            t.write(&format!("{DIR}{name}"), text);
        }
        t
    }

    #[test]
    fn passes_when_the_tables_match() {
        let files = committed();
        assert_passes(&repo_with(&files).run_ref(|r| compare(r, &files)));
    }

    #[test]
    fn fails_when_a_committed_verdict_is_changed() {
        let files = committed();
        let mut broken = files.clone();
        broken[0].1 = mutate(&broken[0].1, "\tE0509\t", "\tok\t");
        let report = repo_with(&broken).run_ref(|r| compare(r, &files));
        assert_fails(&report, "linearity.tsv:");
    }

    #[test]
    fn fails_when_a_committed_fragment_is_changed() {
        let files = committed();
        let mut broken = files.clone();
        broken[1].1 = mutate(&broken[1].1, "borrow mut r[p] {", "borrow r[p] {");
        let report = repo_with(&broken).run_ref(|r| compare(r, &files));
        assert_fails(&report, "linearity.eyk:");
    }

    /// 負の証拠：壊した検査は 3 つ以上あり、どれも表のケースで正しい検査と食い違う。
    /// 最初に食い違うケースの正しい判定は、表の判定と同じ。
    #[test]
    fn every_mutant_is_caught_by_a_case_in_the_table() {
        let verdicts: Vec<(String, String)> = real(TSV)
            .lines()
            .skip(1)
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                let v = if f[4] == "ok" {
                    "ok".to_string()
                } else {
                    format!("{} {}", f[4], f[5])
                };
                (f[0].to_string(), v)
            })
            .collect();
        let rows: Vec<String> = real(MUTANTS).lines().skip(1).map(str::to_string).collect();
        assert!(rows.len() >= 3, "壊した検査が 3 つ未満");
        for row in &rows {
            let f: Vec<&str> = row.split('\t').collect();
            let differing: usize = f[2].parse().unwrap();
            assert!(differing > 0, "{}: 食い違うケースがない", f[0]);
            let (_, want) = verdicts
                .iter()
                .find(|(c, _)| c == f[3])
                .unwrap_or_else(|| panic!("{}: ケース {} が表にない", f[0], f[3]));
            assert_eq!(want, f[4], "{}: {} の正しい判定", f[0], f[3]);
            assert_ne!(f[4], f[5], "{}: 判定が同じ", f[0]);
        }
    }

    #[test]
    fn fails_when_a_table_is_missing() {
        let files = committed();
        let report = repo_with(&files[1..]).run_ref(|r| compare(r, &files));
        assert_fails(&report, "がありません");
    }

    #[test]
    fn fails_when_a_stale_file_is_left() {
        let files = committed();
        let mut extra = files.clone();
        extra.push(("old.tsv".to_string(), "x\n".to_string()));
        let report = repo_with(&extra).run_ref(|r| compare(r, &files));
        assert_fails(&report, "モデルが作らないファイル");
    }

    #[test]
    fn fails_when_the_model_makes_nothing() {
        let report = repo_with(&[]).run_ref(|r| compare(r, &[]));
        assert_fails(&report, "何も作りませんでした");
    }

    #[test]
    fn fails_when_the_table_has_fewer_rows() {
        let files = committed();
        let mut broken = files.clone();
        let last = broken[0].1.lines().last().unwrap().to_string();
        broken[0].1 = mutate(&broken[0].1, &format!("{last}\n"), "");
        let report = repo_with(&broken).run_ref(|r| compare(r, &files));
        assert_fails(&report, "linearity.tsv:");
    }

    /// 表の各ケースの `.eyk` の断片が M1 の構文解析器で読め、表の行と関数が一対一で、
    /// 診断の位置が自分の関数の中にある。
    #[test]
    fn fragments_parse_and_match_the_table() {
        use ebykn_syntax::ast::ItemKind;
        use ebykn_syntax::span::{FileId, SourceFile};
        let text = real(EYK);
        let file = SourceFile::new(FileId(0), EYK, text.clone());
        let parsed = ebykn_syntax::parse_eyk(&file);
        let diags: Vec<String> = parsed
            .diags
            .iter()
            .take(5)
            .map(|d| d.render(&file, Default::default()))
            .collect();
        assert!(parsed.is_ok(), "構文エラー: {diags:?}");
        let fns: Vec<(String, usize, usize)> = parsed
            .file
            .items
            .iter()
            .filter_map(|it| match &it.kind {
                ItemKind::Fn(f) => Some((
                    f.sig.ident.name.clone(),
                    it.span.start.line,
                    it.span.end.line,
                )),
                _ => None,
            })
            .filter(|(n, _, _)| n.starts_with('c') && n[1..].bytes().all(|b| b.is_ascii_digit()))
            .collect();
        let rows: Vec<Vec<String>> = real(TSV)
            .lines()
            .skip(1)
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect();
        assert_eq!(fns.len(), rows.len(), "関数の数と表の行の数");
        let lines: Vec<&str> = text.lines().collect();
        for ((name, lo, hi), row) in fns.iter().zip(&rows) {
            assert_eq!(name, &row[0]);
            if row[4] == "ok" {
                continue;
            }
            let line: usize = row[6].parse().unwrap();
            let col: usize = row[7].parse().unwrap();
            assert!(
                *lo <= line && line <= *hi,
                "{name}: 診断の位置 {line} が関数の外"
            );
            let l = lines[line - 1];
            assert!(
                l.len() >= col && l[..col - 1].trim().is_empty() && !l[col - 1..].is_empty(),
                "{name}: {line}:{col} が文の先頭ではない: {l:?}"
            );
        }
    }
}
