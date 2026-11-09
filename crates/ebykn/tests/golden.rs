//! golden テストのランナー（仕様 §11）。`tests/golden/<name>/` ごとに `ebykn check` をかけ、
//! 診断が `expected/diag.txt`（英語の文言、1 行に一つ、位置の順）と一致することを確かめる。
//! `expected/diag.ja.txt` があるケースは、日本語の文言（`--lang ja`）もその中身と比べる。
//!
//! `expected/verus/` があるケースは、書き下した Verus（`build/verus/` に書くもの）を
//! `expected/verus/` とトークン列で比べる（空白・コメント・記号の結合の仕方は見ない）。`Cargo.toml` は
//! `ebykn_prelude` への `path` だけを除いて行で比べる。`verify` の結果と `expected/status.txt` の比較は
//! Verus が要るので `cargo xtask golden-verify`（CI の verus ジョブ）が行う。

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
    compare_file(name, "diag.txt", actual, expected)
}

fn compare_file(name: &str, file: &str, actual: &[String], expected: &str) -> Result<(), String> {
    let expected: Vec<&str> = expected.lines().collect();
    if actual
        .iter()
        .map(String::as_str)
        .eq(expected.iter().copied())
    {
        return Ok(());
    }
    Err(format!(
        "{name}: 診断が expected/{file} と違います\n--- 期待\n{}\n--- 実際\n{}\n",
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
    compare(&name, &report.render(Lang::En), &expected)?;
    let ja = dir.join("expected/diag.ja.txt");
    if ja.exists() {
        let expected = std::fs::read_to_string(&ja)
            .map_err(|e| format!("{name}: {} を読めません: {e}", ja.display()))?;
        compare_file(&name, "diag.ja.txt", &report.render(Lang::Ja), &expected)?;
    }
    Ok(())
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

/// 日本語の golden がある `linearity` で、日本語の期待を壊すと失敗する（#34 の文言が戻ると落ちる）。
#[test]
fn fails_when_the_expected_japanese_diagnostics_differ() {
    let dir = golden_root().join("linearity");
    let expected = std::fs::read_to_string(dir.join("expected/diag.ja.txt")).unwrap();
    let actual = check_project(&dir).render(Lang::Ja);
    assert!(compare_file("linearity", "diag.ja.txt", &actual, &expected).is_ok());
    let broken = expected.replacen("持っていないので", "なしなので", 1);
    assert_ne!(broken, expected);
    let err = compare_file("linearity", "diag.ja.txt", &actual, &broken).unwrap_err();
    assert!(err.contains("expected/diag.ja.txt"), "{err}");
    // 英語の期待と取り違えても失敗する。
    let en = std::fs::read_to_string(dir.join("expected/diag.txt")).unwrap();
    assert!(compare_file("linearity", "diag.ja.txt", &actual, &en).is_err());
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

#[test]
fn fails_when_the_verus_pin_differs() {
    let tmp = std::env::temp_dir().join(format!("ebykn-pin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("src")).unwrap();
    let manifest = std::fs::read_to_string(golden_root().join("stack/Ebykn.toml")).unwrap();
    let pin = ebykn::verus_pin();
    let broken = manifest.replace(&pin.commit, &"0".repeat(40));
    assert_ne!(broken, manifest);
    std::fs::write(tmp.join("Ebykn.toml"), broken).unwrap();
    std::fs::write(tmp.join("src/lib.eyk"), "pub fn f() {}\n").unwrap();
    let r = check_project(&tmp).render(Lang::En);
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(
        r[0].contains("E0002: `verus.commit` in `Ebykn.toml`"),
        "{r:?}"
    );
}

/// トークン列（空白・コメント・記号の結合を除いた形）。
fn token_list(src: &str) -> Result<Vec<String>, String> {
    use proc_macro2::{TokenStream, TokenTree};
    fn flat(ts: TokenStream, out: &mut Vec<String>) {
        for t in ts {
            match t {
                TokenTree::Group(g) => {
                    out.push(format!("{:?}(", g.delimiter()));
                    flat(g.stream(), out);
                    out.push(")".into());
                }
                TokenTree::Punct(p) => out.push(p.as_char().to_string()),
                t => out.push(t.to_string()),
            }
        }
    }
    let ts: TokenStream = src.parse().map_err(|e| format!("{e}"))?;
    let mut out = Vec::new();
    flat(ts, &mut out);
    Ok(out)
}

/// `ebykn_prelude = { path = ".." }` の行の値を除く（置き場所は環境で変わる。`OPEN.md`）。
fn normalize_toml(s: &str) -> String {
    s.lines()
        .map(|l| {
            if l.starts_with("ebykn_prelude = ") {
                "ebykn_prelude = <path>".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 書き下しを `expected/verus/` と比べる。違えば説明を返す。
fn compare_lowered(dir: &Path) -> Result<(), String> {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let expected = dir.join("expected/verus");
    let report = check_project(dir);
    let output = ebykn::lower_report(&report)
        .ok_or_else(|| format!("{name}: 検査が通らないので書き下せません"))??;
    let mut want: Vec<String> = Vec::new();
    collect_rs(&expected.join("src"), "src", &mut want);
    want.sort();
    let mut got: Vec<String> = output.files.iter().map(|f| f.rel.clone()).collect();
    got.sort();
    if want != got {
        return Err(format!(
            "{name}: 生成したファイルが違います（期待 {want:?}、実際 {got:?}）"
        ));
    }
    for f in &output.files {
        let exp =
            std::fs::read_to_string(expected.join(&f.rel)).map_err(|e| format!("{name}: {e}"))?;
        let a = token_list(&exp).map_err(|e| format!("{name}: {}: {e}", f.rel))?;
        let b = token_list(&f.text).map_err(|e| format!("{name}: 生成した {}: {e}", f.rel))?;
        if a != b {
            let at = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
            return Err(format!(
                "{name}: {} のトークン列が {at} 個目から違います\n--- 期待: {}\n--- 実際: {}\n--- 生成した全文\n{}",
                f.rel,
                a[at.min(a.len())..]
                    .iter()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
                b[at.min(b.len())..]
                    .iter()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
                f.text
            ));
        }
    }
    let toml = ebykn_lower::cargo_toml(
        &report.project,
        &ebykn_lower::Deps {
            vstd: &ebykn::verus_pin().vstd,
            prelude_path: "-",
        },
    );
    let exp =
        std::fs::read_to_string(expected.join("Cargo.toml")).map_err(|e| format!("{name}: {e}"))?;
    if normalize_toml(&toml) != normalize_toml(&exp) {
        return Err(format!(
            "{name}: Cargo.toml が違います\n--- 期待\n{exp}\n--- 実際\n{toml}"
        ));
    }
    Ok(())
}

fn collect_rs(dir: &Path, rel: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let n = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            collect_rs(&p, &format!("{rel}/{n}"), out);
        } else if n.ends_with(".rs") {
            out.push(format!("{rel}/{n}"));
        }
    }
}

/// ケースの比べ方。`expected/verus/` があれば書き下しを比べる。なければ検査が失敗するケース
/// （診断だけを比べる）でなければならない。書き下せるのに `expected/verus/` がないケースは失敗にする
/// （黙って飛ばさない）。
fn compare_case(dir: &Path) -> Result<&'static str, String> {
    if dir.join("expected/verus").is_dir() {
        return compare_lowered(dir).map(|_| "verus");
    }
    if check_project(dir).has_errors() {
        return Ok("diag");
    }
    Err(format!(
        "{}: 検査を通るのに expected/verus/ がありません（書き下しを比べられません）",
        dir.display()
    ))
}

#[test]
fn lowered_verus_matches_expected() {
    let mut kinds = Vec::new();
    let mut errors = Vec::new();
    for c in cases() {
        let name = c.file_name().unwrap().to_string_lossy().into_owned();
        match compare_case(&c) {
            Ok(k) => kinds.push(format!("{name}:{k}")),
            Err(e) => errors.push(e),
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    for want in [
        "stack:verus",
        "lowering:verus",
        "assumed:verus",
        "linearity:diag",
    ] {
        assert!(kinds.iter().any(|k| k == want), "{want} がない: {kinds:?}");
    }
}

#[test]
fn fails_when_a_lowerable_case_has_no_expected_verus() {
    let tmp = std::env::temp_dir().join(format!("ebykn-golden-nov-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("src")).unwrap();
    std::fs::copy(
        golden_root().join("stack/Ebykn.toml"),
        tmp.join("Ebykn.toml"),
    )
    .unwrap();
    std::fs::write(tmp.join("src/lib.eyk"), "pub fn f() {}\n").unwrap();
    let r = compare_case(&tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(r.unwrap_err().contains("expected/verus/ がありません"));
}

#[test]
fn fails_when_the_lowered_verus_differs() {
    let a = token_list("fn f() { proof { x.insert(p); } }").unwrap();
    // 空白とコメントは見ない。
    assert_eq!(
        a,
        token_list("fn   f ( ) {\n proof { x . insert ( p ) ; } // c\n }").unwrap()
    );
    // `proof { }` を外すと違う。
    assert_ne!(a, token_list("fn f() { x.insert(p); }").unwrap());
    // 期待のファイルの 1 トークンを壊すと、比較が失敗する。
    let dir = golden_root().join("stack");
    let exp = std::fs::read_to_string(dir.join("expected/verus/src/lib.rs")).unwrap();
    let broken = exp.replacen("final(self)", "self", 1);
    assert_ne!(broken, exp);
    assert_ne!(token_list(&exp).unwrap(), token_list(&broken).unwrap());
    assert!(compare_lowered(&dir).is_ok());
}
