//! Verus の出力から項目の状態を決める（`--time` の関数ごとの成否と、エラーの失敗の場所の項目への
//! 割り当て。#79）。

use ebykn_lower::{OutFile, Output};

use super::{ErrorSites, ItemStatus, Summary, verus_name};

/// `--time` の JSON の `times-ms.smt.smt-run-module-times[].function-breakdown[]` から、`success` が
/// `false` の関数の名前（`func-details` の鍵と同じ形）を集める（#79）。
///
/// `success` は Verus の `func_fails`（関数の検証の問い合わせが失敗した関数）から作られる。`errors` は
/// 失敗した問い合わせの数で、一つの関数の `ensures` が二つ成り立たなくても 1 と数える（エラーの見出しは
/// 2 つ）。bit_vector の問い合わせの失敗は `errors` に数えるが `func_fails` に入れないので、`errors` と
/// この集合の大きさが違えば、[`statuses`] は割り当てられないとみなす。
///
/// 項目が一つでも読めなければ `None`（読めた分だけで決めない）。
pub fn failed_functions(v: &serde_json::Value) -> Option<std::collections::BTreeSet<String>> {
    let modules = v
        .get("times-ms")?
        .get("smt")?
        .get("smt-run-module-times")?
        .as_array()?;
    let mut failed = std::collections::BTreeSet::new();
    for m in modules {
        for f in m.get("function-breakdown")?.as_array()? {
            let name = f.get("function")?.as_str()?;
            if !f.get("success")?.as_bool()? {
                failed.insert(name.to_string());
            }
        }
    }
    Some(failed)
}

/// 一つの項目の事実。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemFacts {
    /// 本体のない `lemma fn`（`admit()`）。
    pub admitted: bool,
    pub unchecked: bool,
    /// `#[test]`（`#[verifier::external]`。Verus は見ない）。
    pub external: bool,
    /// Verus がこの項目の関数を失敗とした（[`failed_functions`]）。成否が読めないか Verus の名前が決まらない
    /// 項目では、Verus のエラーの失敗の場所がこの項目の行にある（[`ErrorSites::blamed`]）。
    pub error: bool,
    /// Verus のエラーがこの項目の行を副の位置として指す（[`ErrorSites::related`]。PR #76 のレビューの B1）か、
    /// 失敗の場所がこの項目の行にあるのに Verus がこの項目の関数を通した（#79）。
    pub related: bool,
    /// Verus の `func-details` にこの項目の関数がある（Verus が見た）。
    pub checked: bool,
    /// どの項目にも割り当てられないエラーがある（生成コードの外、位置がない、項目の外）か、Verus の関数ごと
    /// の成否とエラーの失敗の場所が合わない（[`statuses`] の 3 つの条件。#79）。
    pub stray_errors: bool,
    /// Verus が検証の前に止まった（[`Summary::Stopped`]）。`func-details` に関数があっても、検証はしていない。
    pub stopped: bool,
    /// `--no-cheating` の照合で、Ebykn が出した前提のほかの前提が見つかったか、ないと言えない
    /// （[`crate::no_cheating`]）。どの項目がそれに頼るか分からないので、どの項目も verified にしない。
    pub tainted: bool,
}

/// 項目の状態を決める（PR #26 のレビューの方針）。`verified` と表示してよいのは、Verus が見た関数
/// （`func-details` にある）で、その項目にエラーがなく、どの項目にも割り当てられないエラーがないときだけ。
///
/// - `admit` / `unchecked` / `external`：証明なしの前提。`verify` は必ず一覧に出す。前提の状態はエラーより
///   優先する（その項目にエラーがあっても `failed` にしない。エラーの場所は、標準エラーに出す Verus の診断の
///   生成コードの位置で示される）。
/// - `failed`：その項目にエラーがある。
/// - `unknown`：Verus が見なかった（`#[cfg]` で消えた、など）か、エラーが副の位置としてその項目を指すか、
///   割り当てられないエラーがあるか、
///   Verus が検証の前に止まった（コンパイルエラーなど。#43）か、紛れ込んだ前提がある
///   （[`ItemFacts::tainted`]、#37）。`verify` を成功にしない。
pub fn decide(f: ItemFacts) -> &'static str {
    if f.admitted {
        "admit"
    } else if f.unchecked {
        "unchecked"
    } else if f.external {
        "external"
    } else if f.error {
        "failed"
    } else if !f.checked || f.related || f.stray_errors || f.stopped || f.tainted {
        "unknown"
    } else {
        "verified"
    }
}

/// 全項目の状態を決める。`checked` は Verus の `func-details` の鍵（`None` なら一覧がない）。`failed` は
/// Verus が失敗とした関数（[`failed_functions`]。`None` なら読めない）。[`Summary::Stopped`] なら何も検証
/// していない。`tainted` は [`ItemFacts::tainted`]（`--no-cheating` の照合の結果）。
///
/// [`Summary::Ran`] でエラーがあるとき、次のすべてが成り立てば、エラーを項目に割り当てられたとみなす
/// （#79）。一つでも崩れれば、失敗していない項目をすべて unknown にする。
///
/// - `failed` が読めて、その数が Verus の数えたエラー（失敗した問い合わせ）の数と同じ。
/// - 各エラーの失敗の場所の項目が、どれも `failed` にある（Verus が通した関数のせいにしていない）。
/// - `failed` の各関数に、失敗の場所を持つエラーがある（エラーを読み落としていない。呼ばれる側の行しか
///   出ないエラーで、失敗した呼び出し元を見落としていない）。
///
/// エラーの見出しの数とは比べない。一つの関数の `ensures` が二つ成り立たなければ、見出しは 2 つで、
/// Verus の数えるエラーは 1 つになる。
pub fn statuses(
    files: &[OutFile],
    output: &Output,
    errors: &[ErrorSites],
    checked: Option<&std::collections::BTreeSet<String>>,
    failed: Option<&std::collections::BTreeSet<String>>,
    summary: Summary,
    tainted: bool,
) -> Vec<ItemStatus> {
    let (error_count, stopped) = match summary {
        Summary::Ran { errors, .. } => (Some(errors), false),
        Summary::Stopped { .. } => (None, true),
    };
    // 項目の Verus の名前。パスで決まらない項目（クレートの外の型への impl のメソッド）は、形に当たる
    // 鍵がちょうど一つのときだけ決まる。
    let verus_of = |item: &str| -> Option<String> {
        match output.verus_patterns.get(item) {
            Some(pat) => {
                let (pre, post) = pat.split_once('*')?;
                let hits: Vec<&String> = checked?
                    .iter()
                    .filter(|k| {
                        k.strip_prefix(pre)
                            .and_then(|r| r.strip_suffix(post))
                            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                    })
                    .collect();
                match hits.as_slice() {
                    [k] => Some((*k).clone()),
                    _ => None,
                }
            }
            None => Some(verus_name(item)),
        }
    };
    let names: Vec<(String, Option<String>)> = files
        .iter()
        .flat_map(|f| f.items.iter().map(|(_, _, i)| (i.clone(), verus_of(i))))
        .collect();
    // 同じ Verus の名前になる項目が二つ以上あれば、どれの証拠か分からないので数えない。
    let shared = |n: &Option<String>| match n {
        Some(n) => names.iter().filter(|(_, m)| m.as_ref() == Some(n)).count() > 1,
        None => false,
    };
    // 行を範囲に含む項目。
    let items_at = |file: &str, line: usize| -> Vec<&String> {
        files
            .iter()
            .filter(|f| f.rel == file)
            .flat_map(|f| f.items.iter())
            .filter(|(s, e, _)| *s <= line && line <= *e)
            .map(|(_, _, i)| i)
            .collect()
    };
    // 失敗の場所がないか、項目の外にあるエラーは、どの項目にも割り当てられない。
    let outside = errors.iter().any(|e| {
        e.blamed.is_empty()
            || e.blamed
                .iter()
                .any(|(file, line)| items_at(file, *line).is_empty())
    });
    // 失敗した関数とエラーの失敗の場所が、両向きに合うか（上の 3 つの条件）。
    let blamed_names: Vec<Option<String>> = errors
        .iter()
        .flat_map(|e| e.blamed.iter())
        .flat_map(|(file, line)| items_at(file, *line))
        .map(|i| verus_of(i))
        .collect();
    let attributed = match (error_count, failed) {
        (None, _) => true,
        (Some(n), None) => n == 0 && errors.is_empty(),
        (Some(n), Some(fails)) => {
            n == fails.len() as u64
                && blamed_names
                    .iter()
                    .all(|b| b.as_ref().is_some_and(|b| fails.contains(b)))
                && fails
                    .iter()
                    .all(|f| blamed_names.iter().any(|b| b.as_ref() == Some(f)))
        }
    };
    let stray = outside || !attributed;
    let mut out = Vec::new();
    for f in files {
        for (start, end, item) in &f.items {
            let here =
                |(file, line): &(String, usize)| *file == f.rel && start <= line && line <= end;
            let name = verus_of(item);
            let seen = !shared(&name)
                && name
                    .as_ref()
                    .is_some_and(|n| checked.is_some_and(|c| c.contains(n)));
            let blamed = errors.iter().any(|e| e.blamed.iter().any(here));
            // 関数ごとの成否が読めて、Verus が見た関数なら、成否は Verus に従う。失敗の場所がこの項目に
            // あっても Verus が通した関数なら、failed ではなく unknown にする（related）。
            let verdict = match (failed, &name) {
                (Some(f), Some(n)) if seen && !stopped => Some(f.contains(n)),
                _ => None,
            };
            let error = verdict.unwrap_or(blamed);
            let related = errors.iter().any(|e| e.related.iter().any(here))
                || (blamed && verdict == Some(false));
            let facts = ItemFacts {
                admitted: output.admitted.contains(item),
                unchecked: output.unchecked.contains(item),
                external: output.external.contains(item),
                error,
                related,
                checked: seen,
                stray_errors: stray,
                stopped,
                tainted,
            };
            out.push(ItemStatus {
                item: item.clone(),
                status: decide(facts),
                via: None,
            });
        }
        // 依存クレートの derive と属性マクロが作る項目は、Verus が検証しない（`verus!` の外の項目に
        // なる）。証明なしの前提として、同じ一覧に必ず出す（#49）。
        for m in output.macros.iter().filter(|m| m.rel == f.rel) {
            out.push(ItemStatus {
                item: m.item.clone(),
                status: "macro",
                via: Some((m.kind, m.path.clone())),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verus::tests::{names, ran};
    use crate::verus::{parse_json, read_diagnostics};

    fn facts() -> ItemFacts {
        ItemFacts {
            admitted: false,
            unchecked: false,
            external: false,
            error: false,
            related: false,
            checked: true,
            stray_errors: false,
            stopped: false,
            tainted: false,
        }
    }

    #[test]
    fn verified_needs_verus_evidence() {
        assert_eq!(decide(facts()), "verified");
        // Verus が見なかった（#[cfg] で消えた、func-details にない）。
        assert_eq!(
            decide(ItemFacts {
                checked: false,
                ..facts()
            }),
            "unknown"
        );
        // エラーが副の位置としてこの項目を指す（PR #76 のレビューの B1）。
        assert_eq!(
            decide(ItemFacts {
                related: true,
                ..facts()
            }),
            "unknown"
        );
        // 割り当てられないエラーがある。
        assert_eq!(
            decide(ItemFacts {
                stray_errors: true,
                ..facts()
            }),
            "unknown"
        );
        // Verus が検証の前に止まった（#43）。func-details にあっても verified にしない。
        assert_eq!(
            decide(ItemFacts {
                stopped: true,
                ..facts()
            }),
            "unknown"
        );
        // 紛れ込んだ前提がある（#37）。
        assert_eq!(
            decide(ItemFacts {
                tainted: true,
                ..facts()
            }),
            "unknown"
        );
        assert_eq!(
            decide(ItemFacts {
                error: true,
                ..facts()
            }),
            "failed"
        );
        assert_eq!(
            decide(ItemFacts {
                error: true,
                stopped: true,
                ..facts()
            }),
            "failed"
        );
        assert_eq!(
            decide(ItemFacts {
                external: true,
                ..facts()
            }),
            "external"
        );
        assert_eq!(
            decide(ItemFacts {
                unchecked: true,
                error: true,
                ..facts()
            }),
            "unchecked"
        );
        assert_eq!(
            decide(ItemFacts {
                admitted: true,
                ..facts()
            }),
            "admit"
        );
    }

    /// #79 の再現（`two_errors`）の生成コードの項目と、Verus が見た関数。
    fn two_errors() -> (Vec<OutFile>, Output, std::collections::BTreeSet<String>) {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (10, 12, "two_errors::ok".into()),
                (14, 16, "two_errors::bad".into()),
            ],
        }];
        let output = Output {
            files: files.clone(),
            ..Default::default()
        };
        (files, output, names(&["two_errors::ok", "two_errors::bad"]))
    }

    fn got(st: &[ItemStatus]) -> Vec<&'static str> {
        st.iter().map(|s| s.status).collect()
    }

    /// #79：一つの関数の `ensures` が二つ成り立たないと、見出しは 2 つで Verus の `errors` は 1。
    /// Verus が通したほかの関数まで unknown にしない。
    #[test]
    fn several_errors_in_one_function_leave_others_verified() {
        let (files, output, checked) = two_errors();
        let raw = "error: postcondition not satisfied
  --> build/verus/src/lib.rs:15:9（生成コード）
   |
15 | ensures r == 1, r == 2, { x }
   |         ^^^^^^            - at the end of the function body
   |         |
   |         failed this postcondition

error: postcondition not satisfied
  --> build/verus/src/lib.rs:15:17（生成コード）
   |
15 | ensures r == 1, r == 2, { x }
   |                 ^^^^^^    - at the end of the function body
   |                 |
   |                 failed this postcondition

error: could not compile `two_errors` (lib) due to 2 previous errors
";
        let (_, errors) = read_diagnostics(raw, false);
        let sites: Vec<ErrorSites> = errors.iter().map(|e| e.sites.clone()).collect();
        assert_eq!(sites.len(), 2);
        let bad = names(&["two_errors::bad"]);
        let st = statuses(
            &files,
            &output,
            &sites,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["verified", "failed"]);
    }

    /// #79 のコメント：出力に呼ばれる側の行しか出ないエラーで、Verus が失敗とした呼び出し元を verified に
    /// しない。エラーが Verus の通した関数を指せば、割り当てられないとみなす。見出しの数がたまたま合っても
    /// （見出し 1 つ、`errors` 1）同じ。
    #[test]
    fn errors_blaming_functions_verus_passed_are_not_attributed() {
        let (files, output, checked) = two_errors();
        let bad = names(&["two_errors::bad"]);
        // エラーは ok の行だけを指し、Verus が失敗としたのは bad。
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::at("src/lib.rs", 11)],
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 見出しが 2 つでも同じ（Verus が失敗とした関数を指すエラーがない）。
        let twice = [
            ErrorSites::at("src/lib.rs", 11),
            ErrorSites::at("src/lib.rs", 12),
        ];
        let st = statuses(
            &files,
            &output,
            &twice,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 成否が読めなければ、今までどおりエラーの場所で決める（ok が failed、bad は unknown）。
        let st = statuses(&files, &output, &twice, Some(&checked), None, ran(1), false);
        assert_eq!(got(&st), ["failed", "unknown"]);
        // Verus の数えたエラーがあるのに失敗とした関数が一つもなければ、どれも verified にしない。
        let none = names(&[]);
        let st = statuses(
            &files,
            &output,
            &[ErrorSites::at("src/lib.rs", 15)],
            Some(&checked),
            Some(&none),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "unknown"]);
    }

    /// 条件 2：数が合い、Verus が失敗とした関数を指すエラーもあるが、別のエラーが Verus の通した関数を
    /// 指す。そのエラーの持ち主が分からないので、ほかの項目も verified にしない（PR #142 のレビュー）。
    #[test]
    fn an_error_blaming_a_passed_function_makes_the_rest_unknown() {
        let files = vec![OutFile {
            rel: "src/lib.rs".into(),
            text: String::new(),
            items: vec![
                (10, 12, "k::ok".into()),
                (14, 16, "k::bad".into()),
                (18, 20, "k::other".into()),
            ],
        }];
        let output = Output {
            files: files.clone(),
            ..Default::default()
        };
        let checked = names(&["k::ok", "k::bad", "k::other"]);
        let bad = names(&["k::bad"]);
        let sites = [
            ErrorSites::at("src/lib.rs", 15),
            ErrorSites::at("src/lib.rs", 11),
        ];
        let st = statuses(
            &files,
            &output,
            &sites,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed", "unknown"]);
        // 余分なエラーがなければ、other は verified（壊す操作が当たったことを確かめる）。
        let st = statuses(
            &files,
            &output,
            &sites[..1],
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["verified", "failed", "verified"]);
    }

    /// 数と名前がほかで合っていても、位置のないエラーや項目の外のエラーが余分にあれば、割り当てられない
    /// とみなす（`outside`。PR #142 のレビュー）。
    #[test]
    fn extra_errors_without_an_item_make_the_rest_unknown() {
        let (files, output, checked) = two_errors();
        let bad = names(&["two_errors::bad"]);
        for extra in [ErrorSites::default(), ErrorSites::at("src/lib.rs", 100)] {
            let sites = [ErrorSites::at("src/lib.rs", 15), extra.clone()];
            let st = statuses(
                &files,
                &output,
                &sites,
                Some(&checked),
                Some(&bad),
                ran(1),
                false,
            );
            assert_eq!(got(&st), ["unknown", "failed"], "{extra:?}");
        }
    }

    /// 割り当てを決められないときは、失敗していない項目を unknown にする（#79 のコメントの条件）。
    #[test]
    fn unattributable_errors_make_the_rest_unknown() {
        let (files, output, checked) = two_errors();
        let bad = names(&["two_errors::bad"]);
        let both = names(&["two_errors::ok", "two_errors::bad"]);
        let at_bad = [ErrorSites::at("src/lib.rs", 15)];
        // Verus が失敗とした関数のエラーを読み落とした（ok の見出しがない）。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&both),
            ran(2),
            false,
        );
        assert_eq!(got(&st), ["failed", "failed"]);
        // Verus の数えたエラーが、失敗とした関数の数と違う（bit_vector の問い合わせの失敗は
        // `func_fails` に入らない）。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&bad),
            ran(2),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 関数ごとの成否が読めない。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            None,
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // Verus が失敗とした関数が、どの項目でもない。
        let other = names(&["two_errors::bad", "two_errors::hidden"]);
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&other),
            ran(2),
            false,
        );
        assert_eq!(got(&st), ["unknown", "failed"]);
        // 同じ場合で、照合が成り立てば ok は verified（壊す操作が当たったことを確かめる）。
        let st = statuses(
            &files,
            &output,
            &at_bad,
            Some(&checked),
            Some(&bad),
            ran(1),
            false,
        );
        assert_eq!(got(&st), ["verified", "failed"]);
        // エラーがなく、成否が読めなくても verified。
        let st = statuses(&files, &output, &[], Some(&checked), None, ran(0), false);
        assert_eq!(got(&st), ["verified", "verified"]);
    }

    /// `--time` の JSON から、Verus が失敗とした関数を読む（固定した Verus の出力を縮めたもの）。
    #[test]
    fn reads_failed_functions_from_the_time_breakdown() {
        let json = r#"{"func-details":{"two_errors::bad":{},"two_errors::ok":{}},
            "verification-results":{"success":false,"verified":1,"errors":1},
            "times-ms":{"smt":{"smt-run-module-times":[{"module":"","function-breakdown":[
                {"function":"two_errors::bad","time":0,"success":false},
                {"function":"two_errors::ok","time":0,"success":true}]}]}}}"#;
        let j = parse_json(json).unwrap();
        assert_eq!(j.failed, Some(names(&["two_errors::bad"])));
        assert_eq!(j.results, Some((1, 1)));
        // 一つでも読めない項目があれば、読めた分で決めない。
        let broken = json.replace(r#""success":true"#, r#""success":"yes""#);
        assert_eq!(parse_json(&broken).unwrap().failed, None);
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut no_success = v.clone();
        no_success["times-ms"]["smt"]["smt-run-module-times"][0]["function-breakdown"][1]
            .as_object_mut()
            .unwrap()
            .remove("success");
        assert_eq!(failed_functions(&no_success), None);
        // `--time` の出力がなければ読めない。
        let mut no_time = v;
        no_time.as_object_mut().unwrap().remove("times-ms");
        assert_eq!(failed_functions(&no_time), None);
    }
}
