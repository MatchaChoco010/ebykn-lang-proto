//! 実行関数の `verified` が停止を含まないことを、`ebykn verify --json` と標準エラーの両方が示す（#81）。
//!
//! 固定した Verus が要るので `#[ignore]` にし、`cargo xtask golden-verify`（CI の Verus のジョブ）が
//! `--ignored` で走らせる（`xtask/src/golden.rs` の `IGNORED`）。ケースは `tests/golden/exec_decreases/`。
//! 期待は `expected/status.json`：本体を検証する実行関数の項目にだけ `"termination": "not_proven"` があり、
//! `spec fn`・`unchecked` の項目にはない。`ebykn` のバイナリを走らせるので、一覧（`Output::exec`）を書き下しから
//! `--json` へ渡す道（`main.rs`）も確かめる。

use std::path::{Path, PathBuf};
use std::process::Command;

fn case() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/exec_decreases")
}

#[test]
#[ignore = "固定した Verus が要る。cargo xtask golden-verify が --ignored で走らせる"]
fn verify_json_marks_exec_items_as_termination_not_proven() {
    let dir = case();
    // 依存の版を固定する（golden-verify と同じ）。
    let build = dir.join("build/verus");
    std::fs::create_dir_all(&build).expect("build/verus/ を作る");
    std::fs::copy(
        dir.join("expected/verus/Cargo.lock"),
        build.join("Cargo.lock"),
    )
    .expect("Cargo.lock を写す");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ebykn"));
    for v in [
        "CARGO",
        "RUSTC",
        "RUSTDOC",
        "RUSTUP_TOOLCHAIN",
        "CARGO_TARGET_DIR",
    ] {
        cmd.env_remove(v);
    }
    let out = cmd
        .args(["verify", "--json"])
        .current_dir(&dir)
        .output()
        .expect("ebykn を走らせる");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // `stuck` が failed なので、verify は失敗する。
    assert!(!out.status.success(), "{stdout}\n{stderr}");
    // 標準出力は 1 行に 1 つの JSON（check の要約と verify の結果）。verify の結果の行を比べる。
    let lines: Vec<serde_json::Value> = stdout
        .lines()
        .map(|l| {
            serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("--json の行を読めません: {e}\n{l}\n{stderr}"))
        })
        .collect();
    let verify: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|v| v["$message_type"] == "ebykn-verify")
        .collect();
    assert_eq!(
        verify.len(),
        1,
        "ebykn-verify の行がちょうど 1 つない:\n{stdout}"
    );
    let got = verify[0].clone();
    let want: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("expected/status.json")).expect("expected/status.json"),
    )
    .expect("expected/status.json を読む");
    assert_eq!(
        got,
        want,
        "--json の出力が expected/status.json と違います:\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
    // 期待そのものが、実行関数にだけ termination を付けていることを確かめる（期待の書き損じで通らない）。
    let items = want["items"].as_array().expect("items");
    let marked: Vec<&str> = items
        .iter()
        .filter(|i| i["termination"] == "not_proven")
        .map(|i| i["item"].as_str().unwrap())
        .collect();
    assert!(
        marked.contains(&"exec_decreases::halve_to_zero"),
        "{marked:?}"
    );
    assert!(
        marked.contains(&"exec_decreases::Drain::spin_down"),
        "{marked:?}"
    );
    assert!(!marked.contains(&"exec_decreases::tri"), "{marked:?}");
    assert!(!marked.contains(&"exec_decreases::spin"), "{marked:?}");
    // verify が失敗したので Verus の出力を見せるが、関数の `decreases` を書いた `count` について、`.eyk` では
    // 従えない `#[verifier::allow(..)]` の案内は出ない（書き下しが付けた `allow` で消えている）。
    assert!(
        !stderr.contains("do not guarantee termination"),
        "Verus の decreases の警告が出ている:\n{stderr}"
    );
    assert!(
        stderr.contains("decreases not satisfied"),
        "stuck の失敗が出ていない:\n{stderr}"
    );
    // 人が読む出力にも、停止を含まないことの注記が出る。
    assert!(
        stderr.contains(ebykn::verus::TERMINATION_NOTE_EN),
        "停止の注記がない:\n{stderr}"
    );
}
