//! プロジェクトの読み込み（仕様 §3）：`Ebykn.toml` とモジュールの探索。
//!
//! モジュールは Rust と同じ規則で探す。`src/lib.eyk`（か `src/main.eyk`）がクレートルートで、
//! `mod foo;` は `foo.eyk` か `foo/mod.eyk`。`mod.eyk` でないファイル `src/a.eyk` の中の `mod b;` は
//! `src/a/b.eyk` か `src/a/b/mod.eyk`、インラインの `mod x { mod y; }` は `x/` の下を探す。
//! 証明ファイル `proof/<p>.eykp` は `src/<p>.eyk` のモジュールに対応づけて読む（§3.1）。
//! まだ実装しない機能（性質ファイル、`[props]`、`[ebykn-dependencies]`）は、黙って無視せず `E0003` にする。

use std::path::{Path, PathBuf};

use ebykn_syntax::ast::{self, ItemKind};
use ebykn_syntax::diag::{CheckMsg, Diagnostic, Message, Unimpl};
use ebykn_syntax::reserved::DeferredName;
use ebykn_syntax::reserved::{RESERVED_CRATE_NAMES, RESERVED_DEPENDENCY_NAMES};
use ebykn_syntax::span::{FileId, SourceFile, Span};
use ebykn_syntax::{parse_eyk, parse_eykp};
use toml::de::{DeTable, DeValue};

/// `Ebykn.toml` の値（`[dependencies]` を `build/verus/Cargo.toml` に写すために持つ）。
#[derive(Debug, Clone, PartialEq)]
pub enum TomlValue {
    Str(String),
    /// 整数・浮動小数点数・日時は書かれたとおりの文字列で持つ。
    Raw(String),
    Bool(bool),
    Array(Vec<TomlValue>),
    Table(Vec<(String, TomlValue)>),
}

/// `Ebykn.toml`（仕様 §3.2）。
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub verus_release: String,
    pub verus_commit: String,
    /// `[dependencies]`（鍵の名前の順）。
    pub dependencies: Vec<(String, TomlValue)>,
}

/// 一つの `.eyk` ファイルに当たるモジュール。
#[derive(Debug, Clone)]
pub struct FileModule {
    pub file: FileId,
    /// クレートルートからのモジュールのパス（ルートは空）。
    pub module: Vec<String>,
    /// プロジェクトのルートからの相対パス（`src/foo/mod.eyk`）。
    pub rel: String,
}

/// 一つの証明ファイル `proof/<p>.eykp` と、対応する `src/<p>.eyk`（仕様 §3.1）。
#[derive(Debug, Clone)]
pub struct ProofFile {
    pub file: FileId,
    /// 対応する `.eyk` のファイル。
    pub src: FileId,
    /// 対応するモジュールのパス（ルートは空）。
    pub module: Vec<String>,
    /// プロジェクトのルートからの相対パス（`proof/foo/mod.eykp`）。
    pub rel: String,
}

/// 読み込んだプロジェクト。
#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
    /// `FileId` の番号で引く。0 は `Ebykn.toml`。
    pub files: Vec<SourceFile>,
    /// `files` と同じ番号の AST（`.eyk` でないファイルは `None`）。
    pub asts: Vec<Option<ast::File>>,
    /// `files` と同じ番号の、構文解析が検査に任せた名前（[`ebykn_syntax::reserved`]）。
    pub deferred: Vec<Vec<(Span, DeferredName)>>,
    /// `files` と同じ番号の、構文解析の診断が出なかったか（`.eyk` / `.eykp` でないファイルは `false`）。
    pub parsed_ok: Vec<bool>,
    /// 見つけた `.eyk`（クレートルートが先頭）。
    pub modules: Vec<FileModule>,
    /// 対応する `.eyk` のある証明ファイル（パスの順）。
    pub proofs: Vec<ProofFile>,
    /// クレートルートが `src/main.eyk` か。
    pub bin: bool,
}

impl Project {
    /// 依存クレートがコードで使われる名前（`-` は `_`）。改名（`package = ..`）があれば鍵の名前、
    /// なければそのクレートの `[lib] name`（なければ `[package] name`）。`[lib] name` は path の依存の
    /// `Cargo.toml` を読んで決める。返すのは（名前の一覧、すべて決められたか）。決められないもの
    /// （registry と git の依存、読めない `Cargo.toml`）があれば 2 つ目は `false` で、呼ぶ側は名前が
    /// 分からないとして扱う（#49）。鍵の名前は、どの場合も一覧に入れる。
    pub fn dependency_crate_names(&self) -> (Vec<String>, bool) {
        let mut names = Vec::new();
        let mut known = true;
        for (key, v) in &self.manifest.dependencies {
            names.push(key.replace('-', "_"));
            let TomlValue::Table(kvs) = v else {
                known = false;
                continue;
            };
            if kvs.iter().any(|(k, _)| k == "package") {
                continue;
            }
            let path = kvs.iter().find_map(|(k, x)| match (k.as_str(), x) {
                ("path", TomlValue::Str(p)) => Some(p.clone()),
                _ => None,
            });
            match path.and_then(|p| lib_name(&self.root.join(p).join("Cargo.toml"))) {
                Some(n) => names.push(n.replace('-', "_")),
                None => known = false,
            }
        }
        (names, known)
    }

    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    pub fn ast(&self, id: FileId) -> Option<&ast::File> {
        self.asts[id.0 as usize].as_ref()
    }

    /// 全モジュール（インラインの `mod` を含む）の項目を、モジュールのパスと一緒に渡す。
    pub fn for_each_module<'a>(&'a self, mut f: impl FnMut(&[String], FileId, &'a [ast::Item])) {
        fn walk<'a>(
            path: &mut Vec<String>,
            file: FileId,
            items: &'a [ast::Item],
            f: &mut impl FnMut(&[String], FileId, &'a [ast::Item]),
        ) {
            f(path, file, items);
            for item in items {
                if let ItemKind::Mod(m) = &item.kind
                    && let Some((_, inner)) = &m.content
                {
                    path.push(m.ident.name.clone());
                    walk(path, file, inner, f);
                    path.pop();
                }
            }
        }
        for m in &self.modules {
            if let Some(ast) = self.ast(m.file) {
                let mut path = m.module.clone();
                walk(&mut path, m.file, &ast.items, &mut f);
            }
        }
    }
}

pub const MANIFEST: &str = "Ebykn.toml";

/// ファイルを読まずに、`src/lib.eyk` 一つだけのプロジェクトを作る（テストと差分テスト用）。
pub fn single(text: &str) -> (Project, Vec<Diagnostic>) {
    let manifest = SourceFile::new(FileId(0), MANIFEST, "");
    let file = SourceFile::new(FileId(1), "src/lib.eyk", text);
    let parsed = parse_eyk(&file);
    let p = Project {
        root: PathBuf::from("."),
        manifest: Manifest {
            name: "single".into(),
            version: "0.1.0".into(),
            edition: "2024".into(),
            ..Manifest::default()
        },
        files: vec![manifest, file],
        asts: vec![None, Some(parsed.file)],
        deferred: vec![Vec::new(), parsed.deferred],
        parsed_ok: vec![false, parsed.diags.is_empty()],
        modules: vec![FileModule {
            file: FileId(1),
            module: Vec::new(),
            rel: "src/lib.eyk".into(),
        }],
        proofs: Vec::new(),
        bin: false,
    };
    (p, parsed.diags)
}

/// ファイルを読まずに、`src/lib.eyk` と `proof/lib.eykp` だけのプロジェクトを作る（テスト用）。
pub fn single_with_proof(src: &str, proof: &str) -> (Project, Vec<Diagnostic>) {
    let (mut p, mut diags) = single(src);
    let file = SourceFile::new(FileId(2), "proof/lib.eykp", proof);
    let parsed = parse_eykp(&file);
    diags.extend(parsed.diags.iter().cloned());
    p.parsed_ok.push(parsed.diags.is_empty());
    p.files.push(file);
    p.asts.push(Some(parsed.file));
    p.deferred.push(parsed.deferred);
    p.proofs.push(ProofFile {
        file: FileId(2),
        src: FileId(1),
        module: Vec::new(),
        rel: "proof/lib.eykp".into(),
    });
    (p, diags)
}

/// `dir` から上へ `Ebykn.toml` を探す。
pub fn find_root(dir: &Path) -> Option<PathBuf> {
    let mut d = Some(dir);
    while let Some(x) = d {
        if x.join(MANIFEST).is_file() {
            return Some(x.to_path_buf());
        }
        d = x.parent();
    }
    None
}

/// `root`（`Ebykn.toml` のあるディレクトリ）のプロジェクトを読む。診断は位置の順ではなく見つけた順。
pub fn load(root: &Path) -> (Project, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let manifest_text = std::fs::read_to_string(root.join(MANIFEST));
    let manifest_file = SourceFile::new(
        FileId(0),
        MANIFEST,
        manifest_text.as_deref().unwrap_or_default(),
    );
    let mut p = Project {
        root: root.to_path_buf(),
        manifest: Manifest::default(),
        files: vec![manifest_file],
        asts: vec![None],
        deferred: vec![Vec::new()],
        parsed_ok: vec![false],
        modules: Vec::new(),
        proofs: Vec::new(),
        bin: false,
    };
    match &manifest_text {
        Ok(text) => p.manifest = read_manifest(&p.files[0], text, &mut diags),
        Err(e) => diags.push(Diagnostic::new(
            p.files[0].eof(),
            Message::Check(CheckMsg::ReadFailed {
                path: MANIFEST.into(),
                detail: e.to_string(),
            }),
        )),
    }
    let mut found = Vec::new();
    walk_dir(&p.root, &p.root, &mut found);
    found.sort();
    scan_unsupported(&mut p, &found, &mut diags);
    let lib = root.join("src/lib.eyk").is_file();
    let main = root.join("src/main.eyk").is_file();
    let at0 = p.files[0].span(0, 0);
    let rel = match (lib, main) {
        (true, true) => {
            diags.push(Diagnostic::new(
                at0,
                Message::Check(CheckMsg::CrateRootAmbiguous),
            ));
            "src/lib.eyk"
        }
        (true, false) => "src/lib.eyk",
        (false, true) => "src/main.eyk",
        (false, false) => {
            diags.push(Diagnostic::new(
                at0,
                Message::Check(CheckMsg::CrateRootMissing),
            ));
            load_proofs(&mut p, &found, &mut diags);
            return (p, diags);
        }
    };
    p.bin = rel == "src/main.eyk";
    // (モジュールのパス, ファイルの相対パス, 子を探すディレクトリ)
    let mut queue = vec![(Vec::new(), rel.to_string(), "src".to_string())];
    while let Some((module, rel, dir)) = queue.pop() {
        let Some(id) = parse_file(&mut p, &rel, false, &mut diags) else {
            continue;
        };
        p.modules.push(FileModule {
            file: id,
            module: module.clone(),
            rel,
        });
        let mut found = Vec::new();
        if let Some(ast) = p.ast(id) {
            collect_mods(&ast.items, &module, &dir, &mut found);
        }
        let mut seen = std::collections::BTreeSet::new();
        for (child, child_dir, span) in found {
            // 同じモジュールを二度宣言すると、同じファイルを二度読むことになる。
            if !seen.insert(child.clone()) {
                diags.push(Diagnostic::new(
                    span,
                    Message::Check(CheckMsg::ModuleDuplicate {
                        name: child.join("::"),
                    }),
                ));
                continue;
            }
            let name = child.last().cloned().unwrap_or_default();
            let a = format!("{child_dir}.eyk");
            let b = format!("{child_dir}/mod.eyk");
            match (root.join(&a).is_file(), root.join(&b).is_file()) {
                (true, true) => diags.push(Diagnostic::new(
                    span,
                    Message::Check(CheckMsg::ModuleAmbiguous { name, a, b }),
                )),
                (true, false) => queue.push((child, a, child_dir)),
                (false, true) => queue.push((child, b, child_dir)),
                (false, false) => diags.push(Diagnostic::new(
                    span,
                    Message::Check(CheckMsg::ModuleNotFound { name, a, b }),
                )),
            }
        }
    }
    // クレートルートを先頭に、あとはパスの順に並べる（出力を決定的にする）。
    p.modules.sort_by(|x, y| x.module.cmp(&y.module));
    load_proofs(&mut p, &found, &mut diags);
    (p, diags)
}

/// 証明ファイルを読む（仕様 §3.1）。`proof/<p>.eykp` は `src/<p>.eyk` のモジュールに対応する。対応する
/// モジュールがなければ E0301、`proof/` の外の `.eykp`（`props/` の下は性質ファイル）と `proof/` の中の
/// `.eyk` は E0302。`found` はプロジェクトのファイルの相対パス（名前の順）。
fn load_proofs(p: &mut Project, found: &[String], diags: &mut Vec<Diagnostic>) {
    for rel in found {
        let in_proof = rel.starts_with("proof/");
        let misplaced = if rel.ends_with(".eykp") {
            !in_proof && !rel.starts_with("props/")
        } else {
            in_proof && rel.ends_with(".eyk")
        };
        if misplaced {
            let id = push_unread(p, rel);
            diags.push(Diagnostic::new(
                p.files[id.0 as usize].span(0, 0),
                Message::Check(CheckMsg::ProofMisplaced {
                    eyk: rel.ends_with(".eyk"),
                }),
            ));
            continue;
        }
        if !(in_proof && rel.ends_with(".eykp")) {
            continue;
        }
        let stem = &rel["proof/".len()..rel.len() - ".eykp".len()];
        let src_rel = format!("src/{stem}.eyk");
        let Some(m) = p.modules.iter().find(|m| m.rel == src_rel).cloned() else {
            let id = push_unread(p, rel);
            diags.push(Diagnostic::new(
                p.files[id.0 as usize].span(0, 0),
                Message::Check(CheckMsg::ProofWithoutSource {
                    exists: found.contains(&src_rel),
                    src: src_rel,
                }),
            ));
            continue;
        };
        let Some(id) = parse_file(p, rel, true, diags) else {
            continue;
        };
        p.proofs.push(ProofFile {
            file: id,
            src: m.file,
            module: m.module,
            rel: rel.clone(),
        });
    }
}

/// 中を読まないファイル（診断の位置にだけ使う）を足す。
fn push_unread(p: &mut Project, rel: &str) -> FileId {
    let id = FileId(p.files.len() as u32);
    p.files.push(SourceFile::new(id, rel, ""));
    p.asts.push(None);
    p.deferred.push(Vec::new());
    p.parsed_ok.push(false);
    id
}

/// `mod foo;` を集める。インラインの `mod x { .. }` の中は `x/` の下を探す。
fn collect_mods(
    items: &[ast::Item],
    module: &[String],
    dir: &str,
    out: &mut Vec<(Vec<String>, String, Span)>,
) {
    for item in items {
        let ItemKind::Mod(m) = &item.kind else {
            continue;
        };
        let mut child = module.to_vec();
        child.push(m.ident.name.clone());
        let child_dir = format!("{dir}/{}", m.ident.name);
        match &m.content {
            None => out.push((child, child_dir, item.span)),
            Some((_, inner)) => collect_mods(inner, &child, &child_dir, out),
        }
    }
}

/// `proof` なら `.eykp` として読む。
fn parse_file(
    p: &mut Project,
    rel: &str,
    proof: bool,
    diags: &mut Vec<Diagnostic>,
) -> Option<FileId> {
    let id = FileId(p.files.len() as u32);
    let text = match std::fs::read_to_string(p.root.join(rel)) {
        Ok(t) => t,
        Err(e) => {
            diags.push(Diagnostic::new(
                p.files[0].span(0, 0),
                Message::Check(CheckMsg::ReadFailed {
                    path: rel.into(),
                    detail: e.to_string(),
                }),
            ));
            return None;
        }
    };
    let file = SourceFile::new(id, rel, text);
    let parsed = if proof {
        parse_eykp(&file)
    } else {
        parse_eyk(&file)
    };
    let edition = p.manifest.edition.clone();
    let kept: Vec<Diagnostic> = parsed
        .diags
        .into_iter()
        .filter(|d| keeps_edition_keyword(d, &edition))
        .collect();
    p.parsed_ok.push(kept.is_empty());
    diags.extend(kept);
    p.files.push(file);
    p.asts.push(Some(parsed.file));
    p.deferred.push(parsed.deferred);
    Some(id)
}

/// 版の予約語の診断（`gen`）を、`Ebykn.toml` の `edition` がその版より前なら捨てる（#104）。
/// ほかの診断はそのまま残す。
fn keeps_edition_keyword(d: &Diagnostic, edition: &str) -> bool {
    match &d.message {
        Message::EditionKeyword { since, .. } => {
            ebykn_syntax::reserved::edition_reserves(edition, *since)
        }
        _ => true,
    }
}

/// まだ読めないファイル（`props/` の下の性質ファイル）を探し、`E0003` にする。
fn scan_unsupported(p: &mut Project, found: &[String], diags: &mut Vec<Diagnostic>) {
    for rel in found {
        if !(rel.starts_with("props/") && (rel.ends_with(".eyk") || rel.ends_with(".eykp"))) {
            continue;
        }
        let id = push_unread(p, rel);
        diags.push(Diagnostic::new(
            p.files[id.0 as usize].span(0, 0),
            Message::Check(CheckMsg::Unimplemented(Unimpl::PropsDir)),
        ));
    }
}

/// 生成物と隠しディレクトリ（`build/`、`.ebykn/`、`.git/` など）と `target/` を除いて、ファイルを集める。
fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = path
            .strip_prefix(root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if path.is_dir() {
            if name.starts_with('.') || rel == "build" || rel == "target" {
                continue;
            }
            walk_dir(root, &path, out);
        } else {
            out.push(rel);
        }
    }
}

const TOP_KEYS: &[&str] = &[
    "package",
    "verus",
    "dependencies",
    "props",
    "ebykn-dependencies",
];

fn read_manifest(file: &SourceFile, text: &str, diags: &mut Vec<Diagnostic>) -> Manifest {
    let mut m = Manifest::default();
    let table = match DeTable::parse(text) {
        Ok(t) => t.into_inner(),
        Err(e) => {
            let span = e
                .span()
                .map(|r| file.span(r.start, r.end))
                .unwrap_or_else(|| file.span(0, 0));
            diags.push(Diagnostic::new(
                span,
                Message::Check(CheckMsg::ManifestParse {
                    detail: e.message().trim().to_string(),
                }),
            ));
            return m;
        }
    };
    let at = |r: std::ops::Range<usize>| file.span(r.start, r.end);
    let push = |diags: &mut Vec<Diagnostic>, span: Span, msg: CheckMsg| {
        diags.push(Diagnostic::new(span, Message::Check(msg)));
    };
    for (k, _) in table.iter() {
        if !TOP_KEYS.contains(&k.get_ref().as_ref()) {
            push(
                diags,
                at(k.span()),
                CheckMsg::ManifestUnknown {
                    key: k.get_ref().to_string(),
                },
            );
        }
    }
    let section = |diags: &mut Vec<Diagnostic>, name: &str| -> Option<&DeTable<'_>> {
        let (k, v) = table.get_key_value(name)?;
        match v.get_ref() {
            DeValue::Table(t) => Some(t),
            _ => {
                diags.push(Diagnostic::new(
                    at(k.span()),
                    Message::Check(CheckMsg::ManifestType {
                        key: name.into(),
                        expected: "table",
                    }),
                ));
                None
            }
        }
    };
    let strings =
        |diags: &mut Vec<Diagnostic>, sec: &str, keys: &[&str], required: &[&str]| -> Vec<String> {
            let mut out = vec![String::new(); keys.len()];
            let Some(t) = section(diags, sec) else {
                if table.get(sec).is_none() {
                    for r in required {
                        diags.push(Diagnostic::new(
                            file.span(0, 0),
                            Message::Check(CheckMsg::ManifestMissing {
                                key: format!("{sec}.{r}"),
                            }),
                        ));
                    }
                }
                return out;
            };
            for (k, v) in t.iter() {
                let key = k.get_ref().as_ref();
                let full = format!("{sec}.{key}");
                let Some(i) = keys.iter().position(|x| *x == key) else {
                    diags.push(Diagnostic::new(
                        at(k.span()),
                        Message::Check(CheckMsg::ManifestUnknown { key: full }),
                    ));
                    continue;
                };
                match v.get_ref() {
                    DeValue::String(s) => out[i] = s.to_string(),
                    _ => diags.push(Diagnostic::new(
                        at(v.span()),
                        Message::Check(CheckMsg::ManifestType {
                            key: full,
                            expected: "string",
                        }),
                    )),
                }
            }
            for r in required {
                if t.get(*r).is_none() {
                    diags.push(Diagnostic::new(
                        file.span(0, 0),
                        Message::Check(CheckMsg::ManifestMissing {
                            key: format!("{sec}.{r}"),
                        }),
                    ));
                }
            }
            out
        };
    let pkg = strings(
        diags,
        "package",
        &["name", "version", "edition"],
        &["name", "version"],
    );
    m.name = pkg[0].clone();
    m.version = pkg[1].clone();
    m.edition = if pkg[2].is_empty() {
        "2024".into()
    } else {
        pkg[2].clone()
    };
    let verus = strings(
        diags,
        "verus",
        &["release", "commit"],
        &["release", "commit"],
    );
    m.verus_release = verus[0].clone();
    m.verus_commit = verus[1].clone();
    if let Some(t) = section(diags, "dependencies") {
        for (k, v) in t.iter() {
            let name = k.get_ref().to_string();
            // std などの名前の依存クレートは、`::core::..` や `use core::..` を依存クレートに向ける
            // （#49。`.eyk` の改名の拒否（#16 の既定）と同じ名前）。
            let key = name.replace('-', "_");
            if RESERVED_CRATE_NAMES.contains(&key.as_str())
                || RESERVED_DEPENDENCY_NAMES.contains(&key.as_str())
            {
                push(
                    diags,
                    at(k.span()),
                    CheckMsg::ReservedDependency { name: name.clone() },
                );
            }
            m.dependencies.push((name, value(v.get_ref())));
        }
    }
    for (name, what) in [
        ("props", Unimpl::PropsTable),
        ("ebykn-dependencies", Unimpl::EbyknDeps),
    ] {
        if let Some((k, _)) = table.get_key_value(name) {
            push(diags, at(k.span()), CheckMsg::Unimplemented(what));
        }
    }
    m
}

/// `Cargo.toml` の `[lib] name`（なければ `[package] name`）。読めなければ `None`。
fn lib_name(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let table = DeTable::parse(&text).ok()?.into_inner();
    let get = |sec: &str| match table.get(sec).map(|v| v.get_ref()) {
        Some(DeValue::Table(t)) => match t.get("name").map(|v| v.get_ref()) {
            Some(DeValue::String(s)) => Some(s.to_string()),
            _ => None,
        },
        _ => None,
    };
    get("lib").or_else(|| get("package"))
}

fn value(v: &DeValue<'_>) -> TomlValue {
    match v {
        DeValue::String(s) => TomlValue::Str(s.to_string()),
        DeValue::Integer(i) => TomlValue::Raw(i.to_string()),
        DeValue::Float(f) => TomlValue::Raw(f.to_string()),
        DeValue::Boolean(b) => TomlValue::Bool(*b),
        DeValue::Datetime(d) => TomlValue::Raw(d.to_string()),
        DeValue::Array(a) => TomlValue::Array(a.iter().map(|x| value(x.get_ref())).collect()),
        DeValue::Table(t) => TomlValue::Table(
            t.iter()
                .map(|(k, v)| (k.get_ref().to_string(), value(v.get_ref())))
                .collect(),
        ),
    }
}
