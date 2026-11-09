//! 判定の表を作る：`cargo run --locked --example oracle -- <出力先のディレクトリ>`。
//!
//! 通常は `cargo xtask formal-oracle`（作り直してコミットされた表と比べる）と
//! `cargo xtask formal-oracle-write`（`formal/oracle/` に書く）から呼ぶ。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir] = args.as_slice() else {
        eprintln!("使い方: oracle <出力先のディレクトリ>");
        return ExitCode::from(2);
    };
    let t = ebykn_formal_linearity::oracle::build();
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("error: {dir}: {e}");
        return ExitCode::FAILURE;
    }
    for (name, text) in &t.files {
        let path = std::path::Path::new(dir).join(name);
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("error: {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    }
    if !t.undetected.is_empty() {
        eprintln!(
            "error: 表のどのケースでも正しい検査と食い違わない壊れた検査があります（列挙の上限を上げてください）: {}",
            t.undetected.join(", ")
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
