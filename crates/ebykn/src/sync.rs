//! `ebykn sync`（仕様 §9、§12）：`src/` の変更を `proof/` の写しに 3-way merge で取り込む。
//!
//! 各 `proof/<p>.eykp` について、前回の `src/<p>.eyk` の写し（基準、`.ebykn/sync/<p>.eyk`）から今の
//! `src/<p>.eyk` への変更を、`git merge-file` で `.eykp` に当てる。そのあと基準を今の `src/<p>.eyk` で
//! 置き換える。衝突は印（`<<<<<<<` など）をファイルに残し、終了コードを 1 にする。
//!
//! 決めたこと（`docs/current-spec.md` §9「`ebykn sync` の決めごと」）：
//! - 基準がない `.eykp` は、今の `src/<p>.eyk` を基準として書くだけで、`.eykp` は変えない。
//! - 衝突しても基準は今の `src/<p>.eyk` に進める（印に `src/` の変更が入っているので、次の `sync` で
//!   同じ変更を二度当てない）。
//! - 対応する `src/<p>.eyk` がない `.eykp` は変えず、誤りとして数える。
//! - `git merge-file -p` の出力を書き戻す。`git` が動かなければ、そのファイルは変えない。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 一つの `.eykp` の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// `src/` が基準から変わっておらず、何もしなかった。
    Unchanged,
    /// `src/` の変更を衝突なく取り込んだ。
    Merged,
    /// 衝突した。数は衝突の箇所の数。印はファイルに残した。
    Conflict(u32),
    /// 基準がなかったので、今の `src/` を基準として書いた（`.eykp` は変えていない）。
    BaseCreated,
    /// 対応する `src/<p>.eyk` がない（E0301 に当たる）。何も変えていない。
    MissingSource,
    /// 読み書きか `git merge-file` が失敗した。`.eykp` は変えていない。
    Failed(String),
    /// `.eykp` には取り込んだが、基準を書けなかった。基準を直すまで `sync` をもう一度実行すると、
    /// 同じ変更を二度当てる。
    BaseNotUpdated(String),
}

impl Outcome {
    /// `sync` を失敗（終了コード 1）にするか。
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            Outcome::Conflict(_)
                | Outcome::MissingSource
                | Outcome::Failed(_)
                | Outcome::BaseNotUpdated(_)
        )
    }

    fn key(&self) -> &'static str {
        match self {
            Outcome::Unchanged => "unchanged",
            Outcome::Merged => "merged",
            Outcome::Conflict(_) => "conflict",
            Outcome::BaseCreated => "base-created",
            Outcome::MissingSource => "missing-source",
            Outcome::Failed(_) => "failed",
            Outcome::BaseNotUpdated(_) => "base-not-updated",
        }
    }
}

/// 一つの `.eykp` について、どのファイルをどう扱ったか。パスはプロジェクトのルートからの相対。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSync {
    pub proof: String,
    pub source: String,
    pub base: String,
    pub outcome: Outcome,
}

/// `sync` の結果。`files` は `.eykp` のパスの順。
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub files: Vec<FileSync>,
    /// `proof/` を読めなかったなど、ファイルごとでない誤り。
    pub errors: Vec<String>,
}

impl SyncReport {
    pub fn success(&self) -> bool {
        self.errors.is_empty() && !self.files.iter().any(|f| f.outcome.is_failure())
    }

    /// 1 ファイル 1 行の説明。
    pub fn lines(&self, lang: ebykn_syntax::diag::Lang) -> Vec<String> {
        use ebykn_syntax::diag::Lang::{En, Ja};
        let mut out: Vec<String> = self.errors.iter().map(|e| format!("error: {e}")).collect();
        for f in &self.files {
            let (p, s, b) = (&f.proof, &f.source, &f.base);
            let line = match (&f.outcome, lang) {
                (Outcome::Unchanged, En) => format!("unchanged {p}"),
                (Outcome::Unchanged, Ja) => {
                    format!("unchanged {p}（`{s}` は前回から変わっていません）")
                }
                (Outcome::Merged, En) => format!("merged {p} (changes in `{s}`)"),
                (Outcome::Merged, Ja) => format!("merged {p}（`{s}` の変更を取り込みました）"),
                (Outcome::Conflict(n), En) => format!(
                    "conflict {p}: {n} conflict(s) left in the file; resolve them, then run `ebykn check`"
                ),
                (Outcome::Conflict(n), Ja) => format!(
                    "conflict {p}: 衝突 {n} か所をファイルに残しました。直してから `ebykn check` を実行してください"
                ),
                (Outcome::BaseCreated, En) => format!(
                    "base {p}: no base `{b}` yet; recorded the current `{s}` as the base without changing `{p}`"
                ),
                (Outcome::BaseCreated, Ja) => format!(
                    "base {p}: 基準 `{b}` がなかったので、今の `{s}` を基準として書きました（`{p}` は変えていません）"
                ),
                (Outcome::MissingSource, En) => {
                    format!("error {p}: `{s}` does not exist; `{p}` was left unchanged")
                }
                (Outcome::MissingSource, Ja) => {
                    format!("error {p}: `{s}` がありません。`{p}` は変えていません")
                }
                (Outcome::Failed(e), En) => format!("error {p}: {e}; `{p}` was left unchanged"),
                (Outcome::Failed(e), Ja) => format!("error {p}: {e}。`{p}` は変えていません"),
                (Outcome::BaseNotUpdated(e), En) => format!(
                    "error {p}: merged the changes in `{s}`, but cannot write the base `{b}` ({e}); copy `{s}` to `{b}` before running `ebykn sync` again"
                ),
                (Outcome::BaseNotUpdated(e), Ja) => format!(
                    "error {p}: `{s}` の変更は取り込みましたが、基準 `{b}` を書けません（{e}）。`ebykn sync` をもう一度実行する前に、`{s}` を `{b}` に写してください"
                ),
            };
            out.push(line);
        }
        out
    }

    /// `--json` の 1 行。
    pub fn json(&self) -> String {
        let s = |x: &str| {
            let mut o = String::from("\"");
            for c in x.chars() {
                match c {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    '\n' => o.push_str("\\n"),
                    c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                    c => o.push(c),
                }
            }
            o.push('"');
            o
        };
        let files: Vec<String> = self
            .files
            .iter()
            .map(|f| {
                let extra = match &f.outcome {
                    Outcome::Conflict(n) => format!(",\"conflicts\":{n}"),
                    Outcome::Failed(e) | Outcome::BaseNotUpdated(e) => {
                        format!(",\"reason\":{}", s(e))
                    }
                    _ => String::new(),
                };
                format!(
                    "{{\"proof\":{},\"source\":{},\"base\":{},\"state\":\"{}\"{extra}}}",
                    s(&f.proof),
                    s(&f.source),
                    s(&f.base),
                    f.outcome.key()
                )
            })
            .collect();
        let errors: Vec<String> = self.errors.iter().map(|e| s(e)).collect();
        format!(
            "{{\"$message_type\":\"ebykn-sync\",\"success\":{},\"files\":[{}],\"errors\":[{}]}}",
            self.success(),
            files.join(","),
            errors.join(",")
        )
    }
}

/// 基準を置くディレクトリ（仕様 §3.1）。
pub const BASE_DIR: &str = ".ebykn/sync";

/// プロジェクト `root` の `proof/` の `.eykp` をすべて追従させる。
pub fn sync(root: &Path) -> SyncReport {
    let mut report = SyncReport::default();
    let proof_dir = root.join("proof");
    let mut proofs = Vec::new();
    if proof_dir.exists()
        && let Err(e) = collect(&proof_dir, &mut proofs)
    {
        report.errors.push(format!("cannot read `proof/`: {e}"));
        return report;
    }
    let mut rels: Vec<String> = proofs
        .iter()
        .filter_map(|p| p.strip_prefix(&proof_dir).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .collect();
    rels.sort();
    for rel in rels {
        // `proof/<p>.eykp` ↔ `src/<p>.eyk` ↔ `.ebykn/sync/<p>.eyk`（仕様 §3.1、§9）。
        let stem = rel.strip_suffix(".eykp").expect(".eykp だけを集めた");
        let f = FileSync {
            proof: format!("proof/{rel}"),
            source: format!("src/{stem}.eyk"),
            base: format!("{BASE_DIR}/{stem}.eyk"),
            outcome: Outcome::Unchanged,
        };
        let outcome = sync_one(root, &f);
        report.files.push(FileSync { outcome, ..f });
    }
    report
}

/// `dir` の下の `.eykp` を集める。隠しディレクトリは見ない。
fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if e.file_type()?.is_dir() {
            if !name.starts_with('.') {
                collect(&path, out)?;
            }
        } else if name.ends_with(".eykp") {
            out.push(path);
        }
    }
    Ok(())
}

fn sync_one(root: &Path, f: &FileSync) -> Outcome {
    let proof = root.join(&f.proof);
    let source = root.join(&f.source);
    let base = root.join(&f.base);
    let src = match std::fs::read(&source) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Outcome::MissingSource,
        Err(e) => return Outcome::Failed(format!("cannot read `{}`: {e}", f.source)),
    };
    let old = match std::fs::read(&base) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return match write_base(&base, &src) {
                Ok(()) => Outcome::BaseCreated,
                Err(e) => Outcome::Failed(format!("cannot write `{}`: {e}", f.base)),
            };
        }
        Err(e) => return Outcome::Failed(format!("cannot read `{}`: {e}", f.base)),
    };
    if old == src {
        return Outcome::Unchanged;
    }
    let out = Command::new("git")
        .arg("merge-file")
        .arg("-p")
        .args(["-L", &f.proof, "-L", &f.base, "-L", &f.source])
        .arg(&proof)
        .arg(&base)
        .arg(&source)
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Outcome::Failed(format!("cannot run `git merge-file`: {e}")),
    };
    // 終了コードは衝突の数（0 なら衝突なし）。負の値（シグナルや 255 など）は失敗。
    let conflicts = match out.status.code() {
        Some(n) if (0..=127).contains(&n) => n as u32,
        _ => {
            return Outcome::Failed(format!(
                "`git merge-file` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    };
    if let Err(e) = std::fs::write(&proof, &out.stdout) {
        return Outcome::Failed(format!("cannot write `{}`: {e}", f.proof));
    }
    // 衝突しても基準を進める（印に `src/` の変更が入っている）。
    if let Err(e) = write_base(&base, &src) {
        return Outcome::BaseNotUpdated(e.to_string());
    }
    if conflicts == 0 {
        Outcome::Merged
    } else {
        Outcome::Conflict(conflicts)
    }
}

fn write_base(base: &Path, src: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = base.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(base, src)
}
