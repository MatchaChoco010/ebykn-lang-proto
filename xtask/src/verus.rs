//! 固定した Verus の読み込み・導入・検証（`verus-install` / `verus-verify`）。
//!
//! 固定値はルートの `Cargo.toml` の `[workspace.metadata.verus]` に置く。Verus はリポジトリの外の
//! キャッシュ（`$XDG_CACHE_HOME/ebykn/verus/<版>/`、無ければ `~/.cache/ebykn/verus/<版>/`）に入れる。

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Verus で検証するクレート。`verus-verify` は各クレートを Verus で検証し、Verus なしの cargo でもビルドする。
/// vstd に依存するクレートはすべてここに載せ、`Cargo.lock` を追跡する（`check-verus-pin` が検査する）。
pub const CRATES: &[&str] = &["crates/ebykn_prelude", "tests/golden/stack/expected/verus"];

/// リリースの zip を展開してできるディレクトリ。
const UNPACKED: &str = "verus-x86-linux";
/// 展開が終わったことを示す印。中身は確かめた sha256。
const MARKER: &str = ".sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub release: String,
    pub version: String,
    pub commit: String,
    pub asset: String,
    pub sha256: String,
    pub toolchain: String,
    pub vstd: String,
}

const KEYS: &[&str] = &[
    "release",
    "version",
    "commit",
    "asset",
    "sha256",
    "toolchain",
    "vstd",
];
const SECTION: &str = "[workspace.metadata.verus]";

/// ルートの `Cargo.toml` の `[workspace.metadata.verus]` を読む。読めない行・足りない鍵・重複は失敗にする。
pub fn read_pin(cargo_toml: &str) -> Result<Pin, String> {
    let mut lines = cargo_toml.lines().enumerate();
    if !lines.any(|(_, l)| l.trim() == SECTION) {
        return Err(format!("Cargo.toml に {SECTION} がありません"));
    }
    let mut found: Vec<(&str, String)> = Vec::new();
    for (i, line) in lines {
        let t = line.trim();
        if t.starts_with('[') {
            break;
        }
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let parsed = t.split_once('=').and_then(|(k, v)| {
            let v = v.trim();
            let v = v.strip_prefix('"')?.strip_suffix('"')?;
            (!v.contains('"')).then(|| (k.trim(), v.to_string()))
        });
        let Some((k, v)) = parsed else {
            return Err(format!(
                "Cargo.toml:{}: {SECTION} の行を読めません: {t}",
                i + 1
            ));
        };
        let Some(&key) = KEYS.iter().find(|&&x| x == k) else {
            return Err(format!("Cargo.toml:{}: {SECTION} に未知の鍵 {k}", i + 1));
        };
        if found.iter().any(|(x, _)| *x == key) {
            return Err(format!("Cargo.toml:{}: {SECTION} の {k} が重複", i + 1));
        }
        found.push((key, v));
    }
    let get = |k: &str| {
        found
            .iter()
            .find(|(x, _)| *x == k)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("{SECTION} に {k} がありません"))
    };
    let pin = Pin {
        release: get("release")?,
        version: get("version")?,
        commit: get("commit")?,
        asset: get("asset")?,
        sha256: get("sha256")?,
        toolchain: get("toolchain")?,
        vstd: get("vstd")?,
    };
    let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex(&pin.sha256, 64) {
        return Err(format!("{SECTION} の sha256 が 16 進 64 桁ではありません"));
    }
    if !hex(&pin.commit, 40) {
        return Err(format!("{SECTION} の commit が 16 進 40 桁ではありません"));
    }
    if pin.release != format!("release/{}", pin.version) {
        return Err(format!(
            "{SECTION} の release（{}）と version（{}）が合いません",
            pin.release, pin.version
        ));
    }
    if !pin.asset.starts_with(&format!("verus-{}-", pin.version)) {
        return Err(format!(
            "{SECTION} の asset（{}）が version（{}）の資産ではありません",
            pin.asset, pin.version
        ));
    }
    if !pin
        .commit
        .starts_with(pin.version.rsplit('.').next().unwrap_or_default())
    {
        return Err(format!(
            "{SECTION} の commit（{}）が version（{}）の末尾と合いません",
            pin.commit, pin.version
        ));
    }
    Ok(pin)
}

pub fn root_pin(root: &Path) -> Result<Pin, String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| format!("Cargo.toml を読めません: {e}"))?;
    read_pin(&text)
}

fn cache_dir(pin: &Pin) -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(std::env::var_os("HOME").ok_or("HOME も XDG_CACHE_HOME もありません")?)
            .join(".cache"),
    };
    Ok(base.join("ebykn").join("verus").join(&pin.version))
}

/// 導入済みの Verus のディレクトリ（`verus` と `cargo-verus` がある）。
///
/// 印の sha256 が固定値と違うとき、`verus --version` の版・コミット・toolchain が固定値と合わないとき
/// （キャッシュの中身が差し替わった、toolchain が入っていない）は失敗にする。
pub fn installed(pin: &Pin) -> Result<PathBuf, String> {
    let bin = unpacked(pin)?;
    let out = Command::new(bin.join("verus"))
        .args(["--version", "--output-json"])
        .env("RUSTUP_TOOLCHAIN", &pin.toolchain)
        .output()
        .map_err(|e| format!("verus を起動できません: {e}"))?;
    check_version(&String::from_utf8_lossy(&out.stdout), pin)?;
    Ok(bin)
}

/// `verus --version --output-json` の出力が固定値の版・コミット・toolchain を含む。
fn check_version(text: &str, pin: &Pin) -> Result<(), String> {
    for (key, want) in [
        ("version", &pin.version),
        ("commit", &pin.commit),
        ("toolchain", &pin.toolchain),
    ] {
        let line = format!("\"{key}\": \"{want}");
        if !text.contains(&line) {
            return Err(format!(
                "verus --version の {key} が固定値 {want} ではありません。`cargo xtask verus-install` で入れ直してください:\n{text}"
            ));
        }
    }
    Ok(())
}

/// 展開済みの Verus のディレクトリ。印の sha256 が固定値と違えば失敗にする。
fn unpacked(pin: &Pin) -> Result<PathBuf, String> {
    let dir = cache_dir(pin)?;
    let marker = std::fs::read_to_string(dir.join(MARKER)).unwrap_or_default();
    let bin = dir.join(UNPACKED);
    if marker.trim() != pin.sha256 || !bin.join("verus").is_file() {
        return Err(format!(
            "固定した Verus（{}）が {} にありません。`cargo xtask verus-install` で導入してください",
            pin.release,
            dir.display()
        ));
    }
    Ok(bin)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    eprintln!("$ {cmd:?}");
    let status = cmd
        .status()
        .map_err(|e| format!("{:?} を起動できません: {e}", cmd.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{:?} が失敗しました（{status}）",
            cmd.get_program()
        ))
    }
}

fn sha256_of(file: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum")
        .arg(file)
        .output()
        .map_err(|e| format!("sha256sum を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(format!("sha256sum が失敗しました: {}", file.display()));
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| "sha256sum の出力を読めません".to_string())
}

/// 固定した Verus をキャッシュに入れ、Verus が要求する toolchain を rustup で入れる。
pub fn install(root: &Path) -> bool {
    match try_install(root) {
        Ok(dir) => {
            eprintln!("ok: verus-install: {}", dir.display());
            true
        }
        Err(e) => {
            eprintln!("error: verus-install: {e}");
            false
        }
    }
}

fn try_install(root: &Path) -> Result<PathBuf, String> {
    let pin = root_pin(root)?;
    let dir = cache_dir(&pin)?;
    if unpacked(&pin).is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let zip = dir.join(&pin.asset);
        let url = format!(
            "https://github.com/verus-lang/verus/releases/download/{}/{}",
            pin.release, pin.asset
        );
        run(Command::new("curl")
            .args(["-sSfL", "-o"])
            .arg(&zip)
            .arg(&url))?;
        let got = sha256_of(&zip)?;
        if got != pin.sha256 {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!(
                "{} の sha256 が固定値と違います（期待 {}、実際 {got}）",
                pin.asset, pin.sha256
            ));
        }
        run(Command::new("unzip")
            .args(["-q", "-o"])
            .arg(&zip)
            .arg("-d")
            .arg(&dir))?;
        std::fs::remove_file(&zip).map_err(|e| format!("{}: {e}", zip.display()))?;
        std::fs::write(dir.join(MARKER), format!("{}\n", pin.sha256))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    run(Command::new("rustup").args([
        "toolchain",
        "install",
        &pin.toolchain,
        "--profile",
        "minimal",
    ]))?;
    installed(&pin)
}

/// cargo を子として呼ぶとき、xtask を起動した cargo の環境を引き継がない。
fn clean_cargo(cmd: &mut Command) -> &mut Command {
    for var in [
        "CARGO",
        "RUSTC",
        "RUSTDOC",
        "RUSTUP_TOOLCHAIN",
        "CARGO_TARGET_DIR",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

/// `Cargo.toml` の `[package]` の `name`。
fn package_name(dir: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(dir.join("Cargo.toml"))
        .map_err(|e| format!("Cargo.toml を読めません: {e}"))?;
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if let Some(v) = t
            .strip_prefix("name = \"")
            .and_then(|v| v.strip_suffix('"'))
            && in_package
        {
            return Ok(v.to_string());
        }
    }
    Err("Cargo.toml の [package] に name がありません".to_string())
}

/// `cargo verus` の結果を判定する。結果の行があり、0 errors で、1 件以上を検証し、終了コードが 0 のときだけ通す。
pub fn judge(results: Option<(u64, u64)>, success: bool) -> Result<u64, String> {
    match results {
        Some((n, 0)) if success && n > 0 => Ok(n),
        Some((0, 0)) => Err("何も検証していません（0 verified）".to_string()),
        Some((_, 0)) => Err("0 errors なのに終了コードが 0 ではありません".to_string()),
        Some((n, e)) => Err(format!("{n} verified, {e} errors")),
        None => Err("結果の行（verification results::）がありません".to_string()),
    }
}

/// `verification results:: N verified, M errors` の N と M。
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

/// 各クレートを固定した Verus で検証し（`--no-cheating`）、Verus なしの cargo でもビルドする（OPEN-11）。
/// `verus-verify` が一つのクレートについて順に呼ぶ cargo。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// 前回の成果物が残っていると cargo が Verus を呼ばず、結果の行が出ない。対象のクレートだけ消す。
    Clean,
    /// 固定した Verus で検証する。
    Verify,
    /// Verus なしの cargo（リポジトリの toolchain）でビルドする（OPEN-11）。
    Plain,
}

/// 各段階の cargo の引数。どれも `--locked` を付け、追跡している `Cargo.lock` を書き換えさせない
/// （`cargo clean -p` も依存を解決するので、付けないと lock を作り直す）。
fn verify_steps(root: &Path, name: &str) -> Vec<(Step, Vec<OsString>)> {
    let os = |xs: &[&str]| xs.iter().map(OsString::from).collect::<Vec<_>>();
    let with = |mut a: Vec<OsString>, p: PathBuf, rest: &[&str]| {
        a.push(p.into_os_string());
        a.extend(os(rest));
        a
    };
    vec![
        // `cargo verus focus` は target-dir の下の verus-partial に成果物を置く。
        (
            Step::Clean,
            with(
                os(&["clean", "--quiet", "--locked", "-p", name, "--target-dir"]),
                root.join("target/verus"),
                &[],
            ),
        ),
        (
            Step::Clean,
            with(
                os(&["clean", "--quiet", "--locked", "-p", name, "--target-dir"]),
                root.join("target/verus/verus-partial"),
                &[],
            ),
        ),
        (
            Step::Verify,
            with(
                os(&["verus", "focus", "--locked", "--target-dir"]),
                root.join("target/verus"),
                &["--", "--no-cheating", "--triggers-mode", "silent"],
            ),
        ),
        (
            Step::Plain,
            with(
                os(&["build", "--quiet", "--locked", "--target-dir"]),
                root.join("target/verus-plain"),
                &[],
            ),
        ),
    ]
}

/// cargo の呼び出しはここでだけ作る（`verify_steps` を通らない呼び出しを作らない。テストが数える）。
fn cargo_command(args: &[OsString], dir: &Path) -> Command {
    let mut cmd = Command::new("cargo");
    clean_cargo(&mut cmd).args(args).current_dir(dir);
    cmd
}

/// 各クレートを固定した Verus で検証し（`--no-cheating`）、Verus なしの cargo でもビルドする（OPEN-11）。
pub fn verify(root: &Path) -> bool {
    let pin = match root_pin(root).and_then(|p| installed(&p).map(|b| (p, b))) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error: verus-verify: {e}");
            return false;
        }
    };
    let (pin, bin) = pin;
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("PATH を組み立てられる");
    let mut ok = true;
    'crates: for krate in CRATES {
        let dir = root.join(krate);
        let name = match package_name(&dir) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("error: verus-verify: {krate}: {e}");
                ok = false;
                continue;
            }
        };
        for (step, args) in verify_steps(root, &name) {
            let mut cmd = cargo_command(&args, &dir);
            if step != Step::Plain {
                cmd.env("RUSTUP_TOOLCHAIN", &pin.toolchain);
            }
            match step {
                Step::Clean => {
                    if let Err(e) = run(&mut cmd) {
                        eprintln!("error: verus-verify: {krate}: {e}");
                        ok = false;
                        continue 'crates;
                    }
                }
                Step::Verify => {
                    cmd.env("PATH", &path);
                    eprintln!("$ (cd {krate} && cargo {})", join(&args));
                    let out = match cmd.output() {
                        Ok(o) => o,
                        Err(e) => {
                            eprintln!("error: verus-verify: cargo を起動できません: {e}");
                            return false;
                        }
                    };
                    let text = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    eprint!("{text}");
                    match judge(results_line(&text), out.status.success()) {
                        Ok(n) => eprintln!("ok: verus-verify: {krate}: {n} verified, 0 errors"),
                        Err(e) => {
                            eprintln!(
                                "error: verus-verify: {krate}: 検証が通りません（{e}、終了 {}）",
                                out.status
                            );
                            ok = false;
                            continue 'crates;
                        }
                    }
                }
                Step::Plain => match run(&mut cmd) {
                    Ok(()) => eprintln!("ok: verus-verify: {krate}: Verus なしの cargo build"),
                    Err(e) => {
                        eprintln!(
                            "error: verus-verify: {krate}: Verus なしでビルドできません: {e}"
                        );
                        ok = false;
                    }
                },
            }
        }
    }
    ok
}

fn join(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn reads_the_real_pin() {
        let pin = read_pin(&real("Cargo.toml")).unwrap();
        assert!(pin.release.starts_with("release/"));
    }

    #[test]
    fn rejects_a_wrong_sha256_length() {
        let broken = mutate(&real("Cargo.toml"), "sha256 = \"", "sha256 = \"0");
        assert!(read_pin(&broken).unwrap_err().contains("sha256"));
    }

    #[test]
    fn rejects_a_missing_key() {
        let src = real("Cargo.toml");
        let line = src.lines().find(|l| l.starts_with("toolchain = ")).unwrap();
        let broken = mutate(&src, &format!("{line}\n"), "");
        assert!(read_pin(&broken).unwrap_err().contains("toolchain"));
    }

    #[test]
    fn rejects_an_unreadable_line() {
        let broken = mutate(&real("Cargo.toml"), "vstd = \"", "vstd = '");
        assert!(read_pin(&broken).unwrap_err().contains("読めません"));
    }

    #[test]
    fn rejects_a_release_that_does_not_match_the_version() {
        let broken = mutate(
            &real("Cargo.toml"),
            "release = \"release/0.",
            "release = \"release/1.",
        );
        assert!(read_pin(&broken).unwrap_err().contains("release"));
    }

    #[test]
    fn fails_unless_every_cargo_step_is_locked() {
        let steps = verify_steps(Path::new("/r"), "stack");
        let kinds: Vec<Step> = steps.iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds, [Step::Clean, Step::Clean, Step::Verify, Step::Plain]);
        for (kind, args) in &steps {
            assert!(
                args.iter().any(|a| a == "--locked"),
                "{kind:?} に --locked がない: {args:?}"
            );
        }
    }

    #[test]
    fn fails_when_cargo_is_called_outside_cargo_command() {
        // verify_steps を通らない cargo の呼び出しは --locked の検査をすり抜ける。
        let src = real("xtask/src/verus.rs");
        let body = &src[..src.find("#[cfg(test)]").unwrap()];
        let needle = ["Command::new(", "\"cargo\")"].concat();
        assert_eq!(
            body.matches(needle.as_str()).count(),
            1,
            "cargo の呼び出しは cargo_command だけにする"
        );
    }

    #[test]
    fn passes_a_verified_crate() {
        assert_eq!(judge(Some((5, 0)), true), Ok(5));
    }

    #[test]
    fn fails_when_verification_has_errors() {
        assert!(
            judge(Some((4, 1)), false)
                .unwrap_err()
                .contains("4 verified, 1 errors")
        );
        assert!(judge(Some((4, 1)), true).is_err());
    }

    #[test]
    fn fails_when_there_is_no_results_line() {
        assert!(judge(None, true).unwrap_err().contains("結果の行"));
    }

    #[test]
    fn fails_when_the_exit_code_is_not_zero() {
        assert!(
            judge(Some((5, 0)), false)
                .unwrap_err()
                .contains("終了コード")
        );
    }

    #[test]
    fn fails_when_nothing_is_verified() {
        assert!(
            judge(Some((0, 0)), true)
                .unwrap_err()
                .contains("0 verified")
        );
    }

    fn version_output(pin: &Pin) -> String {
        format!(
            "{{\n  \"verus\": {{\n    \"version\": \"{}\",\n    \"toolchain\": \"{} (overridden by environment variable RUSTUP_TOOLCHAIN)\",\n    \"commit\": \"{}\"\n  }}\n}}\n",
            pin.version, pin.toolchain, pin.commit
        )
    }

    #[test]
    fn accepts_the_pinned_version() {
        let pin = read_pin(&real("Cargo.toml")).unwrap();
        assert_eq!(check_version(&version_output(&pin), &pin), Ok(()));
    }

    #[test]
    fn fails_when_the_installed_verus_is_another_version() {
        let pin = read_pin(&real("Cargo.toml")).unwrap();
        let other = mutate(&version_output(&pin), &pin.commit, &"0".repeat(40));
        assert!(check_version(&other, &pin).unwrap_err().contains("commit"));
        let tc = mutate(
            &version_output(&pin),
            &pin.toolchain,
            "1.98.0-x86_64-unknown-linux-gnu",
        );
        assert!(check_version(&tc, &pin).unwrap_err().contains("toolchain"));
    }

    #[test]
    fn parses_the_results_line() {
        assert_eq!(
            results_line("x\nverification results:: 24 verified, 0 errors\n"),
            Some((24, 0))
        );
        assert_eq!(
            results_line("verification results:: 3 verified, 2 errors"),
            Some((3, 2))
        );
        assert_eq!(results_line("verification results:: many"), None);
        assert_eq!(results_line(""), None);
    }
}
