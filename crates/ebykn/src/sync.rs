//! `ebykn sync`（仕様 §9、§12）：`src/` の変更を `proof/` の写しに 3-way merge で取り込む。
//!
//! 各 `proof/<p>.eykp` について、前回の `src/<p>.eyk` の写し（基準、`.ebykn/sync/<p>.eyk`）から今の
//! `src/<p>.eyk` への変更を、`git merge-file` で `.eykp` に当てる。そのあと基準を今の `src/<p>.eyk` で
//! 置き換える。衝突は印（`<<<<<<<` など）をファイルに残し、終了コードを 1 にする。
//!
//! 決めたことは `docs/current-spec.md` §9「`ebykn sync` の決めごと」にある。ファイルは同じ
//! ディレクトリの一時ファイルに書いてから名前を変えて置き換え（[`write_atomic`]）、途中で失敗しても
//! `.eykp` と基準が途中までの中身で残らないようにする。

use std::path::{Path, PathBuf};
use std::process::Command;

use ebykn_syntax::diag::Lang;

/// 失敗の理由。文言は [`Reason::text`] で言語ごとに作る。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Read {
        path: String,
        error: String,
    },
    Write {
        path: String,
        error: String,
    },
    GitSpawn {
        error: String,
    },
    /// `git merge-file` が衝突の数でない終了コードで終わった（中身が二進のファイルなど）。
    Git {
        stderr: String,
    },
}

impl Reason {
    pub fn text(&self, lang: Lang) -> String {
        match (self, lang) {
            (Reason::Read { path, error }, Lang::En) => format!("cannot read `{path}`: {error}"),
            (Reason::Read { path, error }, Lang::Ja) => format!("`{path}` を読めません（{error}）"),
            (Reason::Write { path, error }, Lang::En) => {
                format!("cannot write `{path}`: {error}")
            }
            (Reason::Write { path, error }, Lang::Ja) => {
                format!("`{path}` を書けません（{error}）")
            }
            (Reason::GitSpawn { error }, Lang::En) => {
                format!("cannot run `git merge-file`: {error}")
            }
            (Reason::GitSpawn { error }, Lang::Ja) => {
                format!("`git merge-file` を実行できません（{error}）")
            }
            (Reason::Git { stderr }, Lang::En) => format!("`git merge-file` failed: {stderr}"),
            (Reason::Git { stderr }, Lang::Ja) => {
                format!("`git merge-file` が失敗しました（{stderr}）")
            }
        }
    }
}

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
    /// 基準がなく、対応する `.eykp` のない基準があるので、基準を作らなかった（名前を変えた
    /// モジュールの古い基準かもしれない）。何も変えていない。
    BaseHeld,
    /// 前の `sync` の衝突の印が `.eykp` に残っている。何も変えていない。
    UnresolvedMarkers,
    /// 対応する `src/<p>.eyk` がない（E0301 に当たる）。何も変えていない。
    MissingSource,
    /// 読み書きか `git merge-file` が失敗した。`.eykp` も基準も変えていない。
    Failed(Reason),
    /// `.eykp` には取り込んだが（`conflicts` はそのときの衝突の数）、基準を書けなかった。
    BaseNotUpdated { conflicts: u32, reason: Reason },
}

impl Outcome {
    /// `sync` を失敗（終了コード 1）にするか。
    pub fn is_failure(&self) -> bool {
        !matches!(
            self,
            Outcome::Unchanged | Outcome::Merged | Outcome::BaseCreated
        )
    }

    fn key(&self) -> &'static str {
        match self {
            Outcome::Unchanged => "unchanged",
            Outcome::Merged => "merged",
            Outcome::Conflict(_) => "conflict",
            Outcome::BaseCreated => "base-created",
            Outcome::BaseHeld => "base-held",
            Outcome::UnresolvedMarkers => "unresolved-markers",
            Outcome::MissingSource => "missing-source",
            Outcome::Failed(_) => "failed",
            Outcome::BaseNotUpdated { .. } => "base-not-updated",
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
    /// 書く前に `.eykp` と基準の両方を git が追跡していて、作業ツリーに変更がなかったか。そのときだけ、
    /// `git checkout` でこの `sync` を取りやめられる（インデックスの版に戻るので、コミットしていない
    /// 変更があれば一緒に消える）。
    pub undoable: bool,
}

/// `sync` の結果。`files` は `.eykp` のパスの順。
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    /// プロジェクトのルート（取りやめのコマンドの `git -C` に使う）。
    pub root: String,
    pub files: Vec<FileSync>,
    /// 対応する `.eykp` のない基準（`.ebykn/sync/<p>.eyk`）。名前を変えたか消したモジュールの基準で、
    /// 残すと、あとで同じ名前のモジュールが古い基準から変更を当てる。
    pub orphan_bases: Vec<String>,
    /// `proof/` を読めなかったなど、ファイルごとでない誤り。
    pub errors: Vec<Reason>,
}

impl SyncReport {
    pub fn success(&self) -> bool {
        self.errors.is_empty()
            && self.orphan_bases.is_empty()
            && !self.files.iter().any(|f| f.outcome.is_failure())
    }

    /// 1 ファイル 1 行の説明。
    pub fn lines(&self, lang: Lang) -> Vec<String> {
        use Lang::{En, Ja};
        let mut out: Vec<String> = self
            .errors
            .iter()
            .map(|e| format!("error: {}", e.text(lang)))
            .collect();
        for b in &self.orphan_bases {
            out.push(match lang {
                En => format!(
                    "error {b}: no `.eykp` uses this base; if the module was renamed, move it to the new name under `.ebykn/sync/`, otherwise delete it"
                ),
                Ja => format!(
                    "error {b}: この基準を使う `.eykp` がありません。モジュールの名前を変えたなら `.ebykn/sync/` の下の新しい名前に移し、消したなら削除してください"
                ),
            });
        }
        for f in &self.files {
            let (p, s, b) = (&f.proof, &f.source, &f.base);
            let undo = if f.undoable {
                match lang {
                    En => format!(
                        " (to undo this sync, run `git -C {} checkout -- {p} {b}`)",
                        self.root
                    ),
                    Ja => format!(
                        "（この sync を取りやめるには `git -C {} checkout -- {p} {b}`）",
                        self.root
                    ),
                }
            } else {
                match lang {
                    En => format!(
                        " (`git checkout` cannot undo this sync: `{p}` or `{b}` was not committed as it was before the sync, so restoring them would lose uncommitted changes)"
                    ),
                    Ja => format!(
                        "（この sync は `git checkout` では取りやめられません。`{p}` か `{b}` が sync の前の状態のままコミットされていないので、戻すとコミットしていない変更が消えます）"
                    ),
                }
            };
            let line = match (&f.outcome, lang) {
                (Outcome::Unchanged, En) => format!("unchanged {p}"),
                (Outcome::Unchanged, Ja) => {
                    format!("unchanged {p}（`{s}` は前回から変わっていません）")
                }
                (Outcome::Merged, En) => format!("merged {p} (changes in `{s}`)"),
                (Outcome::Merged, Ja) => format!("merged {p}（`{s}` の変更を取り込みました）"),
                (Outcome::Conflict(n), En) => format!(
                    "conflict {p}: {n} conflict(s) left in the file; resolve them, then run `ebykn check`{undo}"
                ),
                (Outcome::Conflict(n), Ja) => format!(
                    "conflict {p}: 衝突 {n} か所をファイルに残しました。直してから `ebykn check` を実行してください{undo}"
                ),
                (Outcome::BaseCreated, En) => format!(
                    "base {p}: no base `{b}` yet; recorded the current `{s}` as the base without changing `{p}`"
                ),
                (Outcome::BaseCreated, Ja) => format!(
                    "base {p}: 基準 `{b}` がなかったので、今の `{s}` を基準として書きました（`{p}` は変えていません）"
                ),
                (Outcome::BaseHeld, En) => format!(
                    "error {p}: no base `{b}`, and some bases have no `.eykp` (listed above); if this module was renamed, move its old base to `{b}`, then run `ebykn sync` again"
                ),
                (Outcome::BaseHeld, Ja) => format!(
                    "error {p}: 基準 `{b}` がなく、`.eykp` のない基準があります（上に挙げたもの）。このモジュールの名前を変えたなら、古い基準を `{b}` に移してから `ebykn sync` をもう一度実行してください"
                ),
                (Outcome::UnresolvedMarkers, En) => format!(
                    "error {p}: conflict markers from a previous `ebykn sync` remain; resolve them first"
                ),
                (Outcome::UnresolvedMarkers, Ja) => format!(
                    "error {p}: 前の `ebykn sync` の衝突の印が残っています。先に直してください"
                ),
                (Outcome::MissingSource, En) => {
                    format!("error {p}: `{s}` does not exist; `{p}` was left unchanged")
                }
                (Outcome::MissingSource, Ja) => {
                    format!("error {p}: `{s}` がありません。`{p}` は変えていません")
                }
                (Outcome::Failed(r), En) => {
                    format!(
                        "error {p}: {}; `{p}` and `{b}` were left unchanged",
                        r.text(En)
                    )
                }
                (Outcome::Failed(r), Ja) => {
                    format!("error {p}: {}。`{p}` と `{b}` は変えていません", r.text(Ja))
                }
                (Outcome::BaseNotUpdated { conflicts, reason }, En) => {
                    let c = if *conflicts == 0 {
                        String::new()
                    } else {
                        format!(" with {conflicts} conflict(s) left in the file")
                    };
                    format!(
                        "error {p}: merged the changes in `{s}`{c}, but {}; copy `{s}` to `{b}` before running `ebykn sync` again",
                        reason.text(En)
                    )
                }
                (Outcome::BaseNotUpdated { conflicts, reason }, Ja) => {
                    let c = if *conflicts == 0 {
                        String::new()
                    } else {
                        format!("（衝突 {conflicts} か所をファイルに残しました）")
                    };
                    format!(
                        "error {p}: `{s}` の変更は取り込みましたが{c}、{}。`ebykn sync` をもう一度実行する前に、`{s}` を `{b}` に写してください",
                        reason.text(Ja)
                    )
                }
            };
            out.push(line);
        }
        out
    }

    /// `--json` の 1 行。理由は英語の文言。
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
                    Outcome::Failed(r) => format!(",\"reason\":{}", s(&r.text(Lang::En))),
                    Outcome::BaseNotUpdated { conflicts, reason } => format!(
                        ",\"conflicts\":{conflicts},\"reason\":{}",
                        s(&reason.text(Lang::En))
                    ),
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
        let orphans: Vec<String> = self.orphan_bases.iter().map(|b| s(b)).collect();
        let errors: Vec<String> = self.errors.iter().map(|e| s(&e.text(Lang::En))).collect();
        format!(
            "{{\"$message_type\":\"ebykn-sync\",\"success\":{},\"files\":[{}],\"orphan_bases\":[{}],\"errors\":[{}]}}",
            self.success(),
            files.join(","),
            orphans.join(","),
            errors.join(",")
        )
    }
}

/// 基準を置くディレクトリ（仕様 §3.1）。
pub const BASE_DIR: &str = ".ebykn/sync";

/// プロジェクト `root` の `proof/` の `.eykp` をすべて追従させる。
pub fn sync(root: &Path) -> SyncReport {
    let mut report = SyncReport {
        root: root.display().to_string(),
        ..SyncReport::default()
    };
    let Some(proofs) = rel_files(root, "proof", ".eykp", &mut report) else {
        return report;
    };
    let Some(bases) = rel_files(root, BASE_DIR, ".eyk", &mut report) else {
        return report;
    };
    let stems: Vec<&str> = proofs
        .iter()
        .map(|r| r.strip_suffix(".eykp").expect(".eykp だけを集めた"))
        .collect();
    report.orphan_bases = bases
        .iter()
        .filter(|b| !stems.contains(&b.strip_suffix(".eyk").expect(".eyk だけを集めた")))
        .map(|b| format!("{BASE_DIR}/{b}"))
        .collect();
    for stem in stems {
        // `proof/<p>.eykp` ↔ `src/<p>.eyk` ↔ `.ebykn/sync/<p>.eyk`（仕様 §3.1、§9）。
        let f = FileSync {
            proof: format!("proof/{stem}.eykp"),
            source: format!("src/{stem}.eyk"),
            base: format!("{BASE_DIR}/{stem}.eyk"),
            outcome: Outcome::Unchanged,
            undoable: false,
        };
        let (outcome, undoable) = sync_one(root, &f, !report.orphan_bases.is_empty());
        report.files.push(FileSync {
            outcome,
            undoable,
            ..f
        });
    }
    report
}

/// `root/dir` の下の、拡張子 `ext` のファイルの `dir` からの相対パス（並べたもの）。
fn rel_files(root: &Path, dir: &str, ext: &str, report: &mut SyncReport) -> Option<Vec<String>> {
    let base = root.join(dir);
    let mut found = Vec::new();
    if base.exists()
        && let Err(e) = collect(&base, ext, &mut found)
    {
        report.errors.push(Reason::Read {
            path: format!("{dir}/"),
            error: e.to_string(),
        });
        return None;
    }
    let mut rels: Vec<String> = found
        .iter()
        .filter_map(|p| p.strip_prefix(&base).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .collect();
    rels.sort();
    Some(rels)
}

/// `dir` の下の、拡張子 `ext` のファイルを集める。隠しディレクトリと一時ファイルは見ない。
fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if e.file_type()?.is_dir() {
            collect(&path, ext, out)?;
        } else if name.ends_with(ext) {
            out.push(path);
        }
    }
    Ok(())
}

/// `.eykp` に、前の `sync` が残した衝突の印の行があるか。
fn has_markers(proof: &[u8], f: &FileSync) -> bool {
    let open = format!("<<<<<<< {}", f.proof);
    let close = format!(">>>>>>> {}", f.source);
    proof
        .split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .any(|l| l == open.as_bytes() || l == close.as_bytes())
}

fn sync_one(root: &Path, f: &FileSync, orphans: bool) -> (Outcome, bool) {
    let mut undoable = false;
    let outcome = sync_inner(root, f, orphans, &mut undoable);
    (outcome, undoable)
}

fn sync_inner(root: &Path, f: &FileSync, orphans: bool, undoable: &mut bool) -> Outcome {
    let read = |rel: &str| {
        std::fs::read(root.join(rel)).map_err(|e| {
            let missing = e.kind() == std::io::ErrorKind::NotFound;
            (
                missing,
                Reason::Read {
                    path: rel.to_string(),
                    error: e.to_string(),
                },
            )
        })
    };
    let src = match read(&f.source) {
        Ok(s) => s,
        Err((true, _)) => return Outcome::MissingSource,
        Err((false, r)) => return Outcome::Failed(r),
    };
    let proof = match read(&f.proof) {
        Ok(p) => p,
        Err((_, r)) => return Outcome::Failed(r),
    };
    if has_markers(&proof, f) {
        return Outcome::UnresolvedMarkers;
    }
    let base = root.join(&f.base);
    let old = match read(&f.base) {
        Ok(b) => b,
        Err((true, _)) if orphans => return Outcome::BaseHeld,
        Err((true, _)) => {
            return match write_atomic(&base, &src) {
                Ok(()) => Outcome::BaseCreated,
                Err(e) => Outcome::Failed(Reason::Write {
                    path: f.base.clone(),
                    error: e.to_string(),
                }),
            };
        }
        Err((false, r)) => return Outcome::Failed(r),
    };
    if old == src {
        return Outcome::Unchanged;
    }
    let out = Command::new("git")
        .arg("merge-file")
        .arg("-p")
        .args(["-L", &f.proof, "-L", &f.base, "-L", &f.source])
        .arg(root.join(&f.proof))
        .arg(&base)
        .arg(root.join(&f.source))
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            return Outcome::Failed(Reason::GitSpawn {
                error: e.to_string(),
            });
        }
    };
    let Some(conflicts) = merge_conflicts(out.status.code()) else {
        return Outcome::Failed(Reason::Git {
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    };
    // 衝突を数えたのに印がない出力は信じない（`.eykp` を印のない中身で置き換えない）。
    if conflicts > 0 && !has_markers(&out.stdout, f) {
        return Outcome::Failed(Reason::Git {
            stderr: format!("reported {conflicts} conflict(s) but wrote no conflict markers"),
        });
    }
    *undoable = committed_as_is(root, &[&f.proof, &f.base]);
    if let Err(e) = write_atomic(&root.join(&f.proof), &out.stdout) {
        return Outcome::Failed(Reason::Write {
            path: f.proof.clone(),
            error: e.to_string(),
        });
    }
    // 衝突しても基準を進める。進めないと、手で解いた hunk が次の `sync` のたびにまた衝突する。
    if let Err(e) = write_atomic(&base, &src) {
        return Outcome::BaseNotUpdated {
            conflicts,
            reason: Reason::Write {
                path: f.base.clone(),
                error: e.to_string(),
            },
        };
    }
    if conflicts == 0 {
        Outcome::Merged
    } else {
        Outcome::Conflict(conflicts)
    }
}

/// `paths`（ルートからの相対）をすべて git が追跡していて、作業ツリーに変更がないか。
fn committed_as_is(root: &Path, paths: &[&str]) -> bool {
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .arg("--")
            .args(paths)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    git(&["ls-files", "--error-unmatch"]) && git(&["diff", "--quiet"])
}

/// `git merge-file` の終了コードを衝突の数として読む。0〜127 が衝突の数（git は 127 で打ち切る）で、
/// それ以外（二進のファイルの 255、シグナルで止まったときの `None` など）は失敗。
pub fn merge_conflicts(code: Option<i32>) -> Option<u32> {
    match code {
        Some(n) if (0..=127).contains(&n) => Some(n as u32),
        _ => None,
    }
}

/// `path` を `bytes` で置き換える。同じディレクトリの一時ファイルに書いてから名前を変えるので、
/// 途中で失敗しても `path` は前の中身のまま残る。
///
/// - `path` が symlink なら、指す先を置き換える（symlink はそのまま残る）。
/// - すでにあるファイルの mode を引き継ぐ。読み取り専用のファイルは置き換えずに失敗にする
///   （`std::fs::write` と同じく、読み取り専用のファイルを黙って上書きしない）。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let resolved;
    let path = match std::fs::canonicalize(path) {
        Ok(p) => {
            resolved = p;
            resolved.as_path()
        }
        Err(_) => path,
    };
    let perms = match std::fs::metadata(path) {
        Ok(m) if m.permissions().readonly() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the file is read-only",
            ));
        }
        Ok(m) => Some(m.permissions()),
        Err(_) => None,
    };
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.ebykn-sync-tmp"));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Some(p) = perms {
            std::fs::set_permissions(&tmp, p)?;
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
