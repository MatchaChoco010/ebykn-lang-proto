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
            "props/rayon.eyk:1:1: E0003: not implemented yet: props files (`props/`) are implemented in M5",
        ]
    );
}

/// 証明ファイルは `src/` の対応するモジュールと組にして読む（仕様 §3.1）。
#[test]
fn proof_files_belong_to_source_modules() {
    let t = project(
        "proofs",
        &[
            ("Ebykn.toml", MANIFEST),
            ("src/lib.eyk", "mod a;\nmod b;\n"),
            ("src/a.eyk", "pub fn f() {}\n"),
            ("src/b/mod.eyk", ""),
            ("src/orphan.eyk", ""),
            ("proof/lib.eykp", "mod a;\nmod b;\n"),
            ("proof/a.eykp", "pub fn f() { proof { } }\n"),
            ("proof/b/mod.eykp", ""),
            // 対応する .eyk がない、またはモジュールでない。
            ("proof/c.eykp", ""),
            ("proof/b.eykp", ""),
            ("proof/orphan.eykp", ""),
            // proof/ の外の .eykp と、proof/ の中の .eyk。
            ("src/a.eykp", ""),
            ("other/x.eykp", ""),
            ("proof/x.eyk", ""),
            // 隠しディレクトリと生成物は見ない。
            (".ebykn/sync/x.eykp", ""),
            ("build/verus/x.eykp", ""),
        ],
    );
    let (p, d) = load(&t.0);
    let lines: Vec<String> = d
        .iter()
        .map(|d| d.render(p.file(d.span.file), Lang::En))
        .collect();
    assert_eq!(
        lines,
        [
            "other/x.eykp:1:1: E0302: a proof file (`.eykp`) must be under `proof/`; `proof/<p>.eykp` belongs to `src/<p>.eyk`",
            "proof/b.eykp:1:1: E0301: this proof file has no source file `src/b.eyk`; `proof/<p>.eykp` belongs to the module in `src/<p>.eyk`",
            "proof/c.eykp:1:1: E0301: this proof file has no source file `src/c.eyk`; `proof/<p>.eykp` belongs to the module in `src/<p>.eyk`",
            "proof/orphan.eykp:1:1: E0301: `src/orphan.eyk`, which this proof file belongs to, is not a module of the crate; declare it with `mod`",
            "proof/x.eyk:1:1: E0302: a source file (`.eyk`) cannot be under `proof/`; put the source in `src/`, and the proof in `proof/` with the extension `.eykp`",
            "src/a.eykp:1:1: E0302: a proof file (`.eykp`) must be under `proof/`; `proof/<p>.eykp` belongs to `src/<p>.eyk`",
        ]
    );
    let pairs: Vec<(String, String, String)> = p
        .proofs
        .iter()
        .map(|x| {
            (
                x.rel.clone(),
                p.file(x.src).path.clone(),
                x.module.join("::"),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            ("proof/a.eykp".into(), "src/a.eyk".into(), "a".into()),
            (
                "proof/b/mod.eykp".into(),
                "src/b/mod.eyk".into(),
                "b".into()
            ),
            ("proof/lib.eykp".into(), "src/lib.eyk".into(), "".into()),
        ]
    );
    // `.eykp` は §5.7 の証明構文を読む。
    let a = p.proofs.iter().find(|x| x.rel == "proof/a.eykp").unwrap();
    assert!(p.parsed_ok[a.file.0 as usize]);
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

#[test]
fn fails_when_a_module_is_declared_twice() {
    let t = project(
        "dupmod",
        &[
            ("Ebykn.toml", MANIFEST),
            ("src/lib.eyk", "mod a;\nmod a;\n"),
            ("src/a.eyk", ""),
        ],
    );
    let (p, d) = load(&t.0);
    let d: Vec<String> = d
        .iter()
        .map(|x| x.render(p.file(x.span.file), Lang::En))
        .collect();
    assert_eq!(d, ["src/lib.eyk:2:1: E0002: module `a` is declared twice"]);
    // ファイルは一度だけ読む。
    assert_eq!(p.modules.len(), 2);
}

#[test]
fn file_level_cfg_gates_the_module() {
    let t = project(
        "filecfg",
        &[
            ("Ebykn.toml", MANIFEST),
            (
                "src/lib.eyk",
                "mod m;\npub fn f(x: bool) { m::requires(x); }\n",
            ),
            ("src/m.eyk", "#![cfg(test)]\npub fn requires(x: bool) {}\n"),
        ],
    );
    let (p, mut d) = load(&t.0);
    let c = ebykn_check::check(&p);
    d.extend(c.diags);
    let d: Vec<String> = d
        .iter()
        .map(|x| x.render(p.file(x.span.file), Lang::En))
        .collect();
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].contains("`m::requires(..)` is Verus syntax"), "{d:?}");
}

/// std などの名前の依存クレートは書けない（#49。`::core::..` や `use core::..` が依存クレートを指す）。
#[test]
fn rejects_dependencies_named_like_std() {
    for name in [
        "std",
        "core",
        "alloc",
        "vstd",
        "ebykn_prelude",
        "ebykn-prelude",
        "verus_builtin",
        "verus-builtin-macros",
    ] {
        let t = project(
            &format!("dep-{name}"),
            &[
                (
                    "Ebykn.toml",
                    &format!(
                        "{MANIFEST}\n[dependencies]\n{name} = {{ path = \"../x\", package = \"x\" }}\n"
                    ),
                ),
                ("src/lib.eyk", ""),
            ],
        );
        let d = diags(&t.0);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(
            d[0].contains("E0002") && d[0].contains(&format!("dependency `{name}`")),
            "{d:?}"
        );
    }
}

/// 依存クレートがコードで使われる名前：改名は鍵、path の依存は `[lib] name`（なければ
/// `[package] name`）。registry の依存と読めない `Cargo.toml` があれば、すべては分からない（#49）。
#[test]
fn reads_the_names_dependencies_are_used_by() {
    let t = project(
        "names",
        &[
            (
                "Ebykn.toml",
                &format!(
                    "{MANIFEST}\n[dependencies]\ninnocent = {{ path = \"innocent\" }}\nplain = {{ path = \"plain\" }}\nren = {{ path = \"innocent\", package = \"innocent\" }}\n"
                ),
            ),
            ("src/lib.eyk", ""),
            (
                "innocent/Cargo.toml",
                "[package]\nname = \"innocent\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"clippy\"\nproc-macro = true\n",
            ),
            (
                "plain/Cargo.toml",
                "[package]\nname = \"plain-crate\"\nversion = \"0.1.0\"\n",
            ),
        ],
    );
    let (p, d) = load(&t.0);
    assert!(d.is_empty(), "{d:?}");
    let (names, known) = p.dependency_crate_names();
    assert!(known);
    for want in ["innocent", "clippy", "plain", "plain_crate", "ren"] {
        assert!(names.iter().any(|n| n == want), "{want}: {names:?}");
    }
    // registry の依存の `[lib] name` は分からない。
    let t = project(
        "names-registry",
        &[
            (
                "Ebykn.toml",
                &format!("{MANIFEST}\n[dependencies]\nserde = \"1\"\n"),
            ),
            ("src/lib.eyk", ""),
        ],
    );
    let (p, _) = load(&t.0);
    assert!(!p.dependency_crate_names().1);
    // 読めない `Cargo.toml` も。
    let t = project(
        "names-missing",
        &[
            (
                "Ebykn.toml",
                &format!("{MANIFEST}\n[dependencies]\ngone = {{ path = \"gone\" }}\n"),
            ),
            ("src/lib.eyk", ""),
        ],
    );
    let (p, _) = load(&t.0);
    assert!(!p.dependency_crate_names().1);
}
