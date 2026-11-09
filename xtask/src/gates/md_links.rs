//! `check-md-links`：Markdown の相対リンク先がリポジトリにある。
//!
//! 存在の判定はディスクではなくファイルの一覧で行う。手元にだけあるファイルへのリンクを通さないため。

use super::{Repo, Report, fence_close, fence_open};

/// 空のリンク先 `[題名]()` を下書き章として許すファイル。中身は `check-book` が検査する。
const DRAFTS_ALLOWED: &str = "book/src/SUMMARY.md";

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    let paths = repo.paths();
    let mut count = 0;
    for md in repo.files.iter().filter(|f| f.ends_with(".md")) {
        let text = match repo.read(md) {
            Ok(t) => t,
            Err(e) => {
                r.errors.push(e);
                continue;
            }
        };
        let links = match links(&text) {
            Ok(l) => l,
            Err(e) => {
                r.errors.push(format!("{md}: {e}"));
                continue;
            }
        };
        let dir = md.rsplit_once('/').map_or("", |(d, _)| d);
        for (line, target) in links {
            count += 1;
            if target.is_empty() {
                if md != DRAFTS_ALLOWED {
                    r.errors.push(format!("{md}:{line}: リンク先が空です"));
                }
                continue;
            }
            if target.contains("://") || target.starts_with("mailto:") || target.starts_with('#') {
                continue;
            }
            let path = target.split(['#', '?']).next().unwrap_or_default();
            match resolve(dir, path) {
                Some(p) if p.is_empty() || paths.contains(&p) => {}
                Some(p) => r.errors.push(format!(
                    "{md}:{line}: リンク先 {target} （{p}）がありません"
                )),
                None => r.errors.push(format!(
                    "{md}:{line}: リンク先 {target} がリポジトリの外を指しています"
                )),
            }
        }
    }
    r.notes.push(format!(
        "{count} 個のリンクを検査（空のリンク先は {DRAFTS_ALLOWED} の下書き章だけ許す）"
    ));
    r
}

/// コードフェンスとインラインコードの外にあるリンク先を、行番号とともに返す。
fn links(text: &str) -> Result<Vec<(usize, String)>, String> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize, usize)> = None;
    for (i, line) in text.lines().enumerate() {
        let no = i + 1;
        if let Some((c, n, _)) = fence {
            if fence_close(line, c, n) {
                fence = None;
            }
            continue;
        }
        if let Some((c, n, _)) = fence_open(line) {
            fence = Some((c, n, no));
            continue;
        }
        // インラインコードを除く（バッククォートで区切った偶数番目だけを見る）。
        let prose: String = line.split('`').step_by(2).collect::<Vec<_>>().join(" ");
        if let Some(def) = prose.trim_start().strip_prefix('[')
            && let Some((_, rest)) = def.split_once("]:")
        {
            out.push((no, target_of(rest)));
            continue;
        }
        let mut rest = prose.as_str();
        while let Some(i) = rest.find("](") {
            rest = &rest[i + 2..];
            let end = rest
                .find(')')
                .ok_or(format!("{no} 行目のリンクが閉じていません"))?;
            out.push((no, target_of(&rest[..end])));
            rest = &rest[end..];
        }
    }
    if let Some((_, _, no)) = fence {
        return Err(format!("{no} 行目のコードフェンスが閉じていません"));
    }
    Ok(out)
}

/// `<a b>` か、空白の前まで（後ろの題名を捨てる）。
fn target_of(s: &str) -> String {
    let s = s.trim();
    match s.strip_prefix('<').and_then(|x| x.split_once('>')) {
        Some((t, _)) => t.to_string(),
        None => s.split_whitespace().next().unwrap_or_default().to_string(),
    }
}

/// `dir` からの相対パスを、リポジトリ直下からのパスにする。外に出れば `None`。
fn resolve(dir: &str, path: &str) -> Option<String> {
    let (base, path) = match path.strip_prefix('/') {
        Some(p) => ("", p),
        None => (dir, path),
    };
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    fn readme_repo(readme: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write("README.md", readme);
        for f in [
            "AGENTS.md",
            "docs/ebykn-thesis.md",
            "docs/ebykn-spec.md",
            "docs/current-spec.md",
            "docs/ebykn-design-notes.md",
            "book/src/ch00-00-introduction.md",
        ] {
            t.write(f, "# x\n");
        }
        t
    }

    #[test]
    fn passes_on_the_real_readme() {
        assert_passes(&readme_repo(&real("README.md")).run(run));
    }

    #[test]
    fn fails_when_a_link_target_is_missing() {
        let broken = mutate(
            &real("README.md"),
            "(docs/ebykn-spec.md)",
            "(docs/missing.md)",
        );
        assert_fails(&readme_repo(&broken).run(run), "docs/missing.md");
    }

    #[test]
    fn fails_when_a_link_leaves_the_repository() {
        let broken = mutate(&real("README.md"), "(AGENTS.md)", "(../AGENTS.md)");
        assert_fails(&readme_repo(&broken).run(run), "外を指して");
    }

    #[test]
    fn fails_on_an_empty_target_outside_summary() {
        let broken = format!("{}\n[下書き]()\n", real("README.md"));
        assert_fails(&readme_repo(&broken).run(run), "リンク先が空");
    }

    #[test]
    fn fails_on_an_unclosed_fence() {
        let broken = format!("{}\n```text\n[x](missing.md)\n", real("README.md"));
        assert_fails(&readme_repo(&broken).run(run), "閉じていません");
    }

    #[test]
    fn ignores_code_urls_and_anchors() {
        let text = "```text\n[a](missing.md)\n```\n`[b](missing.md)`\n\
                    [c](https://example.com/x) [d](#top) [e](AGENTS.md#issue \"題\")\n\
                    [f]: <docs/ebykn-spec.md>\n[g](docs/)\n";
        assert_passes(&readme_repo(text).run(run));
    }
}
