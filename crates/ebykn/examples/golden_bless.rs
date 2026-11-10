//! `cargo xtask golden-bless [<ケース>...]` の本体。golden の書き下しの期待（`tests/golden/<ケース>/expected/verus/`
//! の `src/` と `Cargo.toml`）を、今の書き下しの生成物から作り直す。ケースを省くと、`expected/verus/` のある
//! 全ケースを作り直す。名前を挙げたケースに `expected/verus/` がなければ作る（`Cargo.lock` は作らない。
//! 新しいケースの作り方は `ebykn-pr` のスキル）。
//!
//! 作り直しと比べ方は `tests/golden_lowered/mod.rs`（`lowered_verus_matches_expected` と同じもの）にある。
//! 作り直したら、差分を読んで意図した変更だけかを確かめ、`cargo xtask golden-verify` が通る（`expected/status.txt`
//! と一致する）ことを確かめる（`ebykn-pr` のスキル）。`expected/status.txt` は作り直さない。

use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

#[path = "../tests/golden_lowered/mod.rs"]
mod golden_lowered;

/// `expected/verus/` のある全ケース。読めない項目は黙って飛ばさず失敗にする。
fn all_cases(root: &Path) -> Result<Vec<PathBuf>, String> {
    let rd =
        std::fs::read_dir(root).map_err(|e| format!("{} を読めません: {e}", root.display()))?;
    let mut all = Vec::new();
    for e in rd {
        let p = e
            .map_err(|e| format!("{} を読めません: {e}", root.display()))?
            .path();
        match std::fs::metadata(p.join("expected/verus")) {
            Ok(m) if m.is_dir() => all.push(p),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}/expected/verus を読めません: {e}", p.display())),
        }
    }
    all.sort();
    Ok(all)
}

/// 名前で挙げたケース。`tests/golden/` の直下の名前（ふつうの部分 1 つ）だけを受け付ける。
fn named_cases(root: &Path, names: &[String]) -> Result<Vec<PathBuf>, String> {
    names
        .iter()
        .map(|n| {
            let mut parts = Path::new(n).components();
            match (parts.next(), parts.next()) {
                (Some(Component::Normal(_)), None) => Ok(root.join(n)),
                _ => Err(format!(
                    "ケースは tests/golden/ の直下の名前で挙げてください（`{n}` は使えません）"
                )),
            }
        })
        .collect()
}

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden");
    let names: Vec<String> = std::env::args().skip(1).collect();
    let cases = if names.is_empty() {
        all_cases(&root)
    } else {
        named_cases(&root, &names)
    };
    let cases = match cases {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: golden-bless: {e}");
            return ExitCode::FAILURE;
        }
    };
    if cases.is_empty() {
        eprintln!("error: golden-bless: expected/verus/ のあるケースがありません");
        return ExitCode::FAILURE;
    }
    let mut ok = true;
    for dir in &cases {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        if !dir.join("Ebykn.toml").is_file() {
            eprintln!("error: golden-bless: {name}: tests/golden/{name}/Ebykn.toml がありません");
            ok = false;
            continue;
        }
        match golden_lowered::bless(dir) {
            Ok(b) if b.written.is_empty() && b.removed.is_empty() => {
                eprintln!("ok: golden-bless: {name}: 変更なし")
            }
            Ok(b) => {
                for f in &b.written {
                    eprintln!("ok: golden-bless: {name}: expected/verus/{f} を書きました");
                }
                for f in &b.removed {
                    eprintln!("ok: golden-bless: {name}: expected/verus/{f} を消しました");
                }
            }
            Err(e) => {
                eprintln!("error: golden-bless: {e}");
                ok = false;
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
