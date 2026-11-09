//! `verus-probes`：設計ノート §7.2 の前提を確かめる Verus のプローブを固定した Verus で走らせ、
//! 各プローブの先頭に書いた期待と比べる。Verus の版を上げて前提が崩れたら失敗する。
//!
//! Verus が要るので `gates`（pre-commit）には入れず、CI の Verus のジョブで走らせる。
//!
//! プローブの先頭のコメント行（`// ` で始まる行が続く間）が期待で、すべて次のどれかでなければならない。
//!
//! ```text
//! // probe: <何を確かめるか>                 （一つ）
//! // expect: verified <N>                     （一つ。N verified, 0 errors で成功する）
//! // expect: errors <V> <E>                   （V verified, E errors で失敗する）
//! // expect: rejected                         （検証の前に失敗し、結果の行が出ない）
//! // expect: compile-error <N>                （N verified, 0 errors で検証が通った後、コンパイルで失敗する）
//! // expect-output: <部分文字列>              （任意個。出力に含まれる）
//! // expect-output-count: <回数> <部分文字列>  （任意個。出力にちょうどその回数だけ含まれる）
//! // verus-args: <引数 ..>                    （任意。verus に足す引数）
//! ```
//!
//! 失敗を期待するプローブ（`errors` / `rejected` / `compile-error`）は、`expect-output` か `expect-output-count` を
//! 少なくとも一つ持つ。持たないと、タイプミスや構文エラーのような別の失敗も期待どおりに数えてしまう。
//! 部分文字列の中の `\n` は改行、`\\` は `\` と読む。ヘッダの後ろに指示に見える行（`//expect:` など）が
//! あれば失敗にする。
//!
//! Verus は `target/verus-probes/` で走らせる（`--compile` の成果物をリポジトリに置かない）。

use std::path::Path;
use std::process::Command;

use super::{Repo, Report};
use crate::verus::{installed, read_pin, results_line};

pub const DIR: &str = "tests/verus-probes/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    Verified(u64),
    Errors(u64, u64),
    Rejected,
    /// 検証は `N verified, 0 errors` で通り、その後のコンパイル（`--compile`）で失敗する。
    CompileError(u64),
}

#[derive(Debug)]
pub struct Probe {
    pub file: String,
    pub expect: Expect,
    pub outputs: Vec<String>,
    /// 出力にちょうど `.0` 回含まれる部分文字列 `.1`。
    pub counts: Vec<(usize, String)>,
    pub args: Vec<String>,
}

const DIRECTIVES: &[&str] = &[
    "probe",
    "expect",
    "expect-output",
    "expect-output-count",
    "verus-args",
];

/// `\n` を改行に、`\\` を `\` に戻す。それ以外の `\` は読めない値として失敗にする。
fn unescape(v: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            _ => return Err(format!("値の中の \\ を読めません: {v}")),
        }
    }
    Ok(out)
}

/// ヘッダの外にある、指示に見える行（`//expect:`、`// expect-output:` など）。
fn stray_directive(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("//") else {
        return false;
    };
    let rest = rest.trim_start_matches(['/', '!', ' ']);
    DIRECTIVES.iter().any(|d| {
        rest.strip_prefix(d)
            .is_some_and(|r| r.trim_start().starts_with(':'))
    })
}

/// プローブのファイルと追加の引数を受け取って Verus を走らせる関数。テストでは偽物を渡す。
pub type RunVerus<'a> = &'a dyn Fn(&str, &[String]) -> Result<Outcome, String>;

/// Verus を一度走らせた結果。`text` は標準出力と標準エラーをつないだもの。
pub struct Outcome {
    pub success: bool,
    pub text: String,
}

pub fn parse(file: &str, text: &str) -> Result<Probe, String> {
    let mut desc = None;
    let mut expect = None;
    let mut outputs = Vec::new();
    let mut counts = Vec::new();
    let mut args = None;
    let mut header_end = 0;
    for (i, line) in text.lines().enumerate() {
        let Some(body) = line.strip_prefix("// ") else {
            break;
        };
        header_end = i + 1;
        let at = |m: &str| format!("{file}:{}: {m}", i + 1);
        let (key, value) = body
            .split_once(": ")
            .ok_or_else(|| at(&format!("期待の行を読めません: {line}")))?;
        let value = value.trim();
        if value.is_empty() {
            return Err(at(&format!("{key} の値が空です")));
        }
        match key {
            "probe" if desc.is_none() => desc = Some(value.to_string()),
            "expect" if expect.is_none() => expect = Some(parse_expect(value).map_err(|e| at(&e))?),
            "expect-output" => outputs.push(unescape(value).map_err(|e| at(&e))?),
            "expect-output-count" => {
                let (n, sub) = value
                    .split_once(' ')
                    .and_then(|(n, sub)| Some((n.parse::<usize>().ok()?, sub)))
                    .ok_or_else(|| at(&format!("expect-output-count を読めません: {value}")))?;
                counts.push((n, unescape(sub).map_err(|e| at(&e))?));
            }
            "verus-args" if args.is_none() => {
                args = Some(value.split_whitespace().map(str::to_string).collect())
            }
            "probe" | "expect" | "verus-args" => {
                return Err(at(&format!("{key} が重複しています")));
            }
            _ => return Err(at(&format!("未知の指示 {key}"))),
        }
    }
    if desc.is_none() {
        return Err(format!("{file}: 先頭に `// probe: ..` がありません"));
    }
    let expect = expect.ok_or_else(|| format!("{file}: 先頭に `// expect: ..` がありません"))?;
    if !matches!(expect, Expect::Verified(_)) && outputs.is_empty() && counts.is_empty() {
        return Err(format!(
            "{file}: 失敗を期待するプローブには `// expect-output: ..` が要ります（どの失敗かを固定する）"
        ));
    }
    for (i, line) in text.lines().enumerate().skip(header_end) {
        if stray_directive(line) {
            return Err(format!(
                "{file}:{}: ヘッダの外に指示があります（ヘッダは先頭の `// ` の行が続く間だけ）: {line}",
                i + 1
            ));
        }
    }
    Ok(Probe {
        file: file.to_string(),
        expect,
        outputs,
        counts,
        args: args.unwrap_or_default(),
    })
}

fn parse_expect(v: &str) -> Result<Expect, String> {
    let words: Vec<&str> = v.split_whitespace().collect();
    let num = |s: &str| {
        s.parse::<u64>()
            .map_err(|_| format!("expect の数を読めません: {v}"))
    };
    match words.as_slice() {
        ["verified", n] => Ok(Expect::Verified(num(n)?)),
        ["errors", a, b] if num(b)? > 0 => Ok(Expect::Errors(num(a)?, num(b)?)),
        ["rejected"] => Ok(Expect::Rejected),
        ["compile-error", n] => Ok(Expect::CompileError(num(n)?)),
        _ => Err(format!("expect を読めません: {v}")),
    }
}

/// 検証の件数。テキストの結果の行か、`--output-json` の `verification-results`。
fn counts(text: &str) -> Option<(u64, u64)> {
    if let Some(c) = results_line(text) {
        return Some(c);
    }
    let start = text.find("\"verification-results\"")?;
    let section = &text[start..];
    let section = &section[..section.find('}')?];
    let field = |name: &str| -> Option<u64> {
        let at = section.find(&format!("\"{name}\":"))? + name.len() + 3;
        let digits: String = section[at..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    };
    Some((field("verified")?, field("errors")?))
}

pub fn check(probe: &Probe, out: &Outcome) -> Vec<String> {
    let mut errs = Vec::new();
    let f = &probe.file;
    let got = counts(&out.text);
    match (&probe.expect, got) {
        (Expect::Verified(n), Some((v, 0))) if out.success && v == *n => {}
        (Expect::Errors(a, b), Some((v, e))) if !out.success && v == *a && e == *b => {}
        (Expect::Rejected, None) if !out.success => {}
        (Expect::CompileError(n), Some((v, 0))) if !out.success && v == *n => {}
        (want, got) => errs.push(format!(
            "{f}: 期待 {want:?} に対し、結果 {got:?}・終了 {}",
            if out.success { "成功" } else { "失敗" }
        )),
    }
    for o in &probe.outputs {
        if !out.text.contains(o.as_str()) {
            errs.push(format!("{f}: 出力に {o:?} がありません"));
        }
    }
    for (n, o) in &probe.counts {
        let got = out.text.matches(o.as_str()).count();
        if got != *n {
            errs.push(format!("{f}: 出力の {o:?} が {got} 回（期待 {n} 回）"));
        }
    }
    errs
}

pub fn run(repo: &Repo) -> Report {
    let bin = repo
        .read("Cargo.toml")
        .and_then(|t| read_pin(&t))
        .and_then(|p| installed(&p).map(|b| (p, b)));
    match bin {
        Ok((pin, bin)) => {
            let work = repo.root.join("target/verus-probes");
            if let Err(e) = std::fs::create_dir_all(&work) {
                return Report {
                    errors: vec![format!("{} を作れません: {e}", work.display())],
                    notes: Vec::new(),
                };
            }
            run_with(repo, &|file, args| {
                verus(
                    &bin.join("verus"),
                    &pin.toolchain,
                    &repo.root.join(file),
                    args,
                    &work,
                )
            })
        }
        Err(e) => Report {
            errors: vec![e],
            notes: Vec::new(),
        },
    }
}

fn verus(
    bin: &Path,
    toolchain: &str,
    file: &Path,
    args: &[String],
    work: &Path,
) -> Result<Outcome, String> {
    let out = Command::new(bin)
        .current_dir(work)
        .arg("--crate-type=lib")
        .args(args)
        .arg(file)
        .env("RUSTUP_TOOLCHAIN", toolchain)
        .output()
        .map_err(|e| format!("verus を起動できません: {e}"))?;
    Ok(Outcome {
        success: out.status.success(),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    })
}

pub fn run_with(repo: &Repo, verus: RunVerus) -> Report {
    let mut r = Report::default();
    let files: Vec<&String> = repo.files.iter().filter(|f| f.starts_with(DIR)).collect();
    let mut n = 0;
    for f in &files {
        if !f.ends_with(".rs") || f[DIR.len()..].contains('/') {
            r.errors.push(format!(
                "{f}: {DIR} にはプローブ（直下の .rs）だけを置きます"
            ));
            continue;
        }
        let probe = match repo.read(f).and_then(|t| parse(f, &t)) {
            Ok(p) => p,
            Err(e) => {
                r.errors.push(e);
                continue;
            }
        };
        match verus(f, &probe.args) {
            Ok(out) => {
                let errs = check(&probe, &out);
                if !errs.is_empty() {
                    r.errors.extend(errs);
                    r.notes.push(format!("{f} の出力:\n{}", out.text));
                }
            }
            Err(e) => r.errors.push(format!("{f}: {e}")),
        }
        n += 1;
    }
    if n == 0 {
        r.errors.push(format!("{DIR} にプローブがありません"));
    }
    r.notes.push(format!("{n} 個のプローブを実行"));
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const PROBE: &str = "tests/verus-probes/simple_pptr.rs";
    const REJECTED: &str = "tests/verus-probes/assume_specification_duplicate.rs";
    const JSON_FAILURE: &str = "tests/verus-probes/json_output_failure.rs";
    const COMPILE_ERROR: &str = "tests/verus-probes/non_zst_compile.rs";

    /// 期待どおりの結果を返す偽の Verus。期待は `original` から読む（壊す前の実ファイル）。
    fn consistent(original: &str) -> impl Fn(&str, &[String]) -> Result<Outcome, String> + use<> {
        let probe = parse("x", original).unwrap();
        move |_, _| {
            let line = match probe.expect {
                Expect::Verified(n) => format!("verification results:: {n} verified, 0 errors"),
                Expect::Errors(a, b) => format!("verification results:: {a} verified, {b} errors"),
                Expect::Rejected => "error: aborting".to_string(),
                Expect::CompileError(n) => {
                    format!("verification results:: {n} verified, 0 errors\nerror: aborting")
                }
            };
            let mut text = format!("{line}\n{}\n", probe.outputs.join("\n"));
            for (n, o) in &probe.counts {
                let have = text.matches(o.as_str()).count();
                for _ in have..*n {
                    text.push_str(&format!("{o}\n"));
                }
            }
            Ok(Outcome {
                success: matches!(probe.expect, Expect::Verified(_)),
                text,
            })
        }
    }

    fn one(file: &str, content: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write(file, content);
        t
    }

    fn run_one(file: &str, content: &str, original: &str) -> Report {
        let t = one(file, content);
        let fake = consistent(original);
        t.run_ref(|repo| run_with(repo, &fake))
    }

    #[test]
    fn passes_on_every_real_probe_with_consistent_output() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut count = 0;
        for e in std::fs::read_dir(root.join(DIR)).unwrap() {
            let name = e.unwrap().file_name().into_string().unwrap();
            let file = format!("{DIR}{name}");
            let text = real(&file);
            assert_passes(&run_one(&file, &text, &text));
            count += 1;
        }
        assert!(count > 0);
    }

    #[test]
    fn fails_when_the_verified_count_changes() {
        let src = real(PROBE);
        let broken = mutate(&src, "// expect: verified 3", "// expect: verified 4");
        assert_fails(&run_one(PROBE, &broken, &src), "期待 Verified(4)");
    }

    #[test]
    fn fails_when_a_rejected_probe_verifies() {
        let src = real(REJECTED);
        // 壊すのは Verus の結果の側：拒まれるはずのプローブが通ったことにする。
        let verifies = mutate(&src, "// expect: rejected", "// expect: verified 1");
        let t = one(REJECTED, &src);
        let fake = consistent(&verifies);
        let r = t.run_ref(|repo| run_with(repo, &fake));
        assert_fails(&r, "期待 Rejected");
    }

    #[test]
    fn fails_when_a_compile_error_probe_compiles() {
        let src = real(COMPILE_ERROR);
        // 壊すのは Verus の結果の側：コンパイルで止まるはずのプローブが通ったことにする。
        let compiles = mutate(&src, "// expect: compile-error 4", "// expect: verified 4");
        let t = one(COMPILE_ERROR, &src);
        let fake = consistent(&compiles);
        let r = t.run_ref(|repo| run_with(repo, &fake));
        assert_fails(&r, "期待 CompileError(4)");
    }

    #[test]
    fn fails_when_a_compile_error_probe_fails_verification() {
        let src = real(COMPILE_ERROR);
        let t = one(COMPILE_ERROR, &src);
        let fails = |_: &str, _: &[String]| {
            Ok(Outcome {
                success: false,
                text: "verification results:: 3 verified, 1 errors\nerror[E0080]\nzero-sized T\n"
                    .to_string(),
            })
        };
        assert_fails(
            &t.run_ref(|repo| run_with(repo, &fails)),
            "期待 CompileError(4)",
        );
    }

    #[test]
    fn fails_when_the_expected_output_is_missing() {
        let src = real(REJECTED);
        let broken = mutate(&src, "duplicate specification", "two specifications");
        assert_fails(&run_one(REJECTED, &broken, &src), "two specifications");
    }

    #[test]
    fn fails_when_a_probe_has_no_expectation() {
        let src = real(PROBE);
        let broken = mutate(&src, "// expect: verified 3\n", "");
        assert_fails(
            &run_one(PROBE, &broken, &src),
            "`// expect: ..` がありません",
        );
    }

    #[test]
    fn fails_on_an_unknown_directive() {
        let src = real(PROBE);
        let broken = mutate(&src, "// expect: verified 3", "// expects: verified 3");
        assert_fails(&run_one(PROBE, &broken, &src), "未知の指示 expects");
    }

    #[test]
    fn fails_when_the_output_has_no_results() {
        let t = one(PROBE, &real(PROBE));
        let silent = |_: &str, _: &[String]| {
            Ok(Outcome {
                success: true,
                text: String::new(),
            })
        };
        assert_fails(&t.run_ref(|repo| run_with(repo, &silent)), "結果 None");
    }

    #[test]
    fn fails_when_there_are_no_probes() {
        let t = one("README.md", "# x\n");
        let fake = consistent(&real(PROBE));
        assert_fails(
            &t.run_ref(|repo| run_with(repo, &fake)),
            "プローブがありません",
        );
    }

    #[test]
    fn fails_on_a_non_probe_file() {
        let mut t = one(PROBE, &real(PROBE));
        t.write("tests/verus-probes/notes.md", "# x\n");
        let fake = consistent(&real(PROBE));
        assert_fails(&t.run_ref(|repo| run_with(repo, &fake)), "notes.md");
    }

    #[test]
    fn fails_when_a_failing_probe_has_no_expected_output() {
        let src = real(REJECTED);
        let line = src
            .lines()
            .find(|l| l.starts_with("// expect-output: "))
            .unwrap();
        let broken = mutate(&src, &format!("{line}\n"), "");
        assert_fails(
            &run_one(REJECTED, &broken, &src),
            "`// expect-output: ..` が要ります",
        );
    }

    #[test]
    fn fails_on_a_directive_outside_the_header() {
        let src = real(PROBE);
        let broken = mutate(
            &src,
            "// expect: verified 3\n",
            "// expect: verified 3\n\n// expect-output: x\n",
        );
        assert_fails(&run_one(PROBE, &broken, &src), "ヘッダの外に指示があります");
        let typo = mutate(
            &src,
            "// expect: verified 3\n",
            "// expect: verified 3\n//expect-output: x\n",
        );
        assert_fails(&run_one(PROBE, &typo, &src), "ヘッダの外に指示があります");
    }

    #[test]
    fn fails_when_an_output_count_changes() {
        let src = real(JSON_FAILURE);
        let broken = mutate(
            &src,
            "// expect-output-count: 1 json_output_failure::bad",
            "// expect-output-count: 2 json_output_failure::bad",
        );
        assert_fails(
            &run_one(JSON_FAILURE, &broken, &src),
            "が 1 回（期待 2 回）",
        );
    }

    #[test]
    fn reads_escaped_newlines() {
        assert_eq!(unescape("a\\nb\\\\c").unwrap(), "a\nb\\c");
        assert!(unescape("a\\tb").is_err());
    }

    #[test]
    fn reads_json_counts() {
        let text =
            "{\n  \"verification-results\": {\n    \"verified\": 2,\n    \"errors\": 1\n  }\n}";
        assert_eq!(counts(text), Some((2, 1)));
        assert_eq!(counts("{}"), None);
    }
}
