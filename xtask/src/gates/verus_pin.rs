//! `check-verus-pin`：固定した Verus の値が読め、Verus のクレートが固定した vstd を使っている。
//!
//! Verus は実行しない。固定値（ルートの `Cargo.toml` の `[workspace.metadata.verus]`）と、
//! `vstd` に依存する全 `Cargo.toml` の版の食い違いを検出する。

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
    for krate in CRATES {
        if !repo.has(&format!("{krate}/Cargo.toml")) {
            r.errors.push(format!(
                "Verus で検証するクレート {krate} に Cargo.toml がありません。xtask/src/verus.rs の CRATES を直してください"
            ));
        }
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
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            let Some(rest) = t.strip_prefix("vstd") else {
                continue;
            };
            let Some(value) = rest.trim_start().strip_prefix('=') else {
                continue;
            };
            checked += 1;
            let want = format!("\"{}\"", pin.vstd);
            if value.trim() != want {
                r.errors.push(format!(
                    "{f}:{}: vstd は固定した版 {want} にしてください（今は {}）",
                    i + 1,
                    value.trim()
                ));
            }
        }
    }
    if checked == 0 {
        r.errors
            .push("vstd に依存する Cargo.toml が一つもありません".to_string());
    }
    r.notes.push(format!("vstd の依存 {checked} 個を検査"));
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const PRELUDE: &str = "crates/ebykn_prelude/Cargo.toml";
    const STACK: &str = "tests/golden/stack/expected/verus/Cargo.toml";

    fn repo(root: &str, prelude: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write("Cargo.toml", root);
        t.write(PRELUDE, prelude);
        t.write(STACK, &real(STACK));
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
        assert_fails(
            &t.run(run),
            "crates/ebykn_prelude に Cargo.toml がありません",
        );
    }
}
