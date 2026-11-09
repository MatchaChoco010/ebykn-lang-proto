//! `ebykn` コマンド（仕様 §9）。サブコマンドは各マイルストーンで足す。

use std::process::ExitCode;

const HELP: &str = "\
Ebykn: .eyk / .eykp を Verus のソースに書き下すトランスパイラ

使い方: ebykn [オプション]

オプション:
  -h, --help     この説明を表示する
  -V, --version  版を表示する
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["-h"] | ["--help"] => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        ["-V"] | ["--version"] => {
            println!("ebykn {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("ebykn: 不明な引数です: {}\n\n{HELP}", args.join(" "));
            // 使い方の誤りは終了コード 2（仕様 §9）。
            ExitCode::from(2)
        }
    }
}
