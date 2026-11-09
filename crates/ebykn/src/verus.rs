//! 固定した Verus の呼び出し（仕様 §9 の `check` の後半、`verify`、`build`）。
//!
//! Verus は `Ebykn.toml` の `[verus]` と同じ固定値のものを使う（`ebykn` をビルドしたときの
//! ルートの `Cargo.toml` の `[workspace.metadata.verus]`。`check` が一致を確かめる）。探す場所は
//! 環境変数 `EBYKN_VERUS`（`verus` と `cargo-verus` のあるディレクトリ）か、`cargo xtask verus-install`
//! が入れるキャッシュ（`$XDG_CACHE_HOME/ebykn/verus/<版>/verus-x86-linux`）。版・コミット・toolchain が
//! 違えば失敗する。

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use ebykn_check::project::Project;
use ebykn_lower::{Deps, OutFile, Output};

use crate::{VerusPin, verus_pin};

/// 見つけた Verus。
pub struct Verus {
    pub dir: PathBuf,
    pub pin: VerusPin,
}

/// 固定した Verus を探し、`verus --version` で版を確かめる。
pub fn find(lang: ebykn_syntax::diag::Lang) -> Result<Verus, String> {
    let ja = lang == ebykn_syntax::diag::Lang::Ja;
    let pin = verus_pin();
    let dir = match std::env::var_os("EBYKN_VERUS") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => {
            let base = match std::env::var_os("XDG_CACHE_HOME") {
                Some(d) if !d.is_empty() => PathBuf::from(d),
                _ => PathBuf::from(
                    std::env::var_os("HOME").ok_or("HOME も XDG_CACHE_HOME もありません")?,
                )
                .join(".cache"),
            };
            base.join("ebykn/verus")
                .join(&pin.version)
                .join("verus-x86-linux")
        }
    };
    let verus = dir.join("verus");
    if !verus.is_file() {
        return Err(if ja {
            format!(
                "固定した Verus（{}）が `{}` にありません。`cargo xtask verus-install` で入れるか、`EBYKN_VERUS` にそのディレクトリを指定します",
                pin.release,
                dir.display()
            )
        } else {
            format!(
                "the pinned Verus ({}) was not found at `{}`; install it with `cargo xtask verus-install` or set `EBYKN_VERUS` to its directory",
                pin.release,
                dir.display()
            )
        });
    }
    let out = Command::new(&verus)
        .args(["--version", "--output-json"])
        .env("RUSTUP_TOOLCHAIN", &pin.toolchain)
        .output()
        .map_err(|e| {
            if ja {
                format!("`{}` を起動できません: {e}", verus.display())
            } else {
                format!("cannot run `{}`: {e}", verus.display())
            }
        })?;
    let text = String::from_utf8_lossy(&out.stdout);
    if let Err(key) = check_version(&text, &pin) {
        return Err(if ja {
            format!(
                "`{}` の Verus は固定したものではありません（{key} が違います）:\n{text}",
                dir.display()
            )
        } else {
            format!(
                "the Verus at `{}` is not the pinned one ({key} differs):\n{text}",
                dir.display()
            )
        });
    }
    Ok(Verus { dir, pin })
}

/// `verus --version --output-json` の版・コミットが固定値と完全に一致し、toolchain が固定値で始まるか
/// （toolchain の値には「overridden by environment variable」が続く）。違えば違った鍵を返す。
pub fn check_version(text: &str, pin: &VerusPin) -> Result<(), &'static str> {
    let field = |key: &str| -> Option<String> {
        let at = text.find(&format!("\"{key}\": \""))? + key.len() + 5;
        let rest = &text[at..];
        Some(rest[..rest.find('"')?].to_string())
    };
    if field("version").as_deref() != Some(pin.version.as_str()) {
        return Err("version");
    }
    if field("commit").as_deref() != Some(pin.commit.as_str()) {
        return Err("commit");
    }
    let tc = field("toolchain").unwrap_or_default();
    if tc != pin.toolchain && !tc.starts_with(&format!("{} ", pin.toolchain)) {
        return Err("toolchain");
    }
    Ok(())
}

/// `ebykn_prelude` の置き場所。環境変数 `EBYKN_PRELUDE` か、`ebykn` をビルドしたリポジトリの
/// `crates/ebykn_prelude`。
pub fn prelude_dir() -> PathBuf {
    match std::env::var_os("EBYKN_PRELUDE") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("../ebykn_prelude"),
    }
}

/// `from` から `to` への相対パス（両方を正規化できなければ `to` の絶対パス）。
pub fn relative(from: &Path, to: &Path) -> String {
    let (Ok(a), Ok(b)) = (from.canonicalize(), to.canonicalize()) else {
        return to.display().to_string();
    };
    let a: Vec<Component> = a.components().collect();
    let b: Vec<Component> = b.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = vec!["..".into(); a.len() - common];
    parts.extend(
        b[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// `build/verus/` に書き出す（前の `src/` は消す）。書き出したディレクトリを返す。
pub fn write_build(project: &Project, output: &Output) -> Result<PathBuf, String> {
    let dir = project.root.join("build/verus");
    let src = dir.join("src");
    if src.exists() {
        std::fs::remove_dir_all(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    }
    std::fs::create_dir_all(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    let pin = verus_pin();
    let prelude = relative(&dir, &prelude_dir());
    let toml = ebykn_lower::cargo_toml(
        project,
        &Deps {
            vstd: &pin.vstd,
            prelude_path: &prelude,
        },
    );
    std::fs::write(dir.join("Cargo.toml"), toml).map_err(|e| format!("Cargo.toml: {e}"))?;
    for f in &output.files {
        let p = dir.join(&f.rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&p, &f.text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(dir)
}

/// 何を走らせるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `verus --no-verify`（型と名前解決だけ）。
    NoVerify,
    /// 検証する。
    Verify,
    /// `cargo verus build`。
    Build,
}

/// Verus の実行の結果。
pub struct Run {
    pub success: bool,
    /// Verus（と cargo）の出力。生成コードの位置に `（生成コード）` を付けてある。
    pub text: String,
    /// 検証の件数（`--output-json` の `verification-results` か、`verification results::` の行）。
    pub results: Option<(u64, u64)>,
    /// エラーごとの主な位置（`build/verus/` からの相対パスと行）。生成コードの外か位置がないものは `None`。
    pub errors: Vec<Option<(String, usize)>>,
    /// Verus が検証した関数（`--output-json` の `func-details` の鍵）。
    pub checked: Option<std::collections::BTreeSet<String>>,
}

pub fn run(verus: &Verus, build: &Path, mode: Mode) -> Result<Run, String> {
    let path = std::env::join_paths(std::iter::once(verus.dir.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .map_err(|e| e.to_string())?;
    let mut cmd = Command::new("cargo");
    for v in [
        "CARGO",
        "RUSTC",
        "RUSTDOC",
        "CARGO_TARGET_DIR",
        "CARGO_MANIFEST_DIR",
    ] {
        cmd.env_remove(v);
    }
    // 診断の位置を読むので、色の制御文字を出させない（CI は `CARGO_TERM_COLOR=always`）。
    cmd.current_dir(build)
        .env("RUSTUP_TOOLCHAIN", &verus.pin.toolchain)
        .env("CARGO_TERM_COLOR", "never")
        .env("PATH", path);
    match mode {
        Mode::NoVerify => cmd.args(["verus", "focus", "--", "--no-verify"]),
        Mode::Verify => cmd.args([
            "verus",
            "focus",
            "--",
            "--triggers-mode",
            "silent",
            "--output-json",
        ]),
        Mode::Build => cmd.args(["verus", "build"]),
    };
    // 前の成果物が残っていると cargo が Verus を呼ばない（結果の行が出ない）ので、このクレートだけ消す。
    // `cargo verus focus` は `target/verus-partial` にも成果物を置く。
    let name = package_name(build).unwrap_or_default();
    for target in ["target", "target/verus-partial"] {
        let _ = Command::new("cargo")
            .current_dir(build)
            .env("RUSTUP_TOOLCHAIN", &verus.pin.toolchain)
            .args(["clean", "--quiet", "-p", &name, "--target-dir", target])
            .status();
    }
    let out = cmd
        .output()
        .map_err(|e| format!("cannot run `cargo verus`: {e}"))?;
    let stdout = strip_ansi(&String::from_utf8_lossy(&out.stdout));
    let stderr = strip_ansi(&String::from_utf8_lossy(&out.stderr));
    let json = parse_json(&stdout);
    let (text, errors) = read_diagnostics(&format!("{stdout}{stderr}"), json.is_some());
    let results = match &json {
        Some(j) => j.results,
        None => results_line(&format!("{stdout}{stderr}")),
    };
    Ok(Run {
        success: out.status.success(),
        results,
        text,
        errors,
        checked: json.map(|j| j.checked),
    })
}

/// `--output-json` の出力。
struct Json {
    results: Option<(u64, u64)>,
    checked: std::collections::BTreeSet<String>,
}

/// 標準出力の JSON（`{` から最後の `}` まで）を読む。
fn parse_json(stdout: &str) -> Option<Json> {
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(&stdout[start..=end]).ok()?;
    let checked = v
        .get("func-details")?
        .as_object()?
        .keys()
        .cloned()
        .collect();
    let r = v.get("verification-results");
    let num = |k: &str| r.and_then(|r| r.get(k)).and_then(|x| x.as_u64());
    let results = match (num("verified"), num("errors")) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };
    Some(Json { results, checked })
}

/// 診断を読む：生成コードの位置に `（生成コード）` を付け、エラーごとの主な位置を集める。
/// `json` のときは標準出力の JSON の部分を本文から除く。
fn read_diagnostics(raw: &str, json: bool) -> (String, Vec<Option<(String, usize)>>) {
    let mut errors: Vec<Option<(String, usize)>> = Vec::new();
    let mut text = String::new();
    // 今のエラーの主な位置をまだ読んでいないか。
    let mut pending = false;
    let mut depth = 0i64;
    for line in raw.lines() {
        if json {
            // JSON の部分（`{` で始まり、`}` で閉じるまで）を飛ばす。
            if depth > 0 || line.starts_with('{') {
                depth += line.matches('{').count() as i64 - line.matches('}').count() as i64;
                continue;
            }
        }
        let t = line.trim_start();
        let top = !line.starts_with(' ');
        if top && (t.starts_with("error:") || t.starts_with("error[")) {
            // 全体のまとめの行（`could not compile`、`aborting due to`）は個々のエラーではない。
            let summary = t.contains("could not compile") || t.contains("aborting due to");
            if !summary {
                errors.push(None);
                pending = true;
            }
        } else if top && (t.starts_with("warning") || t.starts_with("note")) {
            pending = false;
        }
        if let Some(loc) = t.strip_prefix("--> ") {
            let (file, rest) = loc.split_once(':').unwrap_or((loc, ""));
            if pending {
                pending = false;
                if file.starts_with("src/") {
                    let line_no = rest
                        .split(':')
                        .next()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0);
                    if let Some(last) = errors.last_mut() {
                        *last = Some((file.to_string(), line_no));
                    }
                }
            }
            if file.starts_with("src/") {
                let indent = &line[..line.len() - t.len()];
                text.push_str(&format!("{indent}--> build/verus/{loc}（生成コード）\n"));
                continue;
            }
        }
        text.push_str(line);
        text.push('\n');
    }
    (text, errors)
}

/// 端末の制御文字（`ESC [ .. 文字`）を除く。
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' && it.peek() == Some(&'[') {
            it.next();
            for d in it.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn package_name(dir: &Path) -> Option<String> {
    let t = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    t.lines()
        .find_map(|l| l.strip_prefix("name = \""))
        .and_then(|v| v.strip_suffix('"'))
        .map(str::to_string)
}

/// `verification results:: N verified, M errors` の N と M（最後の行）。
pub fn results_line(text: &str) -> Option<(u64, u64)> {
    let mut last = None;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("verification results:: ") else {
            continue;
        };
        let (v, e) = rest.split_once(" verified, ")?;
        let e = e.strip_suffix(" errors")?;
        last = Some((v.parse().ok()?, e.parse().ok()?));
    }
    last
}

/// 項目ごとの検証の状態（`status` は M5。M2 の仮の形。`OPEN.md`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStatus {
    pub item: String,
    /// [`decide`] の結果。
    pub status: &'static str,
}

/// 一つの項目の事実。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemFacts {
    /// 本体のない `lemma fn`（`admit()`）。
    pub admitted: bool,
    pub unchecked: bool,
    /// `#[test]`（`#[verifier::external]`。Verus は見ない）。
    pub external: bool,
    /// Verus のエラーがこの項目の行にある。
    pub error: bool,
    /// Verus の `func-details` にこの項目の関数がある（Verus が見た）。
    pub checked: bool,
    /// どの項目にも割り当てられないエラーがある（生成コードの外、位置がない、項目の外）。
    pub stray_errors: bool,
}

/// 項目の状態を決める（PR #26 のレビューの方針）。`verified` と表示してよいのは、Verus が見た関数
/// （`func-details` にある）で、その項目にエラーがなく、どの項目にも割り当てられないエラーがないときだけ。
///
/// - `admit` / `unchecked` / `external`：証明なしの前提。`verify` は必ず一覧に出す。
/// - `failed`：その項目にエラーがある。
/// - `unknown`：Verus が見なかった（`#[cfg]` で消えた、など）か、割り当てられないエラーがある。
///   `verify` を成功にしない。
pub fn decide(f: ItemFacts) -> &'static str {
    if f.admitted {
        "admit"
    } else if f.unchecked {
        "unchecked"
    } else if f.external {
        "external"
    } else if f.error {
        "failed"
    } else if !f.checked || f.stray_errors {
        "unknown"
    } else {
        "verified"
    }
}

/// 全項目の状態を決める。`checked` は Verus の `func-details` の鍵（`None` なら一覧がない）。
pub fn statuses(
    files: &[OutFile],
    output: &Output,
    errors: &[Option<(String, usize)>],
    checked: Option<&std::collections::BTreeSet<String>>,
) -> Vec<ItemStatus> {
    let in_item = |file: &str, line: usize| {
        files
            .iter()
            .any(|f| f.rel == file && f.items.iter().any(|(s, e, _)| *s <= line && line <= *e))
    };
    let stray = errors.iter().any(|e| match e {
        Some((file, line)) => !in_item(file, *line),
        None => true,
    });
    let mut out = Vec::new();
    for f in files {
        for (start, end, item) in &f.items {
            let error = errors
                .iter()
                .flatten()
                .any(|(file, line)| *file == f.rel && start <= line && line <= end);
            let facts = ItemFacts {
                admitted: output.admitted.contains(item),
                unchecked: output.unchecked.contains(item),
                external: output.external.contains(item),
                error,
                checked: checked.is_some_and(|c| c.contains(&verus_name(item))),
                stray_errors: stray,
            };
            out.push(ItemStatus {
                item: item.clone(),
                status: decide(facts),
            });
        }
    }
    out
}

/// Verus の `func-details` の鍵での項目の名前（クレート名の `-` は `_`）。
pub fn verus_name(item: &str) -> String {
    match item.split_once("::") {
        Some((k, rest)) => format!("{}::{rest}", k.replace('-', "_")),
        None => item.replace('-', "_"),
    }
}

/// 証明なしの前提（`admit` / `unchecked` / `external`）か。
pub fn assumed(s: &ItemStatus) -> bool {
    matches!(s.status, "admit" | "unchecked" | "external")
}

/// `verify item..` で出す項目：指定した項目（パスかその末尾）と、証明なしの前提の全部（絞り込みに
/// よらない。前提を隠さない）。指定した項目が一つもなければ `None`。
pub fn select(all: &[ItemStatus], items: &[&str]) -> Option<Vec<ItemStatus>> {
    let wanted = |i: &str| {
        items.is_empty()
            || items
                .iter()
                .any(|w| i == *w || i.ends_with(&format!("::{w}")))
    };
    if !items.is_empty() && !all.iter().any(|s| wanted(&s.item)) {
        return None;
    }
    Some(
        all.iter()
            .filter(|s| wanted(&s.item) || assumed(s))
            .cloned()
            .collect(),
    )
}

/// `verify --json` の結果（1 行）。
pub fn status_json(items: &[ItemStatus], results: Option<(u64, u64)>, success: bool) -> String {
    let items: Vec<serde_json::Value> = items
        .iter()
        .map(|i| serde_json::json!({"item": i.item, "status": i.status}))
        .collect();
    let results = results.map(|(v, e)| serde_json::json!({"verified": v, "errors": e}));
    serde_json::json!({
        "$message_type": "ebykn-verify",
        "success": success,
        "items": items,
        "results": results,
    })
    .to_string()
}

/// `verify` の結果の文章（`expected/status.txt` と比べる形）。
pub fn status_text(items: &[ItemStatus], results: Option<(u64, u64)>) -> String {
    let mut s = String::new();
    for i in items {
        s.push_str(&format!("{} {}\n", i.status, i.item));
    }
    match results {
        Some((v, e)) => s.push_str(&format!("results {v} verified, {e} errors\n")),
        None => s.push_str("results none\n"),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths() {
        let base = std::env::temp_dir().join(format!("ebykn-rel-{}", std::process::id()));
        std::fs::create_dir_all(base.join("a/b")).unwrap();
        std::fs::create_dir_all(base.join("c")).unwrap();
        assert_eq!(relative(&base.join("a/b"), &base.join("c")), "../../c");
        let _ = std::fs::remove_dir_all(&base);
    }

    fn facts() -> ItemFacts {
        ItemFacts {
            admitted: false,
            unchecked: false,
            external: false,
            error: false,
            checked: true,
            stray_errors: false,
        }
    }

    #[test]
    fn filtering_keeps_every_assumption() {
        let st = |item: &str, status: &'static str| ItemStatus {
            item: item.into(),
            status,
        };
        let all = vec![
            st("g1::liar", "unchecked"),
            st("g1::caller", "verified"),
            st("g1::other", "verified"),
        ];
        let shown = select(&all, &["caller"]).unwrap();
        assert_eq!(
            shown,
            [st("g1::liar", "unchecked"), st("g1::caller", "verified")]
        );
        assert!(select(&all, &["nothing"]).is_none());
        assert_eq!(select(&all, &[]).unwrap(), all);
    }

    #[test]
    fn verified_needs_verus_evidence() {
        assert_eq!(decide(facts()), "verified");
        // Verus が見なかった（#[cfg] で消えた、func-details にない）。
        assert_eq!(
            decide(ItemFacts {
                checked: false,
                ..facts()
            }),
            "unknown"
        );
        // 割り当てられないエラーがある。
        assert_eq!(
            decide(ItemFacts {
                stray_errors: true,
                ..facts()
            }),
            "unknown"
        );
        assert_eq!(
            decide(ItemFacts {
                error: true,
                ..facts()
            }),
            "failed"
        );
        assert_eq!(
            decide(ItemFacts {
                external: true,
                ..facts()
            }),
            "external"
        );
        assert_eq!(
            decide(ItemFacts {
                unchecked: true,
                error: true,
                ..facts()
            }),
            "unchecked"
        );
        assert_eq!(
            decide(ItemFacts {
                admitted: true,
                ..facts()
            }),
            "admit"
        );
    }

    #[test]
    fn status_marks_failed_items_by_error_lines() {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (1, 5, "k::f".into()),
                (6, 9, "k::g".into()),
                (10, 12, "k::l".into()),
            ],
        }];
        let output = Output {
            files: files.clone(),
            admitted: vec!["k::l".into()],
            unchecked: vec![],
            external: vec![],
        };
        let checked: std::collections::BTreeSet<String> = ["k::f", "k::g", "k::l"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let st = statuses(
            &files,
            &output,
            &[Some(("src/lib.rs".into(), 7))],
            Some(&checked),
        );
        assert_eq!(
            status_text(&st, Some((1, 1))),
            "verified k::f\nfailed k::g\nadmit k::l\nresults 1 verified, 1 errors\n"
        );
        // 項目に入らないエラーがあれば、失敗していない項目は unknown。
        let st = statuses(&files, &output, &[None], Some(&checked));
        assert_eq!(st[0].status, "unknown");
        // func-details がなければ unknown。
        let st = statuses(&files, &output, &[], None);
        assert_eq!(st[0].status, "unknown");
    }

    #[test]
    fn reads_diagnostics_and_skips_the_json() {
        let raw = "{\n  \"func-details\": {\n    \"k::f\": {}\n  }\n}\nerror: postcondition not satisfied\n  --> src/lib.rs:33:13\n   |\nnote: x\n  --> /p/prelude.rs:3:1\nerror: precondition\n  --> /abs/other.rs:1:1\nerror: could not compile `k`\n";
        let (text, errors) = read_diagnostics(raw, true);
        assert_eq!(errors, [Some(("src/lib.rs".into(), 33)), None]);
        assert!(!text.contains("func-details"), "{text}");
        assert!(
            text.contains("--> build/verus/src/lib.rs:33:13（生成コード）"),
            "{text}"
        );
        let j = parse_json("noise\n{\"func-details\": {\"k::f\": {}}, \"verification-results\": {\"verified\": 3, \"errors\": 1}}\n").unwrap();
        assert_eq!(j.results, Some((3, 1)));
        assert!(j.checked.contains("k::f"));
    }

    #[test]
    fn version_must_match_exactly() {
        let pin = crate::verus_pin();
        let out = |v: &str| {
            format!(
                "{{\"verus\": {{\"version\": \"{v}\", \"toolchain\": \"{} (overridden by environment variable RUSTUP_TOOLCHAIN)\", \"commit\": \"{}\"}}}}",
                pin.toolchain, pin.commit
            )
        };
        assert_eq!(check_version(&out(&pin.version), &pin), Ok(()));
        // 前方一致では通さない。
        assert_eq!(
            check_version(&out(&format!("{}x", pin.version)), &pin),
            Err("version")
        );
    }

    #[test]
    fn reads_error_locations_through_colors() {
        let colored = "\u{1b}[1m\u{1b}[91merror\u{1b}[0m: postcondition not satisfied\n  \u{1b}[1m\u{1b}[94m--> \u{1b}[0msrc/lib.rs:33:13";
        let plain = strip_ansi(colored);
        assert_eq!(
            plain,
            "error: postcondition not satisfied\n  --> src/lib.rs:33:13"
        );
    }
}
