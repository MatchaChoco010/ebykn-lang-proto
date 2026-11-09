//! `cargo xtask golden-bless [<ケース>...]` の本体。golden の書き下しの期待（`tests/golden/<ケース>/expected/verus/`
//! の `src/` と `Cargo.toml`）を、今の書き下しの生成物から作り直す。ケースを省くと、`expected/verus/` のある
//! 全ケースを作り直す。名前を挙げたケースに `expected/verus/` がなければ作る。
//!
//! 作り直しと比べ方は `tests/golden_lowered/mod.rs`（`lowered_verus_matches_expected` と同じもの）にある。
//! 作り直したら、差分を読んで意図した変更だけかを確かめ、`cargo xtask golden-verify` で `expected/status.txt`
//! が変わらないことを確かめる（`ebykn-pr` のスキル）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[path = "../tests/golden_lowered/mod.rs"]
mod golden_lowered;

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden");
    let names: Vec<String> = std::env::args().skip(1).collect();
    let cases: Vec<PathBuf> = if names.is_empty() {
        let mut all: Vec<PathBuf> = match std::fs::read_dir(&root) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.join("expected/verus").is_dir())
                .collect(),
            Err(e) => {
                eprintln!("error: golden-bless: {} を読めません: {e}", root.display());
                return ExitCode::FAILURE;
            }
        };
        all.sort();
        all
    } else {
        names.iter().map(|n| root.join(n)).collect()
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
