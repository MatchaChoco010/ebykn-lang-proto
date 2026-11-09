//! `check-file-size`：ソースファイルを 1 ファイル 2000 行までに保つ。

use super::{Repo, Report};

pub const LIMIT: usize = 2000;
const EXTENSIONS: &[&str] = &["rs", "md", "toml", "yml", "sh"];
/// 対象外。作者の正典で、エージェントは分割も変更もしない。
pub const EXEMPT: &[&str] = &[
    "docs/ebykn-thesis.md",
    "docs/ebykn-spec.md",
    "docs/ebykn-design-notes.md",
];

pub fn run(repo: &Repo) -> Report {
    let mut r = Report::default();
    for e in EXEMPT {
        if !repo.has(e) {
            r.errors.push(format!(
                "対象外リストの {e} がありません。リストから消してください"
            ));
        }
    }
    r.notes
        .push(format!("対象外（明示）: {}", EXEMPT.join(", ")));
    let mut checked = 0;
    for f in &repo.files {
        let ext = f.rsplit_once('.').map(|(_, e)| e);
        if !ext.is_some_and(|e| EXTENSIONS.contains(&e)) || EXEMPT.contains(&f.as_str()) {
            continue;
        }
        checked += 1;
        match repo.read_bytes(f) {
            Err(e) => r.errors.push(e),
            Ok(bytes) => {
                let lines = count_lines(&bytes);
                if lines > LIMIT {
                    r.errors.push(format!(
                        "{f}: {lines} 行あります（上限 {LIMIT} 行）。ファイルを分けてください"
                    ));
                }
            }
        }
    }
    r.notes.push(format!("{checked} ファイルを検査"));
    r
}

fn count_lines(bytes: &[u8]) -> usize {
    let n = bytes.iter().filter(|&&b| b == b'\n').count();
    n + usize::from(bytes.last().is_some_and(|&b| b != b'\n'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    fn with_exempt(t: &mut TempRepo) {
        for e in EXEMPT {
            t.write(e, "# 正典\n");
        }
    }

    #[test]
    fn fails_when_a_file_exceeds_the_limit() {
        let src = real("Cargo.toml");
        let pad = "\n".repeat(LIMIT);
        let broken = format!("{src}{pad}");
        assert!(
            count_lines(broken.as_bytes()) > LIMIT,
            "壊す操作が当たらない"
        );
        let mut t = TempRepo::new();
        with_exempt(&mut t);
        t.write("Cargo.toml", &broken);
        assert_fails(&t.run(run), "Cargo.toml:");
    }

    #[test]
    fn fails_when_an_exempt_file_is_gone() {
        let mut t = TempRepo::new();
        t.write("README.md", &real("README.md"));
        assert_fails(&t.run(run), EXEMPT[0]);
    }

    #[test]
    fn passes_at_the_limit_and_ignores_other_extensions() {
        let mut t = TempRepo::new();
        with_exempt(&mut t);
        t.write("a.rs", &"x\n".repeat(LIMIT));
        t.write("b.txt", &"x\n".repeat(LIMIT + 1));
        t.write("docs/ebykn-spec.md", &"x\n".repeat(LIMIT + 1));
        assert_passes(&t.run(run));
    }

    #[test]
    fn counts_a_last_line_without_newline() {
        assert_eq!(count_lines(b""), 0);
        assert_eq!(count_lines(b"a"), 1);
        assert_eq!(count_lines(b"a\nb"), 2);
        assert_eq!(count_lines(b"a\nb\n"), 2);
    }
}
