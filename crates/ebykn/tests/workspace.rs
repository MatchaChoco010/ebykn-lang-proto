//! #78：Cargo のワークスペースの下に置いたプロジェクトでも、書き出した `build/verus/` を cargo が読める。
//!
//! 生成する `build/verus/Cargo.toml` が空の `[workspace]` を持たないと、cargo は上のディレクトリの
//! `Cargo.toml` をワークスペースとみなして止まる（`current package believes it's in a workspace when
//! it's not`）。Verus は要らないので、Verus なしの `cargo metadata` で確かめる。

use std::path::Path;
use std::process::Command;

/// `root/` に Cargo のワークスペースを作り、その下の `root/app/` に `ebykn init` の雛形を書き下して、
/// `build/verus/` で `cargo metadata --no-deps` を走らせる。走らせた結果を返す。
fn metadata_under_workspace(root: &Path, workspace: &str) -> std::process::Output {
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("Cargo.toml"), workspace).unwrap();
    let app = ebykn::init(root, "app").expect("雛形を作る");
    let report = ebykn::check_project(&app);
    assert!(
        !report.has_errors(),
        "{:?}",
        report.render(ebykn_syntax::diag::Lang::En)
    );
    let output = ebykn::lower_report(&report)
        .expect("書き下す")
        .expect("書き下せる");
    let build = ebykn::verus::write_build(&report.project, &output).expect("build/verus/ に書く");
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    for v in ["CARGO_TARGET_DIR", "CARGO_MANIFEST_DIR"] {
        cmd.env_remove(v);
    }
    cmd.args([
        "metadata",
        "--no-deps",
        "--format-version",
        "1",
        "--offline",
    ])
    .current_dir(&build)
    .output()
    .expect("cargo を起動する")
}

#[test]
fn build_crate_under_a_cargo_workspace_is_its_own_workspace() {
    let root = std::env::temp_dir().join(format!("ebykn-ws-{}", std::process::id()));
    let out = metadata_under_workspace(&root, "[workspace]\nmembers = []\n");
    let _ = std::fs::remove_dir_all(&root);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    // ワークスペースの根は、生成したクレート自身。
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("cargo metadata の JSON");
    let ws = v["workspace_root"].as_str().expect("workspace_root");
    assert!(ws.ends_with("app/build/verus"), "{ws}");
}
