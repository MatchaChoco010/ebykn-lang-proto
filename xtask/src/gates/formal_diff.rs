//! `check-formal-diff`：§6.2 の参照モデルの判定の表（`formal/oracle/linearity.tsv`）と、
//! 実装の検査（`ebykn check` と同じ `ebykn_check::check`）の判定を比べる差分テスト（#63）。
//!
//! `formal/oracle/linearity.eyk` を一つのプロジェクトの `src/lib.eyk` として検査し、診断を
//! 位置で関数 `c<番号>` に割り当てる。表の `verdict` が `ok` なら診断がないこと、コードなら
//! そのコードが診断にあることを確かめる。位置はまだ比べない（モデルと実装で選び方が違う）。
//!
//! 食い違いは [`EXCEPTIONS`] に、ケースと実装の判定と理由と追跡の Issue で持つ。次を失敗にする：
//! 一覧にない食い違い、一覧にあるのに食い違わないケースか実装の判定が一覧と違うケース、
//! 表にないケースを挙げた一覧の行、補助の関数（`c<番号>` でない関数）か関数の外の診断、
//! 表と関数の対応の崩れ（行と関数の数・名前・順）、読めない表。

use std::collections::BTreeMap;

use ebykn_check::project;
use ebykn_syntax::ast::ItemKind;

use super::{Repo, Report};

pub const TSV: &str = "formal/oracle/linearity.tsv";
pub const EYK: &str = "formal/oracle/linearity.eyk";

/// モデルと実装の判定の、明示した食い違い。
pub struct Exception {
    /// 表のケース（`c<番号>`）。
    pub case: &'static str,
    /// 実装の判定（診断のコードを位置の順に `,` でつないだもの。診断がなければ `ok`）。
    pub got: &'static str,
    pub reason: &'static str,
    /// 追跡の Issue。
    pub issue: u32,
}

/// 食い違いの一覧。ここにないケースは、モデルと実装の判定が一致しなければならない。
pub const EXCEPTIONS: &[Exception] = &[];

/// 表の 1 行のうち、比べるもの。
struct Row {
    case: String,
    verdict: String,
}

fn parse_tsv(text: &str) -> Result<Vec<Row>, String> {
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, h))
            if h.split('\t')
                .take(5)
                .eq(["case", "tier", "header", "body", "verdict"]) => {}
        _ => return Err(format!("{TSV}:1: 見出しの行が読めません")),
    }
    let mut rows = Vec::new();
    for (i, l) in lines {
        let f: Vec<&str> = l.split('\t').collect();
        let ok_case = f.first().is_some_and(|c| {
            c.len() > 1 && c.starts_with('c') && c[1..].bytes().all(|b| b.is_ascii_digit())
        });
        let ok_verdict = f.get(4).is_some_and(|v| *v == "ok" || is_code(v));
        if f.len() != 8 || !ok_case || !ok_verdict {
            return Err(format!("{TSV}:{}: 行が読めません: {l:?}", i + 1));
        }
        rows.push(Row {
            case: f[0].to_string(),
            verdict: f[4].to_string(),
        });
    }
    if rows.is_empty() {
        return Err(format!("{TSV}: ケースがありません"));
    }
    Ok(rows)
}

fn is_code(s: &str) -> bool {
    s.len() == 5 && s.starts_with('E') && s[1..].bytes().all(|b| b.is_ascii_digit())
}

/// `linearity.eyk` を検査し、ケースごとの実装の判定（コードの列。診断がなければ空）を返す。
/// ケースでない場所の診断は `stray` に入れる。
fn implementation(eyk: &str) -> (Vec<(String, Vec<String>)>, Vec<String>) {
    let (p, mut diags) = project::single(eyk);
    let checked = ebykn_check::check(&p);
    diags.extend(checked.diags.iter().cloned());
    ebykn_check::sort_diags(&mut diags);
    let file = p.file(ebykn_syntax::span::FileId(1));
    let mut cases: Vec<(String, usize, usize, Vec<String>)> = Vec::new();
    if let Some(ast) = p.ast(ebykn_syntax::span::FileId(1)) {
        for it in &ast.items {
            if let ItemKind::Fn(f) = &it.kind {
                let n = &f.sig.ident.name;
                if n.len() > 1 && n.starts_with('c') && n[1..].bytes().all(|b| b.is_ascii_digit()) {
                    cases.push((n.clone(), it.span.lo, it.span.hi, Vec::new()));
                }
            }
        }
    }
    let mut stray = Vec::new();
    for d in &diags {
        let at = d.span.lo;
        match cases
            .iter_mut()
            .find(|(_, lo, hi, _)| d.span.file == file.id && *lo <= at && at < *hi)
        {
            Some((_, _, _, codes)) => codes.push(d.code().as_str().to_string()),
            None => stray.push(format!(
                "{EYK}:{}:{}: ケースの外の診断 {}",
                d.span.start.line,
                d.span.start.col,
                d.code().as_str()
            )),
        }
    }
    (
        cases.into_iter().map(|(n, _, _, c)| (n, c)).collect(),
        stray,
    )
}

/// 実装の判定の表記（[`Exception::got`] と同じ形）。
fn show(codes: &[String]) -> String {
    if codes.is_empty() {
        "ok".to_string()
    } else {
        codes.join(",")
    }
}

/// モデルの判定と実装の判定が一致するか。
fn agrees(verdict: &str, codes: &[String]) -> bool {
    if verdict == "ok" {
        codes.is_empty()
    } else {
        codes.iter().any(|c| c == verdict)
    }
}

pub fn run(repo: &Repo) -> Report {
    let missing: Vec<String> = [TSV, EYK]
        .iter()
        .filter(|f| !repo.has(f))
        .map(|f| format!("{f}: 追跡されていません"))
        .collect();
    if !missing.is_empty() {
        return Report {
            errors: missing,
            notes: Vec::new(),
        };
    }
    let (tsv, eyk) = match (repo.read(TSV), repo.read(EYK)) {
        (Ok(t), Ok(e)) => (t, e),
        (t, e) => {
            return Report {
                errors: [t.err(), e.err()].into_iter().flatten().collect(),
                notes: Vec::new(),
            };
        }
    };
    compare(&tsv, &eyk, EXCEPTIONS)
}

pub fn compare(tsv: &str, eyk: &str, exceptions: &[Exception]) -> Report {
    let mut r = Report::default();
    let rows = match parse_tsv(tsv) {
        Ok(rows) => rows,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    let (cases, stray) = implementation(eyk);
    r.errors.extend(stray);
    if cases.len() != rows.len() {
        r.errors.push(format!(
            "{EYK}: ケースの関数が {} 個、{TSV} の行が {} 個で、数が違います",
            cases.len(),
            rows.len()
        ));
        return r;
    }
    let mut by_case: BTreeMap<&str, &Exception> = BTreeMap::new();
    for x in exceptions {
        if by_case.insert(x.case, x).is_some() {
            r.errors
                .push(format!("例外の一覧: {} が二度あります", x.case));
        }
    }
    let mut agreed = 0;
    let mut excepted: BTreeMap<u32, usize> = BTreeMap::new();
    let mut listed = Vec::new();
    for (row, (name, codes)) in rows.iter().zip(&cases) {
        if row.case != *name {
            r.errors.push(format!(
                "{EYK}: 関数 {name} が {TSV} の {} と対応しません（順が違います）",
                row.case
            ));
            return r;
        }
        let got = show(codes);
        match (agrees(&row.verdict, codes), by_case.remove(row.case.as_str())) {
            (true, None) => agreed += 1,
            (false, None) => r.errors.push(format!(
                "{}: モデルは {}、実装は {got} です。実装かモデルを直すか、理由と Issue を付けて例外の一覧（xtask/src/gates/formal_diff.rs の EXCEPTIONS）に足してください",
                row.case, row.verdict
            )),
            (true, Some(x)) => r.errors.push(format!(
                "{}: 例外の一覧にありますが、モデルと実装の判定が一致します（{}）。一覧から消してください（#{}）",
                row.case, row.verdict, x.issue
            )),
            (false, Some(x)) if x.got != got => r.errors.push(format!(
                "{}: 例外の一覧では実装は {} ですが、実際は {got} です（モデルは {}、#{}）",
                row.case, x.got, row.verdict, x.issue
            )),
            (false, Some(x)) => {
                *excepted.entry(x.issue).or_default() += 1;
                listed.push(format!(
                    "{}: モデルは {}、実装は {got}（例外：{}。#{}）",
                    row.case, row.verdict, x.reason, x.issue
                ));
            }
        }
    }
    for x in by_case.values() {
        r.errors.push(format!(
            "例外の一覧: {} は {TSV} にないケースです（#{}）",
            x.case, x.issue
        ));
    }
    r.notes.push(format!(
        "{} ケースのうち {agreed} ケースでモデルと実装のコードが一致（位置は比べていない）",
        rows.len()
    ));
    r.notes.extend(listed);
    for (issue, n) in excepted {
        r.notes.push(format!(
            "例外の一覧で食い違いを認めたケース {n} 個（#{issue}）"
        ));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    /// 表の先頭の `n` ケースと、それに当たる `.eyk`（補助の関数を含む）。
    fn head(n: usize) -> (String, String) {
        let tsv: String = real(TSV)
            .lines()
            .take(n + 1)
            .map(|l| format!("{l}\n"))
            .collect();
        let eyk = real(EYK);
        let next = eyk
            .find(&format!("fn c{:05}(", n + 1))
            .and_then(|i| eyk[..i].rfind("\n// "))
            .expect("ケースの区切りがない");
        let eyk = eyk[..next + 1].to_string();
        (tsv, eyk)
    }

    const X: Exception = Exception {
        case: "c00004",
        got: "ok",
        reason: "テスト",
        issue: 1,
    };

    #[test]
    fn passes_on_the_committed_tables() {
        let report = compare(&real(TSV), &real(EYK), EXCEPTIONS);
        assert_passes(&report);
    }

    #[test]
    fn passes_on_the_head_of_the_tables() {
        let (tsv, eyk) = head(20);
        assert_passes(&compare(&tsv, &eyk, &[]));
    }

    /// 実装の判定が変わる（`c00004` の `p.free();` を消す）と、一覧にない食い違いで落ちる。
    #[test]
    fn fails_when_the_implementation_disagrees() {
        let (tsv, eyk) = head(20);
        let eyk = mutate(
            &eyk,
            "fn c00004(p: Ptr<u32>) uses r: &mut Region<Ptr<u32>> {\n    p.free();\n",
            "fn c00004(p: Ptr<u32>) uses r: &mut Region<Ptr<u32>> {\n",
        );
        let report = compare(&tsv, &eyk, &[]);
        assert_fails(&report, "c00004: モデルは E0504、実装は ok です");
    }

    /// モデルの判定が変わると落ちる（`ok` のケースを拒否にする）。
    #[test]
    fn fails_when_the_model_disagrees() {
        let (tsv, eyk) = head(20);
        let tsv = mutate(
            &tsv,
            "c00001\tcore\tp=-\t{}\tok\t",
            "c00001\tcore\tp=-\t{}\tE0505\t",
        );
        assert_fails(
            &compare(&tsv, &eyk, &[]),
            "c00001: モデルは E0505、実装は ok です",
        );
    }

    /// 一覧にある食い違いは通し、出力に出す。
    #[test]
    fn passes_with_a_listed_disagreement() {
        let (tsv, eyk) = head(20);
        let eyk = mutate(
            &eyk,
            "{\n    p.free();\n}\n\n// p=- { inspect p }",
            "{\n}\n\n// p=- { inspect p }",
        );
        let report = compare(&tsv, &eyk, &[X]);
        assert_passes(&report);
        assert!(
            report.notes.iter().any(|n| n.contains("1 個（#1）"))
                && report
                    .notes
                    .iter()
                    .any(|n| n.contains("c00004: モデルは E0504、実装は ok（例外：テスト。#1）")),
            "{:?}",
            report.notes
        );
    }

    #[test]
    fn fails_when_a_listed_case_agrees() {
        let (tsv, eyk) = head(20);
        assert_fails(&compare(&tsv, &eyk, &[X]), "c00004: 例外の一覧にありますが");
    }

    #[test]
    fn fails_when_a_listed_result_is_different() {
        let (tsv, eyk) = head(20);
        let eyk = mutate(
            &eyk,
            "{\n    p.free();\n}\n\n// p=- { inspect p }",
            "{\n    p.free();\n    p.free();\n}\n\n// p=- { inspect p }",
        );
        let tsv = mutate(
            &tsv,
            "c00004\tcore\tp=-\t{ free p }\tE0504\t",
            "c00004\tcore\tp=-\t{ free p }\tok\t",
        );
        assert_fails(
            &compare(&tsv, &eyk, &[Exception { got: "E0504", ..X }]),
            "c00004: 例外の一覧では実装は E0504 ですが、実際は E0504,E0504 です",
        );
    }

    #[test]
    fn fails_when_a_listed_case_is_not_in_the_table() {
        let (tsv, eyk) = head(20);
        let gone = Exception {
            case: "c99999",
            ..X
        };
        assert_fails(
            &compare(&tsv, &eyk, &[gone]),
            "c99999 は formal/oracle/linearity.tsv にないケース",
        );
    }

    #[test]
    fn fails_when_a_case_is_listed_twice() {
        let (tsv, eyk) = head(20);
        let eyk = mutate(
            &eyk,
            "{\n    p.free();\n}\n\n// p=- { inspect p }",
            "{\n}\n\n// p=- { inspect p }",
        );
        assert_fails(&compare(&tsv, &eyk, &[X, X]), "c00004 が二度あります");
    }

    /// 補助の関数の診断は、どのケースにも割り当てず失敗にする。
    #[test]
    fn fails_on_a_diagnostic_outside_the_cases() {
        let (tsv, eyk) = head(20);
        let eyk = mutate(
            &eyk,
            "fn consume(p: Ptr<u32>) uses p@ {\n    p.free();\n}",
            "fn consume(p: Ptr<u32>) uses p@ {\n}",
        );
        assert_fails(&compare(&tsv, &eyk, &[]), "ケースの外の診断 E0505");
    }

    #[test]
    fn fails_when_rows_and_functions_differ() {
        let (tsv, eyk) = head(20);
        let last = tsv.lines().last().unwrap().to_string();
        let short = mutate(&tsv, &format!("{last}\n"), "");
        assert_fails(&compare(&short, &eyk, &[]), "数が違います");
        let swapped = mutate(&tsv, "c00002\t", "c00099\t");
        assert_fails(
            &compare(&swapped, &eyk, &[]),
            "c00002 が formal/oracle/linearity.tsv の c00099 と対応しません",
        );
    }

    #[test]
    fn fails_on_an_unreadable_table() {
        let (tsv, eyk) = head(20);
        let broken = mutate(
            &tsv,
            "c00003\tcore\tp=-\t{ take p }\tE0505\t",
            "c00003\tcore\tp=-\t{ take p }\tbad\t",
        );
        assert_fails(
            &compare(&broken, &eyk, &[]),
            "linearity.tsv:4: 行が読めません",
        );
        let header = mutate(&tsv, "case\ttier", "kase\ttier");
        assert_fails(&compare(&header, &eyk, &[]), "見出しの行が読めません");
        assert_fails(
            &compare(
                "case\ttier\theader\tbody\tverdict\tpos\tline\tcol\n",
                &eyk,
                &[],
            ),
            "ケースがありません",
        );
    }

    #[test]
    fn fails_when_the_tables_are_not_tracked() {
        let mut t = TempRepo::new();
        t.write(TSV, &real(TSV));
        assert_fails(
            &t.run(run),
            "formal/oracle/linearity.eyk: 追跡されていません",
        );
    }
}
