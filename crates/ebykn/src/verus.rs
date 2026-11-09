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
pub fn find() -> Result<Verus, String> {
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
        return Err(format!(
            "the pinned Verus ({}) was not found at `{}`; install it with `cargo xtask verus-install` or set `EBYKN_VERUS` to its directory",
            pin.release,
            dir.display()
        ));
    }
    let out = Command::new(&verus)
        .args(["--version", "--output-json"])
        .env("RUSTUP_TOOLCHAIN", &pin.toolchain)
        .output()
        .map_err(|e| format!("cannot run `{}`: {e}", verus.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    for (key, want) in [
        ("version", &pin.version),
        ("commit", &pin.commit),
        ("toolchain", &pin.toolchain),
    ] {
        if !text.contains(&format!("\"{key}\": \"{want}")) {
            return Err(format!(
                "the Verus at `{}` is not the pinned one ({key} must be `{want}`):\n{text}",
                dir.display()
            ));
        }
    }
    Ok(Verus { dir, pin })
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
    /// `verification results:: N verified, M errors`。
    pub results: Option<(u64, u64)>,
    /// エラーの位置（`build/verus/` からの相対パス、行）。
    pub errors: Vec<(String, usize)>,
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
        Mode::Verify => cmd.args(["verus", "focus", "--", "--triggers-mode", "silent"]),
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
    let raw = strip_ansi(&format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ));
    let mut errors = Vec::new();
    let mut text = String::new();
    let mut in_error = false;
    for line in raw.lines() {
        let t = line.trim_start();
        if t.starts_with("error") {
            in_error = true;
        } else if t.starts_with("warning") || t.starts_with("note:") && !line.starts_with(' ') {
            in_error = false;
        }
        if let Some(loc) = t.strip_prefix("--> ")
            && let Some((file, rest)) = loc.split_once(':')
            && file.starts_with("src/")
        {
            let line_no = rest
                .split(':')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            if in_error {
                errors.push((file.to_string(), line_no));
            }
            let indent = &line[..line.len() - t.len()];
            text.push_str(&format!("{indent}--> build/verus/{loc}（生成コード）\n"));
            continue;
        }
        text.push_str(line);
        text.push('\n');
    }
    Ok(Run {
        success: out.status.success(),
        results: results_line(&raw),
        text,
        errors,
    })
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
    /// `verified` / `failed` / `admit` / `unchecked` / `unknown`（エラーがあるのに、どの項目のものか
    /// 決められなかったとき。`verified` と表示しない）。
    pub status: &'static str,
}

/// エラーの位置から、項目ごとの状態を決める。`error_count` は Verus が数えたエラーの数。
/// エラーがあるのに、どのエラーの位置もどの項目にも入らなければ、`verified` の代わりに `unknown` にする。
pub fn statuses(
    files: &[OutFile],
    output: &Output,
    errors: &[(String, usize)],
    error_count: u64,
) -> Vec<ItemStatus> {
    let attributed = errors.iter().any(|(file, line)| {
        files
            .iter()
            .any(|f| f.rel == *file && f.items.iter().any(|(s, e, _)| s <= line && line <= e))
    });
    let unknown = error_count > 0 && !attributed;
    let mut out = Vec::new();
    for f in files {
        for (start, end, item) in &f.items {
            let failed = errors
                .iter()
                .any(|(file, line)| *file == f.rel && start <= line && line <= end);
            let status = if output.admitted.contains(item) {
                "admit"
            } else if output.unchecked.contains(item) {
                "unchecked"
            } else if failed {
                "failed"
            } else if unknown {
                "unknown"
            } else {
                "verified"
            };
            out.push(ItemStatus {
                item: item.clone(),
                status,
            });
        }
    }
    out
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
        };
        let st = statuses(&files, &output, &[("src/lib.rs".into(), 7)], 1);
        assert_eq!(
            status_text(&st, Some((1, 1))),
            "verified k::f\nfailed k::g\nadmit k::l\nresults 1 verified, 1 errors\n"
        );
    }

    #[test]
    fn fails_closed_when_errors_cannot_be_attributed() {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![(1, 5, "k::f".into())],
        }];
        let output = Output {
            files: files.clone(),
            admitted: vec![],
            unchecked: vec![],
        };
        // エラーが 1 件あるのに位置を読めなかった：verified と言わない。
        let st = statuses(&files, &output, &[], 1);
        assert_eq!(st[0].status, "unknown");
        // エラーがなければ verified。
        assert_eq!(statuses(&files, &output, &[], 0)[0].status, "verified");
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
