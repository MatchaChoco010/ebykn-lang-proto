//! Verus のエラーのブロックから、エラーが指す生成コードの行を読む（PR #76 のレビューの B1）。
//!
//! 主な位置（`-->` の行）だけで項目に割り当てると、失敗した呼び出し元を verified と出してしまうことがある。
//! `callee may open invariants that caller cannot` は、呼ばれる側の署名・呼び出し元の
//! `opens_invariants`・呼び出しの位置の 3 つのラベルをどれも `^` で描き、`-->` はそのうちファイルの中で
//! 先に出るものを指す。呼ばれる側が先にあれば、`-->` は呼ばれる側を指す。そこで、ブロックの中のラベルの
//! 付いた行（`NN | ..` の行と、その下の `^` / `-` の行）をすべて読み、次のように分ける。
//!
//! - 失敗の場所（[`ErrorSites::blamed`]。その項目を failed にする）：主な span（`^`）の行と、
//!   [`BLAME_LABELS`] のラベルの行。
//! - 副の位置（[`ErrorSites::related`]。その項目を verified にしない）：そのほかのラベルの行（`-`）と、
//!   ラベルの付く行の範囲がつかめない複数行の span の行。
//! - 数えない行：[`CALLEE_LABELS`] のラベル（呼ばれる側の契約や署名を示すだけで、呼ばれる側の失敗では
//!   ない）と、ラベルのない文脈の行。
//!
//! ブロックに `NN |` の行が一つもなければ、`-->` の位置には頼らず、失敗の場所を空にする（どの項目にも
//! 割り当てられないエラーとして、失敗していない項目をすべて unknown にする）。
//!
//! Verus の `--output-json` の `func-details` は関数ごとの成否を持たない（失敗した関数も
//! `failed_proof_notes` が空）ので、ブロックの行で決める。

/// 呼び出し元の失敗の場所を示すラベル（失敗した呼び出しの位置）。`-` で描かれても失敗の場所にする。
pub const BLAME_LABELS: &[&str] = &["at this call-site"];

/// 呼ばれる側を示すだけのラベル。その行の項目のエラーではない（呼ばれる側は自分の検証で決まる）。
pub const CALLEE_LABELS: &[&str] = &[
    "failed precondition",
    "invariants opened by callee",
    "perhaps you need to mark this function as `no_unwind`?",
];

/// 一つのエラーが指す生成コードの行（`build/verus/` からの相対パスと行）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ErrorSites {
    /// 失敗の場所。空なら、どの項目にも割り当てられない。
    pub blamed: Vec<(String, usize)>,
    /// 副の位置。
    pub related: Vec<(String, usize)>,
}

impl ErrorSites {
    /// 失敗の場所が一つだけのエラー。
    pub fn at(file: &str, line: usize) -> ErrorSites {
        ErrorSites {
            blamed: vec![(file.to_string(), line)],
            related: Vec::new(),
        }
    }
}

/// 下線の一つ：主な span（`^`）か、そのラベル。
#[derive(Debug, Clone)]
struct Mark {
    primary: bool,
    label: String,
}

/// ブロックの中の `NN | ..` の行。
#[derive(Debug)]
struct Shown {
    /// 生成コードのファイル（生成コードの外なら `None`）。
    file: Option<String>,
    line: usize,
    marks: Vec<Mark>,
}

/// `-->` / `:::` の後の位置から、生成コードのファイル（`src/..`）を取り出す。
fn generated_file(loc: &str) -> Option<String> {
    let loc = loc.trim();
    let loc = loc.strip_suffix("（生成コード）").unwrap_or(loc);
    let loc = loc.strip_prefix("build/verus/").unwrap_or(loc);
    let file = loc.split(':').next()?;
    file.starts_with("src/").then(|| file.to_string())
}

/// `NN | code` の行なら、行番号と `|` の後。
fn numbered(t: &str) -> Option<(usize, &str)> {
    let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let n = t[..digits].parse().ok()?;
    let rest = t[digits..].trim_start().strip_prefix('|')?;
    Some((n, rest))
}

/// 下線の語（`^^^`、`---`、`|___^` など）か。
fn underline(tok: &str) -> bool {
    tok.contains(['^', '-']) && tok.chars().all(|c| matches!(c, '^' | '-' | '_' | '|'))
}

/// エラーのブロックから、エラーが指す生成コードの行を読む。
pub fn error_sites(block: &str) -> ErrorSites {
    let mut shown: Vec<Shown> = Vec::new();
    // 終わりの印（`|__^`）を待つ、複数行の span の行。
    let mut pending: Vec<usize> = Vec::new();
    // 終わりの印が見つからなかった複数行の span の行（ラベルがつかめないので副の位置にする）。
    let mut unresolved: Vec<usize> = Vec::new();
    let mut file: Option<String> = None;
    for raw in block.lines() {
        let t = raw.trim_start();
        if let Some(loc) = t.strip_prefix("--> ").or_else(|| t.strip_prefix("::: ")) {
            file = generated_file(loc);
            unresolved.append(&mut pending);
            continue;
        }
        if let Some((n, code)) = numbered(t) {
            let code = code.strip_prefix(' ').unwrap_or(code);
            // 複数行の span の途中の行（`NN | | ..`、`NN | / ..`）。
            let span = code.starts_with("/ ") || code.starts_with("| ") || code == "|";
            shown.push(Shown {
                file: file.clone(),
                line: n,
                marks: Vec::new(),
            });
            if span {
                pending.push(shown.len() - 1);
            }
            continue;
        }
        let Some(ann) = t.strip_prefix('|') else {
            continue;
        };
        let Some(last) = shown.len().checked_sub(1) else {
            continue;
        };
        let toks: Vec<&str> = ann.split_whitespace().collect();
        match toks.iter().rposition(|t| underline(t)) {
            Some(i) => {
                let mut marks: Vec<Mark> = toks[..i]
                    .iter()
                    .filter(|t| underline(t))
                    .map(|t| Mark {
                        primary: t.contains('^'),
                        label: String::new(),
                    })
                    .collect();
                marks.push(Mark {
                    primary: toks[i].contains('^'),
                    label: toks[i + 1..].join(" "),
                });
                // `|___^` は複数行の span の終わり。待っていた行にも同じ印を付ける。
                if toks[i].starts_with('|') && toks[i].contains('_') {
                    for &p in &pending {
                        shown[p].marks.extend(marks.iter().cloned());
                    }
                    pending.clear();
                }
                shown[last].marks.extend(marks);
            }
            None => {
                // ラベルだけの行（下線から `|` でつないだもの）。ラベルのない最初の下線に付ける。
                let label = toks
                    .iter()
                    .skip_while(|t| **t == "|")
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
                if label.is_empty() {
                    continue;
                }
                let marks = &mut shown[last].marks;
                match marks.iter_mut().find(|m| m.label.is_empty()) {
                    Some(m) => m.label = label,
                    None => marks.push(Mark {
                        primary: false,
                        label,
                    }),
                }
            }
        }
    }
    unresolved.append(&mut pending);
    let mut out = ErrorSites::default();
    for (i, s) in shown.iter().enumerate() {
        let Some(f) = &s.file else { continue };
        let mut blamed = false;
        let mut related = unresolved.contains(&i);
        for m in &s.marks {
            if BLAME_LABELS.contains(&m.label.as_str()) {
                blamed = true;
            } else if CALLEE_LABELS.contains(&m.label.as_str()) {
            } else if m.primary {
                blamed = true;
            } else {
                related = true;
            }
        }
        let site = (f.clone(), s.line);
        if blamed {
            out.blamed.push(site);
        } else if related {
            out.related.push(site);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[(&str, usize)]) -> Vec<(String, usize)> {
        v.iter().map(|(f, n)| (f.to_string(), *n)).collect()
    }

    /// PR #76 のレビューの B1：`drop` がクレートの関数を呼ぶ。主な span は呼ばれる側の署名にあり、
    /// 失敗したのは呼び出しのある `drop`（Verus の出力のまま）。
    const CALLEE_OPENS: &str = "error: callee may open invariants that caller cannot
  --> build/verus/src/lib.rs:10:1（生成コード）
   |
10 | pub fn helper(x: u64) -> u64
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ invariants opened by callee
...
16 |     fn drop(&mut self) opens_invariants none
   |                                         ^^^^ invariants opened by caller
17 |     no_unwind {
18 |         self.a = helper(self.a);
   |                  ^^^^^^^^^^^^^^ at this call-site

";

    #[test]
    fn callee_side_spans_do_not_blame_the_callee() {
        let s = error_sites(CALLEE_OPENS);
        assert_eq!(s.blamed, lines(&[("src/lib.rs", 16), ("src/lib.rs", 18)]));
        assert!(s.related.is_empty(), "{s:?}");
    }

    #[test]
    fn failed_preconditions_blame_the_caller_only() {
        let block = "error: precondition not satisfied
  --> build/verus/src/lib.rs:13:21（生成コード）
   |
11 | requires x > 0, { x }
   |          ----- failed precondition
12 |
13 | pub fn g() -> u64 { f(0) }
   |                     ^^^^

";
        let s = error_sites(block);
        assert_eq!(s.blamed, lines(&[("src/lib.rs", 13)]));
        assert!(s.related.is_empty(), "{s:?}");
    }

    #[test]
    fn other_secondary_labels_are_related() {
        let block = "error: postcondition not satisfied
  --> build/verus/src/lib.rs:16:9（生成コード）
   |
 9 | pub open spec fn p() -> bool { true }
   |                  - some other note
16 | ensures r > 0, { x }
   |         ^^^^^    - at the end of the function body
   |         |
   |         failed this postcondition

";
        let s = error_sites(block);
        assert_eq!(s.blamed, lines(&[("src/lib.rs", 16)]));
        assert_eq!(s.related, lines(&[("src/lib.rs", 9)]));
    }

    #[test]
    fn lines_outside_the_generated_code_are_ignored() {
        let block = "error: callee may open invariants that caller cannot
   --> build/verus/src/lib.rs:25:41（生成コード）
    |
 25 |     fn drop(&mut self) opens_invariants none
    |                                         ^^^^ invariants opened by caller
 26 |     no_unwind {
 27 |         let v: Vec < u64 > = Vec::new();
    |                              ^^^^^^^^^^ at this call-site
    |
   ::: /home/u/.cargo/registry/src/vstd/std_specs/vec.rs:100:1
    |
100 | pub assume_specification<T>[ Vec::<T>::new ]() -> (v: Vec<T>)
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ invariants opened by callee

";
        let s = error_sites(block);
        assert_eq!(s.blamed, lines(&[("src/lib.rs", 25), ("src/lib.rs", 27)]));
        assert!(s.related.is_empty(), "{s:?}");
    }

    #[test]
    fn multi_line_spans_mark_every_line() {
        let block = "error: something
  --> build/verus/src/lib.rs:40:25（生成コード）
   |
40 |       struct L { z: u8, } impl Drop for L {
   |  _________________________^
41 | |         fn drop(&mut self) {}
42 | |     }
   | |_____^

";
        let s = error_sites(block);
        assert_eq!(
            s.blamed,
            lines(&[("src/lib.rs", 40), ("src/lib.rs", 41), ("src/lib.rs", 42)])
        );
    }

    #[test]
    fn unfinished_multi_line_spans_are_related() {
        let block = "error: something
  --> build/verus/src/lib.rs:5:1（生成コード）
   |
 3 | / fn a() {
 4 | |     b();
   |
";
        let s = error_sites(block);
        assert!(s.blamed.is_empty(), "{s:?}");
        assert_eq!(s.related, lines(&[("src/lib.rs", 3), ("src/lib.rs", 4)]));
    }

    /// PR #76 の再レビューの N3：`NN |` の行がないブロックは、`-->` の位置に頼らない（呼ばれる側が先に
    /// 出るエラーで、呼ばれる側を失敗にしてしまう）。どの項目にも割り当てない。
    #[test]
    fn blocks_without_shown_lines_blame_nothing() {
        let s = error_sites("error: x\n  --> build/verus/src/lib.rs:3:1（生成コード）\n");
        assert_eq!(s, ErrorSites::default());
        assert_eq!(error_sites("error: x\n"), ErrorSites::default());
    }

    /// `at this call-site` が `-` で描かれても、呼び出しの行を失敗の場所にする（[`BLAME_LABELS`]）。
    #[test]
    fn call_sites_drawn_as_secondary_are_blamed() {
        let block = "error: callee may open invariants that caller cannot
  --> build/verus/src/lib.rs:3:1（生成コード）
   |
 3 | pub fn helper(x: u64) -> u64
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ invariants opened by callee
...
 9 |         self.a = helper(self.a);
   |                  -------------- at this call-site

";
        let s = error_sites(block);
        assert_eq!(s.blamed, lines(&[("src/lib.rs", 9)]));
        assert!(s.related.is_empty(), "{s:?}");
    }
}
