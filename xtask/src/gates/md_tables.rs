//! `check-md-tables`：Markdown の表が途中で切れていない。
//!
//! 表の途中に空行があると、表はそこで終わり、後ろの `|` の行はただの段落として表示される。
//! 列の数が見出しと違う行は、足りないセルが空で埋められ、余ったセルは捨てられる。
//! 見出しと区切りの行の列の数が違えば、表にならない。どれも黙って表示が変わる。
//!
//! 表として見るのは `|` で始まる行（行頭の空白を除く）だけで、コードフェンスの中は見ない。
//! セルは `\|` を除く `|` で区切る。GFM では、インラインコードの中の `|` もセルを区切る。

use super::{Repo, Report, fence_close, fence_open};

/// 対象外のディレクトリ。外部から取り込んだコードで、上流のまま置く（AGENTS.md「外部から取り込むコード」）。
pub const EXEMPT_DIRS: &[&str] = &["vendor/verus_syn"];

fn in_exempt_dir(f: &str) -> bool {
    EXEMPT_DIRS
        .iter()
        .any(|d| f.strip_prefix(d).is_some_and(|rest| rest.starts_with('/')))
}

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    for d in EXEMPT_DIRS {
        if !repo.files.iter().any(|f| f.starts_with(&format!("{d}/"))) {
            r.errors.push(format!(
                "対象外リストのディレクトリ {d}/ がありません。リストから消してください"
            ));
        }
    }
    let skipped = repo
        .files
        .iter()
        .filter(|f| f.ends_with(".md") && in_exempt_dir(f))
        .count();
    r.notes.push(format!(
        "対象外のディレクトリ（明示）: {}（{skipped} ファイル）",
        EXEMPT_DIRS
            .iter()
            .map(|d| format!("{d}/"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let (mut files, mut tables) = (0, 0);
    for md in repo
        .files
        .iter()
        .filter(|f| f.ends_with(".md") && !in_exempt_dir(f))
    {
        let text = match repo.read(md) {
            Ok(t) => t,
            Err(e) => {
                r.errors.push(e);
                continue;
            }
        };
        let found = match check(&text) {
            Ok(f) => f,
            Err(e) => {
                r.errors.push(format!("{md}: {e}"));
                continue;
            }
        };
        files += 1;
        tables += found.tables;
        for (line, why) in found.problems {
            r.errors.push(format!("{md}:{line}: {why}"));
        }
    }
    r.notes
        .push(format!("{files} ファイルの {tables} 個の表を検査"));
    r
}

struct Found {
    tables: usize,
    /// 行番号（1 から）と理由。
    problems: Vec<(usize, String)>,
}

fn check(text: &str) -> Result<Found, String> {
    // コードフェンスの中（開きと閉じの行を含む）は `None`。
    let mut lines: Vec<Option<&str>> = Vec::new();
    let mut fence: Option<(char, usize, usize)> = None;
    for (i, line) in text.lines().enumerate() {
        if let Some((c, n, _)) = fence {
            if fence_close(line, c, n) {
                fence = None;
            }
            lines.push(None);
        } else if let Some((c, n, _)) = fence_open(line) {
            fence = Some((c, n, i + 1));
            lines.push(None);
        } else {
            lines.push(Some(line));
        }
    }
    if let Some((_, _, no)) = fence {
        return Err(format!("{no} 行目のコードフェンスが閉じていません"));
    }

    let row = |i: usize| lines.get(i).copied().flatten().filter(|l| is_row(l));
    let mut found = Found {
        tables: 0,
        problems: Vec::new(),
    };
    // 直前の表の最後の行（間に空行しかないあいだだけ持つ）。
    let mut prev_table_end: Option<usize> = None;
    let mut i = 0;
    while i < lines.len() {
        let Some(line) = lines[i] else {
            prev_table_end = None;
            i += 1;
            continue;
        };
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if !is_row(line) {
            prev_table_end = None;
            i += 1;
            continue;
        }
        // `|` で始まる行のかたまり [i, end)。
        let mut end = i;
        while row(end).is_some() {
            end += 1;
        }
        let is_table = end - i >= 2 && is_delimiter(lines[i + 1].unwrap_or_default());
        if !is_table {
            let why = match prev_table_end {
                Some(last) => format!(
                    "{} 行目の空行で表が切れ、この行から後ろは表として表示されません。空行を消してください",
                    last + 2
                ),
                None => "`|` で始まる行が表になっていません（見出しの行の次に区切りの行 `| --- |` がない）"
                    .to_string(),
            };
            found.problems.push((i + 1, why));
            prev_table_end = None;
            i = end;
            continue;
        }
        found.tables += 1;
        let head = cells(line).len();
        let delim = cells(lines[i + 1].unwrap_or_default()).len();
        if head != delim {
            found.problems.push((
                i + 1,
                format!(
                    "見出しの列の数（{head}）と区切りの行の列の数（{delim}）が違うので、表になりません"
                ),
            ));
        } else {
            for (j, l) in lines.iter().enumerate().take(end).skip(i + 2) {
                let n = cells(l.unwrap_or_default()).len();
                if n != head {
                    found.problems.push((
                        j + 1,
                        format!(
                            "列の数（{n}）が見出し（{} 行目）の {head} と違います。セルの中の `|` は `\\|` と書きます",
                            i + 1
                        ),
                    ));
                }
            }
        }
        prev_table_end = Some(end - 1);
        i = end;
    }
    Ok(found)
}

fn is_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// セル。前後の `|` を除き、`\|` では区切らない。
fn cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let mut out = Vec::new();
    let (mut start, mut escaped) = (0, false);
    for (i, c) in t.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '|' => {
                out.push(&t[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !t[start..].trim().is_empty() {
        out.push(&t[start..]);
    }
    out
}

/// 区切りの行（`| --- | :--: |`）か。
fn is_delimiter(line: &str) -> bool {
    let cs = cells(line);
    !cs.is_empty()
        && cs.iter().all(|c| {
            let c = c.trim();
            let c = c.strip_prefix(':').unwrap_or(c);
            let c = c.strip_suffix(':').unwrap_or(c);
            !c.is_empty() && c.chars().all(|x| x == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const FILE: &str = "OPEN.md";

    fn repo(open: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write(FILE, open);
        t.write("vendor/verus_syn/README.md", "| a |\n\n| b |\n");
        t
    }

    /// 実ファイルの最初の表で、本文の行が 2 行以上続くところの、最初の 2 行。
    fn two_body_rows(src: &str) -> (String, String) {
        let lines: Vec<&str> = src.lines().collect();
        let k = (1..lines.len() - 2)
            .find(|&k| is_delimiter(lines[k]) && is_row(lines[k + 1]) && is_row(lines[k + 2]))
            .expect("本文の行が 2 行以上ある表がない");
        (lines[k + 1].to_string(), lines[k + 2].to_string())
    }

    #[test]
    fn passes_on_the_real_files() {
        for f in [FILE, "AGENTS.md", ".claude/skills/ebykn-release/SKILL.md"] {
            assert_passes(&repo(&real(f)).run(run));
        }
    }

    #[test]
    fn fails_when_a_blank_line_splits_a_table() {
        let src = real(FILE);
        let (a, b) = two_body_rows(&src);
        let broken = mutate(&src, &format!("{a}\n{b}\n"), &format!("{a}\n\n{b}\n"));
        assert_fails(&repo(&broken).run(run), "空行で表が切れ");
    }

    #[test]
    fn fails_when_a_row_has_another_number_of_columns() {
        let src = real(FILE);
        let (a, _) = two_body_rows(&src);
        let broken = mutate(&src, &format!("{a}\n"), &format!("{a} 余り |\n"));
        assert_fails(&repo(&broken).run(run), "が見出し");
    }

    #[test]
    fn fails_when_a_pipe_in_code_splits_a_cell() {
        let src = real(FILE);
        let broken = mutate(&src, "`forall\\|x\\|`", "`forall|x|`");
        assert_fails(&repo(&broken).run(run), "が見出し");
    }

    #[test]
    fn fails_when_the_delimiter_row_has_another_number_of_columns() {
        let src = real(FILE);
        let lines: Vec<&str> = src.lines().collect();
        let delim = lines
            .iter()
            .find(|l| is_delimiter(l))
            .expect("区切りの行がない");
        let broken = mutate(&src, &format!("{delim}\n"), &format!("{delim} --- |\n"));
        assert_fails(&repo(&broken).run(run), "表になりません");
    }

    #[test]
    fn fails_on_a_stray_row() {
        let broken = format!("{}\n段落\n\n| a | b |\n", real(FILE));
        assert_fails(&repo(&broken).run(run), "表になっていません");
    }

    #[test]
    fn fails_on_an_unclosed_fence() {
        let broken = format!("{}\n```text\n| a |\n", real(FILE));
        assert_fails(&repo(&broken).run(run), "閉じていません");
    }

    #[test]
    fn fails_when_an_exempt_directory_is_missing() {
        let mut t = TempRepo::new();
        t.write(FILE, &real(FILE));
        assert_fails(&t.run(run), "ディレクトリ vendor/verus_syn/ がありません");
    }

    #[test]
    fn ignores_fences_escapes_and_separate_tables() {
        let text = "\
| a | b |
| :-- | --: |
| `x \\| y` | 2 |

| c |
| --- |
| 3 |

```text
| 表 | ではない |

| ここも |
```
";
        let r = repo(text).run(run);
        assert_passes(&r);
        assert!(
            r.notes.iter().any(|n| n.contains("1 ファイルの 2 個の表")),
            "{:?}",
            r.notes
        );
    }
}
