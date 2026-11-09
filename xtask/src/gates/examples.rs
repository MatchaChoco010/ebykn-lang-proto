//! `examples/` の例のプロジェクト（`examples/<名前>/`）が、README に書いた期待のとおりに動くか。
//!
//! - `check-examples`（`gates`）：形だけを見る。各例に `Ebykn.toml` と `README.md` があり、目次
//!   （`examples/README.md`）から各例へリンクがあり、README の ` ```console ` ブロックを読める。
//! - `examples-verify`（Verus のジョブ）：各例を `target/examples/<名前>/` に写し、README の
//!   ` ```console ` ブロックのコマンドを順に実行して、出力を README と比べる。
//!
//! ` ```console ` ブロックの形：
//!
//! ```text
//! $ <コマンド>      sh -c で、例のディレクトリで実行する。標準エラーも標準出力に合わせる
//! <出力の行>        直前のコマンドの出力。行ごとに完全に等しいこと
//! ```
//!
//! - `ebykn` は、このリポジトリでビルドしたもの（`target/debug/ebykn`）を `PATH` の先頭に置いて使う。
//! - 出力の中の、例のディレクトリの絶対パスは `$PWD` と書く。
//! - `$ echo $?` は、直前のコマンドの終了コードを出す（各コマンドの前に、直前の終了コードを `$?` に戻す）。
//! - コマンドの前に出力の行があるもの、コマンドのないブロック、`\` で続けるコマンドは読めないので失敗にする。
//! - 言語のないフェンスと閉じていないフェンスも失敗にする。` ```console ` 以外のフェンス（` ```eyk ` など）は実行しない。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{Repo, Report, fence_close, fence_open};

pub const DIR: &str = "examples/";
pub const INDEX: &str = "examples/README.md";

/// README の 1 つのコマンドと、期待する出力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// README の行番号（1 始まり）。
    pub line: usize,
    pub cmd: String,
    pub expected: Vec<String>,
}

/// 例のプロジェクト。
#[derive(Debug)]
pub struct Example {
    pub name: String,
    /// 例の中の追跡しているファイル（`examples/<名前>/` からの相対パス）。
    pub files: Vec<String>,
    pub steps: Vec<Step>,
}

/// README の ` ```console ` ブロックを読む。
pub fn parse_readme(file: &str, text: &str) -> Result<Vec<Step>, String> {
    let mut steps: Vec<Step> = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((i, line)) = lines.next() {
        let Some((c, n, info)) = fence_open(line) else {
            continue;
        };
        let open = i + 1;
        if info.is_empty() {
            return Err(format!(
                "{file}:{open}: 言語のないフェンスです（コマンドの例は ```console）"
            ));
        }
        let console = info.split_whitespace().next() == Some("console");
        let mut block = 0;
        let mut closed = false;
        for (j, l) in lines.by_ref() {
            if fence_close(l, c, n) {
                closed = true;
                break;
            }
            if !console {
                continue;
            }
            let no = j + 1;
            if let Some(cmd) = l.strip_prefix("$ ") {
                let cmd = cmd.trim();
                if cmd.is_empty() {
                    return Err(format!("{file}:{no}: `$ ` の後にコマンドがありません"));
                }
                if cmd.ends_with('\\') {
                    return Err(format!(
                        "{file}:{no}: `\\` で続けるコマンドは読めません（1 行に書きます）"
                    ));
                }
                steps.push(Step {
                    line: no,
                    cmd: cmd.to_string(),
                    expected: Vec::new(),
                });
                block += 1;
            } else if l.trim() == "$" {
                return Err(format!("{file}:{no}: `$` の後にコマンドがありません"));
            } else if block == 0 {
                return Err(format!(
                    "{file}:{no}: コマンド（`$ ` で始まる行）より前に出力の行があります"
                ));
            } else {
                steps.last_mut().unwrap().expected.push(l.to_string());
            }
        }
        if !closed {
            return Err(format!("{file}:{open}: フェンスが閉じていません"));
        }
        if console && block == 0 {
            return Err(format!(
                "{file}:{open}: ```console のブロックにコマンドがありません"
            ));
        }
    }
    if steps.is_empty() {
        return Err(format!(
            "{file}: ```console のブロックがありません（打つコマンドと期待する出力を書きます）"
        ));
    }
    Ok(steps)
}

/// 追跡しているファイルから例を集め、形を確かめる。読めたものを返し、誤りは `errors` に足す。
pub fn collect(repo: &Repo, errors: &mut Vec<String>) -> Vec<Example> {
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &repo.files {
        let Some(rest) = f.strip_prefix(DIR) else {
            continue;
        };
        match rest.split_once('/') {
            Some((name, file)) => by_name
                .entry(name.to_string())
                .or_default()
                .push(file.to_string()),
            None if f == INDEX => {}
            None => errors.push(format!(
                "{f}: {DIR} の直下には目次（{INDEX}）と例のディレクトリだけを置きます"
            )),
        }
    }
    if by_name.is_empty() {
        errors.push(format!("{DIR} に例がありません"));
        return Vec::new();
    }
    let index = match repo.read(INDEX) {
        Ok(t) => Some(t),
        Err(e) => {
            errors.push(e);
            None
        }
    };
    let mut out = Vec::new();
    for (name, files) in by_name {
        let base = format!("{DIR}{name}");
        if !files.iter().any(|f| f == "Ebykn.toml") {
            errors.push(format!("{base}/: Ebykn.toml がありません"));
            continue;
        }
        if let Some(index) = &index
            && !index.contains(&format!("]({name}/)"))
        {
            errors.push(format!(
                "{base}/: 目次（{INDEX}）に `[..]({name}/)` のリンクがありません"
            ));
        }
        if !files.iter().any(|f| f == "README.md") {
            errors.push(format!("{base}/: README.md がありません"));
            continue;
        }
        let readme = format!("{base}/README.md");
        match repo.read(&readme).and_then(|t| parse_readme(&readme, &t)) {
            Ok(steps) => out.push(Example { name, files, steps }),
            Err(e) => errors.push(e),
        }
    }
    out
}

/// `check-examples`：形だけを見る（実行しない）。
pub fn run_static(repo: &Repo) -> Report {
    let mut r = Report::default();
    let examples = collect(repo, &mut r.errors);
    let steps: usize = examples.iter().map(|e| e.steps.len()).sum();
    r.notes.push(format!(
        "例 {}、コマンド {steps} の形を検査した（実行は examples-verify）",
        examples.len()
    ));
    r
}

/// コマンドを実行するもの：（例のディレクトリ、コマンド、直前の終了コード）→（出力、終了コード）。
pub type RunCmd<'a> = &'a dyn Fn(&Path, &str, i32) -> Result<(String, i32), String>;

/// `examples-verify`：各例を写して README のコマンドを実行し、出力を比べる。
pub fn run(repo: &Repo) -> Report {
    let pin = repo
        .read("Cargo.toml")
        .and_then(|t| crate::verus::read_pin(&t))
        .and_then(|p| crate::verus::installed(&p));
    if let Err(e) = pin {
        return Report {
            errors: vec![e],
            notes: Vec::new(),
        };
    }
    if !crate::cargo(&["build", "--quiet", "-p", "ebykn"]) {
        return Report {
            errors: vec!["ebykn をビルドできません".into()],
            notes: Vec::new(),
        };
    }
    let bin = repo.root.join("target/debug");
    run_with(repo, &|dir, cmd, prev| sh(&bin, dir, cmd, prev))
}

fn sh(bin: &Path, dir: &Path, cmd: &str, prev: i32) -> Result<(String, i32), String> {
    let path = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .map_err(|e| e.to_string())?;
    let mut c = Command::new("sh");
    for v in [
        "CARGO",
        "RUSTC",
        "RUSTDOC",
        "RUSTUP_TOOLCHAIN",
        "CARGO_TARGET_DIR",
        "CARGO_MANIFEST_DIR",
    ] {
        c.env_remove(v);
    }
    let out = c
        .arg("-c")
        .arg(format!("(exit {prev}); {{ {cmd}\n}} 2>&1"))
        .current_dir(dir)
        .env("PATH", path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("sh を起動できません: {e}"))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}

/// 例を `target/examples/<名前>/` に写す。前の `build/` は残す（vstd のビルドを使い回す）。
fn prepare(repo: &Repo, ex: &Example) -> Result<PathBuf, String> {
    let dir = repo.root.join("target/examples").join(&ex.name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if p.file_name().is_some_and(|n| n == "build") {
            continue;
        }
        let removed = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
        removed.map_err(|e| format!("{}: {e}", p.display()))?;
    }
    for f in &ex.files {
        let from = repo.root.join(DIR).join(&ex.name).join(f);
        let to = dir.join(f);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    dir.canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))
}

pub fn run_with(repo: &Repo, run_cmd: RunCmd) -> Report {
    let mut r = Report::default();
    let examples = collect(repo, &mut r.errors);
    let mut ran = 0;
    for ex in &examples {
        let dir = match prepare(repo, ex) {
            Ok(d) => d,
            Err(e) => {
                r.errors.push(format!("{DIR}{}: 写せません: {e}", ex.name));
                continue;
            }
        };
        let readme = format!("{DIR}{}/README.md", ex.name);
        let shown = dir.display().to_string();
        let mut prev = 0;
        for step in &ex.steps {
            eprintln!("$ (cd {DIR}{} && {})", ex.name, step.cmd);
            let (out, status) = match run_cmd(&dir, &step.cmd, prev) {
                Ok(o) => o,
                Err(e) => {
                    r.errors.push(format!("{readme}:{}: {e}", step.line));
                    break;
                }
            };
            ran += 1;
            prev = status;
            let actual: Vec<String> = out
                .replace(&shown, "$PWD")
                .lines()
                .map(String::from)
                .collect();
            if actual != step.expected {
                r.errors.push(format!(
                    "{readme}:{}: `{}` の出力が README と違います（終了コード {status}）\n--- README\n{}\n--- 実際\n{}",
                    step.line,
                    step.cmd,
                    step.expected.join("\n"),
                    actual.join("\n")
                ));
                // 後のコマンドは前のコマンドの結果に依るので、この例はここで止める。
                break;
            }
        }
    }
    r.notes
        .push(format!("例 {}、コマンド {ran} を実行した", examples.len()));
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use std::cell::RefCell;

    /// 実リポジトリの例（ゲートが実行する順。名前の順）。
    const EXAMPLES: &[&str] = &[
        "hello",
        "linearity-broken",
        "linearity-fixed",
        "main-program",
        "stack",
    ];

    /// 実リポジトリの例を、一時リポジトリに置く。`edit` で README を書き換えられる。
    fn real_examples(edit: impl Fn(&str, String) -> String) -> TempRepo {
        let mut t = TempRepo::new();
        t.write(INDEX, &real(INDEX));
        for name in EXAMPLES {
            let base = format!("{DIR}{name}");
            t.write(
                &format!("{base}/Ebykn.toml"),
                &real(&format!("{base}/Ebykn.toml")),
            );
            let readme = format!("{base}/README.md");
            t.write(&readme, &edit(name, real(&readme)));
        }
        t
    }

    /// README（壊す前の実ファイル）の期待どおりの出力を順に返す偽の実行。
    fn consistent() -> impl Fn(&Path, &str, i32) -> Result<(String, i32), String> {
        // 例ごとの（コマンド、出力）の列。例の途中で止まっても、次の例は先頭から返す。
        let mut outs = BTreeMap::new();
        for name in EXAMPLES {
            let readme = format!("{DIR}{name}/README.md");
            let steps: Vec<(String, String)> = parse_readme(&readme, &real(&readme))
                .unwrap()
                .into_iter()
                .map(|s| (s.cmd, s.expected.iter().map(|l| format!("{l}\n")).collect()))
                .collect();
            outs.insert(name.to_string(), steps.into_iter());
        }
        let outs = RefCell::new(outs);
        move |dir: &Path, cmd: &str, _| {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            let mut outs = outs.borrow_mut();
            let (want, text) = outs
                .get_mut(&name)
                .unwrap()
                .next()
                .expect("コマンドが多すぎる");
            assert_eq!(cmd, want);
            Ok((text.replace("$PWD", &dir.display().to_string()), 0))
        }
    }

    #[test]
    fn passes_on_the_real_examples() {
        let t = real_examples(|_, s| s);
        assert_passes(&t.run(run_static));
        let r = t.run_ref(|repo| run_with(repo, &consistent()));
        assert_passes(&r);
        assert!(r.notes.iter().any(|n| n.contains("例 5")), "{:?}", r.notes);
    }

    #[test]
    fn fails_when_the_output_differs_from_the_readme() {
        let t = real_examples(|name, s| match name {
            "linearity-fixed" => mutate(
                &s,
                "results 2 verified, 0 errors",
                "results 3 verified, 0 errors",
            ),
            _ => s,
        });
        let r = t.run_ref(|repo| run_with(repo, &consistent()));
        assert_fails(&r, "examples/linearity-fixed/README.md");
        assert_fails(&r, "出力が README と違います");
    }

    #[test]
    fn fails_when_the_exit_status_differs() {
        let t = real_examples(|name, s| match name {
            "main-program" => mutate(&s, "$ echo $?\n0", "$ echo $?\n1"),
            _ => s,
        });
        let r = t.run_ref(|repo| run_with(repo, &consistent()));
        assert_fails(&r, "`echo $?` の出力が README と違います");
    }

    #[test]
    fn replaces_the_example_directory_with_pwd() {
        let t = real_examples(|_, s| s);
        // 実際の出力には絶対パスが出る。README の `$PWD` と一致する。
        let r = t.run_ref(|repo| {
            run_with(repo, &|dir: &Path, cmd: &str, prev| {
                if cmd == "echo $?" {
                    return Ok((format!("{prev}\n"), 0));
                }
                Ok((format!("{}\n", dir.display()), 0))
            })
        });
        assert_fails(&r, "--- 実際\n$PWD");
    }

    #[test]
    fn fails_when_the_readme_cannot_be_read() {
        for (text, want) in [
            ("# x\n", "```console のブロックがありません"),
            (
                "```console\nverified x\n```\n",
                "より前に出力の行があります",
            ),
            ("```console\n```\n", "コマンドがありません"),
            ("```console\n$ ebykn verify\n", "フェンスが閉じていません"),
            ("```\n$ ebykn verify\n```\n", "言語のないフェンス"),
            ("```console\n$ ebykn \\\n  verify\n```\n", "`\\` で続ける"),
            ("```console\n$\n```\n", "`$` の後にコマンドがありません"),
        ] {
            let err = parse_readme("R.md", text).unwrap_err();
            assert!(err.contains(want), "{text:?}: {err}");
        }
        // console 以外のフェンスは実行しない。
        let s = parse_readme("R.md", "```eyk\n$ x\n```\n```console\n$ a\nb\n$ c\n```\n").unwrap();
        assert_eq!(
            s,
            vec![
                Step {
                    line: 5,
                    cmd: "a".into(),
                    expected: vec!["b".into()]
                },
                Step {
                    line: 7,
                    cmd: "c".into(),
                    expected: vec![]
                },
            ]
        );
    }

    #[test]
    fn fails_when_a_real_readme_is_broken() {
        let t = real_examples(|name, s| match name {
            "stack" => mutate(&s, "```console\n$ ebykn check", "```console\nebykn check"),
            _ => s,
        });
        assert_fails(&t.run(run_static), "examples/stack/README.md");
    }

    #[test]
    fn fails_when_an_example_is_incomplete() {
        let mut t = real_examples(|_, s| s);
        t.write(
            "examples/nomanifest/README.md",
            "```console\n$ ebykn check\n```\n",
        );
        t.write("examples/noreadme/Ebykn.toml", "");
        t.write("examples/stray.txt", "");
        let r = t.run(run_static);
        assert_fails(&r, "examples/nomanifest/: Ebykn.toml がありません");
        assert_fails(&r, "examples/noreadme/: README.md がありません");
        assert_fails(&r, "examples/noreadme/: 目次");
        assert_fails(&r, "examples/stray.txt");
    }

    #[test]
    fn fails_when_an_example_is_missing_from_the_index() {
        let mut t = TempRepo::new();
        let index = mutate(&real(INDEX), "](stack/)", "](stack-x/)");
        t.write(INDEX, &index);
        t.write("examples/stack/Ebykn.toml", "");
        t.write(
            "examples/stack/README.md",
            &real("examples/stack/README.md"),
        );
        assert_fails(&t.run(run_static), "examples/stack/: 目次");
    }

    #[test]
    fn fails_when_there_are_no_examples() {
        let t = TempRepo::new();
        assert_fails(&t.run(run_static), "例がありません");
    }
}
