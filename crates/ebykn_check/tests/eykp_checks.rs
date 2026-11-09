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

/// 証明の中では、状態を変える操作を、前と後で状態が同じでも E0520 にする（#149 のレビューの B1）。
#[test]
fn proofs_cannot_take_put_or_leak_even_when_the_state_returns() {
    let src = format!(
        "{NODE}pub fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> requires old(r).contains(p) {{ }}\n"
    );
    for body in [
        // 取り出して捨てる（`.eyk` の契約が「残す」なら、証明で消したことになる）。
        "{ proof { take r[p]; leak p@; } }",
        // 取り出して戻す。
        "{ proof { take r[p]; put r[p]; } }",
        "{ proof { take r[p]; } }",
        "{ assert true by { take r[p]; put r[p]; }; }",
    ] {
        let proof = src.replace("{ }", body);
        let d = run(&src, &proof);
        assert!(
            d.iter().any(|l| l.contains("E0520") && l.contains("`p@`")),
            "{body}: {d:?}"
        );
        assert_eq!(
            d.iter().filter(|l| l.contains("E0520")).count(),
            1,
            "{body}: {d:?}"
        );
    }
    // 領域の間で移す。
    let src2 = format!(
        "{NODE}pub fn mv(p: Ptr<Node>) uses a: &mut Region<Ptr<Node>>, b: &mut Region<Ptr<Node>> requires old(a).contains(p), !old(b).contains(p) {{ }}\n"
    );
    let d = run(
        &src2,
        &src2.replace("{ }", "{ proof { take a[p]; put b[p]; } }"),
    );
    assert!(d.iter().any(|l| l.contains("E0520")), "{d:?}");
    // 読む借用は書ける。
    let d = run(
        &src,
        &src.replace(
            "{ }",
            "{ proof { borrow r[p] { assert (*p).v == (*p).v; } } }",
        ),
    );
    assert!(d.is_empty(), "{d:?}");
    // 消去フィールドへの代入も E0520。
    let src3 =
        "pub struct S { a: u64 } spec { n: int }\nimpl S {\n    pub fn f(&mut self) { }\n}\n";
    let d = run(src3, &src3.replace("{ }", "{ proof { self@n = 1; } }"));
    assert!(
        d.iter().any(|l| l.contains("E0520") && l.contains("`@n`")),
        "{d:?}"
    );
}

/// `spec let` はスコープの名前を覆えない（E0521。#149 のレビューの B2）。覆うと、後ろの `.eyk` の文の
/// 指すものが変わる。
#[test]
fn spec_let_cannot_shadow_names_in_scope() {
    let src = format!(
        "{NODE}pub fn mv(p: Ptr<Node>, q: Ptr<Node>) uses to: &mut Region<Ptr<Node>>, from: &mut Region<Ptr<Node>> requires old(from).contains(p), !old(to).contains(p) {{\n    take from[p];\n    put to[p];\n}}\n"
    );
    let proof = src.replace(
        "    take from[p];",
        "    spec let p = q;\n    take from[p];",
    );
    let d = run(&src, &proof);
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].starts_with("proof/lib.eykp:3:14: E0521") && d[0].contains("`p`"),
        "{d:?}"
    );
    // 消去フィールドへの代入の右辺も同じ。
    let src2 = "pub struct S { a: u64 } spec { n: int }\nimpl S {\n    pub fn f(&mut self, x: int) {\n        self@n = x;\n    }\n}\n";
    let proof2 = src2.replace(
        "        self@n = x;",
        "        spec let x = 0int;\n        self@n = x;",
    );
    let d = run(src2, &proof2);
    assert!(
        d.iter().any(|l| l.contains("E0521") && l.contains("`x`")),
        "{d:?}"
    );
    // 新しい名前なら通る。
    let ok = src.replace(
        "    take from[p];",
        "    spec let before = q;\n    take from[p];",
    );
    let d = run(&src, &ok);
    assert!(d.is_empty(), "{d:?}");
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

/// `lemma group` の要素は、モジュールの直下の `lemma fn` だけ（#149 のレビューの nit）。
#[test]
fn lemma_group_members_are_module_level_lemmas() {
    let src = "pub struct S {}\nimpl S {\n    lemma fn m() ensures true;\n}\nlemma fn l() ensures true;\nfn f() {}\n";
    let ok = format!("{src}lemma group g {{ l }}\n");
    let d = run(src, &ok);
    assert!(d.is_empty(), "{d:?}");
    for bad in ["S::m", "f", "nothing"] {
        let d = run(src, &format!("{src}lemma group g {{ {bad} }}\n"));
        assert!(
            d.iter()
                .any(|l| l.contains("E0001") && l.contains(&format!("`{bad}` in `lemma group`"))),
            "{bad}: {d:?}"
        );
    }
}

/// `spec let` は新しい名前だけを束縛できる（#149 の再レビューの N1）。項目の名前（関数）と `.eyk` に
/// 現れる名前は覆えない。
#[test]
fn spec_let_cannot_shadow_items_or_source_names() {
    // 消去フィールドへの代入の右辺が、モジュールの `spec fn` を指す。
    let src = "pub spec fn val(z: int) -> int = 8;\npub struct S { a: u64 } spec { n: int }\nimpl S {\n    fn f(&mut self) ensures final(self)@n == 7 {\n        self@n = val(0);\n    }\n}\n";
    let proof = src.replace(
        "        self@n = val(0);",
        "        spec let val = |z: int| 7int;\n        self@n = val(0);",
    );
    let d = run(src, &proof);
    assert!(
        d.iter().any(|l| l.contains("E0521") && l.contains("`val`")),
        "{d:?}"
    );
    // ループの句が、モジュールの関数を指す。
    let src2 = "pub spec fn lim() -> int = 10;\npub fn g(n: u64) {\n    let mut i = 0;\n    while i < n\n        invariant i <= n, lim() == 10\n        decreases n - i\n    {\n        i = i + 1;\n    }\n}\n";
    let proof2 = src2.replace(
        "    let mut i = 0;\n",
        "    let mut i = 0;\n    spec let lim = || 10int;\n",
    );
    let d = run(src2, &proof2);
    assert!(
        d.iter().any(|l| l.contains("E0521") && l.contains("`lim`")),
        "{d:?}"
    );
    // `.eyk` に現れる名前（ここでは型の名前）も覆えない。
    let src3 = "pub struct Node { pub v: u64 }\npub fn h() { }\n";
    let d = run(
        src3,
        &src3.replace("pub fn h() { }", "pub fn h() { spec let Node = 1int; }"),
    );
    assert!(d.iter().any(|l| l.contains("E0521")), "{d:?}");
}

/// `assert .. by { }` の中の `borrow` は書けない（Verus がそこで権限を借りられない。N3）。
#[test]
fn borrow_inside_assert_by_is_rejected() {
    let src = format!(
        "{NODE}pub fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> requires old(r).contains(p) {{ }}\n"
    );
    let proof = src.replace(
        "{ }",
        "{ assert true by { borrow r[p] { assert (*p).v == (*p).v; } }; }",
    );
    let d = run(&src, &proof);
    assert!(
        d.iter()
            .any(|l| l.contains("E0001") && l.contains("assert .. by")),
        "{d:?}"
    );
}

/// 証明の中の `free`、権限の移動と `&mut` の `uses`、領域の `&mut` の `uses` も E0520（N4）。論理の文脈で
/// 検査を飛ばす操作も数える。
#[test]
fn proofs_cannot_free_or_pass_permissions_mutably() {
    let src = format!(
        "{NODE}fn touch(p: Ptr<Node>) uses &mut p@ {{ }}\nfn take_it(p: Ptr<Node>) uses p@ {{ leak p@; }}\nfn reg(q: Ptr<Node>) uses r: &mut Region<Ptr<Node>> {{ }}\npub fn f(p: Ptr<Node>, q: Ptr<Node>) uses p@, r: &mut Region<Ptr<Node>> {{ leak p@; }}\n"
    );
    for (body, name) in [
        ("{ proof { p.free(); } leak p@; }", "`p@`"),
        ("{ proof { touch(p); } leak p@; }", "`p@`"),
        ("{ proof { take_it(p); } leak p@; }", "`p@`"),
        ("{ proof { reg(q, uses &mut r); } leak p@; }", "`r`"),
    ] {
        let proof = src.replace("{ leak p@; }\n", &format!("{body}\n"));
        assert_ne!(proof, src);
        let d = run(&src, &proof);
        assert!(
            d.iter().any(|l| l.contains("E0520") && l.contains(name)),
            "{body}: {d:?}"
        );
    }
}

const RES: &str = "pub struct Node { pub v: u64 }\npub fn f(p: Ptr<Node>, c: Cell<u64>) uses p@, c@, r: &mut Region<Ptr<Node>>, a: &mut Region<Ptr<Node>> {\n    leak p@;\n    leak c@;\n}\n";

/// 証明の中で、prelude の `Region` の `proof fn` を直接呼んで領域を変える形（#149 の 3 回目の再レビューの B4）。
#[test]
fn proofs_cannot_call_region_proof_fns_directly() {
    for body in [
        "proof { r.remove(p); }",
        "proof { r.insert(a.remove(p)); }",
        "proof { Region::remove(r, p); }",
        "proof { r.insert(p@); }",
    ] {
        let proof = RES.replace("    leak p@;\n", &format!("    {body}\n    leak p@;\n"));
        assert_ne!(proof, RES);
        let d = run(RES, &proof);
        assert!(d.iter().any(|l| l.contains("E0520")), "{body}: {d:?}");
    }
    // prelude の spec の関数を呼ぶ形と、権限を読む形は通る。
    for body in [
        "proof { assert r.contains(p) || !r.contains(p); }",
        "proof { assert r.len() == r.len(); }",
        "proof { assert p@.pptr() == p@.pptr(); }",
        "proof { assert c@.id() == c@.id(); }",
    ] {
        let proof = RES.replace("    leak p@;\n", &format!("    {body}\n    leak p@;\n"));
        let d = run(RES, &proof);
        assert!(d.is_empty(), "{body}: {d:?}");
    }
}

/// prelude の公開の関数を一つずつ証明の中で呼び、spec の関数は許すリストにあり、ほかはすべて E0520 に
/// なることを確かめる（prelude に関数を足しても、黙って穴が開かないように。B4 の再発防止）。
#[test]
fn every_prelude_fn_is_classified() {
    let spec_list = ebykn_check::body::PRELUDE_SPEC_FNS;
    let mut specs = Vec::new();
    let mut others = Vec::new();
    for (file, text) in [
        (
            "region.rs",
            include_str!("../../ebykn_prelude/src/region.rs"),
        ),
        ("ptr.rs", include_str!("../../ebykn_prelude/src/ptr.rs")),
        ("cell.rs", include_str!("../../ebykn_prelude/src/cell.rs")),
    ] {
        let mut ty = String::new();
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("impl") {
                // `impl<L: Loc> Region<L> {` の型の名前：型の引数の `<..>` を飛ばし、トレイトの impl なら
                // `for` の後を見る。
                let mut depth = 0;
                let mut k = 0;
                for (n, ch) in rest.char_indices() {
                    match ch {
                        '<' => depth += 1,
                        '>' => depth -= 1,
                        _ if depth == 0 => {
                            k = n;
                            break;
                        }
                        _ => {}
                    }
                }
                let head = &rest[k..];
                let head = head.rsplit(" for ").next().unwrap_or(head);
                ty = head
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
            }
            if t.starts_with("pub ") && t.contains(" fn ") {
                let name = t
                    .split(" fn ")
                    .nth(1)
                    .unwrap()
                    .split(['(', '<'])
                    .next()
                    .unwrap();
                if t.contains("spec fn") {
                    specs.push(name.to_string());
                } else {
                    others.push((file, ty.clone(), name.to_string()));
                }
            }
        }
    }
    assert!(
        specs.len() >= 10 && others.len() >= 10,
        "{specs:?} {others:?}"
    );
    for s in &specs {
        assert!(
            spec_list.contains(&s.as_str()),
            "prelude の spec fn `{s}` が PRELUDE_SPEC_FNS にない"
        );
    }
    for s in spec_list {
        assert!(
            specs.iter().any(|x| x == s),
            "PRELUDE_SPEC_FNS の `{s}` が prelude の spec fn にない"
        );
    }
    for (file, ty, name) in &others {
        let recv = match ty.as_str() {
            "Region" => "r",
            "PointsTo" => "p@",
            "CellPointsTo" => "c@",
            "Ptr" => "p",
            "Cell" => "c",
            other => panic!("{file}: 知らない型 `{other}` の `{name}`"),
        };
        for call in [format!("{ty}::{name}({recv})"), format!("{recv}.{name}()")] {
            let proof = RES.replace(
                "    leak p@;\n",
                &format!("    proof {{ {call}; }}\n    leak p@;\n"),
            );
            let d = run(RES, &proof);
            assert!(
                d.iter().any(|l| l.contains("E0520")),
                "{file}: `{call}` が E0520 にならない: {d:?}"
            );
        }
    }
}
