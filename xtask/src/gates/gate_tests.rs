//! `check-gate-tests`：各ゲートが、失敗することを主張するテスト（`fails_` で始まる `#[test]`）を持つ。
//!
//! 通ったゲートは、失敗できると分かるまで何も意味しない。

use super::{Repo, Report};

const DIR: &str = "xtask/src/gates/";

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    let mod_rs = format!("{DIR}mod.rs");
    let registry = match repo.read(&mod_rs) {
        Ok(t) => t,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    let gates: Vec<&String> = repo
        .files
        .iter()
        .filter(|f| {
            f.strip_prefix(DIR)
                .is_some_and(|n| n.ends_with(".rs") && !n.contains('/') && n != "mod.rs")
        })
        .collect();
    if gates.is_empty() {
        r.errors.push(format!("{DIR} にゲートがありません"));
    }
    for f in gates {
        let name = &f[DIR.len()..f.len() - 3];
        let decl = [format!("pub mod {name};"), format!("mod {name};")];
        if !registry.lines().any(|l| decl.iter().any(|d| l.trim() == d)) {
            r.errors.push(format!(
                "{f}: {mod_rs} に `pub mod {name};` がありません（テストがビルドされない）"
            ));
        }
        match repo.read(f) {
            Err(e) => r.errors.push(e),
            Ok(text) => match failing_tests(&text) {
                0 => r.errors.push(format!(
                    "{f}: 入力を壊すと失敗することを主張するテスト（`#[test] fn fails_..`）がありません"
                )),
                n => r.notes.push(format!("{name}: fails_ テスト {n}")),
            },
        }
    }
    r
}

/// `#[ignore]` の付いていない `#[test] fn fails_..` の数。
fn failing_tests(text: &str) -> usize {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut n = 0;
    for (i, l) in lines.iter().enumerate() {
        if *l != "#[test]" {
            continue;
        }
        let mut ignored = false;
        for next in &lines[i + 1..] {
            if next.starts_with("#[") {
                ignored |= next.starts_with("#[ignore");
                continue;
            }
            if next.starts_with("fn fails_") && !ignored {
                n += 1;
            }
            break;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const GATE: &str = "xtask/src/gates/file_size.rs";

    fn gates(gate: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write("xtask/src/gates/mod.rs", &real("xtask/src/gates/mod.rs"));
        t.write(GATE, gate);
        t
    }

    #[test]
    fn passes_on_a_real_gate() {
        assert_passes(&gates(&real(GATE)).run(run));
    }

    #[test]
    fn fails_when_a_gate_has_no_failing_test() {
        let broken = real(GATE).replace("fn fails_", "fn checks_");
        assert_ne!(broken, real(GATE), "壊す操作が当たらない");
        assert_fails(
            &gates(&broken).run(run),
            "file_size.rs: 入力を壊すと失敗する",
        );
    }

    #[test]
    fn fails_when_the_failing_tests_are_ignored() {
        let broken = real(GATE).replace(
            "#[test]\n    fn fails_",
            "#[test]\n    #[ignore]\n    fn fails_",
        );
        assert_ne!(broken, real(GATE), "壊す操作が当たらない");
        assert_fails(
            &gates(&broken).run(run),
            "file_size.rs: 入力を壊すと失敗する",
        );
    }

    #[test]
    fn fails_when_a_gate_is_not_registered() {
        let mut t = gates(&real(GATE));
        t.write("xtask/src/gates/unregistered.rs", &real(GATE));
        assert_fails(&t.run(run), "`pub mod unregistered;` がありません");
    }

    #[test]
    fn fails_when_there_are_no_gates() {
        let mut t = TempRepo::new();
        t.write("xtask/src/gates/mod.rs", &real("xtask/src/gates/mod.rs"));
        assert_fails(&t.run(run), "ゲートがありません");
    }
}
