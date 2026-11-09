//! リポジトリのゲートと開発用コマンド（`cargo xtask <サブコマンド>`）。

mod fetch;
mod gates;
mod golden;
mod playground;
mod slides;
#[cfg(test)]
mod testutil;
mod verus;

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
  check-md-tables   Markdown の表が途中の空行で切れず、各行の列の数が見出しと同じ
  check-book        Ebykn Book の SUMMARY と ```eyk フェンスの形
  check-gate-tests  各ゲートに失敗を主張するテスト（fails_*）がある
  check-verus-pin   固定した Verus の値が読め、Verus のクレートが固定した vstd を使う
  check-prelude-exports  ebykn_prelude が vstd を許した名前のほかに再公開しない
  check-examples    examples/ の各例に Ebykn.toml と README があり、README の ```console を読める
  check-attr-names  組み込みの属性の名前を、raw の綴り（r#cfg）を見逃す形で比べない
  check-vendor-commits  vendor/ を変えるコミットが vendor/ の外を変えない（全履歴が要る）

  verus-install     固定した Verus をキャッシュに入れ（一時的なエラーは再試行）、要求される toolchain を入れる
  verus-verify      ebykn_prelude と §6.2 の参照モデルを Verus で検証し、Verus なしでもビルドする
  golden-verify     golden の各ケースに ebykn verify をかけ、expected/status.txt と比べる。Verus の要るテストも走らせる
  verus-probes      設計ノート §7.2 のプローブを走らせ、期待と比べる
  formal-oracle     §6.2 の参照モデルから判定の表を作り直し、formal/oracle/ と比べる
  formal-oracle-write  §6.2 の参照モデルから判定の表を作り直し、formal/oracle/ に書く
  examples-verify   examples/ の各例で README の ```console のコマンドを実行し、出力を比べる

  playground        ebykn をビルドし、examples/ を playground/ に写す（あるファイルは上書きしない）

  slides-install    報告スライドの道具（固定した Typst とフォント）をキャッシュに入れる（一時的なエラーは再試行）
  slides-build <ディレクトリ>  <ディレクトリ>/slides.typ を slides.pdf に組み、はみ出し・空のページ・密度・見出し・リンクを検査する
  slides-selftest   スキルの見本を 1 か所ずつ壊して組み、slides-build の検査が失敗することを確かめる（slides-install が要る）
";

/// 整形の検査。`--all` は path 依存（取り込んだ `vendor/`）まで含めるので付けない。
/// 仮想ワークスペースでは、`--all` なしでもワークスペースの全メンバーを検査する。
const FMT_CHECK: &[&str] = &["fmt", "--check"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [cmd, dir] = args.as_slice()
        && cmd == "slides-build"
    {
        return exit(slides::build(&root(), dir));
    }
    let [cmd] = args.as_slice() else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let ok = match cmd.as_str() {
        "precommit" => cargo(FMT_CHECK) && run_gates(gates::ALL),
        "ci" => {
            cargo(FMT_CHECK)
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
        "verus-install" => verus::install(&root()),
        "verus-verify" => verus::verify(&root()),
        "formal-oracle-write" => gates::formal_oracle::write(&root()),
        "playground" => match list_files(&root()) {
            Ok(files) => playground::run(&root(), &files),
            Err(e) => {
                eprintln!("error: {e}");
                false
            }
        },
        "golden-verify" => match list_files(&root()) {
            Ok(files) => golden::verify(&root(), &files),
            Err(e) => {
                eprintln!("error: {e}");
                false
            }
        },
        "slides-install" => slides::install(),
        "slides-selftest" => slides::selftest::run(&root()),
        name => match gates::ALL
            .iter()
            .chain(gates::VERUS)
            .find(|g| g.name == name)
        {
            Some(gate) => run_gates(std::slice::from_ref(gate)),
            None => {
                eprint!("不明なサブコマンド: {name}\n\n{USAGE}");
                return ExitCode::from(2);
            }
        },
    };
    exit(ok)
}

fn exit(ok: bool) -> ExitCode {
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
