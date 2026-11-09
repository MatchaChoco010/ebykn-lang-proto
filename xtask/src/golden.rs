//! `golden-verify`：golden のケース（`tests/golden/<名前>/`）のうち `expected/status.txt` があるものに
//! `ebykn verify` をかけ、出力を `expected/status.txt` と比べる（仕様 §11 の (3)）。続けて、書き出した
//! `build/verus/` を Verus なしの cargo でもビルドする（OPEN-11）。最後に、Verus が要るので `#[ignore]` にした
//! テスト（[`IGNORED`]。#37 の `--no-cheating` の照合の再現）を `--ignored` で走らせ、どれも実際に走って
//! 通ったことを確かめる。
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
        // 書き出したクレートを Verus なしの cargo（リポジトリの toolchain）でもビルドする（OPEN-11）。
        let mut plain = Command::new("cargo");
        for v in [
            "CARGO",
            "RUSTC",
            "RUSTDOC",
            "RUSTUP_TOOLCHAIN",
            "CARGO_TARGET_DIR",
        ] {
            plain.env_remove(v);
        }
        plain
            .args(["build", "--quiet", "--locked", "--target-dir"])
            .arg(root.join("target/verus-plain"))
            .current_dir(&build);
        match plain.status() {
            Ok(s) if s.success() => {
                eprintln!("ok: golden-verify: {case}: Verus なしの cargo build")
            }
            other => {
                eprintln!(
                    "error: golden-verify: {case}: Verus なしでビルドできません（{other:?}）"
                );
                ok = false;
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
