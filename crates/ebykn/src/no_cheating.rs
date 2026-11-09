//! `ebykn verify` の 2 回目の実行の照合（#37）。
//!
//! `ebykn verify` は、本体のない `lemma fn` を `admit()` に、`unchecked fn` を
//! `#[verifier::external_body]` に書き下すので、1 回目の検証に `--no-cheating` を付けられない。その
//! ままでは、`.eyk` の検査を通らない経路（依存クレートのマクロの展開、`include!`、マクロが出す入れ子の
//! `verus!`）から入った `assume(false)` を前提に、何でも verified になる（#16 の調査）。
//!
//! そこで、Verus を `--no-cheating --no-verify` でもう一度走らせ、拒否が出た場所を、Ebykn が自分で出した
//! 前提の一覧（[`Output`] の `unchecked` と `admitted`）と照らし合わせる。一覧の項目の行の範囲にあり、
//! 項目の種類に合う文言の拒否が、項目ごとに一つだけ許される。それ以外のエラーは、拒否の文言かどうかに
//! よらず、紛れ込んだ前提として `verify` を失敗にする。`#[test]` の項目（`#[verifier::external]`）は
//! `--no-cheating` でも拒否されないので、許す拒否はない。
//!
//! `--no-verify` でも `--no-cheating` の拒否は出る（固定した Verus で確かめた。`OPEN.md`）。
//!
//! 捕まえるのは、Verus が `--no-cheating` で拒否するもの（`assume` / `admit` / `external_body` /
//! `assume_specification` など）だけ。`verus!` の外で依存クレートの derive や属性マクロが作る項目は、
//! Verus が検証しないだけで拒否しないので、ここでは捕まえない（#49）。

use ebykn_lower::Output;

use crate::verus::{Run, VerusError};

/// `unchecked fn` の `#[verifier::external_body]` への Verus の拒否の文言。
pub const EXTERNAL_BODY: &str =
    "`external_body`/`assume_specification` not allowed with `--no-cheating`";
/// 本体のない補題の `admit()` への Verus の拒否の文言。
pub const ADMIT: &str = "`assume`/`admit` not allowed with `--no-cheating`";

/// 照合の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cheating {
    /// Ebykn が出した前提のほかに拒否はない。
    Clean,
    /// Ebykn が出した前提の一覧にない拒否（紛れ込んだ前提）がある。
    Smuggled(Vec<VerusError>),
    /// 2 回目の実行の結果を読めず、紛れ込んだ前提がないと言えない。
    Unreadable(Unreadable),
    /// 1 回目の実行が検証の前に止まったので、2 回目を走らせていない（`verify` は別の理由で失敗する）。
    NotRun,
}

/// 2 回目の実行の結果を読めなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// Verus が失敗したのに、エラーの見出しを一つも読めなかった。
    NoErrors,
    /// cargo の数えたエラー（`due to N previous errors`）と、読み取った見出しの数が違う。
    ///
    /// cargo は、同じ位置の同じ文言のエラーを 1 つにまとめて出すが、件数はまとめる前の数のまま
    /// （固定した Verus で、`assume` を 2 つ出すマクロを 1 回呼んで確かめた）。そのため、Ebykn が
    /// 出した `admit()` と同じ位置・同じ文言の拒否が紛れ込むと、見出しは 1 つに見えるが、この件数の
    /// 食い違いで照合できないことになる。
    CountMismatch { counted: usize, read: usize },
    /// cargo がクレートを作り直した行（`Checking 名前 v..`）がなく、Verus がこのクレートを見たと
    /// 言えない。
    NotCompiled,
}

impl Cheating {
    /// `verify` を成功にしてよいか。
    pub fn clean(&self) -> bool {
        *self == Cheating::Clean
    }

    /// 項目の verified を信用できないか（紛れ込んだ前提があるか、ないと言えない）。
    pub fn taints(&self) -> bool {
        matches!(self, Cheating::Smuggled(_) | Cheating::Unreadable(_))
    }

    /// `--json` の `no_cheating.state` の値。
    pub fn state(&self) -> &'static str {
        match self {
            Cheating::Clean => "clean",
            Cheating::Smuggled(_) => "smuggled",
            Cheating::Unreadable(_) => "unreadable",
            Cheating::NotRun => "not_run",
        }
    }
}

impl Unreadable {
    pub fn text(self, ja: bool) -> String {
        match (self, ja) {
            (Unreadable::NoErrors, false) => {
                "`verus --no-cheating` failed without an error we could read".into()
            }
            (Unreadable::NoErrors, true) => {
                "`verus --no-cheating` が失敗しましたが、読めるエラーがありません".into()
            }
            (Unreadable::CountMismatch { counted, read }, false) => {
                format!("`verus --no-cheating` reported {counted} error(s), but {read} were read")
            }
            (Unreadable::CountMismatch { counted, read }, true) => format!(
                "`verus --no-cheating` のエラーは {counted} 件ですが、読めたのは {read} 件です"
            ),
            (Unreadable::NotCompiled, false) => {
                "`verus --no-cheating` did not rebuild the crate".into()
            }
            (Unreadable::NotCompiled, true) => {
                "`verus --no-cheating` がクレートを作り直していません".into()
            }
        }
    }
}

/// cargo のまとめの行（`could not compile `k` (lib) due to N previous error(s)`）の N。
pub fn counted_errors(text: &str) -> Option<usize> {
    text.lines().rev().find_map(|l| {
        let rest = l.split_once("could not compile")?.1;
        let n = rest.split_once("due to ")?.1.split(' ').next()?;
        n.parse().ok()
    })
}

/// 2 回目の実行（[`crate::verus::Mode::NoCheating`]）の結果を、Ebykn が出した前提と照らし合わせる。
pub fn judge(output: &Output, run: &Run) -> Cheating {
    if !run.compiled {
        return Cheating::Unreadable(Unreadable::NotCompiled);
    }
    if !run.success && run.errors.is_empty() {
        return Cheating::Unreadable(Unreadable::NoErrors);
    }
    if !run.success || !run.errors.is_empty() {
        let read = run.errors.len();
        match counted_errors(&run.text) {
            Some(counted) if counted == read => {}
            Some(counted) => {
                return Cheating::Unreadable(Unreadable::CountMismatch { counted, read });
            }
            None => return Cheating::Unreadable(Unreadable::CountMismatch { counted: 0, read }),
        }
    }
    // 項目ごとに、許す拒否を一つだけ数える。
    let mut used: Vec<&str> = Vec::new();
    let mut smuggled = Vec::new();
    for e in &run.errors {
        let allowed = e.at.as_ref().and_then(|(file, line)| {
            let item = output
                .files
                .iter()
                .filter(|f| f.rel == *file)
                .find_map(|f| {
                    f.items
                        .iter()
                        .find(|(s, end, _)| s <= line && line <= end)
                        .map(|(_, _, i)| i.as_str())
                })?;
            let expected = if output.unchecked.iter().any(|u| u == item) {
                EXTERNAL_BODY
            } else if output.admitted.iter().any(|a| a == item) {
                ADMIT
            } else {
                return None;
            };
            (e.message == expected && !used.contains(&item)).then_some(item)
        });
        match allowed {
            Some(item) => used.push(item),
            None => smuggled.push(e.clone()),
        }
    }
    if smuggled.is_empty() {
        Cheating::Clean
    } else {
        Cheating::Smuggled(smuggled)
    }
}

/// 紛れ込んだ前提の位置の表示。生成コードなら、プロジェクトのルートからのパスを字句的に正規化する
/// （`src/../../../x.in:1:2` は `build/verus/src/../../../x.in` で、`x.in:1:2` になる）。
pub fn location(e: &VerusError) -> String {
    match &e.loc {
        Some(l) if l.starts_with("src/") => {
            let (file, rest) = l.split_once(':').unwrap_or((l, ""));
            let file = normalize(&format!("build/verus/{file}"));
            if rest.is_empty() {
                file
            } else {
                format!("{file}:{rest}")
            }
        }
        Some(l) => l.clone(),
        None => "(no location)".into(),
    }
}

/// 相対パスの `.` と `..` を字句的に畳む（先頭を越える `..` は残す）。
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ebykn_lower::OutFile;

    /// #16 の調査の再現（`tests/golden/smuggled/` の書き下しに、依存クレートのマクロ、`include!`、
    /// 入れ子の `verus!` を差し込んだもの）で `cargo verus focus -- --no-cheating --no-verify` が出したもの。
    const SMUGGLED: &str = "    Checking smuggled v0.1.0 (/p/build/verus)
error: `external_body`/`assume_specification` not allowed with `--no-cheating`
  --> src/lib.rs:10:28
   |
10 | #[verifier::external_body] fn liar(x: u64) -> (r: u64)
   |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^

error: `assume`/`admit` not allowed with `--no-cheating`
  --> src/lib.rs:14:23
   |
14 | ensures x + 0 == x, { vstd::prelude::admit() }
   |                       ^^^^^^^^^^^^^^^^^^^^^^

error: `assume`/`admit` not allowed with `--no-cheating`
  --> src/lib.rs:19:19
   |
19 | ensures r == 1, { dep::sneaky_assume!(); 0 }
   |                   ^^^^^^^^^^^^^^^^^^^^^
   |
   = note: this error originates in the macro `dep::sneaky_assume` (in Nightly builds, run with -Z macro-backtrace for more info)

error: `assume`/`admit` not allowed with `--no-cheating`
 --> src/../../../src/evil.in:1:26
  |
1 | ::vstd::prelude::proof!{ assume(false); }
  |                          ^^^^^^

error: `assume`/`admit` not allowed with `--no-cheating`
  --> src/lib.rs:26:1
   |
26 | dep::nested!{}
   | ^^^^^^^^^^^^^^
   |
   = note: this error originates in the macro `dep::nested` (in Nightly builds, run with -Z macro-backtrace for more info)

error: could not compile `smuggled` (lib) due to 5 previous errors
";

    fn output() -> Output {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (10, 11, "k::liar".into()),
                (13, 14, "k::helper".into()),
                (16, 16, "k::t".into()),
                (18, 19, "k::a".into()),
                (21, 22, "k::b".into()),
                (24, 25, "k::c".into()),
            ],
        }];
        Output {
            files,
            admitted: vec!["k::helper".into()],
            unchecked: vec!["k::liar".into()],
            external: vec!["k::t".into()],
            verus_patterns: Default::default(),
        }
    }

    fn run(text: &str, success: bool) -> Run {
        let (text, errors) = crate::verus::read_diagnostics(text, false);
        Run {
            success,
            text,
            results: None,
            errors,
            checked: None,
            compiled: true,
        }
    }

    #[test]
    fn smuggled_assumptions_are_found() {
        let Cheating::Smuggled(found) = judge(&output(), &run(SMUGGLED, false)) else {
            panic!("紛れ込んだ前提を見つけていない");
        };
        let locs: Vec<String> = found.iter().map(location).collect();
        assert_eq!(
            locs,
            [
                "build/verus/src/lib.rs:19:19",
                "src/evil.in:1:26",
                "build/verus/src/lib.rs:26:1",
            ]
        );
        // 診断の全文はエラーごとに分かれ、許した前提の診断を含まない。
        assert!(
            found[0].block.contains("dep::sneaky_assume"),
            "{}",
            found[0].block
        );
        assert!(found.iter().all(|e| !e.block.contains("fn liar")));
        assert!(!found[2].block.contains("could not compile"));
    }

    #[test]
    fn locations_are_normalized() {
        assert_eq!(normalize("build/verus/src/../../../x.in"), "x.in");
        assert_eq!(normalize("build/verus/src/../../../../x.in"), "../x.in");
        assert_eq!(
            normalize("build/verus/./src/lib.rs"),
            "build/verus/src/lib.rs"
        );
    }

    #[test]
    fn errors_without_a_generated_location_count_as_smuggled() {
        // `-->` の行のないエラーと、生成コードの外（絶対パス）の位置のエラーは、項目に当たらない。
        for raw in [
            "error: `assume`/`admit` not allowed with `--no-cheating`\n\nerror: could not compile `k` (lib) due to 1 previous error\n",
            "error: `assume`/`admit` not allowed with `--no-cheating`\n --> /abs/x.rs:13:1\n\nerror: could not compile `k` (lib) due to 1 previous error\n",
        ] {
            let r = run(raw, false);
            assert_eq!(r.errors.len(), 1, "{raw}");
            assert_eq!(r.errors[0].at, None, "{raw}");
            let Cheating::Smuggled(v) = judge(&output(), &r) else {
                panic!("通してしまった: {raw}");
            };
            assert_eq!(v.len(), 1);
        }
    }

    #[test]
    fn fails_when_cargo_merged_identical_errors() {
        // cargo は同じ位置・同じ文言のエラーを 1 つにまとめるが、件数はまとめる前の数のまま。
        // `assume` を 2 つ出すマクロ（`dep::twice!()`）を 19 行目で呼ぶと、見出しは 5 つで件数は 6 に
        // なった。Ebykn の admit と同じ位置に紛れ込んだ assume も、この食い違いで照合できないことになる。
        let merged = SMUGGLED
            .replace("dep::sneaky_assume!();", "dep::twice!();")
            .replace("due to 5 previous", "due to 6 previous");
        assert!(merged.contains("dep::twice!()") && merged.contains("due to 6 previous"));
        assert_eq!(
            judge(&output(), &run(&merged, false)),
            Cheating::Unreadable(Unreadable::CountMismatch {
                counted: 6,
                read: 5
            })
        );
    }

    #[test]
    fn fails_when_the_crate_was_not_rebuilt() {
        let mut r = run(&own_only(), false);
        r.compiled = false;
        assert_eq!(
            judge(&output(), &r),
            Cheating::Unreadable(Unreadable::NotCompiled)
        );
        assert!(crate::verus::compiled(
            "    Checking smuggled v0.1.0 (/p)\n",
            "smuggled"
        ));
        assert!(!crate::verus::compiled(
            "    Checking smuggled2 v0.1.0 (/p)\n",
            "smuggled"
        ));
        assert!(!crate::verus::compiled(
            "    Finished `dev` profile\n",
            "smuggled"
        ));
    }

    /// Ebykn が自分で出した前提（`unchecked` の external_body と、本体のない補題の admit）だけなら通る。
    fn own_only() -> String {
        let cut = SMUGGLED
            .find("error: `assume`/`admit` not allowed with `--no-cheating`\n  --> src/lib.rs:19")
            .unwrap();
        format!(
            "{}error: could not compile `smuggled` (lib) due to 2 previous errors\n",
            &SMUGGLED[..cut]
        )
    }

    #[test]
    fn own_assumptions_are_accepted() {
        assert_eq!(judge(&output(), &run(&own_only(), false)), Cheating::Clean);
        assert_eq!(judge(&output(), &run("", true)), Cheating::Clean);
    }

    #[test]
    fn fails_when_the_rejection_is_not_the_items_kind() {
        // unchecked の項目の中の admit、補題の中の external_body、#[test] の項目の中の拒否は許さない。
        let mut o = output();
        std::mem::swap(&mut o.admitted, &mut o.unchecked);
        assert!(matches!(
            judge(&o, &run(&own_only(), false)),
            Cheating::Smuggled(v) if v.len() == 2
        ));
        let mut o = output();
        o.unchecked.clear();
        o.external = vec!["k::liar".into()];
        assert!(matches!(
            judge(&o, &run(&own_only(), false)),
            Cheating::Smuggled(v) if v.len() == 1
        ));
    }

    #[test]
    fn fails_when_an_item_absorbs_two_rejections() {
        let twice = own_only().replacen(
            "error: could not compile `smuggled` (lib) due to 2 previous errors\n",
            "error: `assume`/`admit` not allowed with `--no-cheating`\n  --> src/lib.rs:13:1\n\nerror: could not compile `smuggled` (lib) due to 3 previous errors\n",
            1,
        );
        assert_ne!(twice, own_only());
        assert!(matches!(
            judge(&output(), &run(&twice, false)),
            Cheating::Smuggled(v) if v.len() == 1
        ));
    }

    #[test]
    fn fails_when_the_run_is_unreadable() {
        assert_eq!(
            judge(&output(), &run("error: could not compile `k`\n", false)),
            Cheating::Unreadable(Unreadable::NoErrors)
        );
        let miscounted = own_only().replace("due to 2 previous", "due to 3 previous");
        assert_ne!(miscounted, own_only());
        assert_eq!(
            judge(&output(), &run(&miscounted, false)),
            Cheating::Unreadable(Unreadable::CountMismatch {
                counted: 3,
                read: 2
            })
        );
        let uncounted = own_only().replace(
            "error: could not compile `smuggled` (lib) due to 2 previous errors\n",
            "",
        );
        assert_ne!(uncounted, own_only());
        assert!(matches!(
            judge(&output(), &run(&uncounted, false)),
            Cheating::Unreadable(_)
        ));
    }

    #[test]
    fn other_errors_count_as_smuggled() {
        let other = "error[E0425]: cannot find value `x` in this scope\n --> src/lib.rs:11:1\n\nerror: could not compile `k` (lib) due to 1 previous error\n";
        assert!(matches!(
            judge(&output(), &run(other, false)),
            Cheating::Smuggled(v) if v.len() == 1
        ));
    }
}
