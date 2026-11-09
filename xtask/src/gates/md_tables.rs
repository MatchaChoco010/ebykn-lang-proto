//! `check-md-tables`：Markdown の表が途中で切れていない。
//!
//! 表の途中に空行があると、表はそこで終わり、後ろの `|` の行はただの段落として表示される。
//! 表の直後に空行がないと、続く地の文の行も表の行として表示される。
//! 列の数が見出しと違う行は、足りないセルが空で埋められ、余ったセルは捨てられる。
//! 見出しと区切りの行の列の数が違えば、表にならない。どれも黙って表示が変わる。
//!
//! 表は、見出しの行と区切りの行（`| --- |`）の並びで見つける。表の行は `|` で始める決まりにし、
//! 行頭に `|` のない表は失敗にする。行頭の `|` がないと、表の行と地の文を見分けられないため。
//! 引用の中（`>`）は `>` を剥がして同じように見る。コードフェンスの中は見ない。
//! セルは、直前が `\` でない `|` で区切る。GFM では、インラインコードの中の `|` もセルを区切る。

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

/// 行。引用の `>` を剥がした中身と引用の深さ。コードフェンスの中（開きと閉じの行を含む）は `Code`。
#[derive(Clone, Copy)]
enum Line<'a> {
    Code,
    Text { depth: usize, text: &'a str },
}

fn check(text: &str) -> Result<Found, String> {
    let mut lines: Vec<Line> = Vec::new();
    let mut fence: Option<(char, usize, usize)> = None;
    for (i, raw) in text.lines().enumerate() {
        let (depth, line) = unquote(raw);
        if let Some((c, n, _)) = fence {
            if fence_close(line, c, n) {
                fence = None;
            }
            lines.push(Line::Code);
        } else if let Some((c, n, _)) = fence_open(line.trim_start()) {
            // 字下げは問わない（リストの項目の中のフェンスは 4 文字以上字下げする）。
            fence = Some((c, n, i + 1));
            lines.push(Line::Code);
        } else {
            lines.push(Line::Text { depth, text: line });
        }
    }
    if let Some((_, _, no)) = fence {
        return Err(format!("{no} 行目のコードフェンスが閉じていません"));
    }

    let text_at = |i: usize| match lines.get(i) {
        Some(Line::Text { depth, text }) => Some((*depth, *text)),
        _ => None,
    };
    let blank = |t: &str| t.trim().is_empty();
    // 同じ深さの、空でない行のかたまりの終わり（`from` から数える）。
    let block_end = |from: usize, depth: usize, pred: &dyn Fn(&str) -> bool| {
        let mut end = from;
        while text_at(end).is_some_and(|(d, t)| d == depth && !blank(t) && pred(t)) {
            end += 1;
        }
        end
    };
    let mut found = Found {
        tables: 0,
        problems: Vec::new(),
    };
    // 直前の表の最後の行（間に空行しかないあいだだけ持つ）。
    let mut prev_table_end: Option<usize> = None;
    let mut i = 0;
    while i < lines.len() {
        let Some((depth, line)) = text_at(i) else {
            prev_table_end = None;
            i += 1;
            continue;
        };
        if blank(line) {
            i += 1;
            continue;
        }
        let delim = text_at(i + 1)
            .filter(|&(d, t)| {
                d == depth && is_delimiter(t) && (t.contains('|') || line.contains('|'))
            })
            .map(|(_, t)| t);
        let Some(delim) = delim else {
            if is_row(line) {
                // 表にならない `|` の行のかたまりは、1 つの段落として 1 回だけ報告する。
                let why = match prev_table_end {
                    Some(last) => format!(
                        "{} 行目の空行で表が切れ、この行から後ろは表として表示されません。空行を消してください",
                        last + 2
                    ),
                    None => "`|` で始まる行が表になっていません（見出しの行の次に区切りの行 `| --- |` がない）"
                        .to_string(),
                };
                found.problems.push((i + 1, why));
                i = block_end(i, depth, &|t| is_row(t));
            } else {
                if line.contains('|') && is_delimiter(line) {
                    found.problems.push((
                        i + 1,
                        "区切りの行の形ですが、直前に見出しの行がないので表になりません"
                            .to_string(),
                    ));
                }
                i += 1;
            }
            prev_table_end = None;
            continue;
        };
        found.tables += 1;
        if !is_row(line) || !is_row(delim) {
            found.problems.push((
                i + 1,
                "表の見出しの行と区切りの行は `|` で始めてください（行頭の `|` で、表の行と地の文を見分けて検査します）"
                    .to_string(),
            ));
            prev_table_end = None;
            i = block_end(i, depth, &|_| true);
            continue;
        }
        let head = cells(line).len();
        let delim_n = cells(delim).len();
        if head != delim_n {
            found.problems.push((
                i + 1,
                format!(
                    "見出しの列の数（{head}）と区切りの行の列の数（{delim_n}）が違うので、表になりません"
                ),
            ));
            prev_table_end = None;
            i = block_end(i, depth, &|_| true);
            continue;
        }
        let end = block_end(i + 2, depth, &|t| is_row(t));
        for j in i + 2..end {
            let n = text_at(j).map_or(0, |(_, t)| cells(t).len());
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
        // 表は空行か、ファイルの終わりで閉じる。空行がなければ、続く行も表の行として表示される。
        if !matches!(text_at(end), Some((_, t)) if blank(t)) && end < lines.len() {
            found.problems.push((
                end + 1,
                "表の直後に空行がないので、この行も表の行として表示されることがあります。表の直後に空行を入れてください"
                    .to_string(),
            ));
        }
        prev_table_end = Some(end - 1);
        i = end;
    }
    Ok(found)
}

/// 引用の `>` を剥がす。深さと中身を返す。
fn unquote(line: &str) -> (usize, &str) {
    let (mut depth, mut s) = (0, line);
    while let Some(rest) = s.trim_start().strip_prefix('>') {
        s = rest.strip_prefix(' ').unwrap_or(rest);
        depth += 1;
    }
    (depth, s)
}

fn is_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// セル。前後の `|` を除き、直前が `\` の `|` では区切らない（GFM は `\\|` も `\` と `\|` と読む）。
fn cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let mut out = Vec::new();
    let (mut start, mut prev) = (0, None);
    for (i, c) in t.char_indices() {
        if c == '|' && prev != Some('\\') {
            out.push(&t[start..i]);
            start = i + 1;
        }
        prev = Some(c);
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
        let broken = mutate(&src, "`\\|x\\| -> (r: T)", "`|x| -> (r: T)");
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
        assert_fails(&repo(&broken).run(run), "区切りの行の列の数");
    }

    /// 実ファイルの表の最後の行（直後が空行）。
    fn last_row(src: &str) -> String {
        let lines: Vec<&str> = src.lines().collect();
        (1..lines.len() - 1)
            .find(|&k| is_row(lines[k]) && is_row(lines[k - 1]) && lines[k + 1].is_empty())
            .map(|k| lines[k].to_string())
            .expect("直後が空行の表の行がない")
    }

    #[test]
    fn fails_when_prose_follows_a_table_without_a_blank_line() {
        let src = real(FILE);
        let last = last_row(&src);
        let broken = mutate(&src, &format!("{last}\n\n"), &format!("{last}\n地の文\n\n"));
        assert_fails(&repo(&broken).run(run), "表の直後に空行がない");
    }

    #[test]
    fn fails_when_a_fence_follows_a_table_without_a_blank_line() {
        let src = real(FILE);
        let last = last_row(&src);
        let broken = mutate(
            &src,
            &format!("{last}\n\n"),
            &format!("{last}\n```text\nx\n```\n\n"),
        );
        assert_fails(&repo(&broken).run(run), "表の直後に空行がない");
    }

    #[test]
    fn fails_on_a_table_without_leading_pipes() {
        let src = real(FILE);
        let lines: Vec<&str> = src.lines().collect();
        let k = (1..lines.len())
            .find(|&k| is_delimiter(lines[k]))
            .expect("区切りの行がない");
        let (head, delim) = (lines[k - 1], lines[k]);
        let broken = mutate(
            &src,
            &format!("{head}\n{delim}\n"),
            &format!("{}\n{}\n", &head[1..], &delim[1..]),
        );
        assert_fails(&repo(&broken).run(run), "`|` で始めてください");
        let pipeless = format!("{src}\na | b\n--- | ---\n1 | 2 | 3\n");
        assert_fails(&repo(&pipeless).run(run), "`|` で始めてください");
    }

    #[test]
    fn fails_on_a_stray_delimiter_row() {
        let broken = format!("{}\n段落\n\n--- | ---\n", real(FILE));
        assert_fails(&repo(&broken).run(run), "直前に見出しの行がない");
    }

    #[test]
    fn fails_on_a_broken_table_in_a_quote() {
        let cols = format!("{}\n> | a | b |\n> | --- | --- |\n> | 1 |\n", real(FILE));
        assert_fails(&repo(&cols).run(run), "が見出し");
        let split = format!("{}\n> | a |\n> | --- |\n> | 1 |\n>\n> | 2 |\n", real(FILE));
        assert_fails(&repo(&split).run(run), "空行で表が切れ");
    }

    #[test]
    fn fails_when_inline_triple_backticks_precede_a_broken_table() {
        // 行頭の ```x``` はインラインコードで、フェンスを開かない（後ろの表を隠さない）。
        let broken = format!(
            "{}\n```x``` を書く\n\n| a |\n| --- |\n| 1 | 2 |\n",
            real(FILE)
        );
        assert_fails(&repo(&broken).run(run), "が見出し");
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
| x\\\\| y | 3 |

| c |
| --- |
| 3 |

> | 引用 | の表 |
> | --- | --- |
> | 4 | 5 |

1. リストの項目

    ```text
    | 字下げしたフェンス
    ```

```text
| 表 | ではない |

| ここも |
```
";
        let r = repo(text).run(run);
        assert_passes(&r);
        assert!(
            r.notes.iter().any(|n| n.contains("1 ファイルの 3 個の表")),
            "{:?}",
            r.notes
        );
    }
}
