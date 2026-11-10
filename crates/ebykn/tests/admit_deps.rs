//! 本体のない補題（`admit`）に届く項目は `verified` にしない（仕様 §9.1 の 3。#149 のレビューの B3）。
//! Verus は要らない：書き下しの辺と、Verus が全項目を見てエラーがなかったときの状態を確かめる。

use ebykn::verus::{Summary, statuses, verus_name};
use ebykn_check::{check, project};

/// `src` と `proof` を書き下し、Verus が全項目をエラーなしで検証したとして状態を求める。
fn status_of(src: &str, proof: &str) -> Vec<String> {
    let (p, mut diags) = project::single_with_proof(src, proof);
    let checked = check(&p);
    diags.extend(checked.diags.iter().cloned());
    assert!(diags.is_empty(), "{diags:?}");
    let out = ebykn_lower::lower(&p, &checked).unwrap();
    let seen: std::collections::BTreeSet<String> = out
        .files
        .iter()
        .flat_map(|f| f.items.iter().map(|(_, _, i)| verus_name(i)))
        .collect();
    statuses(
        &out.files,
        &out,
        &[],
        Some(&seen),
        Summary::Ran {
            verified: seen.len() as u64,
            errors: 0,
        },
        false,
    )
    .iter()
    .map(|s| s.line())
    .collect()
}

const SRC: &str = "lemma fn bogus() ensures false;\npub fn one() -> (r: u64) ensures r == 1 { 0 }\npub fn two() -> (r: u64) ensures r == 1 { one() }\npub fn other() -> u64 { 7 }\n";

#[test]
fn a_direct_call_to_a_lemma_without_a_body_is_unverified() {
    let proof = SRC.replace(
        "ensures r == 1 { 0 }",
        "ensures r == 1 { proof { bogus(); } 0 }",
    );
    let st = status_of(SRC, &proof);
    assert!(st.contains(&"admit single::bogus".to_string()), "{st:?}");
    assert!(
        st.contains(
            &"unverified single::one (depends on single::bogus, a lemma without a body)"
                .to_string()
        ),
        "{st:?}"
    );
    // 2 段の呼び出しも届く。
    assert!(
        st.contains(
            &"unverified single::two (depends on single::bogus, a lemma without a body)"
                .to_string()
        ),
        "{st:?}"
    );
    // 届かない項目は verified のまま。
    assert!(st.contains(&"verified single::other".to_string()), "{st:?}");
}

#[test]
fn without_the_proof_nothing_depends_on_the_lemma() {
    let st = status_of(SRC, SRC);
    assert!(st.contains(&"verified single::one".to_string()), "{st:?}");
    assert!(st.contains(&"verified single::two".to_string()), "{st:?}");
}

#[test]
fn broadcast_use_reaches_the_group_members() {
    let src = "spec fn f(x: int) -> int = x;\nlemma fn l(x: int) ensures f(x) == x;\npub fn g(x: u64) -> u64 { x }\npub fn h(x: u64) -> u64 { x }\n";
    // 関数の本体の中の `use lemmas`：その関数だけ。
    let proof = "spec fn f(x: int) -> int = x;\nlemma fn l(x: int) ensures #[trigger] f(x) == x;\nlemma group g_l { l }\npub fn g(x: u64) -> u64 { use lemmas g_l; x }\npub fn h(x: u64) -> u64 { x }\n";
    let st = status_of(src, proof);
    assert!(
        st.iter().any(|l| l.starts_with("unverified single::g ")),
        "{st:?}"
    );
    assert!(st.contains(&"verified single::h".to_string()), "{st:?}");
    // モジュールの直下の `use lemmas`：モジュールのすべての項目。
    let proof2 = "spec fn f(x: int) -> int = x;\nlemma fn l(x: int) ensures #[trigger] f(x) == x;\nlemma group g_l { l }\nuse lemmas g_l;\npub fn g(x: u64) -> u64 { x }\npub fn h(x: u64) -> u64 { x }\n";
    let st = status_of(src, proof2);
    assert!(
        st.iter().any(|l| l.starts_with("unverified single::g ")),
        "{st:?}"
    );
    assert!(
        st.iter().any(|l| l.starts_with("unverified single::h ")),
        "{st:?}"
    );
}

/// 値の位置のパス、マクロの引数、入れ子の関数の中からも届く（#149 の再レビューの N2）。
#[test]
fn values_macro_arguments_and_nested_items_reach_the_lemma() {
    let src = "lemma fn bogus() ensures false;\npub fn one() -> (r: u64) ensures r == 1 { 0 }\npub fn mac3() -> (r: bool) ensures r { matches!(one(), 1) }\npub fn val() -> (r: u64) ensures r == 1 { let h = one; h() }\npub fn outer() -> (r: u64) ensures r == 1 {\n    fn inner() -> (r: u64) ensures r == 1 { 0 }\n    inner()\n}\n";
    let proof = src
        .replace(
            "ensures r == 1 { 0 }\npub fn mac3",
            "ensures r == 1 { proof { bogus(); } 0 }\npub fn mac3",
        )
        .replace(
            "    fn inner() -> (r: u64) ensures r == 1 { 0 }",
            "    fn inner() -> (r: u64) ensures r == 1 { proof { bogus(); } 0 }",
        );
    assert_ne!(proof, src);
    let st = status_of(src, &proof);
    for item in ["one", "mac3", "val", "outer"] {
        assert!(
            st.iter()
                .any(|l| l.starts_with(&format!("unverified single::{item} "))),
            "{item}: {st:?}"
        );
    }
}
