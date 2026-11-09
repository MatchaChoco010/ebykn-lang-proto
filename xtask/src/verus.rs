//! 固定した Verus の読み込み・導入・検証（`verus-install` / `verus-verify`）。
//!
//! 固定値はルートの `Cargo.toml` の `[workspace.metadata.verus]` に置く。Verus はリポジトリの外の
//! キャッシュ（`$XDG_CACHE_HOME/ebykn/verus/<版>/`、無ければ `~/.cache/ebykn/verus/<版>/`）に入れる。

use std::path::{Path, PathBuf};
use std::process::Command;

/// Verus で検証するクレート。`verus-verify` は各クレートを Verus で検証し、Verus なしの cargo でもビルドする。
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

/// 導入済みの Verus のディレクトリ（`verus` と `cargo-verus` がある）。印の sha256 が固定値と違えば失敗にする。
pub fn installed(pin: &Pin) -> Result<PathBuf, String> {
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
    if installed(&pin).is_err() {
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
    let bin = installed(&pin)?;
    let version = Command::new(bin.join("verus"))
        .args(["--version", "--output-json"])
        .env("RUSTUP_TOOLCHAIN", &pin.toolchain)
        .output()
        .map_err(|e| format!("verus を起動できません: {e}"))?;
    let text = String::from_utf8_lossy(&version.stdout);
    for want in [&pin.version, &pin.commit, &pin.toolchain] {
        if !text.contains(want.as_str()) {
            return Err(format!(
                "verus --version の出力に {want} がありません:\n{text}"
            ));
        }
    }
    Ok(bin)
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
    for krate in CRATES {
        let dir = root.join(krate);
        // 前回の成果物が残っていると cargo が Verus を呼ばず、結果の行が出ない。対象のクレートだけ消す。
        let name = match package_name(&dir) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("error: verus-verify: {krate}: {e}");
                ok = false;
                continue;
            }
        };
        // `cargo verus focus` は target-dir の下の verus-partial に成果物を置く。
        let cleaned = ["target/verus", "target/verus/verus-partial"]
            .iter()
            .try_for_each(|t| {
                let mut clean = Command::new("cargo");
                clean_cargo(&mut clean)
                    .args(["clean", "--quiet", "-p", &name, "--target-dir"])
                    .arg(root.join(t))
                    .current_dir(&dir)
                    .env("RUSTUP_TOOLCHAIN", &pin.toolchain);
                run(&mut clean)
            });
        if let Err(e) = cleaned {
            eprintln!("error: verus-verify: {krate}: {e}");
            ok = false;
            continue;
        }
        let mut cmd = Command::new("cargo");
        clean_cargo(&mut cmd)
            .args(["verus", "focus", "--target-dir"])
            .arg(root.join("target/verus"))
            .args(["--", "--no-cheating", "--triggers-mode", "silent"])
            .current_dir(&dir)
            .env("PATH", &path)
            .env("RUSTUP_TOOLCHAIN", &pin.toolchain);
        eprintln!("$ (cd {krate} && cargo verus focus -- --no-cheating)");
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
        match results_line(&text) {
            Some((n, 0)) if out.status.success() && n > 0 => {
                eprintln!("ok: verus-verify: {krate}: {n} verified, 0 errors");
            }
            got => {
                eprintln!(
                    "error: verus-verify: {krate}: 検証が通りません（結果 {got:?}、終了 {}）",
                    out.status
                );
                ok = false;
                continue;
            }
        }
        // Verus なしの cargo（リポジトリの toolchain）でビルドできること。
        let mut plain = Command::new("cargo");
        clean_cargo(&mut plain)
            .args(["build", "--quiet", "--target-dir"])
            .arg(root.join("target/verus-plain"))
            .current_dir(&dir);
        match run(&mut plain) {
            Ok(()) => eprintln!("ok: verus-verify: {krate}: Verus なしの cargo build"),
            Err(e) => {
                eprintln!("error: verus-verify: {krate}: Verus なしでビルドできません: {e}");
                ok = false;
            }
        }
    }
    ok
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
