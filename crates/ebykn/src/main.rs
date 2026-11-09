//! `ebykn` コマンド（仕様 §9）。
//!
//! 終了コード：0 は成功、1 は検査・検証の失敗、2 は使い方の誤り（未実装のサブコマンドを含む）。

use std::path::Path;
use std::process::ExitCode;

use ebykn::{check_project, init};
use ebykn_check::project::{MANIFEST, find_root};
use ebykn_syntax::diag::Lang;

const HELP: &str = "\
Ebykn: .eyk / .eykp を Verus のソースに書き下すトランスパイラ

使い方: ebykn [オプション] <サブコマンド>

サブコマンド:
  init <name>   雛形を <name>/ に作る
  check         §6 の検査、build/verus/ への書き下し（§7）、verus --no-verify
  verify [item] check と Verus による検証。項目ごとの状態と、証明なしの前提（admit / unchecked）を出す
  build         check と cargo verus build（--verified は M5）
  sync          証明ファイルの追従（M3 で実装）
  status        検証の状態（M5 で実装）

オプション:
  --json         診断を rustc 互換の JSON で標準出力に出す
  --lang <en|ja> 診断の言語（既定は en）
  -h, --help     この説明を表示する
  -V, --version  版を表示する
";

struct Opts {
    json: bool,
    lang: Lang,
    rest: Vec<String>,
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("ebykn: {msg}\n\n{HELP}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut opts = Opts {
        json: false,
        lang: Lang::En,
        rest: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--json" => opts.json = true,
            "--lang" => match args.next().as_deref() {
                Some("en") => opts.lang = Lang::En,
                Some("ja") => opts.lang = Lang::Ja,
                _ => return usage("--lang には en か ja を指定します"),
            },
            _ => opts.rest.push(a),
        }
    }
    let rest: Vec<&str> = opts.rest.iter().map(String::as_str).collect();
    match rest.as_slice() {
        [] | ["-h"] | ["--help"] => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        ["-V"] | ["--version"] => {
            println!("ebykn {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["init", name] => match init(Path::new("."), name) {
            Ok(root) => {
                eprintln!("created `{}`", root.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("ebykn: error: {e}");
                ExitCode::from(1)
            }
        },
        ["init", ..] => usage("init には名前を一つ指定します"),
        ["check"] => run(&opts, Step::Check, &[]),
        ["check", "--base", _] => not_yet(&opts, "check --base", "M3"),
        ["check", ..] => usage("check の引数が違います"),
        ["verify", items @ ..] => run(&opts, Step::Verify, items),
        ["build"] => run(&opts, Step::Build, &[]),
        ["build", "--verified"] => {
            // `--verified` は `status`（M5）が要る。黙って `--verified` なしの build にしない。
            let m =
                ebykn_syntax::diag::Message::Check(ebykn_syntax::diag::CheckMsg::Unimplemented(
                    ebykn_syntax::diag::Unimpl::BuildVerified,
                ));
            eprintln!("error[E0003]: {}", m.text(opts.lang));
            ExitCode::from(1)
        }
        ["build", ..] => usage("build の引数が違います"),
        ["sync", ..] => not_yet(&opts, "sync", "M3"),
        ["status", ..] => not_yet(&opts, "status", "M5"),
        _ => usage(&format!("不明な引数です: {}", opts.rest.join(" "))),
    }
}

/// まだないサブコマンド：終了コード 2 と、どのマイルストーンで実装するかの案内。
fn not_yet(opts: &Opts, what: &str, milestone: &str) -> ExitCode {
    match opts.lang {
        Lang::En => eprintln!("ebykn: `{what}` is not implemented yet (planned for {milestone})"),
        Lang::Ja => eprintln!("ebykn: `{what}` は未実装です（{milestone} で実装します）"),
    }
    ExitCode::from(2)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Check,
    Verify,
    Build,
}

fn say(opts: &Opts, en: &str, ja: &str) {
    match opts.lang {
        Lang::En => eprintln!("ebykn: {en}"),
        Lang::Ja => eprintln!("ebykn: {ja}"),
    }
}

/// `check`（§6 → §7 → `verus --no-verify`）、`verify`（`check` ＋ 検証）、`build`（`check` ＋ `cargo verus build`）。
fn run(opts: &Opts, step: Step, items: &[&str]) -> ExitCode {
    let report = match check(opts) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let output = match ebykn::lower_report(&report) {
        Some(Ok(o)) => o,
        Some(Err(e)) => {
            eprintln!("ebykn: error: {e}");
            return ExitCode::from(1);
        }
        None => return ExitCode::from(1),
    };
    let build = match ebykn::verus::write_build(&report.project, &output) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ebykn: error: {e}");
            return ExitCode::from(1);
        }
    };
    let verus = match ebykn::verus::find(opts.lang) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ebykn: error: {e}");
            return ExitCode::from(1);
        }
    };
    let mode = match step {
        Step::Check => ebykn::verus::Mode::NoVerify,
        Step::Verify => ebykn::verus::Mode::Verify,
        Step::Build => ebykn::verus::Mode::Build,
    };
    let r = match ebykn::verus::run(&verus, &build, mode) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ebykn: error: {e}");
            return ExitCode::from(1);
        }
    };
    if step == Step::Check {
        if !r.success {
            eprint!("{}", r.text);
            say(
                opts,
                "`verus --no-verify` failed on the generated Verus",
                "生成した Verus の `verus --no-verify` が失敗しました",
            );
            return ExitCode::from(1);
        }
        say(
            opts,
            &format!(
                "check passed (lowered to `{}`; the self-contained check §6.6 comes with `.eykp` in M3)",
                build.display()
            ),
            &format!(
                "検査を通りました（`{}` に書き下しました。§6.6 の自己完結検査は `.eykp` と一緒に M3 で足します）",
                build.display()
            ),
        );
        return ExitCode::SUCCESS;
    }
    if step == Step::Build {
        if !r.success {
            eprint!("{}", r.text);
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }
    // verify：Verus の診断、項目ごとの状態、未証明の前提（admit と unchecked）を必ず出す。
    if !r.success {
        eprint!("{}", r.text);
    }
    let summary = r.summary();
    let all = ebykn::verus::statuses(
        &output.files,
        &output,
        &r.errors,
        r.checked.as_ref(),
        summary,
    );
    let Some(shown) = ebykn::verus::select(&all, items) else {
        say(
            opts,
            "no item matches the given paths",
            "指定したパスに当たる項目がありません",
        );
        return ExitCode::from(2);
    };
    let unproven: Vec<_> = all.iter().filter(|s| ebykn::verus::assumed(s)).collect();
    let fails = shown
        .iter()
        .any(|s| matches!(s.status, "failed" | "unknown"))
        || !r.success;
    if opts.json {
        println!("{}", ebykn::verus::status_json(&shown, summary, !fails));
    } else {
        print!("{}", ebykn::verus::status_text(&shown, summary));
    }
    if let ebykn::verus::Summary::Stopped { errors } = summary {
        say(
            opts,
            &format!(
                "Verus stopped before verification (a compile error or similar; {errors} error(s) above), so nothing was verified"
            ),
            &format!(
                "コンパイルエラーなどで Verus が検証の前に止まったので、何も検証していません（上のエラー {errors} 件）"
            ),
        );
    }
    if !unproven.is_empty() {
        say(
            opts,
            &format!(
                "{} item(s) are assumed without proof (admit: lemma without a body; unchecked; external: #[test], not seen by Verus): {}",
                unproven.len(),
                unproven
                    .iter()
                    .map(|s| format!("{} ({})", s.item, s.status))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            &format!(
                "{} 個の項目は証明なしの前提です（admit：本体のない補題、unchecked、external：Verus が見ない #[test]）: {}",
                unproven.len(),
                unproven
                    .iter()
                    .map(|s| format!("{}（{}）", s.item, s.status))
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        );
    }
    if shown.iter().any(|s| s.status == "unknown") {
        say(
            opts,
            "`unknown`: Verus did not verify these items (for example, removed by `#[cfg]`), or an error could not be attributed to an item",
            "`unknown`：Verus がこの項目を検証していない（`#[cfg]` で消えた、など）か、どの項目のものか決められないエラーがあります",
        );
    }
    if fails {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// §6 の検査。エラーがあれば診断を出して終了コードを返す。
fn check(opts: &Opts) -> Result<ebykn::CheckReport, ExitCode> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let Some(root) = find_root(&cwd) else {
        if opts.json {
            use ebykn_syntax::diag::{CheckMsg, Diagnostic, Message};
            use ebykn_syntax::span::{FileId, SourceFile};
            let file = SourceFile::new(FileId(0), MANIFEST, "");
            let d = Diagnostic::new(
                file.span(0, 0),
                Message::Check(CheckMsg::ManifestNotFound {
                    dir: cwd.display().to_string(),
                }),
            );
            println!("{}", d.to_json(&file, opts.lang));
            return Err(ExitCode::from(1));
        }
        match opts.lang {
            Lang::En => eprintln!(
                "error[E0002]: `{MANIFEST}` was not found in `{}` or its parent directories",
                cwd.display()
            ),
            Lang::Ja => eprintln!(
                "error[E0002]: `{}` とその上のディレクトリに `{MANIFEST}` がありません",
                cwd.display()
            ),
        }
        return Err(ExitCode::from(1));
    };
    let report = check_project(&root);
    if opts.json {
        for line in report.json(opts.lang) {
            println!("{line}");
        }
        // 実行しなかった段階を機械可読にも出す（テキストでは最後の行で伝える）。
        println!("{}", report.summary_json());
    } else {
        for line in report.render(opts.lang) {
            eprintln!("{line}");
        }
    }
    let errors = report
        .diags
        .iter()
        .filter(|d| !d.code().is_warning())
        .count();
    if errors > 0 {
        if !opts.json {
            match opts.lang {
                Lang::En => eprintln!("ebykn: check failed with {errors} error(s)"),
                Lang::Ja => eprintln!("ebykn: 検査が {errors} 件のエラーで失敗しました"),
            }
        }
        return Err(ExitCode::from(1));
    }
    Ok(report)
}
