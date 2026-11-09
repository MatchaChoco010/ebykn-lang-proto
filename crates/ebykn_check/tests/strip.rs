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
    y: u8
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
        &["in `fn f`", "here it has `)`", "`src/lib.eyk:1:21` has `,`"],
    );
}

#[test]
fn macro_arguments_are_compared_as_written() {
    rejects(
        "fn f() -> Vec<u8> { vec![1, 2,] }\n",
        "fn f() -> Vec<u8> { vec![1, 2] }\n",
        "proof/lib.eykp:1:30: E0304",
        &["here it has `]`", "`src/lib.eyk:1:30` has `,`"],
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

/// 記号のあいだの空白はトークンの区切りを変える。`a && b` は短絡するが、`a & &b` は両方を評価する
/// （#136 のレビューの B1）。
#[test]
fn spaces_between_symbols_that_change_the_tokens_are_compared() {
    let src = "fn bump(c: &mut u64) -> bool { true }\npub fn g(a: bool, c: &mut u64) -> bool requires *c < 100 { a && bump(c) }\n";
    let d = run(src, &replace(src, "a && bump(c)", "a & &bump(c)"));
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].contains("E0304") && d[0].contains("`&&`"), "{d:?}");
    // 演算子のまとまりが同じなら、まわりの空白は比べない。
    matches(src, &replace(src, "a && bump(c)", "a&&bump( c )"));
    // `::`、`->`、`..=`、`==>` も一つの記号として比べる。
    let src2 = "spec fn f(x: int) -> bool = x > 0 ==> x >= 1;\n";
    matches(src2, "spec fn f(x:int)->bool=x>0==>x>=1;\n");
    let d = run(src2, "spec fn f(x: int) -> bool = x > 0 == > x >= 1;\n");
    assert_eq!(d.len(), 1, "{d:?}");
}

/// `{` の前のカンマを捨てるのは本体を開く `{` の前だけ。`(S, {})` は `(S {})` と別の式（B2）。
#[test]
fn a_comma_before_a_brace_inside_parentheses_is_kept() {
    for (src, proof) in [
        (
            "struct S {}\nfn g() { let t = (S {}); }\n",
            "struct S {}\nfn g() { let t = (S, {}); }\n",
        ),
        (
            "struct S {}\nfn g() { let t = [S {}]; }\n",
            "struct S {}\nfn g() { let t = [S, {}]; }\n",
        ),
        (
            "fn h<F: Fn(u64)>(f: F) {}\nfn g(y: u64) { h(|x: u64| y {}); }\n",
            "fn h<F: Fn(u64)>(f: F) {}\nfn g(y: u64) { h(|x: u64| y, {}); }\n",
        ),
    ] {
        let d = run(src, proof);
        assert_eq!(d.len(), 1, "{proof}: {d:?}");
        assert!(d[0].contains("E0304"), "{proof}: {d:?}");
    }
    // 本体を開く `{` の前の末尾のカンマは捨てる。
    matches(
        "fn f(x: u8) requires x < 10 ensures x == x { }\n",
        "fn f(x: u8) requires x < 10, ensures x == x, { }\n",
    );
}

/// `.eykp` だけの項目は、`.eyk` に現れる名前を持てない。glob の import（暗黙の `ebykn_prelude::*` を含む）
/// を覆い、`.eyk` の契約の意味を変えうる（S1）。
#[test]
fn proof_only_items_cannot_take_names_the_source_uses() {
    let src = "mod a { pub spec fn ok(x: u64) -> bool = x > 0; }\nuse a::*;\nfn g(x: u64) -> (r: u64) ensures ok(r) { 1 }\n";
    rejects(
        src,
        &format!("{src}spec fn ok(x: u64) -> bool = true;\n"),
        "proof/lib.eykp:4:9: E0304",
        &["`spec fn ok`", "another name"],
    );
    // `.eyk` の `fn foo` と同じ名前の `.eykp` だけの補題。
    let d = run(
        "fn foo() {}\n",
        "fn foo() {}\nlemma fn foo() ensures true { }\n",
    );
    assert!(
        d.iter()
            .any(|l| l.starts_with("proof/lib.eykp:2:10: E0304")),
        "{d:?}"
    );
    // `.eykp` だけの項目どうしで同じ名前。
    let d = run(
        "fn f() {}\n",
        "fn f() {}\nspec fn s() -> int = 1;\nspec fn s() -> int = 2;\n",
    );
    // 書き下す `.eykp` の索引も、同じ名前の二つの定義を E0003 にする。
    assert!(
        d.iter()
            .any(|l| l.starts_with("proof/lib.eykp:3:9: E0304")
                && l.contains("another proof-only item")),
        "{d:?}"
    );
    // `impl T` の `spec fn` は、同じ名前のトレイトのメソッドより先に選ばれる。
    let src = "struct S {}\nimpl S {\n}\nspec fn m(x: int) -> int = x;\n";
    rejects(
        src,
        &src.replace(
            "impl S {\n}",
            "impl S {\n    spec fn m(&self) -> int = 0;\n}",
        ),
        "proof/lib.eykp:3:13: E0304",
        &["`spec fn m`"],
    );
}

/// 契約の `!(..)` はマクロの呼び出しではない（S2）。中の証明の属性を剥がして比べる。
#[test]
fn negation_after_a_clause_keyword_is_not_a_macro() {
    matches(
        "spec fn f(x: int) -> bool = x > 0;\nfn g() ensures !(forall x: int, f(x)) {}\n",
        "spec fn f(x: int) -> bool = x > 0;\nfn g() ensures !(forall x: int, #[trigger] f(x)) {}\n",
    );
}

/// 丸括弧の中の末尾のカンマは、呼び出しと引数の並びでも書いたとおりに比べる（タプルと見分けるには
/// 構文が要るので、トークンから推し量らない）。角括弧と波括弧の閉じの前のカンマは捨てる。
#[test]
fn trailing_commas_in_parentheses_are_compared_as_written() {
    let src = "fn h(x: u64) {}\nfn g(x: u64) { h(1); }\n";
    let d = run(src, "fn h(x: u64) {}\nfn g(x: u64) { h(1,); }\n");
    assert!(d.iter().any(|l| l.contains("E0304")), "{d:?}");
    let d = run(src, "fn h(x: u64,) {}\nfn g(x: u64) { h(1); }\n");
    assert!(d.iter().any(|l| l.contains("E0304")), "{d:?}");
    matches(
        "struct S { a: u64 }\nfn g() -> [u64; 2] { let s = S { a: 1 }; [s.a, 2] }\n",
        "struct S { a: u64, }\nfn g() -> [u64; 2] { let s = S { a: 1, }; [s.a, 2,] }\n",
    );
}

/// 属性の並びの順は、組み込みの属性だけの並びなら無視し、属性マクロを含む並びは書いた順に比べる
/// （#144 の既定）。
#[test]
fn attribute_order_is_ignored_only_among_builtin_attributes() {
    matches(
        "#[inline]\n#[must_use]\npub fn f() -> u8 { 1 }\n",
        "#[must_use]\n#[inline]\npub fn f() -> u8 { 1 }\n",
    );
    let d = run(
        "#[tokio::main]\n#[inline]\npub fn f() -> u8 { 1 }\n",
        "#[inline]\n#[tokio::main]\npub fn f() -> u8 { 1 }\n",
    );
    assert!(d.iter().any(|l| l.contains("E0304")), "{d:?}");
}

/// 1 要素のタプルのカンマは、`>`・`]`・`pub` の後でも残す（#136 の再確認の blocker）。
#[test]
fn one_element_tuples_keep_their_comma_after_any_token() {
    for (src, proof) in [
        (
            "pub fn g(x: u64) -> (u64,) { match x { _ => (x,) } }\n",
            "pub fn g(x: u64) -> (u64) { match x { _ => (x) } }\n",
        ),
        (
            "fn g(x: u64) -> (u64,) { (x,) }\n",
            "fn g(x: u64) -> (u64) { (x,) }\n",
        ),
        (
            "fn h<F: Fn() -> (u64,)>(f: F) {}\n",
            "fn h<F: Fn() -> (u64)>(f: F) {}\n",
        ),
        ("struct S(pub (u64,));\n", "struct S(pub (u64));\n"),
        (
            "fn g(a: (u64,), b: (u64,)) -> bool { a > (b.0,) }\n",
            "fn g(a: (u64,), b: (u64,)) -> bool { a > (b.0) }\n",
        ),
        (
            "fn g(b: u64) -> (u64,) { #[allow(unused_parens)] (b,) }\n",
            "fn g(b: u64) -> (u64,) { #[allow(unused_parens)] (b) }\n",
        ),
        (
            "pub struct S<'a>(pub(crate) (u64,), &'a (u64,));\n",
            "pub struct S<'a>(pub(crate) (u64,), &'a (u64));\n",
        ),
        (
            "pub struct S(pub(crate) (u64,));\n",
            "pub struct S(pub(crate) (u64));\n",
        ),
        (
            "pub struct S(pub(super) (u64,));\n",
            "pub struct S(pub(super) (u64));\n",
        ),
        (
            "pub struct S(pub(self) (u64,));\n",
            "pub struct S(pub(self) (u64));\n",
        ),
        (
            "mod m { pub struct S(pub(in crate::m) (u64,)); }\n",
            "mod m { pub struct S(pub(in crate::m) (u64)); }\n",
        ),
        ("fn g(x: &'_ (u64,)) {}\n", "fn g(x: &'_ (u64)) {}\n"),
        (
            "fn g<'a>(x: &'a (u64,)) -> &'a (u64,) { x }\n",
            "fn g<'a>(x: &'a (u64,)) -> &'a (u64) { x }\n",
        ),
        (
            "fn g(x: u64) -> (u64,) { 'l: loop { break 'l (x,); } }\n",
            "fn g(x: u64) -> (u64,) { 'l: loop { break 'l (x); } }\n",
        ),
    ] {
        let d = run(src, proof);
        assert!(d.iter().any(|l| l.contains("E0304")), "{proof}: {d:?}");
    }
}

/// 記号の並びは属性の `#` の前で切る（`&&#[trigger] f(x)` を `&& f(x)` と比べる）。
#[test]
fn a_symbol_run_stops_before_an_attribute() {
    matches(
        "spec fn f(x: int) -> bool = x > 0;\nfn g() ensures forall x: int, x > 0 && f(x) {}\n",
        "spec fn f(x: int) -> bool = x > 0;\nfn g() ensures forall x: int, x > 0 &&#[trigger] f(x) {}\n",
    );
}

/// `ebykn_prelude` が公開する名前は、どれも `.eykp` だけの項目に使えない名前の一覧に入っている
/// （公開を足して一覧を忘れると、glob の import を覆う項目が通る）。
#[test]
fn every_prelude_export_is_a_taken_name() {
    let lib = include_str!("../../ebykn_prelude/src/lib.rs");
    // 根の公開は 1 行の `pub use` だけ（ほかの形で公開を足したら、この検査を広げる）。
    for line in lib.lines().filter(|l| l.starts_with("pub")) {
        assert!(
            line.starts_with("pub use ") && line.ends_with(';'),
            "1 行の `pub use` でない公開: {line}"
        );
    }
    let mut leaves = Vec::new();
    for line in lib.lines().filter(|l| l.starts_with("pub use ")) {
        let body = line["pub use ".len()..].trim_end_matches(';');
        match body.split_once('{') {
            Some((_, rest)) => leaves.extend(
                rest.trim_end_matches('}')
                    .split(',')
                    .map(|s| s.trim().to_string()),
            ),
            None => leaves.push(body.rsplit("::").next().unwrap().to_string()),
        }
    }
    assert!(leaves.len() >= 9, "{leaves:?}");
    for leaf in &leaves {
        assert!(
            ebykn_check::index::PRELUDE_TYPES.contains(&leaf.as_str()),
            "{leaf} が PRELUDE_TYPES にない"
        );
        let d = run(
            "fn f() {}\n",
            &format!("fn f() {{}}\nspec fn {leaf}() -> int = 1;\n"),
        );
        assert!(d.iter().any(|l| l.contains("E0304")), "{leaf}: {d:?}");
    }
}

/// カンマで食い違ったら、文の先頭ではなくカンマの位置を出す（#136 のレビューの nit）。
#[test]
fn a_comma_difference_is_reported_at_the_comma() {
    rejects(
        "fn h(a: u8, b: u8) {}\nfn g() {\n    h(1, 2);\n}\n",
        "fn h(a: u8, b: u8) {}\nfn g() {\n    h(1, 2,);\n}\n",
        "proof/lib.eykp:3:11: E0304",
        &["here it has `,`", "`src/lib.eyk:3:11` has `)`"],
    );
}
