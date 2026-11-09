//! 固定した Verus の呼び出し（仕様 §9 の `check` の後半、`verify`、`build`）。
//!
//! Verus は `Ebykn.toml` の `[verus]` と同じ固定値のものを使う（`ebykn` をビルドしたときの
//! ルートの `Cargo.toml` の `[workspace.metadata.verus]`。`check` が一致を確かめる）。探す場所は
//! 環境変数 `EBYKN_VERUS`（`verus` と `cargo-verus` のあるディレクトリ）か、`cargo xtask verus-install`
//! が入れるキャッシュ（`$XDG_CACHE_HOME/ebykn/verus/<版>/verus-x86-linux`）。版・コミット・toolchain が
//! 違えば失敗する。

use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub use crate::sites::ErrorSites;

use ebykn_check::project::Project;
use ebykn_lower::{Deps, OutFile, Output};

use crate::no_cheating::{self, Cheating};
use crate::{VerusPin, verus_pin};

/// 見つけた Verus。
pub struct Verus {
    pub dir: PathBuf,
    pub pin: VerusPin,
    /// 外部のコマンドが失敗したときの文言の言語（`find` に渡した `--lang`）。
    pub lang: ebykn_syntax::diag::Lang,
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
    Ok(Verus { dir, pin, lang })
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
    /// `cargo verus build -- --no-verify`（検証せずにビルドする。#80）。
    Build,
    /// `verus --no-cheating --no-verify`（証明の前提の拒否だけを見る。`crate::no_cheating`）。
    NoCheating,
}

/// Verus の実行の結果。
pub struct Run {
    pub success: bool,
    /// Verus（と cargo）の出力。生成コードの位置に `（生成コード）` を付けてある。
    pub text: String,
    /// 検証の件数（`--output-json` の `verification-results` か、`verification results::` の行）。
    pub results: Option<(u64, u64)>,
    /// エラーごとの文言と位置。
    pub errors: Vec<VerusError>,
    /// Verus が検証した関数（`--output-json` の `func-details` の鍵）。
    pub checked: Option<std::collections::BTreeSet<String>>,
    /// Verus が失敗とした関数（[`failed_functions`]）。読めなければ `None`。
    pub failed: Option<std::collections::BTreeSet<String>>,
    /// cargo がこのクレートを作り直した（`Checking 名前 v..` か `Compiling 名前 v..` の行がある）。
    /// ない実行は、Verus がこのクレートを見ていない。
    pub compiled: bool,
}

/// Verus のエラーの一つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusError {
    /// 見出しの文言（`error: ` / `error[E..]: ` の後）。
    pub message: String,
    /// 主な位置（`build/verus/` からの相対パスと行）。生成コードの外か位置がないものは `None`。
    pub at: Option<(String, usize)>,
    /// 主な位置の `-->` の後（`src/lib.rs:19:19`、生成コードの外の絶対パスなども）。
    pub loc: Option<String>,
    /// このエラーの診断の全文（見出しから、次の見出しかまとめの行の前まで）。
    pub block: String,
    /// このエラーが指す生成コードの行（[`crate::sites::error_sites`]）。
    pub sites: ErrorSites,
}

/// 検証の要約。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Summary {
    /// Verus が検証まで進んだ（検証した件数と、検証のエラーの数）。
    Ran { verified: u64, errors: u64 },
    /// コンパイルエラーなどで、Verus が検証の前に止まった（読み取ったエラーの数）。
    Stopped { errors: usize },
}

impl Run {
    /// エラーごとの生成コードの位置（[`statuses`] に渡す形）。
    pub fn locations(&self) -> Vec<ErrorSites> {
        self.errors.iter().map(|e| e.sites.clone()).collect()
    }

    /// Verus が失敗したのに検証のエラーが 0 件（または件数がない）なら、検証の前に止まった。
    /// `--output-json` はコンパイルエラーでも `verified: 0, errors: 0` を出すので、件数だけでは
    /// 見分けられない（PR #26 の確認）。
    pub fn summary(&self) -> Summary {
        match self.results {
            Some((verified, errors)) if self.success || errors > 0 => {
                Summary::Ran { verified, errors }
            }
            _ => Summary::Stopped {
                errors: self.errors.len(),
            },
        }
    }
}

/// 生成クレートの `cargo metadata --offline` の出力（Verus の実行で依存は取ってある）。依存クレートが
/// コードで使われる名前（改名と `[lib] name` を反映した名前）を [`parse_extern_crate_names`] で読む。
/// 走らなければ `None`（呼ぶ側は、名前が分からないとして扱う。#49）。
pub fn cargo_metadata(verus: &Verus, build: &Path) -> Option<String> {
    let out = cargo(verus, build)
        .ok()?
        .args(["metadata", "--format-version", "1", "--offline"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `cargo metadata` の JSON から、根のパッケージの依存の名前（`resolve.nodes[根].deps[].name`）を読む。
pub fn parse_extern_crate_names(json: &str) -> Option<Vec<String>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let resolve = v.get("resolve")?;
    let root = resolve.get("root")?.as_str()?;
    let node = resolve
        .get("nodes")?
        .as_array()?
        .iter()
        .find(|n| n.get("id").and_then(|i| i.as_str()) == Some(root))?;
    node.get("deps")?
        .as_array()?
        .iter()
        .map(|d| d.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect()
}

/// 生成クレートの成果物の置き場（`build/verus/target`）。[`cargo`] が `CARGO_TARGET_DIR` に渡すので、
/// 利用者の `CARGO_TARGET_DIR` や、親の `.cargo/config.toml` の `build.target-dir` によらない（#137）。
pub fn target_dir(build: &Path) -> Result<PathBuf, String> {
    std::path::absolute(build.join("target")).map_err(|e| format!("{}: {e}", build.display()))
}

/// 生成クレート（`build`）で走らせる cargo。cargo の呼び出しはここでだけ作る（テストが数える）。
///
/// `ebykn` を起動した cargo の環境（`cargo run` や `cargo test` から呼ばれたとき）を引き継がず、
/// 固定した toolchain で走らせ、成果物を [`target_dir`] に置かせる。`CARGO_TARGET_DIR` は
/// `.cargo/config.toml` の `build.target-dir` より優先する。置き場を cargo の設定に任せると、
/// [`run`] が消す成果物と cargo が使う成果物が食い違い、cargo が Verus を呼ばなくなる。
fn cargo(verus: &Verus, build: &Path) -> Result<Command, String> {
    let mut cmd = Command::new("cargo");
    for v in ["CARGO", "RUSTC", "RUSTDOC", "CARGO_MANIFEST_DIR"] {
        cmd.env_remove(v);
    }
    cmd.current_dir(build)
        .env("RUSTUP_TOOLCHAIN", &verus.pin.toolchain)
        .env("CARGO_TARGET_DIR", target_dir(build)?);
    Ok(cmd)
}

pub fn run(verus: &Verus, build: &Path, mode: Mode) -> Result<Run, String> {
    let path = std::env::join_paths(std::iter::once(verus.dir.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .map_err(|e| e.to_string())?;
    let mut cmd = cargo(verus, build)?;
    // 診断の位置を読むので、色の制御文字を出させない（CI は `CARGO_TERM_COLOR=always`）。
    cmd.env("CARGO_TERM_COLOR", "never").env("PATH", path);
    match mode {
        Mode::NoVerify => cmd.args(["verus", "focus", "--", "--no-verify"]),
        Mode::Verify => cmd.args([
            "verus",
            "focus",
            "--",
            "--triggers-mode",
            "silent",
            "--output-json",
            // 関数ごとの成否（`times-ms` の `function-breakdown`）を出させる（#79）。
            "--time",
        ]),
        // `--verified` なしの `build` は、検証の結果によらずビルドする（現行仕様 §9。#80）。
        Mode::Build => cmd.args(["verus", "build", "--", "--no-verify"]),
        Mode::NoCheating => cmd.args(["verus", "focus", "--", "--no-cheating", "--no-verify"]),
    };
    // 前の成果物が残っていると cargo が Verus を呼ばない（結果の行が出ない）ので、このクレートだけ消す。
    // `cargo verus focus` は target-dir の下の `verus-partial` にも成果物を置く。
    let name = package_name(build).unwrap_or_default();
    let target = target_dir(build)?;
    // 消せなければ、前の結果を今の結果と取り違えるおそれがあるので止める。
    for dir in [target.clone(), target.join("verus-partial")] {
        let shown = format!(
            "cargo clean --quiet -p {name} --target-dir {}",
            dir.display()
        );
        let out = cargo(verus, build)?
            .args(["clean", "--quiet", "-p", &name, "--target-dir"])
            .arg(&dir)
            .output()
            .map_err(|e| cannot_run(&shown, &e, verus.lang))?;
        if !out.status.success() {
            return Err(command_failed(&shown, build, &out, verus.lang));
        }
    }
    let out = cmd
        .output()
        .map_err(|e| cannot_run("cargo verus", &e, verus.lang))?;
    let stdout = strip_ansi(&String::from_utf8_lossy(&out.stdout));
    let stderr = strip_ansi(&String::from_utf8_lossy(&out.stderr));
    let json = parse_json(&stdout);
    let (text, errors) = read_diagnostics(&format!("{stdout}{stderr}"), json.is_some());
    let results = match &json {
        Some(j) => j.results,
        None => results_line(&format!("{stdout}{stderr}")),
    };
    let compiled = compiled(&format!("{stdout}{stderr}"), &name);
    Ok(Run {
        success: out.status.success(),
        results,
        text,
        errors,
        failed: json.as_ref().and_then(|j| j.failed.clone()),
        checked: json.map(|j| j.checked),
        compiled,
    })
}

/// 外部のコマンドを起動できなかったときの文言。
fn cannot_run(cmd: &str, e: &std::io::Error, lang: ebykn_syntax::diag::Lang) -> String {
    if lang == ebykn_syntax::diag::Lang::Ja {
        format!("`{cmd}` を起動できません: {e}")
    } else {
        format!("cannot run `{cmd}`: {e}")
    }
}

/// 外部のコマンドが失敗したときの文言：コマンド、走らせたディレクトリ、終了コード（シグナルで
/// 止まったならそう書く）、コマンドの出力（#78。終了の状態を Debug 表記で出さない）。
pub fn command_failed(
    cmd: &str,
    dir: &Path,
    out: &std::process::Output,
    lang: ebykn_syntax::diag::Lang,
) -> String {
    let ja = lang == ebykn_syntax::diag::Lang::Ja;
    let how = match (out.status.code(), signal(&out.status)) {
        (Some(c), _) if ja => format!("終了コード {c}"),
        (Some(c), _) => format!("exit code {c}"),
        (None, Some(s)) if ja => format!("シグナル {s} で停止"),
        (None, Some(s)) => format!("killed by signal {s}"),
        (None, None) if ja => "終了コードなし".into(),
        (None, None) => "no exit code".into(),
    };
    let mut s = if ja {
        format!("`{}` で `{cmd}` が失敗しました（{how}）", dir.display())
    } else {
        format!("`{cmd}` failed in `{}` ({how})", dir.display())
    };
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let output = output.trim_end();
    if !output.is_empty() {
        s.push_str(":\n");
        s.push_str(output);
    }
    s
}

#[cfg(unix)]
fn signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn signal(_: &std::process::ExitStatus) -> Option<i32> {
    None
}

/// cargo の出力に、クレート `name` を作り直した行（`Checking name v..` / `Compiling name v..`）があるか。
pub fn compiled(text: &str, name: &str) -> bool {
    !name.is_empty()
        && text.lines().any(|l| {
            let l = l.trim_start();
            ["Checking ", "Compiling "].iter().any(|v| {
                l.strip_prefix(v)
                    .and_then(|r| r.strip_prefix(name))
                    .is_some_and(|r| r.starts_with(" v"))
            })
        })
}

/// `--output-json` の出力。
struct Json {
    results: Option<(u64, u64)>,
    checked: std::collections::BTreeSet<String>,
    failed: Option<std::collections::BTreeSet<String>>,
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
    Some(Json {
        results,
        checked,
        failed: failed_functions(&v),
    })
}

/// `--time` の JSON の `times-ms.smt.smt-run-module-times[].function-breakdown[]` から、`success` が
/// `false` の関数の名前（`func-details` の鍵と同じ形）を集める（#79）。
///
/// `success` は Verus の `func_fails`（関数の検証の問い合わせが失敗した関数）から作られる。`errors` は
/// 失敗した問い合わせの数で、一つの関数の `ensures` が二つ成り立たなくても 1 と数える（エラーの見出しは
/// 2 つ）。bit_vector の問い合わせの失敗は `errors` に数えるが `func_fails` に入れないので、`errors` と
/// この集合の大きさが違えば、[`statuses`] は割り当てられないとみなす。
///
/// 項目が一つでも読めなければ `None`（読めた分だけで決めない）。
pub fn failed_functions(v: &serde_json::Value) -> Option<std::collections::BTreeSet<String>> {
    let modules = v
        .get("times-ms")?
        .get("smt")?
        .get("smt-run-module-times")?
        .as_array()?;
    let mut failed = std::collections::BTreeSet::new();
    for m in modules {
        for f in m.get("function-breakdown")?.as_array()? {
            let name = f.get("function")?.as_str()?;
            if !f.get("success")?.as_bool()? {
                failed.insert(name.to_string());
            }
        }
    }
    Some(failed)
}

/// 診断を読む：生成コードの位置に `（生成コード）` を付け、エラーごとの主な位置を集める。
/// `json` のときは標準出力の JSON の部分を本文から除く。
pub(crate) fn read_diagnostics(raw: &str, json: bool) -> (String, Vec<VerusError>) {
    let mut errors: Vec<VerusError> = Vec::new();
    let mut text = String::new();
    // 今のエラーの主な位置をまだ読んでいないか。
    let mut pending = false;
    // 今読んでいる診断がエラーなら、その番号（`block` に行を足す）。
    let mut current: Option<usize> = None;
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
                let message = t
                    .split_once(": ")
                    .map(|(_, m)| m.to_string())
                    .unwrap_or_default();
                errors.push(VerusError {
                    message,
                    at: None,
                    loc: None,
                    block: String::new(),
                    sites: ErrorSites::default(),
                });
                pending = true;
                current = Some(errors.len() - 1);
            } else {
                current = None;
            }
        } else if top
            && (t.starts_with("warning")
                || t.starts_with("note")
                || t.starts_with("For more information"))
        {
            pending = false;
            current = None;
        }
        if let Some(loc) = t.strip_prefix("--> ") {
            let (file, rest) = loc.split_once(':').unwrap_or((loc, ""));
            if pending {
                pending = false;
                if let Some(last) = errors.last_mut() {
                    last.loc = Some(loc.to_string());
                    if file.starts_with("src/") {
                        let line_no = rest
                            .split(':')
                            .next()
                            .and_then(|n| n.parse().ok())
                            .unwrap_or(0);
                        last.at = Some((file.to_string(), line_no));
                    }
                }
            }
            if file.starts_with("src/") {
                let indent = &line[..line.len() - t.len()];
                let shown = format!("{indent}--> build/verus/{loc}（生成コード）\n");
                if let Some(i) = current {
                    errors[i].block.push_str(&shown);
                }
                text.push_str(&shown);
                continue;
            }
        }
        if let Some(i) = current {
            errors[i].block.push_str(line);
            errors[i].block.push('\n');
        }
        text.push_str(line);
        text.push('\n');
    }
    for e in &mut errors {
        e.sites = crate::sites::error_sites(&e.block);
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
    /// [`decide`] の結果。手続きマクロが付いた項目は `macro`（[`ItemStatus::via`]）。
    pub status: &'static str,
    /// `status` が `macro` のとき、付いているマクロ（#49）。
    pub via: Option<(ebykn_syntax::macros::MacroKind, String)>,
}

impl ItemStatus {
    /// 一覧の 1 行の本体（`verified k::f`、`macro serde::Serialize derive on k::Foo`）。
    pub fn line(&self) -> String {
        match &self.via {
            Some((ebykn_syntax::macros::MacroKind::Derive, p)) => {
                format!("{} {p} derive on {}", self.status, self.item)
            }
            Some((ebykn_syntax::macros::MacroKind::Attribute, p)) => {
                format!("{} {p} on {}", self.status, self.item)
            }
            None => format!("{} {}", self.status, self.item),
        }
    }
}

/// 一つの項目の事実。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemFacts {
    /// 本体のない `lemma fn`（`admit()`）。
    pub admitted: bool,
    pub unchecked: bool,
    /// `#[test]`（`#[verifier::external]`。Verus は見ない）。
    pub external: bool,
    /// Verus がこの項目の関数を失敗とした（[`failed_functions`]）。成否が読めないか Verus の名前が決まらない
    /// 項目では、Verus のエラーの失敗の場所がこの項目の行にある（[`ErrorSites::blamed`]）。
    pub error: bool,
    /// Verus のエラーがこの項目の行を副の位置として指す（[`ErrorSites::related`]。PR #76 のレビューの B1）か、
    /// 失敗の場所がこの項目の行にあるのに Verus がこの項目の関数を通した（#79）。
    pub related: bool,
    /// Verus の `func-details` にこの項目の関数がある（Verus が見た）。
    pub checked: bool,
    /// どの項目にも割り当てられないエラーがある（生成コードの外、位置がない、項目の外）か、Verus の関数ごと
    /// の成否とエラーの失敗の場所が合わない（[`statuses`] の 3 つの条件。#79）。
    pub stray_errors: bool,
    /// Verus が検証の前に止まった（[`Summary::Stopped`]）。`func-details` に関数があっても、検証はしていない。
    pub stopped: bool,
    /// `--no-cheating` の照合で、Ebykn が出した前提のほかの前提が見つかったか、ないと言えない
    /// （[`crate::no_cheating`]）。どの項目がそれに頼るか分からないので、どの項目も verified にしない。
    pub tainted: bool,
}

/// 項目の状態を決める（PR #26 のレビューの方針）。`verified` と表示してよいのは、Verus が見た関数
/// （`func-details` にある）で、その項目にエラーがなく、どの項目にも割り当てられないエラーがないときだけ。
///
/// - `admit` / `unchecked` / `external`：証明なしの前提。`verify` は必ず一覧に出す。前提の状態はエラーより
///   優先する（その項目にエラーがあっても `failed` にしない。エラーの場所は、標準エラーに出す Verus の診断の
///   生成コードの位置で示される）。
/// - `failed`：その項目にエラーがある。
/// - `unknown`：Verus が見なかった（`#[cfg]` で消えた、など）か、エラーが副の位置としてその項目を指すか、
///   割り当てられないエラーがあるか、
///   Verus が検証の前に止まった（コンパイルエラーなど。#43）か、紛れ込んだ前提がある
///   （[`ItemFacts::tainted`]、#37）。`verify` を成功にしない。
pub fn decide(f: ItemFacts) -> &'static str {
    if f.admitted {
        "admit"
    } else if f.unchecked {
        "unchecked"
    } else if f.external {
        "external"
    } else if f.error {
        "failed"
    } else if !f.checked || f.related || f.stray_errors || f.stopped || f.tainted {
        "unknown"
    } else {
        "verified"
    }
}

/// 全項目の状態を決める。`checked` は Verus の `func-details` の鍵（`None` なら一覧がない）。`failed` は
/// Verus が失敗とした関数（[`failed_functions`]。`None` なら読めない）。[`Summary::Stopped`] なら何も検証
/// していない。`tainted` は [`ItemFacts::tainted`]（`--no-cheating` の照合の結果）。
///
/// [`Summary::Ran`] でエラーがあるとき、次のすべてが成り立てば、エラーを項目に割り当てられたとみなす
/// （#79）。一つでも崩れれば、失敗していない項目をすべて unknown にする。
///
/// - `failed` が読めて、その数が Verus の数えたエラー（失敗した問い合わせ）の数と同じ。
/// - 各エラーの失敗の場所の項目が、どれも `failed` にある（Verus が通した関数のせいにしていない）。
/// - `failed` の各関数に、失敗の場所を持つエラーがある（エラーを読み落としていない。呼ばれる側の行しか
///   出ないエラーで、失敗した呼び出し元を見落としていない）。
///
/// エラーの見出しの数とは比べない。一つの関数の `ensures` が二つ成り立たなければ、見出しは 2 つで、
/// Verus の数えるエラーは 1 つになる。
pub fn statuses(
    files: &[OutFile],
    output: &Output,
    errors: &[ErrorSites],
    checked: Option<&std::collections::BTreeSet<String>>,
    failed: Option<&std::collections::BTreeSet<String>>,
    summary: Summary,
    tainted: bool,
) -> Vec<ItemStatus> {
    let (error_count, stopped) = match summary {
        Summary::Ran { errors, .. } => (Some(errors), false),
        Summary::Stopped { .. } => (None, true),
    };
    // 項目の Verus の名前。パスで決まらない項目（クレートの外の型への impl のメソッド）は、形に当たる
    // 鍵がちょうど一つのときだけ決まる。
    let verus_of = |item: &str| -> Option<String> {
        match output.verus_patterns.get(item) {
            Some(pat) => {
                let (pre, post) = pat.split_once('*')?;
                let hits: Vec<&String> = checked?
                    .iter()
                    .filter(|k| {
                        k.strip_prefix(pre)
                            .and_then(|r| r.strip_suffix(post))
                            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                    })
                    .collect();
                match hits.as_slice() {
                    [k] => Some((*k).clone()),
                    _ => None,
                }
            }
            None => Some(verus_name(item)),
        }
    };
    let names: Vec<(String, Option<String>)> = files
        .iter()
        .flat_map(|f| f.items.iter().map(|(_, _, i)| (i.clone(), verus_of(i))))
        .collect();
    // 同じ Verus の名前になる項目が二つ以上あれば、どれの証拠か分からないので数えない。
    let shared = |n: &Option<String>| match n {
        Some(n) => names.iter().filter(|(_, m)| m.as_ref() == Some(n)).count() > 1,
        None => false,
    };
    // 行を範囲に含む項目。
    let items_at = |file: &str, line: usize| -> Vec<&String> {
        files
            .iter()
            .filter(|f| f.rel == file)
            .flat_map(|f| f.items.iter())
            .filter(|(s, e, _)| *s <= line && line <= *e)
            .map(|(_, _, i)| i)
            .collect()
    };
    // 失敗の場所がないか、項目の外にあるエラーは、どの項目にも割り当てられない。
    let outside = errors.iter().any(|e| {
        e.blamed.is_empty()
            || e.blamed
                .iter()
                .any(|(file, line)| items_at(file, *line).is_empty())
    });
    // 失敗した関数とエラーの失敗の場所が、両向きに合うか（上の 3 つの条件）。
    let blamed_names: Vec<Option<String>> = errors
        .iter()
        .flat_map(|e| e.blamed.iter())
        .flat_map(|(file, line)| items_at(file, *line))
        .map(|i| verus_of(i))
        .collect();
    let attributed = match (error_count, failed) {
        (None, _) => true,
        (Some(n), None) => n == 0 && errors.is_empty(),
        (Some(n), Some(fails)) => {
            n == fails.len() as u64
                && blamed_names
                    .iter()
                    .all(|b| b.as_ref().is_some_and(|b| fails.contains(b)))
                && fails
                    .iter()
                    .all(|f| blamed_names.iter().any(|b| b.as_ref() == Some(f)))
        }
    };
    let stray = outside || !attributed;
    let mut out = Vec::new();
    for f in files {
        for (start, end, item) in &f.items {
            let here =
                |(file, line): &(String, usize)| *file == f.rel && start <= line && line <= end;
            let name = verus_of(item);
            let seen = !shared(&name)
                && name
                    .as_ref()
                    .is_some_and(|n| checked.is_some_and(|c| c.contains(n)));
            let blamed = errors.iter().any(|e| e.blamed.iter().any(here));
            // 関数ごとの成否が読めて、Verus が見た関数なら、成否は Verus に従う。失敗の場所がこの項目に
            // あっても Verus が通した関数なら、failed ではなく unknown にする（related）。
            let verdict = match (failed, &name) {
                (Some(f), Some(n)) if seen && !stopped => Some(f.contains(n)),
                _ => None,
            };
            let error = verdict.unwrap_or(blamed);
            let related = errors.iter().any(|e| e.related.iter().any(here))
                || (blamed && verdict == Some(false));
            let facts = ItemFacts {
                admitted: output.admitted.contains(item),
                unchecked: output.unchecked.contains(item),
                external: output.external.contains(item),
                error,
                related,
                checked: seen,
                stray_errors: stray,
                stopped,
                tainted,
            };
            out.push(ItemStatus {
                item: item.clone(),
                status: decide(facts),
                via: None,
            });
        }
        // 依存クレートの derive と属性マクロが作る項目は、Verus が検証しない（`verus!` の外の項目に
        // なる）。証明なしの前提として、同じ一覧に必ず出す（#49）。
        for m in output.macros.iter().filter(|m| m.rel == f.rel) {
            out.push(ItemStatus {
                item: m.item.clone(),
                status: "macro",
                via: Some((m.kind, m.path.clone())),
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

/// 証明なしの前提（`admit` / `unchecked` / `external` / `macro`）か。
pub fn assumed(s: &ItemStatus) -> bool {
    matches!(s.status, "admit" | "unchecked" | "external" | "macro")
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

/// 前提の行の名前（`cfg` の行の文言）。`cfg_uses` は、クレートが頼っている、ビルドの設定で値の変わる cfg
/// の名前（[`Output::cfg_uses`]）。空でなければ、verified はホストの target・既定の feature・追加の
/// `--cfg` なし・`test` なしで検証した結果で、ほかの組み合わせのビルドと `cargo test` は検証していない（#27、#87）。
pub const CFG_ASSUMPTION_EN: &str = "verified for the host target, the default features, no extra `--cfg` and no `cfg(test)`; builds with other targets, features or `--cfg` (for example through `RUSTFLAGS`) and `cargo test` are not verified";
pub const CFG_ASSUMPTION_JA: &str = "verified は、ホストの target・既定の feature・追加の `--cfg` なし・`test` なしで検証した結果です。ほかの target・feature・`--cfg`（`RUSTFLAGS` で付けたものなど）でのビルドと `cargo test` は検証していません";

/// 頼っている cfg があれば、標準エラーに出す文言（英語と日本語）。なければ `None`（#87 の既定）。
pub fn cfg_assumption_message(cfg_uses: &[String]) -> Option<(String, String)> {
    if cfg_uses.is_empty() {
        return None;
    }
    Some((
        format!(
            "the crate depends on cfg that the build settings can change ({}): {CFG_ASSUMPTION_EN}",
            cfg_uses.join(", ")
        ),
        format!(
            "このクレートは、ビルドの設定で値の変わる cfg に頼っています（{}）。{CFG_ASSUMPTION_JA}",
            cfg_uses.join("、")
        ),
    ))
}

/// 実行関数の `verified` が停止を含まないことの注記（#81）。表示する項目に `verified` の実行関数
/// （`exec`。`Output::exec`）があるときだけ返す。
///
/// `decreases` が示すのは、それを書いたループや再帰が整礎であることだけで、関数が停止することではない。
/// `decreases` を書いた関数も、`decreases` のないループや再帰（呼んだ先のものを含む）に届けば止まらない
/// ことがある。Verus は、関数の `decreases` を書いた実行関数に停止を保証しないという警告をいつも出すが、
/// `.eyk` では従えない `#[verifier::allow(..)]` を案内するので、書き下しがその属性を付けて警告を消し
/// （`ebykn_lower` の `EXEC_NO_DECREASES`）、この注記で断る。
pub fn termination_note(
    items: &[ItemStatus],
    exec: &[String],
    lang: ebykn_syntax::diag::Lang,
) -> Option<&'static str> {
    if !items
        .iter()
        .any(|s| s.status == "verified" && exec.contains(&s.item))
    {
        return None;
    }
    Some(match lang {
        ebykn_syntax::diag::Lang::En => TERMINATION_NOTE_EN,
        ebykn_syntax::diag::Lang::Ja => TERMINATION_NOTE_JA,
    })
}

/// [`termination_note`] の英語の文言。
pub const TERMINATION_NOTE_EN: &str = "`verified` for an executable function does not include termination: it means the function meets its contract whenever it returns. `decreases` shows only that the loop or recursion it is written on is well-founded; code that reaches a loop or recursion without `decreases` may not terminate";

/// [`termination_note`] の日本語の文言。
pub const TERMINATION_NOTE_JA: &str = "実行関数の `verified` は停止を含みません。戻ったときに契約を満たすという意味です。`decreases` が示すのは、それを書いたループや再帰が整礎であることだけです。`decreases` のないループや再帰に届けば、止まらないことがあります";

/// `verify --json` の結果（1 行）。
///
/// `exec` は本体を検証する実行関数（`Output::exec`）。その項目には `"termination": "not_proven"` を付ける。
/// 実行関数の `verified` は停止を含まない（#81）ので、機械で読む側もそれを見分けられるようにする。
pub fn status_json(
    items: &[ItemStatus],
    summary: Summary,
    cheating: &Cheating,
    cfg_uses: &[String],
    success: bool,
    exec: &[String],
) -> String {
    let items: Vec<serde_json::Value> = items
        .iter()
        .map(|i| {
            let mut v = item_json(i);
            if exec.contains(&i.item) {
                v["termination"] = "not_proven".into();
            }
            v
        })
        .collect();
    let results = match summary {
        Summary::Ran { verified, errors } => {
            serde_json::json!({"ran": true, "verified": verified, "errors": errors})
        }
        Summary::Stopped { errors } => serde_json::json!({"ran": false, "errors": errors}),
    };
    // `state` は clean / smuggled / unreadable / not_run（`checked` は照合したか）。
    let mut no_cheating = serde_json::json!({
        "state": cheating.state(),
        "checked": matches!(cheating, Cheating::Clean | Cheating::Smuggled(_)),
    });
    match cheating {
        Cheating::Clean => no_cheating["smuggled"] = serde_json::json!([]),
        Cheating::Smuggled(v) => {
            no_cheating["smuggled"] = v
                .iter()
                .map(|e| serde_json::json!({"location": no_cheating::location(e), "message": e.message}))
                .collect();
        }
        Cheating::Unreadable(u) => no_cheating["reason"] = u.text(false).into(),
        Cheating::NotRun => {
            no_cheating["reason"] =
                "Verus stopped before verification, so the check did not run".into()
        }
    }
    // 頼っている cfg があれば、検証した組み合わせを前提として出す（なければ null）。
    let cfg = if cfg_uses.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!({"names": cfg_uses, "note": CFG_ASSUMPTION_EN})
    };
    serde_json::json!({
        "$message_type": "ebykn-verify",
        "success": success,
        "items": items,
        "no_cheating": no_cheating,
        "cfg_assumption": cfg,
        "results": results,
    })
    .to_string()
}

/// `verify --json` の項目一つ。
fn item_json(i: &ItemStatus) -> serde_json::Value {
    match &i.via {
        Some((kind, path)) => serde_json::json!({
            "item": i.item,
            "status": i.status,
            "macro": path,
            "kind": match kind {
                ebykn_syntax::macros::MacroKind::Derive => "derive",
                ebykn_syntax::macros::MacroKind::Attribute => "attribute",
            },
        }),
        None => serde_json::json!({"item": i.item, "status": i.status}),
    }
}

/// `verify` の結果の文章（`expected/status.txt` と比べる形）。
///
/// 紛れ込んだ前提は `smuggled 位置 文言` の行、照合できなかったときは `smuggled unreadable: 理由` の行にする。
/// 頼っている cfg があれば `assumed cfg 名前, ..` の行を出す（[`CFG_ASSUMPTION_EN`]）。
pub fn status_text(
    items: &[ItemStatus],
    summary: Summary,
    cheating: &Cheating,
    cfg_uses: &[String],
) -> String {
    let mut s = String::new();
    for i in items {
        s.push_str(&i.line());
        s.push('\n');
    }
    if !cfg_uses.is_empty() {
        s.push_str(&format!("assumed cfg {}\n", cfg_uses.join(", ")));
    }
    match cheating {
        Cheating::Smuggled(v) => {
            for e in v {
                s.push_str(&format!(
                    "smuggled {} {}\n",
                    no_cheating::location(e),
                    e.message
                ));
            }
        }
        Cheating::Unreadable(u) => s.push_str(&format!("smuggled unreadable: {}\n", u.text(false))),
        Cheating::Clean | Cheating::NotRun => {}
    }
    match summary {
        Summary::Ran { verified, errors } => {
            s.push_str(&format!("results {verified} verified, {errors} errors\n"))
        }
        Summary::Stopped { errors } => s.push_str(&format!(
            "results not verified: Verus stopped before verification ({errors} errors)\n"
        )),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use ebykn_syntax::diag::Lang;

    #[test]
    fn cargo_puts_artifacts_under_the_build_dir() {
        let verus = Verus {
            dir: PathBuf::from("/verus"),
            pin: verus_pin(),
            lang: Lang::En,
        };
        let build = Path::new("rel/build/verus");
        let cmd = cargo(&verus, build).unwrap();
        let env = |k: &str| {
            cmd.get_envs()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.map(PathBuf::from))
        };
        // 親の `.cargo/config.toml` の `build.target-dir` より優先する環境変数で、絶対パスを渡す（#137）。
        let target = env("CARGO_TARGET_DIR").flatten().expect("CARGO_TARGET_DIR");
        assert!(target.is_absolute(), "{}", target.display());
        assert!(
            target.ends_with("rel/build/verus/target"),
            "{}",
            target.display()
        );
        assert_eq!(target, target_dir(build).unwrap());
        for v in ["CARGO", "RUSTC", "RUSTDOC", "CARGO_MANIFEST_DIR"] {
            assert_eq!(env(v), Some(None), "{v} を外していない");
        }
        assert_eq!(
            env("RUSTUP_TOOLCHAIN").flatten(),
            Some(PathBuf::from(&verus.pin.toolchain))
        );
        assert_eq!(cmd.get_current_dir(), Some(build));
    }

    /// `root` の下の `.rs`（サブディレクトリも）を、`root` からのパスと中身の組で読む。
    fn sources_under(root: &Path) -> Vec<(String, String)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, base, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let rel = path
                        .strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    out.push((rel, std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    fn crate_sources() -> Vec<(String, String)> {
        sources_under(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
    }

    /// cargo の呼び出し（`Command::new("cargo")`）が、テストの外では `verus.rs` の `fn cargo(` の中の
    /// 1 か所だけか。`cargo` を通らない呼び出しは、成果物の置き場を cargo の設定に任せてしまう（#137）。
    fn cargo_only_in_helper(files: &[(String, String)]) -> Result<(), String> {
        let needle = ["Command::new(", "\"cargo\")"].concat();
        let mut found = Vec::new();
        for (rel, text) in files {
            let body = text
                .split_once("\n#[cfg(test)]\nmod tests {")
                .map_or(text.as_str(), |(b, _)| b);
            for (at, _) in body.match_indices(needle.as_str()) {
                found.push((rel.as_str(), body, at));
            }
        }
        let [(rel, body, at)] = found[..] else {
            let at: Vec<&str> = found.iter().map(|(r, _, _)| *r).collect();
            return Err(format!("cargo の呼び出しが 1 か所でない: {at:?}"));
        };
        if rel != "verus.rs" {
            return Err(format!("cargo の呼び出しが verus.rs の外にある: {rel}"));
        }
        match body[..at].rfind("\nfn ") {
            Some(def) if body[def..].starts_with("\nfn cargo(") => Ok(()),
            _ => Err("cargo の呼び出しが fn cargo の中にない".into()),
        }
    }

    /// `from` を `to` に置き換える。壊す操作が当たったことを先に確かめる。
    fn mutate(src: &str, from: &str, to: &str) -> String {
        assert!(src.contains(from), "壊す操作が当たらない: {from:?}");
        let out = src.replacen(from, to, 1);
        assert_ne!(out, src);
        out
    }

    #[test]
    fn cargo_is_called_only_in_the_cargo_helper() {
        let files = crate_sources();
        assert!(files.iter().any(|(r, _)| r == "verus.rs"), "{files:?}");
        assert_eq!(cargo_only_in_helper(&files), Ok(()));
    }

    #[test]
    fn fails_when_cargo_is_called_outside_the_cargo_helper() {
        let files = crate_sources();
        let stray = "\nfn stray() -> Command {\n    Command::new(\"cargo\")\n}\n";
        let with = |rel: &str, edit: &dyn Fn(&str) -> String| -> Vec<(String, String)> {
            let mut f = files.clone();
            let e = f.iter_mut().find(|(r, _)| r == rel).expect(rel);
            e.1 = edit(&e.1);
            f
        };
        // ほかのファイルの呼び出し。
        let other = with("sync.rs", &|t| {
            mutate(t, "\nuse ", &format!("{stray}\nuse "))
        });
        assert!(cargo_only_in_helper(&other).is_err());
        // サブディレクトリのファイルの呼び出し：実際のディレクトリを読み、sources_under が拾うこと。
        let dir = std::env::temp_dir().join(format!("ebykn-cargo-calls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, text) in &files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        assert_eq!(cargo_only_in_helper(&sources_under(&dir)), Ok(()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(
            dir.join("sub/mod.rs"),
            format!("use std::process::Command;{stray}"),
        )
        .unwrap();
        let sub = sources_under(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(sub.iter().any(|(r, _)| r == "sub/mod.rs"), "{sub:?}");
        assert!(cargo_only_in_helper(&sub).is_err());
        // verus.rs の中でも、fn cargo の外の呼び出し。
        let outside = with("verus.rs", &|t| {
            mutate(t, "\npub fn run(", &format!("{stray}\npub fn run("))
        });
        assert!(cargo_only_in_helper(&outside).is_err());
        // fn cargo の中の呼び出しを、別の名前の関数に移したもの。
        let renamed = with("verus.rs", &|t| mutate(t, "\nfn cargo(", "\nfn cargo_any("));
        assert!(cargo_only_in_helper(&renamed).is_err());
        // テストの中の呼び出しは数えない。
        let in_tests = with("verus.rs", &|t| {
            mutate(t, "\nmod tests {\n", &format!("\nmod tests {{\n{stray}"))
        });
        assert_eq!(cargo_only_in_helper(&in_tests), Ok(()));
    }

    #[test]
    fn relative_paths() {
        let base = std::env::temp_dir().join(format!("ebykn-rel-{}", std::process::id()));
        std::fs::create_dir_all(base.join("a/b")).unwrap();
        std::fs::create_dir_all(base.join("c")).unwrap();
        assert_eq!(relative(&base.join("a/b"), &base.join("c")), "../../c");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn failed_commands_show_the_exit_code_and_output() {
        use std::os::unix::process::ExitStatusExt;
        let out = std::process::Output {
            // 終了コード 101（wait の状態は 101 << 8）。
            status: std::process::ExitStatus::from_raw(101 << 8),
            stdout: Vec::new(),
            stderr: b"error: current package believes it's in a workspace when it's not\n".to_vec(),
        };
        let m = command_failed(
            "cargo clean -p app",
            Path::new("/w/app/build/verus"),
            &out,
            Lang::En,
        );
        assert_eq!(
            m,
            "`cargo clean -p app` failed in `/w/app/build/verus` (exit code 101):\nerror: current package believes it's in a workspace when it's not"
        );
        assert!(!m.contains("ExitStatus"), "{m}");
        // `--lang ja` では日本語で出す。
        assert_eq!(
            command_failed(
                "cargo clean -p app",
                Path::new("/w/app/build/verus"),
                &out,
                Lang::Ja
            ),
            "`/w/app/build/verus` で `cargo clean -p app` が失敗しました（終了コード 101）:\nerror: current package believes it's in a workspace when it's not"
        );
        // シグナルで止まったもの（終了コードがない）。
        let out = std::process::Output {
            status: std::process::ExitStatus::from_raw(9),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert_eq!(
            command_failed("cargo", Path::new("/w"), &out, Lang::En),
            "`cargo` failed in `/w` (killed by signal 9)"
        );
        assert_eq!(
            command_failed("cargo", Path::new("/w"), &out, Lang::Ja),
            "`/w` で `cargo` が失敗しました（シグナル 9 で停止）"
        );
    }

    #[test]
    fn cannot_run_follows_the_language() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(cannot_run("cargo verus", &e, Lang::En).starts_with("cannot run `cargo verus`: "));
        assert!(
            cannot_run("cargo verus", &e, Lang::Ja).starts_with("`cargo verus` を起動できません: ")
        );
    }

    fn item(item: &str, status: &'static str) -> ItemStatus {
        ItemStatus {
            item: item.into(),
            status,
            via: None,
        }
    }

    /// 実行関数の `verified` が停止を含まないことの注記（#81）は、`verified` の実行関数があるときだけ出し、
    /// `decreases` のある関数でも停止を含まないと断る（`decreases` を書いたループや再帰だけが止まる、
    /// と言いすぎない）。
    #[test]
    fn termination_note_only_for_verified_exec_functions() {
        let exec = vec!["k::count".to_string(), "k::spin".to_string()];
        let verified = [item("k::count", "verified"), item("k::sum", "verified")];
        let en = termination_note(&verified, &exec, Lang::En).expect("verified の実行関数がある");
        assert!(en.contains("does not include termination"), "{en}");
        assert!(en.contains("may not terminate"), "{en}");
        assert!(!en.contains("termination is proven"), "{en}");
        let ja = termination_note(&verified, &exec, Lang::Ja).expect("verified の実行関数がある");
        assert!(ja.contains("停止を含みません"), "{ja}");
        assert!(ja.contains("止まらないことがあります"), "{ja}");
        // 実行関数が verified でなければ（failed・unchecked）、spec の側だけが verified なら、出さない。
        assert_eq!(
            termination_note(
                &[item("k::count", "failed"), item("k::sum", "verified")],
                &exec,
                Lang::En
            ),
            None
        );
        assert_eq!(
            termination_note(&[item("k::sum", "verified")], &exec, Lang::En),
            None
        );
        assert_eq!(termination_note(&verified, &[], Lang::En), None);
    }

    /// `verify --json` は、実行関数の項目に `"termination":"not_proven"` を付ける（#81）。
    #[test]
    fn json_marks_exec_items_as_termination_not_proven() {
        let exec = vec!["k::count".to_string()];
        let items = [item("k::count", "verified"), item("k::sum", "verified")];
        let ran = Summary::Ran {
            verified: 2,
            errors: 0,
        };
        let json = status_json(&items, ran, &Cheating::Clean, &[], true, &exec);
        assert!(
            json.contains(r#"{"item":"k::count","status":"verified","termination":"not_proven"}"#),
            "{json}"
        );
        assert!(
            json.contains(r#"{"item":"k::sum","status":"verified"}"#),
            "{json}"
        );
        // 実行関数の一覧がなければ付かない（壊す操作が当たったことを確かめる）。
        let json = status_json(&items, ran, &Cheating::Clean, &[], true, &[]);
        assert!(!json.contains("termination"), "{json}");
    }

    fn facts() -> ItemFacts {
        ItemFacts {
            admitted: false,
            unchecked: false,
            external: false,
            error: false,
            related: false,
            checked: true,
            stray_errors: false,
            stopped: false,
            tainted: false,
        }
    }

    /// 関数の名前の集合（`func-details` の鍵や、Verus が失敗とした関数）。
    fn names(v: &[&str]) -> std::collections::BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// 検証まで進んだ実行の要約（検証のエラーが `errors` 件）。
    fn ran(errors: u64) -> Summary {
        Summary::Ran {
            verified: 0,
            errors,
        }
    }

    /// 頼っている cfg の前提（#87 の既定）：テキストの行、`--json` の `cfg_assumption`、標準エラーの文言。
    /// 頼っていなければ、どれも出さない。
    #[test]
    fn cfg_assumption_is_shown_only_when_the_crate_uses_cfg() {
        let names = vec!["my_flag".to_string(), "test".to_string()];
        let text = status_text(&[], ran(0), &Cheating::Clean, &names);
        assert!(text.contains("assumed cfg my_flag, test\n"), "{text}");
        let json: serde_json::Value = serde_json::from_str(&status_json(
            &[],
            ran(0),
            &Cheating::Clean,
            &names,
            true,
            &[],
        ))
        .unwrap();
        assert_eq!(
            json["cfg_assumption"],
            serde_json::json!({"names": ["my_flag", "test"], "note": CFG_ASSUMPTION_EN})
        );
        let (en, ja) = cfg_assumption_message(&names).unwrap();
        assert!(
            en.contains("(my_flag, test)") && en.ends_with(CFG_ASSUMPTION_EN),
            "{en}"
        );
        assert!(
            ja.contains("（my_flag、test）") && ja.ends_with(CFG_ASSUMPTION_JA),
            "{ja}"
        );
        // `test` と `cargo test` を検証していないことを書く（`cargo test` は `cfg(test)` でビルドする）。
        assert!(
            CFG_ASSUMPTION_EN.contains("`cargo test`") && CFG_ASSUMPTION_EN.contains("cfg(test)")
        );
        assert!(
            CFG_ASSUMPTION_JA.contains("`cargo test`") && CFG_ASSUMPTION_JA.contains("`test` なし")
        );

        let text = status_text(&[], ran(0), &Cheating::Clean, &[]);
        assert!(!text.contains("assumed cfg"), "{text}");
        let json: serde_json::Value =
            serde_json::from_str(&status_json(&[], ran(0), &Cheating::Clean, &[], true, &[]))
                .unwrap();
        assert!(json["cfg_assumption"].is_null(), "{json}");
        assert!(cfg_assumption_message(&[]).is_none());
    }

    #[test]
    fn filtering_keeps_every_assumption() {
        let st = |item: &str, status: &'static str| ItemStatus {
            item: item.into(),
            status,
            via: None,
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
        // エラーが副の位置としてこの項目を指す（PR #76 のレビューの B1）。
        assert_eq!(
            decide(ItemFacts {
                related: true,
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
        // Verus が検証の前に止まった（#43）。func-details にあっても verified にしない。
        assert_eq!(
            decide(ItemFacts {
                stopped: true,
                ..facts()
            }),
            "unknown"
        );
        // 紛れ込んだ前提がある（#37）。
        assert_eq!(
            decide(ItemFacts {
                tainted: true,
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
                error: true,
                stopped: true,
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
            exec: vec![],
            verus_patterns: Default::default(),
            macros: vec![],
            cfg_uses: Vec::new(),
        };
        let checked: std::collections::BTreeSet<String> = ["k::f", "k::g", "k::l"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let g = names(&["k::g"]);
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::at("src/lib.rs", 7)],
            Some(&checked),
            Some(&g),
            Summary::Ran {
                verified: 1,
                errors: 1,
            },
            false,
        );
        assert_eq!(
            status_text(
                &st,
                Summary::Ran {
                    verified: 1,
                    errors: 1
                },
                &Cheating::Clean,
                &[]
            ),
            "verified k::f\nfailed k::g\nadmit k::l\nresults 1 verified, 1 errors\n"
        );
        // 項目に入らないエラーがあれば、失敗していない項目は unknown。
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::default()],
            Some(&checked),
            Some(&g),
            ran(1),
            false,
        );
        assert_eq!(st[0].status, "unknown");
        // func-details がなければ unknown。
        let st = statuses(&files, &output, &[], None, None, ran(0), false);
        assert_eq!(st[0].status, "unknown");
        // Verus が失敗とした関数のエラーを読めなければ unknown（nit 4）。失敗した関数は failed。
        let st = statuses(
            &files,
            &output,
            &[],
            Some(&checked),
            Some(&g),
            ran(1),
            false,
        );
        let got: Vec<&str> = st.iter().map(|s| s.status).collect();
        assert_eq!(got, ["unknown", "failed", "admit"]);
        // 紛れ込んだ前提があれば（#37）、失敗していない項目は unknown。前提と failed はそのまま。
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::at("src/lib.rs", 7)],
            Some(&checked),
            Some(&g),
            ran(1),
            true,
        );
        let got: Vec<&str> = st.iter().map(|s| s.status).collect();
        assert_eq!(got, ["unknown", "failed", "admit"]);
        // Verus が検証の前に止まれば、func-details にあっても verified にしない（#43）。テキストと
        // JSON は同じ状態を出す。証明なしの前提は前提のまま出す。
        let stopped = Summary::Stopped { errors: 1 };
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::at("src/lib.rs", 7)],
            Some(&checked),
            Some(&g),
            stopped,
            false,
        );
        assert_eq!(
            status_text(&st, stopped, &Cheating::NotRun, &[]),
            "unknown k::f\nfailed k::g\nadmit k::l\nresults not verified: Verus stopped before verification (1 errors)\n"
        );
        let json = status_json(&st, stopped, &Cheating::NotRun, &[], false, &[]);
        assert!(
            json.contains(r#"{"item":"k::f","status":"unknown"}"#) && !json.contains("verified\""),
            "{json}"
        );
    }

    /// `cargo metadata` の `resolve` から、根のパッケージの依存がコードで使われる名前を読む（#49）。
    #[test]
    fn reads_extern_crate_names_from_cargo_metadata() {
        let json = r#"{"packages":[],"resolve":{"root":"path+file:///b#k@0.1.0","nodes":[
            {"id":"path+file:///i#innocent@0.1.0","deps":[]},
            {"id":"path+file:///b#k@0.1.0","deps":[
                {"name":"clippy","pkg":"path+file:///i#innocent@0.1.0"},
                {"name":"vstd","pkg":"registry+x#vstd@0.0.0"}]}]}}"#;
        assert_eq!(
            parse_extern_crate_names(json),
            Some(vec!["clippy".to_string(), "vstd".to_string()])
        );
        // 根がない・読めないときは名前を決めない。
        assert_eq!(parse_extern_crate_names(r#"{"resolve":null}"#), None);
        assert_eq!(parse_extern_crate_names("not json"), None);
        assert_eq!(
            parse_extern_crate_names(
                r#"{"resolve":{"root":"a","nodes":[{"id":"a","deps":[{"pkg":"x"}]}]}}"#
            ),
            None
        );
    }

    /// 依存クレートの derive と属性マクロが付いた項目は、`macro` の前提として必ず出る（#49）。
    #[test]
    fn macro_uses_are_listed_as_assumptions() {
        use ebykn_syntax::macros::MacroKind;
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![(1, 2, "k5::f".into()), (3, 4, "k5::one".into())],
        }];
        let mac = |item: &str, kind, path: &str| ebykn_lower::MacroUse {
            item: item.into(),
            kind,
            path: path.into(),
            rel: "src/lib.rs".into(),
        };
        let output = Output {
            files: files.clone(),
            macros: vec![
                mac("k5::Foo", MacroKind::Derive, "serde::Serialize"),
                mac("k5::one", MacroKind::Attribute, "tokio::main"),
            ],
            ..Default::default()
        };
        let ran2 = Summary::Ran {
            verified: 2,
            errors: 0,
        };
        let checked: std::collections::BTreeSet<String> =
            ["k5::f", "k5::one"].iter().map(|s| s.to_string()).collect();
        let st = statuses(&files, &output, &[], Some(&checked), None, ran2, false);
        assert_eq!(
            status_text(&st, ran2, &Cheating::Clean, &[]),
            "verified k5::f\nverified k5::one\nmacro serde::Serialize derive on k5::Foo\nmacro tokio::main on k5::one\nresults 2 verified, 0 errors\n"
        );
        // 前提なので、項目を絞っても出る。
        let shown = select(&st, &["f"]).unwrap();
        let lines: Vec<String> = shown.iter().map(ItemStatus::line).collect();
        assert_eq!(
            lines,
            [
                "verified k5::f",
                "macro serde::Serialize derive on k5::Foo",
                "macro tokio::main on k5::one"
            ]
        );
        let json = status_json(&shown, ran2, &Cheating::Clean, &[], true, &[]);
        assert!(
            json.contains(
                r#"{"item":"k5::Foo","kind":"derive","macro":"serde::Serialize","status":"macro"}"#
            ),
            "{json}"
        );
        // 記録がなければ出ない（壊す操作が当たったことを確かめる）。
        let none = Output {
            macros: vec![],
            ..output.clone()
        };
        let st = statuses(&files, &none, &[], Some(&checked), None, ran2, false);
        assert!(st.iter().all(|s| s.status != "macro"));
    }

    /// PR #76 のレビューの B1：主な span が呼ばれる側の署名にあるエラー（`callee may open invariants that
    /// caller cannot`）は、呼び出しのある `drop` を failed にし、呼ばれる側を failed にしない。
    #[test]
    fn errors_are_assigned_by_their_labels() {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (10, 11, "drop::helper".into()),
                (13, 13, "drop::C".into()),
                (15, 20, "drop::C::drop".into()),
                (22, 23, "drop::other".into()),
            ],
        }];
        let output = Output {
            files: files.clone(),
            ..Default::default()
        };
        let checked: std::collections::BTreeSet<String> =
            ["drop::helper", "drop::C::drop", "drop::other"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        let raw = "error: callee may open invariants that caller cannot
  --> src/lib.rs:10:1
   |
10 | pub fn helper(x: u64) -> u64
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ invariants opened by callee
...
16 |     fn drop(&mut self) opens_invariants none
   |                                         ^^^^ invariants opened by caller
17 |     no_unwind {
18 |         self.a = helper(self.a);
   |                  ^^^^^^^^^^^^^^ at this call-site

error: could not compile `drop` (lib) due to 1 previous error
";
        let (_, errors) = read_diagnostics(raw, false);
        let sites: Vec<ErrorSites> = errors.iter().map(|e| e.sites.clone()).collect();
        let c = names(&["drop::C::drop"]);
        let st = statuses(
            &files,
            &output,
            &sites,
            Some(&checked),
            Some(&c),
            ran(1),
            false,
        );
        let got: Vec<&str> = st.iter().map(|s| s.status).collect();
        assert_eq!(got, ["verified", "unknown", "failed", "verified"]);
        // 副の位置（`-` のラベル）だけが指す項目は、verified にしない。
        let related = ErrorSites {
            blamed: vec![("src/lib.rs".into(), 16)],
            related: vec![("src/lib.rs".into(), 22)],
        };
        let st = statuses(
            &files,
            &output,
            &[related],
            Some(&checked),
            Some(&c),
            ran(1),
            false,
        );
        let got: Vec<&str> = st.iter().map(|s| s.status).collect();
        assert_eq!(got, ["verified", "unknown", "failed", "unknown"]);
    }

    /// #79 の再現（`two_errors`）の生成コードの項目と、Verus が見た関数。
    fn two_errors() -> (Vec<OutFile>, Output, std::collections::BTreeSet<String>) {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (10, 12, "two_errors::ok".into()),
                (14, 16, "two_errors::bad".into()),
            ],
        }];
        let output = Output {
            files: files.clone(),
            ..Default::default()
        };
        (files, output, names(&["two_errors::ok", "two_errors::bad"]))
    }

    fn got(st: &[ItemStatus]) -> Vec<&'static str> {
        st.iter().map(|s| s.status).collect()
    }

    /// #79：一つの関数の `ensures` が二つ成り立たないと、見出しは 2 つで Verus の `errors` は 1。
    /// Verus が通したほかの関数まで unknown にしない。
    #[test]
    fn several_errors_in_one_function_leave_others_verified() {
        let (files, output, checked) = two_errors();
        let raw = "error: postcondition not satisfied
  --> build/verus/src/lib.rs:15:9（生成コード）
   |
15 | ensures r == 1, r == 2, { x }
   |         ^^^^^^            - at the end of the function body
   |         |
   |         failed this postcondition

error: postcondition not satisfied
  --> build/verus/src/lib.rs:15:17（生成コード）
   |
15 | ensures r == 1, r == 2, { x }
   |                 ^^^^^^    - at the end of the function body
   |                 |
   |                 failed this postcondition

error: could not compile `two_errors` (lib) due to 2 previous errors
";
        let (_, errors) = read_diagnostics(raw, false);
        let sites: Vec<ErrorSites> = errors.iter().map(|e| e.sites.clone()).collect();
        assert_eq!(sites.len(), 2);
        let bad = names(&["two_errors::bad"]);
        let st = statuses(
            &files,
            &output,
            &sites,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["verified", "failed"]);
    }

    /// #79 のコメント：出力に呼ばれる側の行しか出ないエラーで、Verus が失敗とした呼び出し元を verified に
    /// しない。エラーが Verus の通した関数を指せば、割り当てられないとみなす。見出しの数がたまたま合っても
    /// （見出し 1 つ、`errors` 1）同じ。
    #[test]
    fn errors_blaming_functions_verus_passed_are_not_attributed() {
        let (files, output, checked) = two_errors();
        let bad = names(&["two_errors::bad"]);
        // エラーは ok の行だけを指し、Verus が失敗としたのは bad。
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::at("src/lib.rs", 11)],
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 見出しが 2 つでも同じ（Verus が失敗とした関数を指すエラーがない）。
        let twice = [
            ErrorSites::at("src/lib.rs", 11),
            ErrorSites::at("src/lib.rs", 12),
        ];
        let st = statuses(
            &files,
            &output,
            &twice,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 成否が読めなければ、今までどおりエラーの場所で決める（ok が failed、bad は unknown）。
        let st = statuses(&files, &output, &twice, Some(&checked), None, ran(1), false);
        assert_eq!(got(&st), ["failed", "unknown"]);
        // Verus の数えたエラーがあるのに失敗とした関数が一つもなければ、どれも verified にしない。
        let none = names(&[]);
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::at("src/lib.rs", 15)],
            Some(&checked),
            Some(&none),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "unknown"]);
    }

    /// 条件 2：数が合い、Verus が失敗とした関数を指すエラーもあるが、別のエラーが Verus の通した関数を
    /// 指す。そのエラーの持ち主が分からないので、ほかの項目も verified にしない（PR #142 のレビュー）。
    #[test]
    fn an_error_blaming_a_passed_function_makes_the_rest_unknown() {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (10, 12, "k::ok".into()),
                (14, 16, "k::bad".into()),
                (18, 20, "k::other".into()),
            ],
        }];
        let output = Output {
            files: files.clone(),
            ..Default::default()
        };
        let checked = names(&["k::ok", "k::bad", "k::other"]);
        let bad = names(&["k::bad"]);
        let sites = [
            ErrorSites::at("src/lib.rs", 15),
            ErrorSites::at("src/lib.rs", 11),
        ];
        let st = statuses(
            &files,
            &output,
            &sites,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed", "unknown"]);
        // 余分なエラーがなければ、other は verified（壊す操作が当たったことを確かめる）。
        let st = statuses(
            &files,
            &output,
            &sites[..1],
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["verified", "failed", "verified"]);
    }

    /// 数と名前がほかで合っていても、位置のないエラーや項目の外のエラーが余分にあれば、割り当てられない
    /// とみなす（`outside`。PR #142 のレビュー）。
    #[test]
    fn extra_errors_without_an_item_make_the_rest_unknown() {
        let (files, output, checked) = two_errors();
        let bad = names(&["two_errors::bad"]);
        for extra in [ErrorSites::default(), ErrorSites::at("src/lib.rs", 100)] {
            let sites = [ErrorSites::at("src/lib.rs", 15), extra.clone()];
            let st = statuses(
                &files,
                &output,
                &sites,
                Some(&checked),
                Some(&bad),
                ran(1),
                false,
            );
            assert_eq!(got(&st), ["unknown", "failed"], "{extra:?}");
        }
    }

    /// 割り当てを決められないときは、失敗していない項目を unknown にする（#79 のコメントの条件）。
    #[test]
    fn unattributable_errors_make_the_rest_unknown() {
        let (files, output, checked) = two_errors();
        let bad = names(&["two_errors::bad"]);
        let both = names(&["two_errors::ok", "two_errors::bad"]);
        let at_bad = [ErrorSites::at("src/lib.rs", 15)];
        // Verus が失敗とした関数のエラーを読み落とした（ok の見出しがない）。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&both),
            ran(2),
            false,
        );
        assert_eq!(got(&st), ["failed", "failed"]);
        // Verus の数えたエラーが、失敗とした関数の数と違う（bit_vector の問い合わせの失敗は
        // `func_fails` に入らない）。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&bad),
            ran(2),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 関数ごとの成否が読めない。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            None,
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // Verus が失敗とした関数が、どの項目でもない。
        let other = names(&["two_errors::bad", "two_errors::hidden"]);
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&other),
            ran(2),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 同じ場合で、照合が成り立てば ok は verified（壊す操作が当たったことを確かめる）。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["verified", "failed"]);
        // エラーがなく、成否が読めなくても verified。
        let st = statuses(&files, &output, &[], Some(&checked), None, ran(0), false);
        assert_eq!(got(&st), ["verified", "verified"]);
    }

    /// `--time` の JSON から、Verus が失敗とした関数を読む（固定した Verus の出力を縮めたもの）。
    #[test]
    fn reads_failed_functions_from_the_time_breakdown() {
        let json = r#"{"func-details":{"two_errors::bad":{},"two_errors::ok":{}},
            "verification-results":{"success":false,"verified":1,"errors":1},
            "times-ms":{"smt":{"smt-run-module-times":[{"module":"","function-breakdown":[
                {"function":"two_errors::bad","time":0,"success":false},
                {"function":"two_errors::ok","time":0,"success":true}]}]}}}"#;
        let j = parse_json(json).unwrap();
        assert_eq!(j.failed, Some(names(&["two_errors::bad"])));
        assert_eq!(j.results, Some((1, 1)));
        // 一つでも読めない項目があれば、読めた分で決めない。
        let broken = json.replace(r#""success":true"#, r#""success":"yes""#);
        assert_eq!(parse_json(&broken).unwrap().failed, None);
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut no_success = v.clone();
        no_success["times-ms"]["smt"]["smt-run-module-times"][0]["function-breakdown"][1]
            .as_object_mut()
            .unwrap()
            .remove("success");
        assert_eq!(failed_functions(&no_success), None);
        // `--time` の出力がなければ読めない。
        let mut no_time = v;
        no_time.as_object_mut().unwrap().remove("times-ms");
        assert_eq!(failed_functions(&no_time), None);
    }

    #[test]
    fn items_sharing_a_verus_name_are_unknown() {
        // R2：同じパスになる二つの項目（trait の impl の S::e と、#[cfg] で消える固有の S::e）。
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (1, 2, "k3::S::e".into()),
                (3, 4, "k3::S::e".into()),
                (5, 6, "k3::f".into()),
            ],
        }];
        let output = Output {
            files: files.clone(),
            ..Default::default()
        };
        let checked: std::collections::BTreeSet<String> = ["k3::S::e", "k3::f"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let st = statuses(&files, &output, &[], Some(&checked), None, ran(0), false);
        let got: Vec<&str> = st.iter().map(|s| s.status).collect();
        assert_eq!(got, ["unknown", "unknown", "verified"]);
    }

    #[test]
    fn foreign_impl_methods_match_their_verus_names() {
        // S8：クレートの外の型への impl のメソッドは `impl&%N`。形に当たる鍵が一つなら決まる。
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![(1, 2, "k4::<u32 as Tr>::e".into())],
        }];
        let mut output = Output {
            files: files.clone(),
            ..Default::default()
        };
        output
            .verus_patterns
            .insert("k4::<u32 as Tr>::e".into(), "k4::impl&%*::e".into());
        let one: std::collections::BTreeSet<String> = ["k4::impl&%0::e", "k4::Tr::e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            statuses(&files, &output, &[], Some(&one), None, ran(0), false)[0].status,
            "verified"
        );
        // 当たる鍵が二つ（同じ名前のメソッドを持つ別の impl）なら決められない。
        let two: std::collections::BTreeSet<String> = ["k4::impl&%0::e", "k4::impl&%1::e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            statuses(&files, &output, &[], Some(&two), None, ran(0), false)[0].status,
            "unknown"
        );
    }

    #[test]
    fn reads_diagnostics_and_skips_the_json() {
        let raw = "{\n  \"func-details\": {\n    \"k::f\": {}\n  }\n}\nerror: postcondition not satisfied\n  --> src/lib.rs:33:13\n   |\nnote: x\n  --> /p/prelude.rs:3:1\nerror: precondition\n  --> /abs/other.rs:1:1\nerror: could not compile `k`\n";
        let (text, errors) = read_diagnostics(raw, true);
        let at: Vec<_> = errors.iter().map(|e| e.at.clone()).collect();
        assert_eq!(at, [Some(("src/lib.rs".into(), 33)), None]);
        assert_eq!(errors[0].message, "postcondition not satisfied");
        assert_eq!(errors[1].loc.as_deref(), Some("/abs/other.rs:1:1"));
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

    /// `impl Tr for Missing`（定義のない型）の生成コードで `cargo verus focus -- --output-json` が
    /// 出したもの。コンパイルエラーでも `verification-results` は `verified: 0, errors: 0` になる。
    const MISSING_STDOUT: &str = "{\n  \"func-details\": {},\n  \"verification-results\": {\n    \"encountered-error\": true,\n    \"encountered-vir-error\": false,\n    \"success\": false,\n    \"verified\": 0,\n    \"errors\": 0,\n    \"is-verifying-entire-crate\": true\n  }\n}\n";
    const MISSING_STDERR: &str = "error[E0425]: cannot find type `Missing` in this scope\n --> src/lib.rs:12:13\n   |\n12 | impl Tr for Missing {\n   |             ^^^^^^^ not found in this scope\n\nFor more information about this error, try `rustc --explain E0425`.\nerror: could not compile `envt` (lib) due to 1 previous error\n";

    fn missing_run(success: bool) -> Run {
        let j = parse_json(MISSING_STDOUT).unwrap();
        let (text, errors) = read_diagnostics(&format!("{MISSING_STDOUT}{MISSING_STDERR}"), true);
        Run {
            success,
            text,
            results: j.results,
            errors,
            checked: Some(j.checked),
            failed: j.failed,
            compiled: true,
        }
    }

    #[test]
    fn compile_errors_are_reported_as_not_verified() {
        let r = missing_run(false);
        assert_eq!(r.results, Some((0, 0)));
        assert_eq!(r.summary(), Summary::Stopped { errors: 1 });
        let text = status_text(&[], r.summary(), &Cheating::NotRun, &[]);
        assert_eq!(
            text,
            "results not verified: Verus stopped before verification (1 errors)\n"
        );
        assert!(!text.contains("0 errors"), "{text}");
        let json = status_json(&[], r.summary(), &Cheating::NotRun, &[], false, &[]);
        assert!(json.contains("\"ran\":false"), "{json}");
        // 件数がない（Verus が JSON を出さなかった）ときも同じ。
        let mut none = missing_run(false);
        none.results = None;
        assert_eq!(none.summary(), Summary::Stopped { errors: 1 });
    }

    #[test]
    fn cfg_assumptions_are_reported() {
        let ran = Summary::Ran {
            verified: 1,
            errors: 0,
        };
        let names = vec!["my_flag".to_string(), "test".to_string()];
        assert_eq!(
            status_text(&[], ran, &Cheating::Clean, &names),
            "assumed cfg my_flag, test\nresults 1 verified, 0 errors\n"
        );
        let json = status_json(&[], ran, &Cheating::Clean, &names, true, &[]);
        assert!(
            json.contains("\"cfg_assumption\":{\"names\":[\"my_flag\",\"test\"]"),
            "{json}"
        );
        let none = status_json(&[], ran, &Cheating::Clean, &[], true, &[]);
        assert!(none.contains("\"cfg_assumption\":null"), "{none}");
    }

    #[test]
    fn smuggled_assumptions_are_reported() {
        let e = VerusError {
            message: no_cheating::ADMIT.into(),
            at: Some(("src/lib.rs".into(), 22)),
            loc: Some("src/lib.rs:22:19".into()),
            block: String::new(),
            sites: ErrorSites::default(),
        };
        let ran = Summary::Ran {
            verified: 1,
            errors: 0,
        };
        let smuggled = Cheating::Smuggled(vec![e]);
        let text = status_text(&[], ran, &smuggled, &[]);
        assert_eq!(
            text,
            "smuggled build/verus/src/lib.rs:22:19 `assume`/`admit` not allowed with `--no-cheating`\nresults 1 verified, 0 errors\n"
        );
        let json = status_json(&[], ran, &smuggled, &[], false, &[]);
        assert!(
            json.contains("\"location\":\"build/verus/src/lib.rs:22:19\""),
            "{json}"
        );
        assert!(json.contains("\"checked\":true"), "{json}");
        assert!(json.contains("\"state\":\"smuggled\""), "{json}");
        let unread = Cheating::Unreadable(no_cheating::Unreadable::NoErrors);
        assert!(status_text(&[], ran, &unread, &[]).contains("smuggled unreadable: "));
        // 「読めない」と「走らせていない」は state で見分ける。
        let unread_json = status_json(&[], ran, &unread, &[], false, &[]);
        assert!(unread_json.contains("\"checked\":false"), "{unread_json}");
        assert!(
            unread_json.contains("\"state\":\"unreadable\""),
            "{unread_json}"
        );
        let not_run = status_json(&[], ran, &Cheating::NotRun, &[], false, &[]);
        assert!(not_run.contains("\"state\":\"not_run\""), "{not_run}");
        assert_eq!(
            status_text(&[], ran, &Cheating::Clean, &[]),
            "results 1 verified, 0 errors\n"
        );
    }

    #[test]
    fn verification_runs_keep_their_counts() {
        // 成功したとき、検証のエラーで失敗したときは、件数をそのまま出す。
        let mut ok = missing_run(true);
        ok.results = Some((3, 0));
        assert_eq!(
            ok.summary(),
            Summary::Ran {
                verified: 3,
                errors: 0
            }
        );
        let mut failed = missing_run(false);
        failed.results = Some((1, 2));
        assert_eq!(
            failed.summary(),
            Summary::Ran {
                verified: 1,
                errors: 2
            }
        );
        let json = status_json(&[], failed.summary(), &Cheating::Clean, &[], false, &[]);
        assert!(
            json.contains("\"ran\":true") && json.contains("\"errors\":2"),
            "{json}"
        );
    }
}
