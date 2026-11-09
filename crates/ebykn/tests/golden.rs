//! golden テストのランナー（仕様 §11）。`tests/golden/<name>/` ごとに `ebykn check` をかけ、
//! 診断が `expected/diag.txt`（英語の文言、1 行に一つ、位置の順）と一致することを確かめる。
//!
//! 生成した Verus と `expected/verus/` の比較、`verify` の結果と `expected/status.txt` の比較は、
//! 書き下しと一緒に足す。

use std::path::{Path, PathBuf};

use ebykn::check_project;
use ebykn_syntax::diag::Lang;

fn golden_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// golden の各ケース（`tests/golden/` の直下のディレクトリ）。読めなければ失敗にする。
fn cases() -> Vec<PathBuf> {
    let root = golden_root();
    let mut out: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{} を読めません: {e}", root.display()))
        .map(|e| e.expect("ディレクトリの項目").path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// 実際の診断と期待（ファイルの中身）を比べる。違えば説明を返す。
fn compare(name: &str, actual: &[String], expected: &str) -> Result<(), String> {
    let expected: Vec<&str> = expected.lines().collect();
    if actual
        .iter()
        .map(String::as_str)
        .eq(expected.iter().copied())
    {
        return Ok(());
    }
    Err(format!(
        "{name}: 診断が expected/diag.txt と違います\n--- 期待\n{}\n--- 実際\n{}\n",
        expected.join("\n"),
        actual.join("\n")
    ))
}

fn run_case(dir: &Path) -> Result<(), String> {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let expected = dir.join("expected/diag.txt");
    let expected = std::fs::read_to_string(&expected)
        .map_err(|e| format!("{name}: {} を読めません: {e}", expected.display()))?;
    let report = check_project(dir);
    compare(&name, &report.render(Lang::En), &expected)
}

#[test]
fn every_case_matches_its_expected_diagnostics() {
    let cases = cases();
    let names: Vec<String> = cases
        .iter()
        .map(|c| c.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    // 何も見つけずに通らないよう、知っているケースがあることを確かめる。
    for want in ["linearity", "stack"] {
        assert!(names.iter().any(|n| n == want), "{want} がない: {names:?}");
    }
    let errors: Vec<String> = cases.iter().filter_map(|c| run_case(c).err()).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

/// §11 の `linearity`：E0503〜E0512 がちょうど一つずつ出る。
#[test]
fn linearity_reports_e0503_to_e0512_once_each() {
    let report = check_project(&golden_root().join("linearity"));
    let mut codes: Vec<&str> = report.diags.iter().map(|d| d.code().as_str()).collect();
    codes.sort();
    let want: Vec<String> = (503..=512).map(|n| format!("E0{n}")).collect();
    assert_eq!(codes, want);
}

#[test]
fn stack_has_no_diagnostics() {
    let report = check_project(&golden_root().join("stack"));
    assert!(report.diags.is_empty(), "{:?}", report.render(Lang::En));
}

#[test]
fn fails_when_the_expected_diagnostics_differ() {
    let dir = golden_root().join("linearity");
    let expected = std::fs::read_to_string(dir.join("expected/diag.txt")).unwrap();
    let actual = check_project(&dir).render(Lang::En);
    assert!(compare("linearity", &actual, &expected).is_ok());
    // 期待の 1 行を壊すと失敗する（壊す操作が当たったことを先に確かめる）。
    let broken = expected.replacen("E0507", "E0506", 1);
    assert_ne!(broken, expected);
    assert!(compare("linearity", &actual, &broken).is_err());
    // 期待から 1 行消しても失敗する。
    let fewer: String = expected.lines().skip(1).map(|l| format!("{l}\n")).collect();
    assert_ne!(fewer, expected);
    assert!(compare("linearity", &actual, &fewer).is_err());
    // 何も出なかったことにしても失敗する。
    assert!(compare("linearity", &[], &expected).is_err());
}

#[test]
fn fails_when_a_case_has_no_expected_file() {
    let tmp = std::env::temp_dir().join(format!("ebykn-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("src")).unwrap();
    std::fs::copy(
        golden_root().join("stack/Ebykn.toml"),
        tmp.join("Ebykn.toml"),
    )
    .unwrap();
    std::fs::write(tmp.join("src/lib.eyk"), "pub fn f() {}\n").unwrap();
    let r = run_case(&tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(r.unwrap_err().contains("expected/diag.txt"));
}
