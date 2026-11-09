//! テストの道具：一つのファイルを検査し、診断を短い形で返す。
#![allow(dead_code)]

use std::path::PathBuf;

use ebykn_check::{Checked, check, project, sort_diags};
use ebykn_syntax::diag::Lang;

/// 構造体の定義など、多くのテストで使う前置き。
pub const PRELUDE: &str = "\
pub struct Node { pub val: u64 }
pub struct Pool { len: u64 } owns { nodes: Region<Ptr<Node>> } spec { order: Seq<Ptr<Node>> }
";

pub fn run(src: &str) -> (Checked, Vec<String>) {
    let (p, mut diags) = project::single(src);
    let checked = check(&p);
    diags.extend(checked.diags.iter().cloned());
    sort_diags(&mut diags);
    let lines = diags
        .iter()
        .map(|d| {
            let f = p.file(d.span.file);
            d.render(f, Lang::En)
        })
        .collect();
    (checked, lines)
}

/// 診断のコードの列（位置の順）。
pub fn codes(src: &str) -> Vec<String> {
    let (c, lines) = run(src);
    let _ = c;
    lines
        .iter()
        .map(|l| l.split(": ").nth(1).unwrap_or_default().to_string())
        .collect()
}

/// 前置きを付けて、`impl Pool { .. }` の中に `body` を置く。
pub fn in_pool(body: &str) -> String {
    format!("{PRELUDE}impl Pool {{\n{body}\n}}\n")
}

#[track_caller]
pub fn accepts(src: &str) {
    let (_, lines) = run(src);
    assert!(
        lines.is_empty(),
        "診断が出た:\n{}\n---\n{src}",
        lines.join("\n")
    );
}

/// ちょうど一つの診断 `code` が出て、その文言が `needle` を含む。
#[track_caller]
pub fn rejects(src: &str, code: &str, needle: &str) {
    let (_, lines) = run(src);
    assert_eq!(
        lines.len(),
        1,
        "診断が一つではない:\n{}\n---\n{src}",
        lines.join("\n")
    );
    assert!(
        lines[0].contains(&format!(": {code}: ")) && lines[0].contains(needle),
        "{code} / {needle:?} でない: {}\n---\n{src}",
        lines[0]
    );
}

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `edition`（`None` なら書かない）の `Ebykn.toml` と `src/lib.eyk` のプロジェクトを読み、診断を返す。
/// `tag` はディレクトリの名前を分けるためのもの（テストごとに違う値にする）。
pub fn diags_in_edition(tag: &str, edition: Option<&str>, src: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("ebykn-edition-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let t = Tmp(dir);
    std::fs::create_dir_all(t.0.join("src")).unwrap();
    let edition = edition.map_or(String::new(), |e| format!("edition = \"{e}\"\n"));
    std::fs::write(
        t.0.join("Ebykn.toml"),
        format!(
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n{edition}\n[verus]\nrelease = \"release/x\"\ncommit = \"abc\"\n"
        ),
    )
    .unwrap();
    std::fs::write(t.0.join("src/lib.eyk"), src).unwrap();
    let (p, d) = project::load(&t.0);
    d.iter()
        .map(|d| d.render(p.file(d.span.file), Lang::En))
        .collect()
}
