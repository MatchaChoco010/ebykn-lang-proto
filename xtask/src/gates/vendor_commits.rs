//! `check-vendor-commits`：`vendor/` を変えるコミットは、`vendor/` の外を変えない。
//!
//! AGENTS.md「外部から取り込むコード」：取り込みのコミットとフォークの手入れのコミットには `vendor/` の変更だけを入れ、
//! それを含む PR はマージコミットでマージする。上流を取り込み直すときに、手入れを `git log -- vendor/` から
//! 取り出して当て直すため。
//!
//! `HEAD` から辿れる、マージでないコミットのうち `vendor/` の下を変えたものをすべて集め、`vendor/` の外も変えていたら失敗にする。
//! - 履歴は簡略化せずに辿る（`rev-list --full-history`）。マージコミットの `vendor/` が片方の親と同じでも、
//!   もう片方の親の側のコミットを見る。
//! - PR の CI では `HEAD` は PR を `develop` にマージした形で、PR のコミットを含む。PR の中で混ぜたコミットは、
//!   マージの前に見つかる。pre-commit フックは作っているコミットの直前までの履歴を見るので、手元では次のコミットで見つかる。
//! - 手入れのコミットを分けた PR を squash でマージすると、`develop` に混ざったコミットができ、`develop` への push の CI で
//!   見つかる。`develop` の履歴は書き換えないので、そのコミットを理由を添えて `EXEMPT` に足す。
//! - git は plumbing（`rev-list`、`diff-tree -z`、`cat-file`）だけを使い、利用者の `log.*` などの設定で結果を変えない。
//! - 履歴が浅い（shallow）と、コミットを辿れないので失敗にする。CI のゲートのジョブは `fetch-depth: 0` で取り出す。
//! - `vendor/` の下のファイルを追跡しているのに、それを変えたコミットが見つからなければ失敗にする。

use std::path::Path;
use std::process::Command;

use super::{Repo, Report};

/// 対象外のコミット（完全な SHA と理由）。`vendor/` の外も変えたコミットが `develop` に入ってしまったときだけ足す。
/// 載せたコミットが `vendor/` を変えていないか、`vendor/` の外を変えていなければ失敗にする。
pub const EXEMPT: &[(&str, &str)] = &[];

const VENDOR: &str = "vendor/";

/// `vendor/` を変えた、マージでないコミット。履歴を簡略化しない。
const REV_LIST: &[&str] = &[
    "rev-list",
    "--no-merges",
    "--full-history",
    "HEAD",
    "--",
    VENDOR,
];

/// 1 つのコミットが変えたファイル（NUL 区切り）。ルートのコミットも親のない差分として出す。
const DIFF_TREE: &[&str] = &[
    "diff-tree",
    "-r",
    "-z",
    "--name-only",
    "--no-commit-id",
    "--no-renames",
    "--root",
];

pub fn run(repo: &Repo) -> Report {
    check(&repo.root, &repo.files, EXEMPT)
}

struct Commit {
    sha: String,
    subject: String,
    files: Vec<String>,
}

fn check(root: &Path, files: &[String], exempt: &[(&str, &str)]) -> Report {
    let mut r = Report::default();
    match git(root, &["rev-parse", "--is-shallow-repository"]).as_deref() {
        Ok("false\n") => {}
        Ok("true\n") => {
            r.errors.push(
                "履歴が浅い（shallow）ので、vendor/ を変えたコミットを辿れません。全履歴を取り出してください（CI は actions/checkout の fetch-depth: 0）"
                    .to_string(),
            );
            return r;
        }
        Ok(other) => {
            r.errors.push(format!(
                "git rev-parse --is-shallow-repository の出力を読めません: {other:?}"
            ));
            return r;
        }
        Err(e) => {
            r.errors.push(e.clone());
            return r;
        }
    }
    let commits = match collect(root) {
        Ok(c) => c,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    if commits.is_empty() && files.iter().any(|f| f.starts_with(VENDOR)) {
        r.errors.push(
            "vendor/ の下のファイルを追跡しているのに、HEAD から vendor/ を変えたコミットが見つかりません"
                .to_string(),
        );
    }
    for (sha, _) in exempt {
        match commits.iter().find(|c| c.sha == *sha) {
            None => r.errors.push(format!(
                "対象外リストの {sha} は、HEAD から辿れる vendor/ を変えたコミットにありません。リストから消してください"
            )),
            Some(c) if outside(c).is_empty() => r.errors.push(format!(
                "対象外リストの {sha} は vendor/ の外を変えていません。リストから消してください"
            )),
            Some(_) => {}
        }
    }
    for c in &commits {
        let out = outside(c);
        if out.is_empty() || exempt.iter().any(|(sha, _)| *sha == c.sha) {
            continue;
        }
        r.errors.push(format!(
            "{} {}: vendor/ と、vendor/ の外（{}）を 1 つのコミットで変えています。vendor/ の変更だけのコミットに分けてください（PR の中なら rebase で分ける。squash でマージして develop に入ったものは EXEMPT に足す）",
            &c.sha[..12.min(c.sha.len())],
            c.subject,
            out.join(", ")
        ));
    }
    r.notes.push(format!(
        "vendor/ を変えたコミット {} 件（HEAD から、マージを除く。履歴を簡略化しない）",
        commits.len()
    ));
    let listed: Vec<String> = exempt
        .iter()
        .map(|(sha, why)| format!("{sha}（{why}）"))
        .collect();
    r.notes.push(format!(
        "対象外（明示）: {}",
        if listed.is_empty() {
            "なし".to_string()
        } else {
            listed.join(", ")
        }
    ));
    r
}

fn outside(c: &Commit) -> Vec<&str> {
    c.files
        .iter()
        .filter(|f| !f.starts_with(VENDOR))
        .map(String::as_str)
        .collect()
}

/// `vendor/` を変えたコミットを、変えたファイルと件名と一緒に集める。
fn collect(root: &Path) -> Result<Vec<Commit>, String> {
    let mut commits = Vec::new();
    for sha in parse_shas(&git(root, REV_LIST)?)? {
        let mut args = DIFF_TREE.to_vec();
        args.push(&sha);
        let files = parse_files(&sha, &git(root, &args)?)?;
        let subject = subject(&git(root, &["cat-file", "commit", &sha])?);
        commits.push(Commit {
            sha,
            subject,
            files,
        });
    }
    Ok(commits)
}

/// `rev-list` の出力（1 行に 1 つの SHA）を読む。
fn parse_shas(out: &str) -> Result<Vec<String>, String> {
    out.lines()
        .enumerate()
        .map(|(i, line)| {
            if (line.len() == 40 || line.len() == 64) && line.bytes().all(|b| b.is_ascii_hexdigit())
            {
                Ok(line.to_string())
            } else {
                Err(format!(
                    "git rev-list の {} 行目を SHA として読めません: {line:?}",
                    i + 1
                ))
            }
        })
        .collect()
}

/// `diff-tree -z --name-only` の出力（NUL で終わるパスの並び）を読む。
fn parse_files(sha: &str, out: &str) -> Result<Vec<String>, String> {
    let Some(body) = out.strip_suffix('\0') else {
        return Err(format!(
            "git diff-tree の {sha} の出力が NUL で終わっていません: {out:?}"
        ));
    };
    let files: Vec<String> = body.split('\0').map(str::to_string).collect();
    if files.iter().any(String::is_empty) {
        return Err(format!(
            "git diff-tree の {sha} の出力に空のパスがあります: {out:?}"
        ));
    }
    Ok(files)
}

/// `cat-file commit` の出力から、メッセージの 1 行目を取る。
fn subject(raw: &str) -> String {
    raw.split_once("\n\n")
        .and_then(|(_, msg)| msg.lines().next())
        .unwrap_or("")
        .to_string()
}

/// `root` で git を走らせ、標準出力を返す。フックが設定する変数で、別のリポジトリを見ないようにする。
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .map_err(|e| format!("git を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} が失敗しました: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout)
        .map_err(|_| format!("git {} の出力が UTF-8 ではありません", args.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempRepo, assert_fails, assert_passes, mutate};

    /// テスト用のリポジトリで git を走らせる。利用者とシステムの設定（署名、フック、merge.ff など）を読まない。
    fn sh(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .current_dir(root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn commit(t: &mut TempRepo, files: &[(&str, &str)], msg: &str) -> String {
        for (f, content) in files {
            t.write(f, content);
        }
        sh(t.root(), &["add", "-A"]);
        sh(t.root(), &["commit", "-q", "-m", msg]);
        sh(t.root(), &["rev-parse", "HEAD"]).trim().to_string()
    }

    /// 取り込み、ほかの変更、手入れのコミットを分けて積み、手入れの PR をマージコミットで入れたリポジトリ。
    fn repo() -> TempRepo {
        let mut t = TempRepo::new();
        sh(t.root(), &["init", "-q", "-b", "develop"]);
        commit(&mut t, &[("README.md", "a\n")], "最初");
        commit(&mut t, &[("vendor/syn/lib.rs", "up\n")], "取り込み");
        sh(t.root(), &["checkout", "-q", "-b", "feature"]);
        commit(
            &mut t,
            &[("vendor/syn/lib.rs", "up\n// EBYKN:\n")],
            "手入れ",
        );
        commit(&mut t, &[("crates/a/lib.rs", "use\n")], "使う");
        sh(t.root(), &["checkout", "-q", "develop"]);
        sh(
            t.root(),
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                "Merge pull request",
                "feature",
            ],
        );
        t
    }

    #[test]
    fn passes_when_vendor_commits_touch_only_vendor() {
        let r = repo().run(run);
        assert_passes(&r);
        assert!(r.notes.iter().any(|n| n.contains("2 件")), "{:?}", r.notes);
    }

    #[test]
    fn fails_when_a_commit_mixes_vendor_and_other_files() {
        let mut t = repo();
        assert_passes(&t.run(run));
        let sha = commit(
            &mut t,
            &[("vendor/syn/lib.rs", "x\n"), ("crates/a/lib.rs", "y\n")],
            "混ぜる",
        );
        assert_fails(&t.run(run), &sha[..12]);
    }

    #[test]
    fn fails_when_a_fork_pr_is_squashed() {
        let mut t = repo();
        sh(t.root(), &["checkout", "-q", "-b", "fork"]);
        commit(&mut t, &[("vendor/syn/lib.rs", "z\n")], "手入れ 2");
        commit(&mut t, &[("crates/a/lib.rs", "z\n")], "使う 2");
        sh(t.root(), &["checkout", "-q", "develop"]);
        assert_passes(&t.run(run));
        sh(t.root(), &["merge", "-q", "--squash", "fork"]);
        sh(t.root(), &["commit", "-q", "-m", "手入れ 2 (#9)"]);
        assert_fails(&t.run(run), "手入れ 2 (#9)");
    }

    #[test]
    fn passes_with_an_exempt_mixed_commit() {
        let mut t = repo();
        let sha = commit(
            &mut t,
            &[("vendor/syn/lib.rs", "x\n"), ("crates/a/lib.rs", "y\n")],
            "混ぜる",
        );
        let r = t.run_ref(|repo| {
            check(
                &repo.root,
                &repo.files,
                &[(sha.as_str(), "squash で入った")],
            )
        });
        assert_passes(&r);
        assert!(r.notes.iter().any(|n| n.contains("squash で入った")));
    }

    #[test]
    fn fails_when_an_exempt_commit_is_not_in_history() {
        let t = repo();
        let ghost = "0123456789abcdef0123456789abcdef01234567";
        let r = t.run_ref(|repo| check(&repo.root, &repo.files, &[(ghost, "ない")]));
        assert_fails(&r, ghost);
    }

    #[test]
    fn fails_when_an_exempt_commit_is_not_mixed() {
        let t = repo();
        let import = sh(t.root(), &["rev-parse", "HEAD~1"]);
        let import = import.trim();
        let r = t.run_ref(|repo| check(&repo.root, &repo.files, &[(import, "混ざっていない")]));
        assert_fails(&r, "vendor/ の外を変えていません");
    }

    #[test]
    fn fails_when_history_is_shallow() {
        let t = repo();
        let src = format!("file://{}", t.root().display());
        let dst = t.root().join("shallow");
        sh(
            t.root(),
            &["clone", "-q", "--depth", "1", &src, dst.to_str().unwrap()],
        );
        let files = vec!["vendor/syn/lib.rs".to_string()];
        assert_fails(&check(&dst, &files, &[]), "shallow");
    }

    #[test]
    fn fails_when_vendor_is_tracked_but_no_commit_changed_it() {
        let mut t = TempRepo::new();
        sh(t.root(), &["init", "-q", "-b", "develop"]);
        commit(&mut t, &[("README.md", "a\n")], "最初");
        let files = vec!["vendor/syn/lib.rs".to_string()];
        assert_fails(&check(t.root(), &files, &[]), "見つかりません");
    }

    #[test]
    fn fails_when_a_mixed_commit_is_hidden_by_history_simplification() {
        // PR a と PR b が同じ vendor/ の手入れを別のコミットで持ち、b の手入れは crates/ も変える。
        // a、b の順にマージコミットで入れると、b のマージの vendor/ は第 1 親と同じになる。
        let mut t = repo();
        let base = sh(t.root(), &["rev-parse", "HEAD"]);
        let base = base.trim();
        sh(t.root(), &["checkout", "-q", "-b", "a", base]);
        commit(&mut t, &[("vendor/syn/lib.rs", "same\n")], "手入れ a");
        sh(t.root(), &["checkout", "-q", "-b", "b", base]);
        let mixed = commit(
            &mut t,
            &[("vendor/syn/lib.rs", "same\n"), ("crates/b/lib.rs", "b\n")],
            "手入れ b",
        );
        sh(t.root(), &["checkout", "-q", "develop"]);
        sh(t.root(), &["merge", "-q", "--no-ff", "-m", "Merge a", "a"]);
        assert_passes(&t.run(run));
        sh(t.root(), &["merge", "-q", "--no-ff", "-m", "Merge b", "b"]);
        assert_fails(&t.run(run), &mixed[..12]);
    }

    #[test]
    fn passes_regardless_of_the_users_log_config() {
        // ルートのコミットが vendor/ を含み、パスに引用符と日本語がある。log.* などの設定は効かない。
        let mut t = TempRepo::new();
        sh(t.root(), &["init", "-q", "-b", "develop"]);
        for (k, v) in [
            ("log.follow", "true"),
            ("log.showRoot", "false"),
            ("core.quotePath", "true"),
            ("diff.renames", "copies"),
        ] {
            sh(t.root(), &["config", k, v]);
        }
        commit(
            &mut t,
            &[("vendor/odd/a\"b.rs", "x\n"), ("vendor/odd/日本.rs", "y\n")],
            "取り込み",
        );
        commit(&mut t, &[("README.md", "a\n")], "ほか");
        let r = t.run(run);
        assert_passes(&r);
        assert!(r.notes.iter().any(|n| n.contains("1 件")), "{:?}", r.notes);
    }

    #[test]
    fn fails_when_the_rev_list_is_unreadable() {
        let t = repo();
        let out = git(t.root(), REV_LIST).unwrap();
        assert_eq!(parse_shas(&out).unwrap().len(), 2);
        let broken = mutate(&out, "\n", " x\n");
        assert!(
            parse_shas(&broken).is_err_and(|e| e.contains("SHA として読めません")),
            "{broken:?}"
        );
    }

    #[test]
    fn fails_when_the_diff_tree_is_unreadable() {
        let t = repo();
        let sha = parse_shas(&git(t.root(), REV_LIST).unwrap()).unwrap()[0].clone();
        let mut args = DIFF_TREE.to_vec();
        args.push(&sha);
        let out = git(t.root(), &args).unwrap();
        assert_eq!(parse_files(&sha, &out).unwrap(), ["vendor/syn/lib.rs"]);
        let broken = mutate(&out, "\0", "");
        assert!(
            parse_files(&sha, &broken).is_err_and(|e| e.contains("NUL で終わっていません")),
            "{broken:?}"
        );
    }
}
