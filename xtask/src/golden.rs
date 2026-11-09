//! `golden-verify`：golden のケース（`tests/golden/<名前>/`）のうち `expected/status.txt` があるものに
//! `ebykn verify` をかけ、出力を `expected/status.txt` と比べる（仕様 §11 の (3)）。続けて、書き出した
//! `build/verus/` を Verus なしの cargo と `ebykn build` でビルドする（OPEN-11、#80）。どちらも成功を期待し、
//! `expected/build-error.txt` があるケースだけは、両方が失敗し、その各行がビルドの出力に含まれることを
//! 期待する（検証は通りビルドで止まるもの。#12）。`expected/ebykn-build-error.txt` があるケースは、`ebykn build`
//! だけがその行で失敗することを期待する（`check` で止まるもの）。最後に、Verus が要るので `#[ignore]` にした
//! 結合テスト（[`IGNORED`]）を `--ignored` で走らせ、どれも実際に走って通ったことを確かめる。
//!
//! Verus が要るので `gates` には入れず、CI の Verus のジョブで走らせる。依存の版は、ケースの
//! `expected/verus/Cargo.lock` を `build/verus/` に写して固定する。

use std::path::Path;
use std::process::Command;

/// `ebykn verify` の出力と終了コードを、期待（`expected/status.txt`）と比べる。
///
/// 出力は期待と行ごとに等しく、終了コードは「failed / unknown の行がなく、エラーが 0 件」のときだけ成功。
pub fn judge(expected: &str, stdout: &str, success: bool) -> Result<(), String> {
    if expected.trim().is_empty() {
        return Err("expected/status.txt が空です".into());
    }
    if expected != stdout {
        return Err(format!(
            "ebykn verify の出力が expected/status.txt と違います\n--- 期待\n{expected}--- 実際\n{stdout}"
        ));
    }
    let failed = expected
        .lines()
        .any(|l| l.starts_with("failed ") || l.starts_with("unknown "));
    let errors = expected
        .lines()
        .find_map(|l| l.strip_prefix("results "))
        .and_then(|r| r.split_once(" verified, "))
        .and_then(|(_, e)| e.strip_suffix(" errors"))
        .and_then(|e| e.parse::<u64>().ok());
    // 検証の前に止まった（`results not verified: ..`）なら、検証は成功していない。
    let stopped = expected
        .lines()
        .any(|l| l.starts_with("results not verified"));
    let errors = if stopped { Some(1) } else { errors };
    let Some(errors) = errors else {
        return Err("expected/status.txt に results の行がありません".into());
    };
    let should_succeed = !failed && errors == 0;
    if success != should_succeed {
        return Err(format!(
            "ebykn verify の終了コードが期待と違います（成功 {success}、期待 {should_succeed}）"
        ));
    }
    Ok(())
}

/// Verus が要るので `#[ignore]` にしたテスト（パッケージ、テストのターゲット、テストの名前）。
pub const IGNORED: &[(&str, &str, &str)] = &[
    ("ebykn", "no_cheating", "smuggled_assumptions_fail_verify"),
    (
        "ebykn",
        "target_dir",
        "verify_ignores_the_parent_target_dir",
    ),
];

/// `cargo test` の出力が、ちょうど一つのテストが走って通ったことを示すか。0 件で通ったもの（名前の誤り
/// など）を通さない。
pub fn ran_one(stdout: &str) -> bool {
    stdout
        .lines()
        .any(|l| l.starts_with("test result: ok. 1 passed;"))
}

fn run_ignored(root: &Path) -> bool {
    let mut ok = true;
    for (package, target, name) in IGNORED {
        eprintln!("$ cargo test -p {package} --test {target} -- --ignored --exact {name}");
        let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args([
                "test",
                "-p",
                package,
                "--test",
                target,
                "--",
                "--ignored",
                "--exact",
                name,
            ])
            .current_dir(root)
            .output();
        match out {
            Ok(o) if o.status.success() && ran_one(&String::from_utf8_lossy(&o.stdout)) => {
                eprintln!("ok: golden-verify: {target}::{name}")
            }
            Ok(o) => {
                eprint!("{}", String::from_utf8_lossy(&o.stdout));
                eprint!("{}", String::from_utf8_lossy(&o.stderr));
                eprintln!(
                    "error: golden-verify: {target}::{name} が通りません（1 件走って通る必要があります）"
                );
                ok = false;
            }
            Err(e) => {
                eprintln!("error: golden-verify: cargo test を起動できません: {e}");
                ok = false;
            }
        }
    }
    ok
}

/// golden-verify が `ebykn verify` の後に走らせるビルド。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    /// 書き出した `build/verus/` を Verus なしの cargo でビルドする（OPEN-11）。
    Plain,
    /// `ebykn build`（`cargo verus build`）。prelude を別の依存クレートとして通す、利用者の経路。
    Ebykn,
}

impl Build {
    fn name(self) -> &'static str {
        match self {
            Build::Plain => "Verus なしの cargo build",
            Build::Ebykn => "ebykn build",
        }
    }
}

/// 走らせるビルド。どのケースも、素の cargo と `ebykn build` の両方を走らせる。
///
/// - `ebykn build`（`--verified` なし）は、検証に失敗した項目があってもビルドする（#80）。`failed` の
///   項目を持つケース（`stack` など）で、`ebykn build` が検証の失敗で止まれば気づける。
/// - 素のビルドとプローブ（単一のクレート）だけでは、Verus の版を上げて `cargo verus build` が
///   prelude の検証用のコードを公開するようになったとき、利用者の経路でコンパイル時の検査が効かなく
///   なっても気づけない（#12、PR #47 のレビュー）。
pub fn builds() -> Vec<Build> {
    vec![Build::Plain, Build::Ebykn]
}

/// 期待のファイル（なければ `None`）。
fn read_expected(dir: &Path, file: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(dir.join("expected").join(file)) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("expected/{file} を読めません: {e}")),
    }
}

/// `ebykn verify` の後に走らせるビルドと、それぞれの期待（失敗の行。`None` なら成功を期待する）。
/// `verify` はこれだけを使う（テストが実在のケースで固定する）。
///
/// - 素の cargo は `expected/build-error.txt`。
/// - `ebykn build` は `expected/ebykn-build-error.txt` があればそれ、なければ `expected/build-error.txt`。
///   `ebykn build` は `check`（`verus --no-verify`）を含むので、Verus が受け付けないコードで止まる
///   （`stopped`）。素の cargo はそれをビルドできる。
pub fn case_builds(dir: &Path) -> Result<Vec<(Build, Option<String>)>, String> {
    let plain = read_expected(dir, "build-error.txt")?;
    let ebykn = read_expected(dir, "ebykn-build-error.txt")?.or_else(|| plain.clone());
    Ok(builds()
        .into_iter()
        .map(|b| match b {
            Build::Plain => (b, plain.clone()),
            Build::Ebykn => (b, ebykn.clone()),
        })
        .collect())
}

/// ビルド `what` の結果を、期待（`expected/build-error.txt` か `expected/ebykn-build-error.txt` の行。
/// なければ成功を期待する）と比べる。
pub fn judge_build(
    what: &str,
    expected: Option<&str>,
    success: bool,
    output: &str,
) -> Result<(), String> {
    let Some(expected) = expected else {
        return if success {
            Ok(())
        } else {
            Err(format!("{what} が失敗しました\n{output}"))
        };
    };
    let lines: Vec<&str> = expected.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Err(
            "期待の失敗の行（expected/build-error.txt か expected/ebykn-build-error.txt）が空です"
                .into(),
        );
    }
    if success {
        return Err(format!(
            "{what} が失敗するはずが、通りました（expected/build-error.txt か expected/ebykn-build-error.txt）"
        ));
    }
    let missing: Vec<&str> = lines.into_iter().filter(|l| !output.contains(l)).collect();
    if !missing.is_empty() {
        return Err(format!(
            "{what} の出力に、期待の失敗（expected/build-error.txt か expected/ebykn-build-error.txt）の行がありません: {missing:?}\n{output}"
        ));
    }
    Ok(())
}

/// 書き出したクレートを Verus なしでビルドする cargo の引数（後ろに target-dir が続く）。
/// 出力を `expected/build-error.txt` と比べるので、色を付けない（CI は `CARGO_TERM_COLOR=always`）。
const PLAIN_BUILD: &[&str] = &[
    "build",
    "--quiet",
    "--locked",
    "--color",
    "never",
    "--target-dir",
];

pub fn verify(root: &Path, files: &[String]) -> bool {
    if !crate::cargo(&["build", "--quiet", "-p", "ebykn"]) {
        return false;
    }
    let ebykn = root.join("target/debug/ebykn");
    let cases: Vec<&str> = files
        .iter()
        .filter_map(|f| f.strip_suffix("/expected/status.txt"))
        .filter(|d| d.starts_with("tests/golden/"))
        .collect();
    if cases.is_empty() {
        eprintln!("error: golden-verify: expected/status.txt のあるケースがありません");
        return false;
    }
    let mut ok = true;
    for case in cases {
        let dir = root.join(case);
        let lock = dir.join("expected/verus/Cargo.lock");
        let build = dir.join("build/verus");
        if lock.is_file() {
            let copied = std::fs::create_dir_all(&build)
                .and_then(|_| std::fs::copy(&lock, build.join("Cargo.lock")));
            if let Err(e) = copied {
                eprintln!("error: golden-verify: {case}: Cargo.lock を写せません: {e}");
                ok = false;
                continue;
            }
        }
        let expected = match std::fs::read_to_string(dir.join("expected/status.txt")) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: golden-verify: {case}: {e}");
                ok = false;
                continue;
            }
        };
        eprintln!("$ (cd {case} && ebykn verify)");
        let mut cmd = Command::new(&ebykn);
        for v in [
            "CARGO",
            "RUSTC",
            "RUSTDOC",
            "RUSTUP_TOOLCHAIN",
            "CARGO_TARGET_DIR",
        ] {
            cmd.env_remove(v);
        }
        let out = match cmd.arg("verify").current_dir(&dir).output() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("error: golden-verify: ebykn を起動できません: {e}");
                return false;
            }
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        match judge(&expected, &stdout, out.status.success()) {
            Ok(()) => eprintln!("ok: golden-verify: {case}"),
            Err(e) => {
                eprint!("{}", String::from_utf8_lossy(&out.stderr));
                eprintln!("error: golden-verify: {case}: {e}");
                ok = false;
                continue;
            }
        }
        let plan = match case_builds(&dir) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("error: golden-verify: {case}: {e}");
                ok = false;
                continue;
            }
        };
        for (b, build_error) in plan {
            let mut cmd = match b {
                // 書き出したクレートを Verus なしの cargo（リポジトリの toolchain）でもビルドする（OPEN-11）。
                Build::Plain => {
                    let mut c = Command::new("cargo");
                    c.args(PLAIN_BUILD)
                        .arg(root.join("target/verus-plain"))
                        .current_dir(&build);
                    c
                }
                // 利用者が実際に使う経路。prelude は別の依存クレートとして `cargo verus build` を通る（#12）。
                Build::Ebykn => {
                    let mut c = Command::new(&ebykn);
                    c.arg("build").current_dir(&dir);
                    c
                }
            };
            for v in [
                "CARGO",
                "RUSTC",
                "RUSTDOC",
                "RUSTUP_TOOLCHAIN",
                "CARGO_TARGET_DIR",
            ] {
                cmd.env_remove(v);
            }
            let out = match cmd.output() {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("error: golden-verify: {} を起動できません: {e}", b.name());
                    return false;
                }
            };
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            match judge_build(
                b.name(),
                build_error.as_deref(),
                out.status.success(),
                &text,
            ) {
                Ok(()) if build_error.is_some() => {
                    eprintln!("ok: golden-verify: {case}: {} が期待どおりに失敗", b.name())
                }
                Ok(()) => eprintln!("ok: golden-verify: {case}: {}", b.name()),
                Err(e) => {
                    eprintln!("error: golden-verify: {case}: {e}");
                    ok = false;
                }
            }
        }
    }
    // 普通のケースの後に走らせる（smuggled のケースの build/verus/ を、差し込んだもので書き直すため）。
    run_ignored(root) && ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const STATUS: &str = "tests/golden/stack/expected/status.txt";

    #[test]
    fn passes_when_the_output_matches() {
        let s = real(STATUS);
        // stack は push / pop が通らない（M3 で .eykp を足す）ので、終了コードは失敗。
        assert_eq!(judge(&s, &s, false), Ok(()));
    }

    #[test]
    fn fails_when_the_output_differs() {
        let s = real(STATUS);
        let broken = mutate(
            &s,
            "failed stack::Stack::push",
            "verified stack::Stack::push",
        );
        assert!(judge(&s, &broken, false).is_err());
    }

    #[test]
    fn fails_when_the_exit_code_disagrees() {
        let s = real(STATUS);
        assert!(judge(&s, &s, true).unwrap_err().contains("終了コード"));
        let ok = "verified k::f\nresults 1 verified, 0 errors\n";
        assert!(judge(ok, ok, false).is_err());
        assert_eq!(judge(ok, ok, true), Ok(()));
    }

    #[test]
    fn fails_when_unknown_items_exit_successfully() {
        let s = "unknown k::f\nresults 0 verified, 0 errors\n";
        assert!(judge(s, s, true).is_err());
        assert_eq!(judge(s, s, false), Ok(()));
    }

    #[test]
    fn fails_when_stopped_runs_exit_successfully() {
        let s =
            "unknown k::f\nresults not verified: Verus stopped before verification (1 errors)\n";
        assert!(judge(s, s, true).is_err());
        assert_eq!(judge(s, s, false), Ok(()));
    }

    const BUILD_ERROR: &str = "tests/golden/ptr_zst_build/expected/build-error.txt";

    #[test]
    fn build_passes_when_it_fails_as_expected() {
        let e = real(BUILD_ERROR);
        let out = format!("{e}\nerror: could not compile\n");
        for b in [Build::Plain, Build::Ebykn] {
            assert_eq!(judge_build(b.name(), Some(&e), false, &out), Ok(()));
        }
        assert_eq!(judge_build(Build::Plain.name(), None, true, ""), Ok(()));
    }

    #[test]
    fn plain_build_output_has_no_color_whatever_the_environment() {
        // 環境の CARGO_TERM_COLOR より、コマンドラインの --color が優先する。
        let at = PLAIN_BUILD.iter().position(|a| *a == "--color").unwrap();
        assert_eq!(PLAIN_BUILD[at + 1], "never");
        assert_eq!(PLAIN_BUILD.last(), Some(&"--target-dir"));
    }

    #[test]
    fn fails_unless_ebykn_build_runs_for_every_case() {
        // どのケースも、利用者の経路（ebykn build）を走らせる。
        assert_eq!(builds(), [Build::Plain, Build::Ebykn]);
        // verify が使う case_builds を、実在のケースで確かめる。
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/golden");
        let plan = case_builds(&golden.join("ptr_zst_build")).unwrap();
        assert_eq!(plan.len(), 2);
        for (b, expected) in &plan {
            assert!(
                expected.as_deref().unwrap().contains("error[E0080]"),
                "{b:?}"
            );
        }
        // `failed` の項目があるケースでも ebykn build を走らせ、成功を期待する（#80）。
        let stack = golden.join("stack");
        let status = std::fs::read_to_string(stack.join("expected/status.txt")).unwrap();
        assert!(status.lines().any(|l| l.starts_with("failed ")), "{status}");
        assert_eq!(
            case_builds(&stack).unwrap(),
            [(Build::Plain, None), (Build::Ebykn, None)]
        );
        // `ebykn build` だけが止まるケースは、`ebykn build` にだけ失敗を期待する。
        let plan = case_builds(&golden.join("stopped")).unwrap();
        assert_eq!(plan[0], (Build::Plain, None));
        assert_eq!(plan[1].0, Build::Ebykn);
        assert!(
            plan[1].1.as_deref().unwrap().contains("is not supported"),
            "{plan:?}"
        );
    }

    #[test]
    fn fails_when_ebykn_build_fails_on_a_case_that_should_build() {
        // 検証の失敗で ebykn build が止まった出力（#80 の症状）は、期待（成功）と合わない。
        let out = "error: postcondition not satisfied\nerror: could not compile `stack` (lib) due to 2 previous errors\n";
        let err = judge_build(Build::Ebykn.name(), None, false, out).unwrap_err();
        assert!(err.contains("ebykn build が失敗しました"), "{err}");
        assert!(judge_build(Build::Ebykn.name(), None, true, "").is_ok());
    }

    #[test]
    fn fails_when_ebykn_build_succeeds_or_fails_differently() {
        // 素のビルドが期待どおりに失敗しても、ebykn build が通れば（Verus の版を上げて
        // 検証用のコードが公開されるようになった場合）失敗にする。
        let e = real(BUILD_ERROR);
        let name = Build::Ebykn.name();
        let err = judge_build(name, Some(&e), true, "").unwrap_err();
        assert!(
            err.contains("ebykn build") && err.contains("通りました"),
            "{err}"
        );
        let other = mutate(&e, "error[E0080]", "error[E0308]");
        let err = judge_build(name, Some(&e), false, &other).unwrap_err();
        assert!(
            err.contains("ebykn build") && err.contains("行がありません"),
            "{err}"
        );
    }

    #[test]
    fn fails_when_a_build_expected_to_fail_succeeds() {
        let e = real(BUILD_ERROR);
        let err = judge_build(Build::Plain.name(), Some(&e), true, "").unwrap_err();
        assert!(err.contains("通りました"), "{err}");
    }

    #[test]
    fn fails_when_the_build_fails_differently() {
        let e = real(BUILD_ERROR);
        let plain = Build::Plain.name();
        let other = mutate(&e, "error[E0080]", "error[E0308]");
        let err = judge_build(plain, Some(&e), false, &other).unwrap_err();
        assert!(err.contains("行がありません"), "{err}");
        let err = judge_build(plain, None, false, &e).unwrap_err();
        assert!(err.contains("失敗しました"), "{err}");
        let err = judge_build(plain, Some("\n"), false, &e).unwrap_err();
        assert!(err.contains("空です"), "{err}");
    }

    #[test]
    fn fails_when_no_ignored_test_ran() {
        let one = "running 1 test\ntest smuggled_assumptions_fail_verify ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.97s\n";
        assert!(ran_one(one));
        let none = mutate(one, "ok. 1 passed", "ok. 0 passed");
        assert!(!ran_one(&none));
        let failed = mutate(one, "ok. 1 passed; 0 failed", "FAILED. 0 passed; 1 failed");
        assert!(!ran_one(&failed));
    }

    /// 追跡している `.rs`（`vendor/` を除く）の、パスと中身。
    fn tracked_sources() -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        crate::list_files(root)
            .unwrap()
            .into_iter()
            .filter(|f| f.ends_with(".rs") && !f.starts_with("vendor/"))
            .map(|f| {
                let text = real(&f);
                (f, text)
            })
            .collect()
    }

    /// `#[ignore]` の付いたテストを（パッケージ、テストのターゲット、テストの名前）で集める。
    ///
    /// `IGNORED` は `cargo test -p <パッケージ> --test <ターゲット>` で走らせるので、走らせられるのは
    /// `crates/<パッケージ>/tests/<ターゲット>.rs` の中のものだけ。ほかの場所（`src/` の単体テスト、xtask、
    /// `tests/` の下のディレクトリ）の `#[ignore]`、後に `fn` を読めない `#[ignore]`、条件つきの
    /// `#[cfg_attr(.., ignore)]` は誤りにする。
    fn ignored_tests(files: &[(String, String)]) -> Result<Vec<(String, String, String)>, String> {
        let mut found = Vec::new();
        let mut errors = Vec::new();
        for (path, text) in files {
            let place = path
                .strip_prefix("crates/")
                .and_then(|p| p.split_once("/tests/"))
                .and_then(|(pkg, t)| Some((pkg, t.strip_suffix(".rs")?)))
                .filter(|(pkg, t)| !pkg.contains('/') && !t.contains('/'));
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                let t = l.trim_start();
                let at = format!("{path}:{}", i + 1);
                // 条件つきの ignore は、どの条件で走らないかを IGNORED から決められない。
                if t.starts_with("#[cfg_attr(") && t.contains("ignore") {
                    errors.push(format!(
                        "{at}: #[cfg_attr(..)] の ignore は IGNORED で扱えない"
                    ));
                    continue;
                }
                if !t.starts_with("#[ignore") {
                    continue;
                }
                let Some((pkg, target)) = place else {
                    errors.push(format!("{at}: IGNORED から走らせられない場所の #[ignore]"));
                    continue;
                };
                // 後に続く属性・コメント・空行を飛ばした最初の行が `fn 名前(` であること。
                let name = lines[i + 1..]
                    .iter()
                    .map(|l| l.trim_start())
                    .find(|l| !(l.is_empty() || l.starts_with("#[") || l.starts_with("//")))
                    .and_then(|l| l.strip_prefix("fn "))
                    .and_then(|r| r.split_once('('))
                    .map(|(n, _)| n.trim());
                match name {
                    Some(n) if !n.is_empty() => {
                        found.push((pkg.to_string(), target.to_string(), n.to_string()))
                    }
                    _ => errors.push(format!("{at}: #[ignore] の後に fn 名前( を読めない")),
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        found.sort();
        Ok(found)
    }

    fn listed() -> Vec<(String, String, String)> {
        let mut v: Vec<_> = IGNORED
            .iter()
            .map(|(p, t, n)| (p.to_string(), t.to_string(), n.to_string()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn every_ignored_test_is_listed() {
        // IGNORED にない `#[ignore]` のテストは、どこでも走らない。IGNORED にあって `#[ignore]` のない
        // テストは、golden-verify が 0 件で通さないが、ここでも食い違いとして捕まえる。
        let files = tracked_sources();
        assert!(
            files
                .iter()
                .any(|(f, _)| f == "crates/ebykn/tests/no_cheating.rs")
        );
        assert_eq!(ignored_tests(&files), Ok(listed()));
    }

    #[test]
    fn fails_when_an_ignored_test_is_missing_from_the_list() {
        let files = tracked_sources();
        let with = |path: &str, edit: &dyn Fn(&str) -> String| -> Vec<(String, String)> {
            let mut f = files.clone();
            let e = f.iter_mut().find(|(p, _)| p == path).expect(path);
            e.1 = edit(&e.1);
            f
        };
        let forgotten = "#[test]\n#[ignore = \"x\"]\nfn forgotten_elsewhere() {}\n\n#[test]";
        // ほかのパッケージの結合テスト：集めるが IGNORED にない。
        let other = with("crates/ebykn_check/tests/project.rs", &|t| {
            mutate(t, "#[test]", forgotten)
        });
        let got = ignored_tests(&other).unwrap();
        assert!(
            got.contains(&(
                "ebykn_check".into(),
                "project".into(),
                "forgotten_elsewhere".into()
            )),
            "{got:?}"
        );
        assert_ne!(got, listed());
        // パッケージの列も比べる（同じターゲットと名前でも、別のパッケージなら食い違い）。
        let mut moved = files.clone();
        let e = moved
            .iter_mut()
            .find(|(p, _)| p == "crates/ebykn/tests/target_dir.rs")
            .unwrap();
        e.0 = "crates/ebykn_check/tests/target_dir.rs".into();
        assert_ne!(ignored_tests(&moved).unwrap(), listed());
        // 単体テストの #[ignore]：IGNORED から走らせられない。
        let unit = with("crates/ebykn/src/verus.rs", &|t| {
            mutate(t, "\n    #[test]\n", "\n    #[test]\n    #[ignore]\n")
        });
        let e = ignored_tests(&unit).unwrap_err();
        assert!(e.contains("crates/ebykn/src/verus.rs:"), "{e}");
        // tests/ の下のディレクトリと xtask の #[ignore] も同じ。
        let support = with("crates/ebykn_check/tests/support/mod.rs", &|t| {
            format!("{t}\n#[test]\n#[ignore]\nfn hidden() {{}}\n")
        });
        assert!(ignored_tests(&support).is_err());
        let xtask = with("xtask/src/golden.rs", &|t| {
            format!("{t}\n#[test]\n#[ignore]\nfn hidden() {{}}\n")
        });
        assert!(ignored_tests(&xtask).is_err());
        // 条件つきの ignore。
        let cfg = with("crates/ebykn_check/tests/zst.rs", &|t| {
            mutate(t, "#[test]", "#[test]\n#[cfg_attr(unix, ignore)]")
        });
        let e = ignored_tests(&cfg).unwrap_err();
        assert!(e.contains("zst.rs:") && e.contains("cfg_attr"), "{e}");
        // 後に fn を読めない #[ignore]。
        let unreadable = with("crates/ebykn/tests/no_cheating.rs", &|t| {
            mutate(
                t,
                "fn smuggled_assumptions_fail_verify()",
                "async fn smuggled_assumptions_fail_verify()",
            )
        });
        let e = ignored_tests(&unreadable).unwrap_err();
        assert!(e.contains("no_cheating.rs:"), "{e}");
    }

    #[test]
    fn fails_when_the_expectation_is_unreadable() {
        assert!(judge("", "", true).is_err());
        assert!(judge("verified k::f\n", "verified k::f\n", true).is_err());
    }
}
