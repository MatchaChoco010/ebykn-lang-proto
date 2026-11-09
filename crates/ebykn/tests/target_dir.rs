//! #137 の再現：プロジェクトの親の `.cargo/config.toml` が `build.target-dir` を設定していても、生成クレートの
//! 成果物は `build/verus/target` に置かれ、`ebykn verify` は設定がないときと同じ結果を出す。
//!
//! 固定した Verus が要るので `#[ignore]` にし、`cargo xtask golden-verify`（CI の Verus のジョブ）が
//! `--ignored` で走らせる。ケースは `tests/golden/stack/` を、`build.target-dir` を設定した親の下に写して使う
//! （期待は `golden-verify` が設定なしで確かめている `expected/status.txt`）。

use std::path::{Path, PathBuf};
use std::process::Command;

fn case() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/stack")
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(from, to).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
}

fn ebykn(dir: &Path, arg: &str) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ebykn"));
    // このテストを走らせた cargo の環境を引き継がない（`golden-verify` が `ebykn verify` を呼ぶのと同じ）。
    for v in [
        "CARGO",
        "RUSTC",
        "RUSTDOC",
        "RUSTUP_TOOLCHAIN",
        "CARGO_TARGET_DIR",
    ] {
        cmd.env_remove(v);
    }
    cmd.arg(arg)
        .current_dir(dir)
        .output()
        .expect("ebykn を起動する")
}

/// 落ちた回も含めて、作ったディレクトリ（vstd と prelude のビルドを含む）を消す。
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 親の `.cargo/config.toml` の `[build]` に `extra` を書いたプロジェクトで、`verify` を 2 回と `build` を
/// 走らせる。`elsewhere` は、親の設定の `target-dir` に書く置き場。
fn run_under_parent_config(label: &str, extra: impl Fn(&Path) -> String) {
    // リポジトリの外に置く（中に置くと、リポジトリのワークスペースの下になる）。ビルドを置くので `/tmp` は使わない。
    let cache = match std::env::var_os("XDG_CACHE_HOME") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".cache"),
    };
    let base = cache
        .join("ebykn/scratch")
        .join(format!("target-dir-137-{label}-{}", std::process::id()));
    if base.exists() {
        std::fs::remove_dir_all(&base).unwrap();
    }
    let _scratch = Scratch(base.clone());
    let elsewhere = base.join("elsewhere");
    let config = base.join(".cargo/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        format!(
            "[build]\ntarget-dir = {:?}\n{}",
            elsewhere.display().to_string(),
            extra(&base)
        ),
    )
    .unwrap();
    let dir = base.join("stack");
    let src = case();
    for f in ["Ebykn.toml", "src/lib.eyk"] {
        copy(&src.join(f), &dir.join(f));
    }
    copy(
        &src.join("expected/verus/Cargo.lock"),
        &dir.join("build/verus/Cargo.lock"),
    );
    let expected = std::fs::read_to_string(src.join("expected/status.txt")).unwrap();

    // 前の実行の成果物があっても作り直す（#137 は 2 回目で、成果物が消えずに Verus が呼ばれなかった）。
    for round in 1..=2 {
        let out = ebykn(&dir, "verify");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout,
            expected,
            "{label}：{round} 回目\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        // stack は失敗する項目を持つので、終了コードは 1。
        assert_eq!(out.status.code(), Some(1), "{label}：{round} 回目");
        assert!(
            !elsewhere.exists(),
            "{label}：{round} 回目：成果物が親の設定の target-dir（{}）に置かれた",
            elsewhere.display()
        );
        // `cargo verus focus` の成果物も、消す側（`cargo clean -p`）と同じ target-dir の下にある。
        assert!(
            dir.join("build/verus/target/verus-partial").is_dir(),
            "{label}：{round} 回目：build/verus/target/verus-partial がない"
        );
    }

    // `build` が示す置き場は、実際に成果物のある場所（`build/verus/target/debug`）。
    let out = ebykn(&dir, "build");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{label}：build\n{stderr}");
    let shown = stderr
        .split_once("built `")
        .and_then(|(_, r)| r.split_once('`'))
        .map(|(p, _)| p)
        .unwrap_or_else(|| panic!("{label}：build の置き場の表示がない\n{stderr}"));
    assert_eq!(
        dir.join(shown).canonicalize().unwrap(),
        dir.join("build/verus/target/debug").canonicalize().unwrap(),
        "{label}"
    );
    assert!(
        dir.join(shown).join("libstack.rlib").is_file(),
        "{label}：{shown}"
    );
    assert!(
        !elsewhere.exists(),
        "{label}：build の成果物が親の設定の target-dir に置かれた"
    );
}

#[test]
#[ignore = "固定した Verus が要る。cargo xtask golden-verify が --ignored で走らせる"]
fn verify_ignores_the_parent_target_dir() {
    run_under_parent_config("target-dir", |_| String::new());
    // 中間の成果物の置き場（build-dir）も移した親の下でも、verify と build の結果は同じ。
    run_under_parent_config("build-dir", |base| {
        format!(
            "build-dir = {:?}\n",
            base.join("elsewhere-build").display().to_string()
        )
    });
}
