//! `playground`：`ebykn` をビルドし、`examples/` を git が追跡しない `playground/` に写す。
//!
//! 作者が release のワークスペースで例を書き換えて試すためのもの。`playground/` は `.gitignore` に
//! 入っているので、書き換えても release の早送り（作業ツリーに変更がないことを確かめる）の邪魔にならない。
//! すでにあるファイルは上書きしない（書き換えたものを残す）。新しい例だけが足される。

use std::path::Path;

use crate::gates::examples::DIR;

pub const PLAYGROUND: &str = "playground";

/// 写した結果（`playground/` からの相対パス）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Copied {
    /// 写したファイル。
    pub copied: Vec<String>,
    /// すでにあり、中身が `examples/` と同じなので写さなかったファイル。
    pub same: Vec<String>,
    /// すでにあり、中身が `examples/` と違うので残したファイル（書き換えたものか、`examples/` が変わったもの）。
    pub kept: Vec<String>,
}

/// `files`（リポジトリ直下からの相対パス）のうち `examples/` の下のものを `<root>/playground/` に写す。
pub fn copy(root: &Path, files: &[String]) -> Result<Copied, String> {
    let mut out = Copied::default();
    for f in files {
        let Some(rel) = f.strip_prefix(DIR) else {
            continue;
        };
        let to = root.join(PLAYGROUND).join(rel);
        if to.exists() {
            let same = std::fs::read(&to).ok() == std::fs::read(root.join(f)).ok();
            if same { &mut out.same } else { &mut out.kept }.push(rel.to_string());
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::copy(root.join(f), &to).map_err(|e| format!("{f}: {e}"))?;
        out.copied.push(rel.to_string());
    }
    if out.copied.is_empty() && out.same.is_empty() && out.kept.is_empty() {
        return Err(format!("{DIR} に写すファイルがありません"));
    }
    Ok(out)
}

pub fn run(root: &Path, files: &[String]) -> bool {
    if !crate::cargo(&["build", "--quiet", "-p", "ebykn"]) {
        return false;
    }
    let c = match copy(root, files) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: playground: {e}");
            return false;
        }
    };
    let dir = root.join(PLAYGROUND);
    eprintln!(
        "ok: playground: {} に {} 個のファイルを写した（同じ中身ですでにあるもの {} 個）",
        dir.display(),
        c.copied.len(),
        c.same.len()
    );
    for f in &c.copied {
        eprintln!("    {PLAYGROUND}/{f}");
    }
    if !c.kept.is_empty() {
        eprintln!(
            "  中身が examples/ と違うので、上書きせずに残した（書き換えたものか、examples/ が変わったもの）: {} 個",
            c.kept.len()
        );
        for k in &c.kept {
            eprintln!("    {PLAYGROUND}/{k}");
        }
        eprintln!(
            "  examples/ の形に戻すには、そのファイルを消してからもう一度 `cargo xtask playground` を実行する"
        );
    }
    let pin = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| e.to_string())
        .and_then(|t| crate::verus::read_pin(&t))
        .and_then(|p| crate::verus::installed(&p));
    if let Err(e) = pin {
        eprintln!(
            "  注意：固定した Verus が使えません。先に `cargo xtask verus-install` を実行する\n  （{e}）"
        );
    }
    eprintln!(
        "\n次のようにして試す（説明は {PLAYGROUND}/README.md）:\n  export PATH=\"{}:$PATH\"\n  cd {} && ebykn verify",
        root.join("target/debug").display(),
        dir.join("hello").display()
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn files() -> Vec<String> {
        [
            "examples/README.md",
            "examples/a/Ebykn.toml",
            "examples/a/src/lib.eyk",
            "README.md",
        ]
        .map(String::from)
        .to_vec()
    }

    fn repo() -> TempRepo {
        let mut t = TempRepo::new();
        for f in files() {
            t.write(&f, &format!("元の {f}"));
        }
        t
    }

    #[test]
    fn copies_the_examples() {
        let t = repo();
        let root = t.root();
        let c = copy(root, &files()).unwrap();
        assert_eq!(c.copied, ["README.md", "a/Ebykn.toml", "a/src/lib.eyk"]);
        assert!(c.kept.is_empty() && c.same.is_empty());
        let text = std::fs::read_to_string(root.join("playground/a/src/lib.eyk")).unwrap();
        assert_eq!(text, "元の examples/a/src/lib.eyk");
        // 目次は examples/README.md のもの。リポジトリ直下の README.md は写さない。
        let index = std::fs::read_to_string(root.join("playground/README.md")).unwrap();
        assert_eq!(index, "元の examples/README.md");
    }

    #[test]
    fn does_not_overwrite_existing_files() {
        let t = repo();
        let root = t.root();
        copy(root, &files()).unwrap();
        let edited = root.join("playground/a/src/lib.eyk");
        std::fs::write(&edited, "書き換えた").unwrap();
        std::fs::remove_file(root.join("playground/a/Ebykn.toml")).unwrap();
        let c = copy(root, &files()).unwrap();
        assert_eq!(c.copied, ["a/Ebykn.toml"]);
        assert_eq!(c.same, ["README.md"]);
        assert_eq!(c.kept, ["a/src/lib.eyk"]);
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), "書き換えた");
    }

    #[test]
    fn fails_when_there_is_nothing_to_copy() {
        let t = repo();
        assert!(copy(t.root(), &["README.md".to_string()]).is_err());
    }
}
