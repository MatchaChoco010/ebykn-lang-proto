//! `.eykp` の証明構文の書き下し（仕様 §7.2、§7.4。#67）。

use ebykn_check::{check, project};

/// `src` と `proof` を検査して書き下し、空白をまとめた `src/lib.rs` を返す。
fn lower(src: &str, proof: &str) -> String {
    let (p, mut diags) = project::single_with_proof(src, proof);
    let checked = check(&p);
    diags.extend(checked.diags.iter().cloned());
    assert!(diags.is_empty(), "{diags:?}");
    let out = ebykn_lower::lower(&p, &checked).unwrap();
    out.files[0]
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 空白を除いて比べる（整形の空白の入れ方は見ない）。
#[track_caller]
fn has(text: &str, want: &str) {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    assert!(
        squash(text).contains(&squash(want)),
        "{want:?} がない:\n{text}"
    );
}

#[test]
fn lemma_groups_become_broadcast_groups() {
    let src = "spec fn f(x: int) -> int = x;\nlemma fn l(x: int) ensures f(x) == x;\n";
    let proof = "spec fn f(x: int) -> int = x;\nlemma fn l(x: int) ensures #[trigger] f(x) == x { }\nlemma group g { l }\nuse lemmas g;\nfn u() { use lemmas g; }\n";
    let src = format!("{src}fn u() {{ }}\n");
    let t = lower(&src, proof);
    has(
        &t,
        "broadcast proof fn l(x: int) ensures #[trigger] f(x) == x, { }",
    );
    has(&t, "broadcast group g { l }");
    has(&t, "broadcast use g;");
    has(&t, "fn u() { proof { broadcast use g; } }");
}

#[test]
fn proof_statements_are_lowered() {
    let src =
        "spec fn f(x: int) -> int = x;\nlemma fn l(x: u64) ensures x & 0 == 0;\nfn g(x: u64) { }\n";
    let proof = "#[opaque]\nspec fn f(x: int) -> int = x;\n#[rlimit(20)]\nlemma fn l(x: u64) by(bit_vector) ensures x & 0 == 0 { }\nfn g(x: u64) {\n    reveal(f, 2);\n    hide(f);\n    assert x & 0 == 0 by(bit_vector) requires x == x;\n    proof { spec let y = x; assert y == x; }\n    assert forall i: int, #![auto] f(i) == i by { reveal(f); };\n}\n";
    let t = lower(src, proof);
    has(&t, "#[verifier::opaque] spec fn f(x: int) -> int { x }");
    has(
        &t,
        "#[verifier::rlimit(20)] proof fn l(x: u64) by (bit_vector) ensures x & 0 == 0, { }",
    );
    has(&t, "proof { reveal_with_fuel(f, 2); }");
    has(&t, "} hide(f); proof");
    has(
        &t,
        "proof { assert(x & 0 == 0) by (bit_vector) requires x == x; }",
    );
    // 証明の文脈の中では包まず、`spec let` は `let`。
    has(&t, "proof { let y = x; assert(y == x); }");
    has(
        &t,
        "proof { assert forall|i: int| #![auto] f(i) == i by { reveal(f); }; }",
    );
}

#[test]
fn spec_let_in_exec_code_is_ghost() {
    let src = "fn g(x: u64) -> u64 { x }\n";
    let t = lower(src, "fn g(x: u64) -> u64 { spec let y = x + 1; x }\n");
    has(&t, "let ghost y = x + 1; x");
}
