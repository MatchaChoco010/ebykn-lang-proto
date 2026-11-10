//! golden テストのランナー（仕様 §11）。`tests/golden/<name>/` ごとに `ebykn check` をかけ、
//! 診断が `expected/diag.txt`（英語の文言、1 行に一つ、位置の順）と一致することを確かめる。
//! `expected/diag.ja.txt` があるケースは、日本語の文言（`--lang ja`）もその中身と比べる。
//!
//! `expected/verus/` があるケースは、書き下した Verus（`build/verus/` に書くもの）を
//! `expected/verus/` とトークン列で比べる（空白・コメント・記号の結合の仕方は見ない）。`Cargo.toml` は
//! `ebykn_prelude` への `path` だけを除いて行で比べる。`verify` の結果と `expected/status.txt` の比較は
//! Verus が要るので `cargo xtask golden-verify`（CI の verus ジョブ）が行う。
//!
//! 書き下しの比べ方と作り直し方（`cargo xtask golden-bless`）は `golden_lowered/mod.rs` にある。

use std::path::{Path, PathBuf};

use ebykn::check_project;
use ebykn_syntax::diag::Lang;

mod golden_lowered;

use golden_lowered::{compare_lowered, token_list};

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

/// `.eykp` があれば、`check` は剥がし一致を確かめて通し、`verify` / `build` は E0003 で止める
/// （書き下しが `.eykp` を読むのは #67。それまで証明を黙って捨てない）。
#[test]
fn verify_and_build_stop_when_there_are_proof_files() {
    let tmp = std::env::temp_dir().join(format!("ebykn-eykp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("src")).unwrap();
    std::fs::create_dir_all(tmp.join("proof")).unwrap();
    std::fs::copy(
        golden_root().join("stack/Ebykn.toml"),
        tmp.join("Ebykn.toml"),
    )
    .unwrap();
    std::fs::write(tmp.join("src/lib.eyk"), "pub fn f() {}\n").unwrap();
    let before = check_project(&tmp);
    assert!(ebykn::proofs_not_lowered(&before).is_empty());
    std::fs::write(tmp.join("proof/lib.eykp"), "pub fn f() { proof { } }\n").unwrap();
    let report = check_project(&tmp);
    let blocked: Vec<String> = ebykn::proofs_not_lowered(&report)
        .iter()
        .map(|d| d.render(report.project.file(d.span.file), Lang::En))
        .collect();
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(report.diags.is_empty(), "{:?}", report.render(Lang::En));
    assert_eq!(blocked.len(), 1, "{blocked:?}");
    assert!(
        blocked[0].starts_with(
            "proof/lib.eykp:1:1: E0003: not implemented yet: `verify` and `build` with proof files"
        ),
        "{blocked:?}"
    );
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
        "borrow_exit:verus",
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

/// ケースのディレクトリを `to` に写す（`build/` と `target/` は写さない）。
fn copy_case(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let n = e.file_name();
        if n == "build" || n == "target" {
            continue;
        }
        if e.path().is_dir() {
            copy_case(&e.path(), &to.join(&n));
        } else {
            std::fs::copy(e.path(), to.join(&n)).unwrap();
        }
    }
}

/// 写したケースで `f` を走らせ、写しを消してから結果を返す。
fn with_copy<R>(case: &str, tag: &str, f: impl FnOnce(&Path) -> R) -> R {
    let tmp = std::env::temp_dir().join(format!("ebykn-bless-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let dir = tmp.join(case);
    copy_case(&golden_root().join(case), &dir);
    let r = f(&dir);
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

/// 今のどのケースも、コミットされた期待が生成物そのもの（作り直しても何も変わらない）で、比較を通る。
/// 期待にどんな形のコメントを足しても、ここで落ちる（期待にコメントを置かない。`OPEN.md`）。
#[test]
fn every_blessed_expectation_matches_the_lowering() {
    let mut blessed = Vec::new();
    for c in cases() {
        if !c.join("expected/verus").is_dir() {
            continue;
        }
        let name = c.file_name().unwrap().to_string_lossy().into_owned();
        let r = with_copy(&name, &name, |dir| {
            let b = golden_lowered::bless(dir)?;
            Ok::<_, String>((b, compare_case(dir)?))
        });
        assert_eq!(
            r,
            Ok((golden_lowered::Blessed::default(), "verus")),
            "{name}: 期待が生成物と違います（cargo xtask golden-bless {name} で作り直し、差分を読んでください）"
        );
        blessed.push(name);
    }
    assert!(blessed.iter().any(|n| n == "stack"), "{blessed:?}");
}

/// 壊した期待を作り直すと、比較を通る形に戻る。生成物にない `.rs` は消し、`Cargo.lock` には触らない。
/// もう一度作り直しても何も変わらない。
#[test]
fn bless_repairs_a_broken_expectation() {
    with_copy("stack", "repair", |dir| {
        let verus = dir.join("expected/verus");
        let lib = verus.join("src/lib.rs");
        let exp = std::fs::read_to_string(&lib).unwrap();
        let broken = exp.replacen("final(self)", "self", 1);
        assert_ne!(broken, exp);
        std::fs::write(&lib, broken).unwrap();
        let toml = std::fs::read_to_string(verus.join("Cargo.toml")).unwrap();
        let broken = toml.replacen("panic = \"abort\"\n", "", 1);
        assert_ne!(broken, toml);
        std::fs::write(verus.join("Cargo.toml"), broken).unwrap();
        std::fs::write(verus.join("src/stale.rs"), "fn f() {}\n").unwrap();
        let lock = std::fs::read(verus.join("Cargo.lock")).unwrap();
        assert!(compare_lowered(dir).is_err());

        let b = golden_lowered::bless(dir).unwrap();
        assert_eq!(b.written, ["src/lib.rs", "Cargo.toml"]);
        assert_eq!(b.removed, ["src/stale.rs"]);
        assert!(!verus.join("src/stale.rs").exists());
        assert_eq!(std::fs::read(verus.join("Cargo.lock")).unwrap(), lock);
        compare_lowered(dir).unwrap();
        assert_eq!(
            golden_lowered::bless(dir).unwrap(),
            golden_lowered::Blessed::default()
        );
    });
}

/// 期待に手で書いたコメントがあれば、作り直しは何も書かずに失敗する（コメントを消さない）。
#[test]
fn fails_to_bless_when_it_would_erase_comments() {
    let cases: [(&str, &str, &str, &str); 6] = [
        (
            "src/lib.rs",
            "verus! {\n",
            "verus! {\n// 手で書いた説明\n",
            "// 手で書いた説明",
        ),
        // 生成物にあるコメントに書き足したもの（印の数は変わらない）。
        (
            "src/lib.rs",
            "} // verus!",
            "} // verus! 手で書いた説明",
            "// verus! 手で書いた説明",
        ),
        // 生成物にあるコメントと同じ本文を、もう 1 つ足したもの。
        (
            "src/lib.rs",
            "verus! {\n",
            "verus! {\n// verus!\n",
            "// verus!",
        ),
        (
            "src/lib.rs",
            "verus! {\n",
            "verus! {\n/* 手で書いた説明 */\n",
            "/* 手で書いた説明 */",
        ),
        (
            "Cargo.toml",
            "[workspace]\n",
            "# 手で書いた説明\n[workspace]\n",
            "# 手で書いた説明",
        ),
        // 生成物にないので消すことになる `.rs` のコメント。
        (
            "src/stale.rs",
            "",
            "// 手で書いた説明\n",
            "// 手で書いた説明",
        ),
    ];
    for (i, (rel, from, to, comment)) in cases.into_iter().enumerate() {
        with_copy("stack", &format!("comment{i}"), |dir| {
            let p = dir.join("expected/verus").join(rel);
            let before = std::fs::read_to_string(&p).unwrap_or_default();
            let edited = before.replacen(from, to, 1);
            assert_ne!(edited, before, "{rel}");
            // 壊した期待でも作り直しが止まることを見るため、トークンも 1 つ壊す。
            let lib = dir.join("expected/verus/src/lib.rs");
            std::fs::write(&p, &edited).unwrap();
            let lib_text = std::fs::read_to_string(&lib).unwrap();
            let broken = lib_text.replacen("final(self)", "self", 1);
            assert_ne!(broken, lib_text);
            std::fs::write(&lib, &broken).unwrap();
            let edited = std::fs::read_to_string(&p).unwrap();

            let err = golden_lowered::bless(dir).unwrap_err();
            assert!(err.contains("コメントが消える"), "{rel}: {err}");
            // 消える行を、位置と本文で挙げる。
            assert!(
                err.lines()
                    .any(|l| l.starts_with(&format!("  expected/verus/{rel}:"))
                        && l.ends_with(comment)),
                "{rel}: {err}"
            );
            // 何も書いていない。
            assert_eq!(std::fs::read_to_string(&p).unwrap(), edited, "{rel}");
            assert_eq!(std::fs::read_to_string(&lib).unwrap(), broken, "{rel}");
        });
    }
}
