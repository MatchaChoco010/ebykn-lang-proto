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
  check         §6 の検査（M2 ではここまで。書き下しと verus --no-verify は次の段で足す）
  verify        check と Verus による検証（未実装）
  build         check と cargo verus build（未実装）
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
        ["check"] => check(&opts),
        ["check", "--base", _] => not_yet(&opts, "check --base", "M3"),
        ["check", ..] => usage("check の引数が違います"),
        ["verify", ..] => not_yet(&opts, "verify", "M2 の次の段"),
        ["build", ..] => not_yet(&opts, "build", "M2 の次の段"),
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

fn check(opts: &Opts) -> ExitCode {
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
            return ExitCode::from(1);
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
        return ExitCode::from(1);
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
        return ExitCode::from(1);
    }
    if !opts.json {
        let n = report.project.modules.len();
        match opts.lang {
            Lang::En => eprintln!("ebykn: check passed ({n} file(s); not yet lowered to Verus)"),
            Lang::Ja => eprintln!(
                "ebykn: 検査を通りました（{n} ファイル。Verus への書き下しはまだしていません）"
            ),
        }
    }
    ExitCode::SUCCESS
}
