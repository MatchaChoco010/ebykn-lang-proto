//! `check-book`：Ebykn Book の SUMMARY と章の対応、` ```eyk ` フェンスの種別。
//!
//! `ebykn` が例を実行できるのは M2 以降なので、今は形だけを検査する。実行していない数を出力に出す。

use std::collections::HashSet;

use super::{Repo, Report, fence_close, fence_open};

const SRC: &str = "book/src/";
const SUMMARY: &str = "book/src/SUMMARY.md";
const KINDS: &[&str] = &["check", "verify", "fail", "skip"];

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    let summary = match repo.read(SUMMARY) {
        Ok(s) => s,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    let mut listed = HashSet::new();
    let mut drafts = 0;
    for (i, line) in summary.lines().enumerate() {
        let no = i + 1;
        match summary_entry(line) {
            Entry::Other => {}
            Entry::Bad => r.errors.push(format!(
                "{SUMMARY}:{no}: 解釈できない行です（章は `- [題名](ファイル)`、下書き章は `- [題名]()`）"
            )),
            Entry::Chapter(t) if t.is_empty() => drafts += 1,
            Entry::Chapter(t) => {
                let path = format!("{SRC}{t}");
                if !repo.has(&path) {
                    r.errors.push(format!("{SUMMARY}:{no}: {path} がありません"));
                }
                if !listed.insert(path.clone()) {
                    r.errors.push(format!("{SUMMARY}:{no}: {path} が二度載っています"));
                }
            }
        }
    }
    let mut counts = [0usize; 4];
    for f in repo
        .files
        .iter()
        .filter(|f| f.starts_with(SRC) && f.ends_with(".md"))
    {
        if f == SUMMARY {
            continue;
        }
        if !listed.contains(f) {
            r.errors.push(format!("{f}: {SUMMARY} に載っていません"));
        }
        match repo.read(f) {
            Ok(text) => check_fences(f, &text, &mut counts, &mut r.errors),
            Err(e) => r.errors.push(e),
        }
    }
    let [check, verify, fail, skip] = counts;
    r.notes.push(format!(
        "章 {}、下書き章 {drafts}（ファイルなし）",
        listed.len()
    ));
    r.notes.push(format!(
        "eyk check {check} / verify {verify} / fail {fail} は形だけ検査し、実行していない（実行は M2 以降）"
    ));
    r.notes.push(format!(
        "eyk skip {skip} は実行しない（理由と追跡 Issue を検査）"
    ));
    r
}

enum Entry {
    /// 章。リンク先が空なら下書き章。
    Chapter(String),
    /// 見出し・区切り・空行。
    Other,
    Bad,
}

fn summary_entry(line: &str) -> Entry {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') || t == "---" {
        return Entry::Other;
    }
    let link = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .unwrap_or(t);
    let parsed = link
        .strip_prefix('[')
        .and_then(|x| x.rsplit_once("]("))
        .and_then(|(title, rest)| Some((title, rest.strip_suffix(')')?)));
    match parsed {
        Some((title, target)) if !title.is_empty() && !target.contains(['(', ')', ' ']) => {
            Entry::Chapter(target.to_string())
        }
        _ => Entry::Bad,
    }
}

/// 直後に来なければならないフェンス。
enum Expect {
    None,
    Next(&'static str, &'static str, usize),
}

fn check_fences(file: &str, text: &str, counts: &mut [usize; 4], errors: &mut Vec<String>) {
    let mut expect = Expect::None;
    // 開いているフェンス：文字、長さ、開いた行、種別、中身の 1 行目を見たか。
    let mut open: Option<(char, usize, usize, Option<&str>, bool)> = None;
    for (i, line) in text.lines().enumerate() {
        let no = i + 1;
        if let Some((c, n, at, kind, seen)) = open.as_mut() {
            if fence_close(line, *c, *n) {
                expect = match *kind {
                    Some("verify") => Expect::Next("status", "verify", no),
                    Some("fail") => Expect::Next("diag", "fail", no),
                    _ => Expect::None,
                };
                if *kind == Some("skip") && !*seen {
                    errors.push(format!("{file}:{at}: eyk skip が空です"));
                }
                open = None;
            } else if *kind == Some("skip") && !*seen {
                *seen = true;
                if !skip_reason_ok(line) {
                    errors.push(format!(
                        "{file}:{no}: eyk skip の 1 行目は `// skip(#Issue 番号): 理由` にしてください"
                    ));
                }
            }
            continue;
        }
        let Some((c, n, info)) = fence_open(line) else {
            if let Expect::Next(want, after, _) = expect
                && !line.trim().is_empty()
            {
                errors.push(format!(
                    "{file}:{no}: eyk {after} の直後に ```{want} がありません"
                ));
                expect = Expect::None;
            }
            continue;
        };
        let mut words = info.split_whitespace();
        let lang = words.next().unwrap_or("");
        let rest: Vec<&str> = words.collect();
        if let Expect::Next(want, after, _) = expect {
            if lang != want {
                errors.push(format!(
                    "{file}:{no}: eyk {after} の直後に ```{want} がありません"
                ));
            }
        } else if lang == "status" || lang == "diag" {
            let after = if lang == "status" { "verify" } else { "fail" };
            errors.push(format!(
                "{file}:{no}: ```{lang} は ```eyk {after} の直後にだけ置けます"
            ));
        }
        expect = Expect::None;
        let mut kind = None;
        match (lang, rest.as_slice()) {
            ("", _) => errors.push(format!(
                "{file}:{no}: フェンスに言語がありません（地の文なら ```text）"
            )),
            ("eyk", [k]) if KINDS.contains(k) => {
                let idx = KINDS.iter().position(|x| x == k).unwrap();
                counts[idx] += 1;
                kind = Some(KINDS[idx]);
            }
            ("eyk", []) => errors.push(format!(
                "{file}:{no}: 種別のない ```eyk です（check / verify / fail / skip のどれか）"
            )),
            ("eyk", other) => errors.push(format!(
                "{file}:{no}: ```eyk の種別 `{}` は使えません（check / verify / fail / skip のどれか）",
                other.join(" ")
            )),
            _ => {}
        }
        open = Some((c, n, no, kind, false));
    }
    if let Some((_, _, at, _, _)) = open {
        errors.push(format!("{file}:{at}: コードフェンスが閉じていません"));
    }
    if let Expect::Next(want, after, at) = expect {
        errors.push(format!(
            "{file}:{at}: eyk {after} の直後に ```{want} がありません"
        ));
    }
}

/// `// skip(#12): 理由`
fn skip_reason_ok(line: &str) -> bool {
    let Some(rest) = line.trim().strip_prefix("// skip(#") else {
        return false;
    };
    let Some((num, reason)) = rest.split_once("):") else {
        return false;
    };
    !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) && !reason.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    const INTRO: &str = "book/src/ch00-00-introduction.md";

    fn book(summary: &str, intro: &str) -> TempRepo {
        let mut t = TempRepo::new();
        t.write(SUMMARY, summary).write(INTRO, intro);
        t
    }

    fn real_book() -> TempRepo {
        book(&real(SUMMARY), &real(INTRO))
    }

    fn with_example(example: &str) -> Report {
        book(&real(SUMMARY), &format!("{}\n{example}", real(INTRO))).run(run)
    }

    #[test]
    fn passes_on_the_real_book() {
        assert_passes(&real_book().run(run));
    }

    #[test]
    fn passes_on_well_formed_examples() {
        let ok = "```eyk check\nfn f() {}\n```\n\n```eyk verify\nfn f() {}\n```\n\n```status\nf verified\n```\n\
                  ```eyk fail\nfn f() {}\n```\n```diag\nE0505\n```\n\
                  ```eyk skip\n// skip(#12): M4 で入る部分集合型を使う\n```\n\
                  ```console\n$ ebykn --version\n```\n````text\n```eyk\n````\n";
        let r = with_example(ok);
        assert_passes(&r);
        assert!(
            r.notes
                .iter()
                .any(|n| n.contains("check 1 / verify 1 / fail 1"))
        );
    }

    #[test]
    fn fails_when_summary_links_a_missing_file() {
        let s = mutate(
            &real(SUMMARY),
            "(ch00-00-introduction.md)",
            "(ch00-00-missing.md)",
        );
        assert_fails(
            &book(&s, &real(INTRO)).run(run),
            "ch00-00-missing.md がありません",
        );
    }

    #[test]
    fn fails_when_a_chapter_is_not_in_summary() {
        let mut t = real_book();
        t.write("book/src/ch99-00-extra.md", "# 追加\n");
        assert_fails(
            &t.run(run),
            "ch99-00-extra.md: book/src/SUMMARY.md に載っていません",
        );
    }

    #[test]
    fn fails_on_an_unparsable_summary_line() {
        let s = format!("{}\n- 題名だけ\n", real(SUMMARY));
        assert_fails(&book(&s, &real(INTRO)).run(run), "解釈できない行");
    }

    #[test]
    fn fails_on_eyk_without_kind() {
        assert_fails(&with_example("```eyk\nfn f() {}\n```\n"), "種別のない");
    }

    #[test]
    fn fails_on_an_unknown_kind() {
        assert_fails(
            &with_example("```eyk run\nfn f() {}\n```\n"),
            "`run` は使えません",
        );
    }

    #[test]
    fn fails_on_verify_without_status() {
        let r = with_example("```eyk verify\nfn f() {}\n```\n\n本文\n");
        assert_fails(&r, "eyk verify の直後に ```status");
        let r = with_example("```eyk verify\nfn f() {}\n```\n");
        assert_fails(&r, "eyk verify の直後に ```status");
    }

    #[test]
    fn fails_on_fail_without_diag() {
        let r = with_example("```eyk fail\nfn f() {}\n```\n```status\nx\n```\n");
        assert_fails(&r, "eyk fail の直後に ```diag");
    }

    #[test]
    fn fails_on_orphan_status() {
        let r = with_example("```eyk check\nfn f() {}\n```\n```status\nx\n```\n");
        assert_fails(&r, "```status は ```eyk verify の直後");
    }

    #[test]
    fn fails_on_skip_without_reason_and_issue() {
        assert_fails(
            &with_example("```eyk skip\nfn f() {}\n```\n"),
            "skip の 1 行目",
        );
        assert_fails(
            &with_example("```eyk skip\n// skip(#12):\n```\n"),
            "skip の 1 行目",
        );
        assert_fails(
            &with_example("```eyk skip\n// skip: 未実装\n```\n"),
            "skip の 1 行目",
        );
        assert_fails(&with_example("```eyk skip\n```\n"), "eyk skip が空");
    }

    #[test]
    fn fails_on_a_fence_without_language() {
        assert_fails(&with_example("```\nfn f() {}\n```\n"), "言語がありません");
    }

    #[test]
    fn fails_on_an_unclosed_fence() {
        assert_fails(&with_example("```eyk check\nfn f() {}\n"), "閉じていません");
    }
}
