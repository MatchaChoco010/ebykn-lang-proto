//! `check-verus-pin`：固定した Verus の値が読め、Verus のクレートが固定した vstd を使い、
//! すべて `verus-verify` の対象になっている。
//!
//! Verus は実行しない。次を検査する。
//! - 固定値（ルートの `Cargo.toml` の `[workspace.metadata.verus]`）が読める。
//! - `vstd` に依存する `Cargo.toml` は、`vstd = "<固定した版>"` の形で書いてある（ほかの形は読まずに失敗にする）。
//! - `vstd` に依存するクレートは、すべて `CRATES`（`verus-verify` が検証する一覧）に載っているか、
//!   golden の期待（`tests/golden/<名前>/expected/verus`）で、同じケースに `expected/status.txt` がある
//!   （`golden-verify` が `ebykn verify` の結果と比べる）。
//! - `CRATES` と golden の期待の各クレートは `Cargo.toml` と `Cargo.lock` を追跡し、`Cargo.lock` の vstd が固定した版。

use super::{Repo, Report};
use crate::verus::{CRATES, read_pin};

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    let pin = match repo.read("Cargo.toml").and_then(|t| read_pin(&t)) {
        Ok(p) => p,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    r.notes.push(format!(
        "{}（{}）、toolchain {}、vstd {}",
        pin.release, pin.commit, pin.toolchain, pin.vstd
    ));
    let locked = pin.vstd.trim_start_matches('=');
    let golden: Vec<String> = golden_crates(repo);
    for krate in golden.iter() {
        let case = krate.strip_suffix("/expected/verus").unwrap_or_default();
        if !repo.has(&format!("{case}/expected/status.txt")) {
            r.errors.push(format!(
                "{krate}: golden の期待のクレートには {case}/expected/status.txt が要ります（golden-verify が ebykn verify の結果と比べる）"
            ));
        }
        check_lock(repo, krate, locked, &mut r);
    }
    for krate in CRATES {
        if !repo.has(&format!("{krate}/Cargo.toml")) {
            r.errors.push(format!(
                "Verus で検証するクレート {krate} に Cargo.toml がありません。xtask/src/verus.rs の CRATES を直してください"
            ));
        }
        check_lock(repo, krate, locked, &mut r);
    }
    let mut checked = 0;
    for f in repo
        .files
        .iter()
        .filter(|f| f.ends_with("Cargo.toml") && *f != "Cargo.toml")
    {
        let text = match repo.read(f) {
            Ok(t) => t,
            Err(e) => {
                r.errors.push(e);
                continue;
            }
        };
        let mut depends = false;
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            let at = |m: String| format!("{f}:{}: {m}", i + 1);
            if t.starts_with('[') && t.contains("vstd") {
                r.errors.push(at(format!(
                    "vstd の依存は表の形（{t}）ではなく `vstd = \"{}\"` と書いてください",
                    pin.vstd
                )));
                depends = true;
                continue;
            }
            let Some(rest) = t.strip_prefix("vstd") else {
                continue;
            };
            if !rest.starts_with([' ', '=', '.']) {
                continue;
            }
            depends = true;
            checked += 1;
            let want = format!("\"{}\"", pin.vstd);
            match rest.trim_start().strip_prefix('=') {
                Some(value) if value.trim() == want => {}
                _ => r.errors.push(at(format!(
                    "vstd は `vstd = {want}` の形で固定した版にしてください（今は {t}）"
                ))),
            }
        }
        let dir = f.strip_suffix("/Cargo.toml").unwrap_or_default();
        if depends && !CRATES.contains(&dir) && !golden.iter().any(|g| g == dir) {
            r.errors.push(format!(
                "{f}: vstd に依存するクレート {dir} が verus-verify の対象にありません。xtask/src/verus.rs の CRATES に足すか、golden の期待（tests/golden/<名前>/expected/verus）に置いてください"
            ));
        }
    }
    if checked == 0 {
        r.errors
            .push("vstd に依存する Cargo.toml が一つもありません".to_string());
    }
    r.notes.push(format!(
        "vstd の依存 {checked} 個を検査。verus-verify の対象: {}。golden-verify の対象: {}",
        CRATES.join(", "),
        golden.join(", ")
    ));
    r
}

/// golden の期待のクレート（`tests/golden/<名前>/expected/verus` で `Cargo.toml` を追跡しているもの）。
pub fn golden_crates(repo: &Repo) -> Vec<String> {
    repo.files
        .iter()
        .filter_map(|f| f.strip_suffix("/expected/verus/Cargo.toml"))
        .filter(|d| {
            d.strip_prefix("tests/golden/")
                .is_some_and(|n| !n.is_empty() && !n.contains('/'))
        })
        .map(|d| format!("{d}/expected/verus"))
        .collect()
}

fn check_lock(repo: &Repo, krate: &str, locked: &str, r: &mut Report) {
    let lock = format!("{krate}/Cargo.lock");
    if !repo.has(&lock) {
        r.errors.push(format!(
            "{lock} を追跡してください（依存の版を固定し、Verus なしのビルドを環境に左右させない）"
        ));
        return;
    }
    match repo.read(&lock) {
        Err(e) => r.errors.push(e),
        Ok(text) => {
            if !lock_has(&text, "vstd", locked) {
                r.errors
                    .push(format!("{lock}: vstd が固定した版 {locked} ではありません"));
            }
        }
    }
}

/// `Cargo.lock` に `name = "<name>"` の直後が `version = "<version>"` の項目がある。
fn lock_has(text: &str, name: &str, version: &str) -> bool {
    let want_name = format!("name = \"{name}\"");
    let want_version = format!("version = \"{version}\"");
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    lines
        .windows(2)
        .any(|w| w[0] == want_name && w[1] == want_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const PRELUDE: &str = "crates/ebykn_prelude/Cargo.toml";
    const PRELUDE_LOCK: &str = "crates/ebykn_prelude/Cargo.lock";
    const STACK: &str = "tests/golden/stack/expected/verus/Cargo.toml";
    const STACK_LOCK: &str = "tests/golden/stack/expected/verus/Cargo.lock";
    const FORMAL: &str = "formal/linearity/Cargo.toml";
    const FORMAL_LOCK: &str = "formal/linearity/Cargo.lock";
    const STACK_STATUS: &str = "tests/golden/stack/expected/status.txt";

    fn repo(root: &str, prelude: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write("Cargo.toml", root);
        t.write(PRELUDE, prelude);
        t.write(PRELUDE_LOCK, &real(PRELUDE_LOCK));
        t.write(STACK, &real(STACK));
        t.write(STACK_LOCK, &real(STACK_LOCK));
        t.write(FORMAL, &real(FORMAL));
        t.write(FORMAL_LOCK, &real(FORMAL_LOCK));
        t.write(STACK_STATUS, &real(STACK_STATUS));
        t
    }

    #[test]
    fn passes_on_the_real_files() {
        assert_passes(&repo(&real("Cargo.toml"), &real(PRELUDE)).run(run));
    }

    #[test]
    fn fails_when_a_crate_uses_another_vstd() {
        let broken = mutate(&real(PRELUDE), "vstd = \"=0.0.0-", "vstd = \"=0.0.1-");
        assert_fails(&repo(&real("Cargo.toml"), &broken).run(run), PRELUDE);
    }

    #[test]
    fn fails_when_the_pin_is_unreadable() {
        let broken = mutate(&real("Cargo.toml"), "sha256 = \"", "sha256 = \"x");
        assert_fails(&repo(&broken, &real(PRELUDE)).run(run), "sha256");
    }

    #[test]
    fn fails_when_a_listed_crate_is_gone() {
        let mut t = TempRepo::new();
        t.write("Cargo.toml", &real("Cargo.toml"));
        t.write(STACK, &real(STACK));
        t.write(STACK_LOCK, &real(STACK_LOCK));
        t.write(STACK_STATUS, &real(STACK_STATUS));
        assert_fails(
            &t.run(run),
            "crates/ebykn_prelude に Cargo.toml がありません",
        );
    }

    #[test]
    fn fails_when_a_vstd_crate_is_not_verified() {
        let mut t = repo(&real("Cargo.toml"), &real(PRELUDE));
        t.write("crates/other/Cargo.toml", &real(STACK));
        assert_fails(
            &t.run(run),
            "crates/other が verus-verify の対象にありません",
        );
    }

    #[test]
    fn fails_when_a_golden_crate_has_no_status() {
        let mut t = repo(&real("Cargo.toml"), &real(PRELUDE));
        t.write("tests/golden/dlist/expected/verus/Cargo.toml", &real(STACK));
        t.write(
            "tests/golden/dlist/expected/verus/Cargo.lock",
            &real(STACK_LOCK),
        );
        assert_fails(
            &t.run(run),
            "tests/golden/dlist/expected/status.txt が要ります",
        );
    }

    #[test]
    fn fails_when_a_golden_lock_is_missing() {
        let mut t = repo(&real("Cargo.toml"), &real(PRELUDE));
        t.write("tests/golden/dlist/expected/verus/Cargo.toml", &real(STACK));
        t.write(
            "tests/golden/dlist/expected/status.txt",
            &real(STACK_STATUS),
        );
        assert_fails(
            &t.run(run),
            "tests/golden/dlist/expected/verus/Cargo.lock を追跡してください",
        );
    }

    #[test]
    fn fails_on_other_forms_of_the_vstd_dependency() {
        let src = real(PRELUDE);
        let line = src.lines().find(|l| l.starts_with("vstd = ")).unwrap();
        for form in [
            "vstd.workspace = true".to_string(),
            "vstd = { version = \"=0.0.0-2026-11-01-0251\" }".to_string(),
            "[dependencies.vstd]\nversion = \"=0.0.0-2026-11-01-0251\"".to_string(),
        ] {
            let broken = mutate(&src, line, &form);
            assert_fails(&repo(&real("Cargo.toml"), &broken).run(run), PRELUDE);
        }
    }

    #[test]
    fn fails_when_a_lock_file_is_not_tracked() {
        let mut t = TempRepo::new();
        t.write("Cargo.toml", &real("Cargo.toml"));
        t.write(PRELUDE, &real(PRELUDE));
        t.write(STACK, &real(STACK));
        t.write(STACK_LOCK, &real(STACK_LOCK));
        t.write(STACK_STATUS, &real(STACK_STATUS));
        assert_fails(
            &t.run(run),
            "crates/ebykn_prelude/Cargo.lock を追跡してください",
        );
    }

    #[test]
    fn fails_when_a_lock_file_has_another_vstd() {
        let mut t = repo(&real("Cargo.toml"), &real(PRELUDE));
        let broken = mutate(
            &real(STACK_LOCK),
            "name = \"vstd\"\nversion = \"0.0.0-2026-11-01-0251\"",
            "name = \"vstd\"\nversion = \"0.0.0-2026-10-01-0000\"",
        );
        t.write(STACK_LOCK, &broken);
        assert_fails(&t.run(run), "vstd が固定した版");
    }
}
