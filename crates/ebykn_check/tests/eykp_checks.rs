//! `.eykp` のあるモジュールの §6 の検査（#67）：検査は書き下す `.eykp` にもかけ、`.eyk` と同じ診断は
//! `.eyk` の位置で一度だけ出す。証明の文は権限と領域を読めるが、状態は変えられない（E0520）。

use ebykn_check::{check, project, sort_diags};
use ebykn_syntax::diag::Lang;

fn run(src: &str, proof: &str) -> Vec<String> {
    let (p, mut diags) = project::single_with_proof(src, proof);
    diags.extend(check(&p).diags);
    sort_diags(&mut diags);
    diags
        .iter()
        .map(|d| d.render(p.file(d.span.file), Lang::En))
        .collect()
}

const NODE: &str = "pub struct Node { pub v: u64 }\n";

#[test]
fn proofs_cannot_change_permissions() {
    let src = format!("{NODE}fn f() {{ let p = Ptr::new(Node {{ v: 1 }}); let n = p.free(); }}\n");
    // 解放する前に、証明の中で権限を捨てる（剥がした `.eyk` では、解放する権限を持っている）。
    let proof = src.replace("let n = p.free();", "proof { leak p@; } let n = p.free();");
    let d = run(&src, &proof);
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].starts_with("proof/lib.eykp:2:43: E0520"), "{d:?}");
    assert!(d[0].contains("`p@`"), "{d:?}");
    // 権限を読むだけなら通る。
    let read = src.replace(
        "let n = p.free();",
        "proof { assert (*p).v == 1; } let n = p.free();",
    );
    let d = run(&src, &read);
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn proofs_cannot_take_or_put() {
    let src = format!(
        "{NODE}fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> requires r.contains(p) {{ }}\n"
    );
    let proof = src.replace("{ }", "{ proof { take r[p]; put r[p]; } }");
    let d = run(&src, &proof);
    // 取り出して戻すと状態は同じだが、証明の中の `take` / `put` は剥がした `.eyk` にない操作。
    // 状態が同じなら E0520 にはしない（読むのと同じ）。
    assert!(d.iter().all(|l| !l.contains("E0520")), "{d:?}");
    let proof = src.replace("{ }", "{ proof { take r[p]; } }");
    let d = run(&src, &proof);
    assert!(d.iter().any(|l| l.contains("E0520")), "{d:?}");
}

/// `.eyk` の誤りは `.eyk` の位置で一度だけ出す（`.eykp` の同じ誤りを重ねない）。
#[test]
fn source_errors_are_reported_once_at_the_source() {
    let src = format!("{NODE}fn f() {{ let p = Ptr::new(Node {{ v: 1 }}); }}\n");
    let proof = src.replace("{ let p", "{ proof { } let p");
    let d = run(&src, &proof);
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].starts_with("src/lib.eyk:") && d[0].contains("E0505"),
        "{d:?}"
    );
}

/// 証明の中の誤り（`.eyk` にないもの）は `.eykp` の位置で出す。
#[test]
fn errors_inside_proofs_are_reported_in_the_proof_file() {
    let src = "lemma fn l(x: int) ensures x == x;\n";
    let proof = "lemma fn l(x: int) ensures x == x { assert self@v == 1; }\n";
    let d = run(src, proof);
    assert!(d.iter().any(|l| l.starts_with("proof/lib.eykp:")), "{d:?}");
}
