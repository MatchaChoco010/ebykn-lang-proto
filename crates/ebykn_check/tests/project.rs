//! プロジェクトの読み込み（仕様 §3）：`Ebykn.toml`、モジュールの探索、未実装のファイル。

use std::path::{Path, PathBuf};

use ebykn_check::project::{TomlValue, load};
use ebykn_syntax::diag::Lang;

const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[verus]\nrelease = \"release/x\"\ncommit = \"abc\"\n";

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn project(name: &str, files: &[(&str, &str)]) -> Tmp {
    let dir = std::env::temp_dir().join(format!("ebykn-project-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    Tmp(dir)
}

fn diags(root: &Path) -> Vec<String> {
    let (p, d) = load(root);
    d.iter()
        .map(|d| d.render(p.file(d.span.file), Lang::En))
        .collect()
}

#[test]
fn finds_modules_by_the_rust_rules() {
    let t = project(
        "mods",
        &[
            ("Ebykn.toml", MANIFEST),
            ("src/lib.eyk", "mod a;\nmod b;\nmod c { mod d; }\n"),
            ("src/a.eyk", "mod x;\n"),
            ("src/a/x.eyk", "pub fn f() {}\n"),
            ("src/b/mod.eyk", "mod y;\n"),
            ("src/b/y.eyk", ""),
            ("src/c/d.eyk", ""),
        ],
    );
    let (p, d) = load(&t.0);
    assert!(d.is_empty(), "{d:?}");
    let found: Vec<(String, String)> = p
        .modules
        .iter()
        .map(|m| (m.module.join("::"), m.rel.clone()))
        .collect();
    assert_eq!(
        found,
        [
            ("".into(), "src/lib.eyk".into()),
            ("a".into(), "src/a.eyk".into()),
            ("a::x".into(), "src/a/x.eyk".into()),
            ("b".into(), "src/b/mod.eyk".into()),
            ("b::y".into(), "src/b/y.eyk".into()),
            ("c::d".into(), "src/c/d.eyk".into()),
        ]
    );
    assert_eq!(p.manifest.name, "p");
    assert_eq!(p.manifest.edition, "2024");
}

#[test]
fn fails_for_missing_and_ambiguous_modules() {
    let t = project(
        "missing",
        &[
            ("Ebykn.toml", MANIFEST),
            ("src/lib.eyk", "mod a;\nmod b;\n"),
            ("src/b.eyk", ""),
            ("src/b/mod.eyk", ""),
        ],
    );
    assert_eq!(
        diags(&t.0),
        [
            "src/lib.eyk:1:1: E0002: file not found for module `a`; create `src/a.eyk` or `src/a/mod.eyk`",
            "src/lib.eyk:2:1: E0002: module `b` has two files, `src/b.eyk` and `src/b/mod.eyk`; remove one",
        ]
    );
}

#[test]
fn fails_without_a_crate_root() {
    let t = project("noroot", &[("Ebykn.toml", MANIFEST)]);
    let d = diags(&t.0);
    assert_eq!(d.len(), 1);
    assert!(d[0].contains("E0002: the crate root is missing"), "{d:?}");
}

#[test]
fn reads_the_manifest_and_rejects_unknown_keys() {
    let t = project(
        "manifest",
        &[
            (
                "Ebykn.toml",
                "[package]\nname = \"p\"\nversion = \"0.1.0\"\nauthors = []\n\n[verus]\nrelease = \"r\"\n\n[dependencies]\nrayon = \"1.10\"\nfoo = { path = \"../foo\", features = [\"a\"] }\n\n[workspace]\n",
            ),
            ("src/lib.eyk", ""),
        ],
    );
    let (p, d) = load(&t.0);
    let d: Vec<String> = d
        .iter()
        .map(|x| x.render(p.file(x.span.file), Lang::En))
        .collect();
    assert_eq!(
        d,
        [
            "Ebykn.toml:13:2: E0002: unknown key `workspace` in `Ebykn.toml`",
            "Ebykn.toml:4:1: E0002: unknown key `package.authors` in `Ebykn.toml`",
            "Ebykn.toml:1:1: E0002: `Ebykn.toml` has no `verus.commit`",
        ]
    );
    // 鍵の名前の順（TOML の構文解析器が並べる順）。
    assert_eq!(
        p.manifest.dependencies,
        [
            (
                "foo".into(),
                TomlValue::Table(vec![
                    (
                        "features".into(),
                        TomlValue::Array(vec![TomlValue::Str("a".into())])
                    ),
                    ("path".into(), TomlValue::Str("../foo".into())),
                ])
            ),
            ("rayon".into(), TomlValue::Str("1.10".into())),
        ]
    );
}

#[test]
fn rejects_unimplemented_files_and_tables() {
    let t = project(
        "unimpl",
        &[
            (
                "Ebykn.toml",
                &format!(
                    "{MANIFEST}\n[props]\nrayon = {{ path = \"../r\" }}\n\n[ebykn-dependencies]\nm = {{ path = \"../m\" }}\n"
                ),
            ),
            ("src/lib.eyk", ""),
            ("proof/lib.eykp", ""),
            ("props/rayon.eyk", ""),
            // 生成物は見ない。
            ("build/verus/x.eykp", ""),
        ],
    );
    let d = diags(&t.0);
    assert_eq!(
        d,
        [
            "Ebykn.toml:9:2: E0003: not implemented yet: `[props]` in `Ebykn.toml` is implemented in M5",
            "Ebykn.toml:12:2: E0003: not implemented yet: `[ebykn-dependencies]` in `Ebykn.toml` is implemented in M5",
            "proof/lib.eykp:1:1: E0003: not implemented yet: proof files (`.eykp`) are implemented in M3",
            "props/rayon.eyk:1:1: E0003: not implemented yet: props files (`props/`) are implemented in M5",
        ]
    );
}

#[test]
fn messages_have_japanese_text() {
    let t = project(
        "ja",
        &[("Ebykn.toml", MANIFEST), ("src/lib.eyk", "mod a;\n")],
    );
    let (p, d) = load(&t.0);
    let ja = d[0].render(p.file(d[0].span.file), Lang::Ja);
    assert_eq!(
        ja,
        "src/lib.eyk:1:1: E0002: モジュール `a` のファイルがありません。`src/a.eyk` か `src/a/mod.eyk` を作ります"
    );
}
