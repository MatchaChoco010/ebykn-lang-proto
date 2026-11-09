//! `golden-verify`：golden のケース（`tests/golden/<名前>/`）のうち `expected/status.txt` があるものに
//! `ebykn verify` をかけ、出力を `expected/status.txt` と比べる（仕様 §11 の (3)）。続けて、書き出した
//! `build/verus/` を Verus なしの cargo でもビルドする（OPEN-11）。`expected/build-error.txt` があるケースは、
//! 素の cargo のビルドと `ebykn build` が失敗し、その各行がビルドの出力に含まれることを期待する（検証は通り
//! ビルドで止まるもの。#12）。最後に、Verus が要るので `#[ignore]` にしたテスト（[`IGNORED`]。#37 の
//! `--no-cheating` の照合の再現）を `--ignored` で走らせ、どれも実際に走って通ったことを確かめる。
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
pub const IGNORED: &[(&str, &str, &str)] =
    &[("ebykn", "no_cheating", "smuggled_assumptions_fail_verify")];

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

/// 走らせるビルド。`expected/build-error.txt` のあるケースは、素の cargo に加えて `ebykn build` も
/// 同じ行で失敗することを確かめる。素のビルドとプローブ（単一のクレート）だけでは、Verus の版を上げて
/// `cargo verus build` が prelude の検証用のコードを公開するようになったとき、利用者の経路で
/// コンパイル時の検査が効かなくなっても気づけない（#12、PR #47 のレビュー）。
pub fn builds(expects_error: bool) -> Vec<Build> {
    if expects_error {
        vec![Build::Plain, Build::Ebykn]
    } else {
        vec![Build::Plain]
    }
}

/// ケースの期待（`expected/build-error.txt`。なければ `None`）と、`ebykn verify` の後に走らせるビルド。
/// `verify` はこれだけを使う（テストが実在のケースで固定する）。
pub fn case_builds(dir: &Path) -> Result<(Option<String>, Vec<Build>), String> {
    let build_error = match std::fs::read_to_string(dir.join("expected/build-error.txt")) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("expected/build-error.txt を読めません: {e}")),
    };
    let plan = builds(build_error.is_some());
    Ok((build_error, plan))
}

/// ビルド `what` の結果を、期待（`expected/build-error.txt`。なければ成功を期待する）と比べる。
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
        return Err("expected/build-error.txt が空です".into());
    }
    if success {
        return Err(format!(
            "{what} が失敗するはずが、通りました（expected/build-error.txt）"
        ));
    }
    let missing: Vec<&str> = lines.into_iter().filter(|l| !output.contains(l)).collect();
    if !missing.is_empty() {
        return Err(format!(
            "{what} の出力に expected/build-error.txt の行がありません: {missing:?}\n{output}"
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
        let (build_error, plan) = match case_builds(&dir) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("error: golden-verify: {case}: {e}");
                ok = false;
                continue;
            }
        };
        for b in plan {
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
    fn fails_unless_ebykn_build_runs_for_cases_that_expect_a_build_error() {
        // build-error.txt のあるケースは、利用者の経路（ebykn build）でも失敗を確かめる。
        assert_eq!(builds(true), [Build::Plain, Build::Ebykn]);
        assert_eq!(builds(false), [Build::Plain]);
        // verify が使う case_builds を、実在のケースで確かめる。
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/golden");
        let (expected, plan) = case_builds(&golden.join("ptr_zst_build")).unwrap();
        assert!(expected.unwrap().contains("error[E0080]"));
        assert_eq!(plan, [Build::Plain, Build::Ebykn]);
        let (expected, plan) = case_builds(&golden.join("stack")).unwrap();
        assert!(expected.is_none());
        assert_eq!(plan, [Build::Plain]);
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

    #[test]
    fn ignored_tests_exist() {
        for (_, target, name) in IGNORED {
            let src = real(&format!("crates/ebykn/tests/{target}.rs"));
            assert!(src.contains(&format!("fn {name}()")), "{target}::{name}");
            assert!(src.contains("#[ignore"), "{target}");
        }
    }

    #[test]
    fn fails_when_the_expectation_is_unreadable() {
        assert!(judge("", "", true).is_err());
        assert!(judge("verified k::f\n", "verified k::f\n", true).is_err());
    }
}
