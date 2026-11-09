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
//! // expect-output: <部分文字列>              （任意個。出力に含まれる）
//! // verus-args: <引数 ..>                    （任意。verus に足す引数）
//! ```

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
}

#[derive(Debug)]
pub struct Probe {
    pub file: String,
    pub expect: Expect,
    pub outputs: Vec<String>,
    pub args: Vec<String>,
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
    let mut args = None;
    for (i, line) in text.lines().enumerate() {
        let Some(body) = line.strip_prefix("// ") else {
            break;
        };
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
            "expect-output" => outputs.push(value.to_string()),
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
    Ok(Probe {
        file: file.to_string(),
        expect,
        outputs,
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
    errs
}

pub fn run(repo: &Repo) -> Report {
    let bin = repo
        .read("Cargo.toml")
        .and_then(|t| read_pin(&t))
        .and_then(|p| installed(&p).map(|b| (p, b)));
    match bin {
        Ok((pin, bin)) => run_with(repo, &|file, args| {
            verus(
                &bin.join("verus"),
                &pin.toolchain,
                &repo.root.join(file),
                args,
            )
        }),
        Err(e) => Report {
            errors: vec![e],
            notes: Vec::new(),
        },
    }
}

fn verus(bin: &Path, toolchain: &str, file: &Path, args: &[String]) -> Result<Outcome, String> {
    let out = Command::new(bin)
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

    /// 期待どおりの結果を返す偽の Verus。期待は `original` から読む（壊す前の実ファイル）。
    fn consistent(original: &str) -> impl Fn(&str, &[String]) -> Result<Outcome, String> + use<> {
        let probe = parse("x", original).unwrap();
        move |_, _| {
            let line = match probe.expect {
                Expect::Verified(n) => format!("verification results:: {n} verified, 0 errors"),
                Expect::Errors(a, b) => format!("verification results:: {a} verified, {b} errors"),
                Expect::Rejected => "error: aborting".to_string(),
            };
            Ok(Outcome {
                success: matches!(probe.expect, Expect::Verified(_)),
                text: format!("{line}\n{}\n", probe.outputs.join("\n")),
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
    fn reads_json_counts() {
        let text =
            "{\n  \"verification-results\": {\n    \"verified\": 2,\n    \"errors\": 1\n  }\n}";
        assert_eq!(counts(text), Some((2, 1)));
        assert_eq!(counts("{}"), None);
    }
}
