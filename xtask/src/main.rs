//! リポジトリのゲートと開発用コマンド（`cargo xtask <サブコマンド>`）。

mod gates;
#[cfg(test)]
mod testutil;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use gates::Repo;

const USAGE: &str = "\
使い方: cargo xtask <サブコマンド>

  precommit         cargo fmt --check と全ゲート（pre-commit フックが呼ぶ）
  ci                cargo fmt --check、clippy、cargo test --workspace、全ゲート
  gates             全ゲート
  check-file-size   ソースファイルが 2000 行以下
  check-md-links    Markdown の相対リンク先が存在する
  check-book        Ebykn Book の SUMMARY と ```eyk フェンスの形
  check-gate-tests  各ゲートに失敗を主張するテスト（fails_*）がある
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [cmd] = args.as_slice() else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let ok = match cmd.as_str() {
        "precommit" => cargo(&["fmt", "--all", "--check"]) && run_gates(gates::ALL),
        "ci" => {
            cargo(&["fmt", "--all", "--check"])
                && cargo(&[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ])
                && cargo(&["test", "--workspace"])
                && run_gates(gates::ALL)
        }
        "gates" => run_gates(gates::ALL),
        name => match gates::ALL.iter().find(|g| g.name == name) {
            Some(gate) => run_gates(std::slice::from_ref(gate)),
            None => {
                eprint!("不明なサブコマンド: {name}\n\n{USAGE}");
                return ExitCode::from(2);
            }
        },
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask はリポジトリ直下にある")
        .to_path_buf()
}

fn cargo(args: &[&str]) -> bool {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    eprintln!("$ cargo {}", args.join(" "));
    match Command::new(cargo).args(args).current_dir(root()).status() {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!("error: cargo を起動できません: {e}");
            false
        }
    }
}

/// 追跡されているファイルと、まだ追加していないが無視されていないファイル。
fn list_files(root: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files が失敗しました: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut files: Vec<String> = String::from_utf8(out.stdout)
        .map_err(|_| "git ls-files の出力が UTF-8 ではありません".to_string())?
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

fn run_gates(list: &[gates::Gate]) -> bool {
    let root = root();
    let files = match list_files(&root) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            return false;
        }
    };
    let repo = Repo { root, files };
    let mut ok = true;
    for gate in list {
        let report = (gate.run)(&repo);
        for note in &report.notes {
            eprintln!("  {}: {note}", gate.name);
        }
        if report.errors.is_empty() {
            eprintln!("ok: {}", gate.name);
        } else {
            for e in &report.errors {
                eprintln!("error: {}: {e}", gate.name);
            }
            eprintln!("FAIL: {}（{} 件）", gate.name, report.errors.len());
            ok = false;
        }
    }
    ok
}
