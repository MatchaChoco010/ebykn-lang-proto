//! 剥がし一致検査（仕様 §6.1、E0304・E0305）。

use ebykn_check::{check, project, sort_diags};
use ebykn_syntax::diag::Lang;

/// `src/lib.eyk` と `proof/lib.eykp` を検査した診断（英語）。
fn run(src: &str, proof: &str) -> Vec<String> {
    let (p, mut diags) = project::single_with_proof(src, proof);
    diags.extend(check(&p).diags);
    sort_diags(&mut diags);
    diags
        .iter()
        .map(|d| d.render(p.file(d.span.file), Lang::En))
        .collect()
}

#[track_caller]
fn matches(src: &str, proof: &str) {
    let d = run(src, proof);
    assert!(
        d.is_empty(),
        "診断が出た:\n{}\n--- src\n{src}\n--- proof\n{proof}",
        d.join("\n")
    );
}

/// ちょうど一つの診断が出て、それが `want`（`ファイル:行:列: コード` で始まる）で、文言が `needles` を含む。
#[track_caller]
fn rejects(src: &str, proof: &str, want: &str, needles: &[&str]) {
    let d = run(src, proof);
    assert_eq!(d.len(), 1, "診断が一つではない:\n{}", d.join("\n"));
    assert!(d[0].starts_with(want), "{want:?} で始まらない: {}", d[0]);
    for n in needles {
        assert!(d[0].contains(n), "{n:?} を含まない: {}", d[0]);
    }
}

/// `text` の `from` を `to` に置き換える。`from` がなければ失敗する（何も変えない置き換えで、
/// 通るテストを作らない）。
#[track_caller]
fn replace(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "{from:?} がない");
    text.replacen(from, to, 1)
}

const SRC: &str = "\
pub spec fn double(x: int) -> int = x + x;

lemma fn double_pos(x: int)
    requires x > 0
    ensures double(x) > 0;

pub fn sum(n: u64) -> (r: u64)
    requires n < 100
    ensures r == n
{
    let mut i = 0;
    while i < n
        invariant i <= n
        decreases n - i
    {
        i = i + 1;
    }
    i
}
";

/// 証明構文を一通り足したもの。剥がすと `SRC` に一致する。
const PROOF: &str = "\
/// 証明ファイルだけの doc コメント
#[opaque]
pub spec fn double(x: int) -> int = x + x;

spec fn triple(x: int) -> int = x + x + x;

lemma fn double_pos(x: int)
    by(nonlinear_arith)
    requires x > 0
    ensures double(x) > 0
{
    reveal(double);
    assert double(x) == x + x;
}

lemma fn helper(x: int) ensures triple(x) == x + double(x) {
    reveal(double);
}

lemma group basics { double_pos, helper }

use lemmas basics;

#[rlimit(20)]
pub fn sum(n: u64) -> (r: u64)
    requires n < 100
    ensures r == n
{
    let mut i = 0; // 比べない行のコメント
    spec let start = i;
    while i < n
        invariant i <= n
        invariant start == 0, forall k: int, #![auto] k == k
        decreases n - i
    {
        proof { helper(i as int); }
        i = i + 1;
        assert i <= n;
    }
    use lemmas basics;
    assert forall k in 0..i, k < n by { };
    assert i == n by(nonlinear_arith) requires i == n;
    hide(double);
    i
}
";

#[test]
fn stripping_the_proofs_gives_the_source() {
    matches(SRC, PROOF);
}

#[test]
fn whitespace_comments_attribute_order_and_trailing_commas_are_ignored() {
    let src = "#[inline] #[must_use] pub fn f(x: u8, y: u8) -> u8 requires x < 10 ensures x == x { [x, y][0] }\n";
    let proof = "\
// コメント
#[must_use]
#[inline]
pub fn f(
    x: u8,
    y: u8,
) -> u8
    requires x < 10,
    ensures x == x,
{
    [x, y,][0]
}
";
    matches(src, proof);
}

#[test]
fn a_one_element_tuple_keeps_its_comma() {
    rejects(
        "fn f() -> (u8,) { (1,) }\n",
        "fn f() -> (u8,) { (1) }\n",
        "proof/lib.eykp:1:21: E0304",
        &["in `fn f`", "here it has `)`", "`src/lib.eyk:1:19` has `,`"],
    );
}

#[test]
fn macro_arguments_are_compared_as_written() {
    rejects(
        "fn f() -> Vec<u8> { vec![1, 2,] }\n",
        "fn f() -> Vec<u8> { vec![1, 2] }\n",
        "proof/lib.eykp:1:30: E0304",
        &["here it has `]`", "`src/lib.eyk:1:21` has `,`"],
    );
}

#[test]
fn a_changed_contract_is_e0304_at_the_token() {
    let proof = replace(PROOF, "    requires n < 100\n", "    requires n < 1000\n");
    rejects(
        SRC,
        &proof,
        "proof/lib.eykp:26:18: E0304",
        &["in `fn sum`", "`1000`", "`src/lib.eyk:8:18` has `100`"],
    );
}

#[test]
fn an_added_statement_is_e0304_at_the_statement() {
    let proof = replace(
        PROOF,
        "        i = i + 1;\n",
        "        i = i + 1;\n        i = i + 0;\n",
    );
    rejects(
        SRC,
        &proof,
        "proof/lib.eykp:38:9: E0304",
        &["in `fn sum`", "`src/lib.eyk:17:5` has `}`"],
    );
}

#[test]
fn a_changed_statement_in_a_nested_block_is_reported_at_that_statement() {
    let proof = replace(PROOF, "        i = i + 1;\n", "        i = i + 2;\n");
    rejects(
        SRC,
        &proof,
        "proof/lib.eykp:37:9: E0304",
        &["in `fn sum`", "`src/lib.eyk:16:9` has `1`", "`2`"],
    );
}

#[test]
fn a_removed_item_is_e0304() {
    let proof = replace(
        PROOF,
        "#[opaque]\npub spec fn double(x: int) -> int = x + x;\n",
        "",
    );
    let d = run(SRC, &proof);
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].contains("E0304"), "{d:?}");
}

#[test]
fn a_deleted_loop_clause_is_e0305() {
    let proof = replace(PROOF, "        decreases n - i\n", "");
    rejects(
        SRC,
        &proof,
        "proof/lib.eykp:31:5: E0305",
        &["`decreases n - i`", "src/lib.eyk:14:19"],
    );
}

#[test]
fn reordered_loop_clauses_are_e0305() {
    let src = replace(
        SRC,
        "        invariant i <= n\n        decreases n - i\n",
        "        invariant i <= n, i >= 0\n        decreases n - i\n",
    );
    // 同じ種類の句は連結して読むので、句を分けても式の順が同じなら部分列（仕様 §5.2）。
    let split = replace(
        PROOF,
        "        invariant i <= n\n",
        "        invariant i <= n\n        invariant i >= 0\n",
    );
    matches(&src, &split);
    let swapped = replace(
        PROOF,
        "        invariant i <= n\n",
        "        invariant i >= 0\n        invariant i <= n\n",
    );
    rejects(
        &src,
        &swapped,
        "proof/lib.eykp:31:5: E0305",
        &["`invariant i >= 0`"],
    );
}

#[test]
fn a_loop_clause_added_to_the_source_only_is_e0305() {
    let src = replace(
        SRC,
        "        decreases n - i\n",
        "        decreases n - i\n        invariant true\n",
    );
    rejects(
        &src,
        PROOF,
        "proof/lib.eykp:31:5: E0305",
        &["`invariant true`"],
    );
}

#[test]
fn module_structure_must_match() {
    let src = "mod a { pub fn f() {} }\nmod b;\n";
    matches(src, "mod a {\n    pub fn f() { proof { } }\n}\nmod b;\n");
    rejects(
        src,
        "mod a { pub fn f() {} }\n",
        "proof/lib.eykp:2:1: E0304",
        &["the end of the file", "`src/lib.eyk:2:1` has `mod`"],
    );
    rejects(
        src,
        "mod a { }\npub fn f() {}\nmod b;\n",
        "proof/lib.eykp:1:9: E0304",
        &["in `mod a`"],
    );
}

#[test]
fn proof_only_items_are_only_lemmas_with_a_body_and_spec_fns() {
    let src = "fn f() {}\n";
    matches(
        src,
        "fn f() {}\nlemma fn l() ensures true { }\nspec fn s() -> int = 1;\n",
    );
    // 本体のない `lemma fn` と `unchecked lemma fn` は、証明なしの前提を足すことになる。
    for extra in [
        "lemma fn l() ensures false;\n",
        "unchecked lemma fn l() ensures false;\n",
        "fn g() {}\n",
        "use std::mem;\n",
    ] {
        let d = run(src, &format!("fn f() {{}}\n{extra}"));
        assert_eq!(d.len(), 1, "{extra}: {d:?}");
        assert!(d[0].contains("E0304"), "{extra}: {d:?}");
    }
}

#[test]
fn a_lemma_declared_in_the_source_keeps_its_contract() {
    let src = "lemma fn l(x: int) requires x > 0 ensures x >= 1;\n";
    matches(
        src,
        "lemma fn l(x: int) requires x > 0 ensures x >= 1 { }\n",
    );
    rejects(
        src,
        "lemma fn l(x: int) requires x > 0 ensures x >= 0 { }\n",
        "proof/lib.eykp:1:48: E0304",
        &["in `lemma fn l`", "`0`"],
    );
}

#[test]
fn lemmas_in_impls_are_matched_by_the_impl() {
    let src = "struct S {}\nimpl S {\n    lemma fn l() ensures true;\n}\n";
    matches(
        src,
        "struct S {}\nimpl S {\n    lemma fn l() ensures true { }\n    lemma fn extra() ensures true { }\n}\n",
    );
}

#[test]
fn files_with_syntax_errors_are_not_compared() {
    let d = run("fn f() {}\n", "fn f() { proof { 1 + } }\nfn g() {}\n");
    assert!(d.iter().all(|l| !l.contains("E0304")), "{d:?}");
    assert!(!d.is_empty());
}

/// 付録 A.2 の `.eykp`（`crates/ebykn_syntax/tests/fixtures/dlist.eykp`。現行仕様の形）を剥がすと、
/// その証明を除いた `.eyk` に一致する。
#[test]
fn appendix_a2_strips_to_its_source() {
    let proof = include_str!("../../ebykn_syntax/tests/fixtures/dlist.eykp");
    let src = replace(proof, "        spec let before = *self;\n", "");
    let src = replace(
        &src,
        "        proof { push_keeps_invariant(before, *self, n); }\n",
        "",
    );
    let at = src.find("lemma fn push_keeps_invariant").expect("補題");
    let src = &src[..at];
    matches(src, proof);
    // 契約を一つ変えると一致しない。
    let changed = replace(proof, "final(self).has(h)", "final(self).has(h), true");
    let d = run(src, &changed);
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].contains("E0304") && d[0].contains("fn push_back"),
        "{d:?}"
    );
}

#[test]
fn proof_attributes_and_doc_comments_are_stripped() {
    let src = "\
pub spec fn g(x: int) -> bool = x > 0;
pub fn f() ensures forall x: int, x > 0 ==> g(x), exists y: int, g(y) { }
";
    let proof = "\
//! 証明ファイルだけの doc コメント
pub spec fn g(x: int) -> bool = x > 0;
/** 証明の説明 */
pub fn f()
    ensures
        forall x: int, x > 0 ==> #[trigger] g(x),
        exists y: int, #![trigger g(y)] g(y),
{ }
";
    matches(src, proof);
}

/// 証明ファイルの中の、Verus の関数の名前を最後に持つパスも、`.eyk` と同じく名前解決して判定する。
#[test]
fn proof_files_resolve_reserved_names_like_their_module() {
    let src = "mod m { pub fn assume(b: bool) {} }\nfn f() {}\n";
    matches(
        src,
        "mod m { pub fn assume(b: bool) {} }\nfn f() { proof { crate::m::assume(true); } }\n",
    );
    let d = run(
        src,
        "mod m { pub fn assume(b: bool) {} }\nfn f() { proof { crate::assume(true); } }\n",
    );
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].starts_with("proof/lib.eykp:2:18: E0001"), "{d:?}");
}
