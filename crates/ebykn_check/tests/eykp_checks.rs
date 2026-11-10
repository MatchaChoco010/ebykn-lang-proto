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

/// 受け手の形を変える、マクロや別名を通す形でも、証明の中で領域を変えられない（#149 の 4 回目の再レビューの
/// B5）。資源を含む式は、許した 2 つの形のほかは E0520。
#[test]
fn proofs_cannot_change_regions_through_other_spellings() {
    let src = format!("use ebykn_prelude::Region as R;\npub type RR = Region<Ptr<Node>>;\n{RES}");
    for body in [
        "proof { (*a).remove(p); }",
        "proof { (*r).insert((*a).remove(p)); }",
        "proof { (&mut *r).remove(p); }",
        "proof { { &mut *a }.remove(p); }",
        "proof { if true { &mut *r } else { &mut *r }.remove(p); }",
        "proof { (*r).insert({ p@ }); }",
        "proof { matches!(r.insert(a.remove(p)), _); }",
        "proof { std::matches!(r.insert(a.remove(p)), _); }",
        "proof { *r = R::empty(); }",
        "proof { *r = RR::empty(); }",
        "proof { *r = Region::empty(); }",
        "proof { R::remove(r, p); }",
    ] {
        let proof = src.replace("    leak p@;\n", &format!("    {body}\n    leak p@;\n"));
        assert_ne!(proof, src);
        let d = run(&src, &proof);
        assert!(d.iter().any(|l| l.contains("E0520")), "{body}: {d:?}");
    }
    for body in [
        "proof { assert (*r).contains(p) || !(*r).contains(p); }",
        "proof { assert r.dom().len() == r.dom().len(); }",
        "proof { assert old(r).contains(p) == old(r).contains(p); }",
        "proof { assert r[p].v == r[p].v; }",
        "proof { assert forall q in r, r.contains(q) by { }; }",
    ] {
        let proof = src.replace("    leak p@;\n", &format!("    {body}\n    leak p@;\n"));
        let d = run(&src, &proof);
        assert!(d.is_empty(), "{body}: {d:?}");
    }
}

/// `match` の対象に置いた資源は、腕のパターンが束縛し直すので E0520（#149 の 5 回目の再レビューの B6）。
#[test]
fn proofs_cannot_rebind_resources_through_match() {
    for body in [
        "proof { match r { x => { x.remove(p); } } }",
        "proof { match a { x => match b { y => { y.insert(x.remove(p)); } } } }",
        "proof { match r { x => match p@ { q => { x.insert(q); } } } }",
    ] {
        let proof = RES.replace("    leak p@;\n", &format!("    {body}\n    leak p@;\n"));
        assert_ne!(proof, RES);
        let d = run(RES, &proof);
        assert!(d.iter().any(|l| l.contains("E0520")), "{body}: {d:?}");
    }
    let body = "proof { match r.len() { _ => {} } }";
    let proof = RES.replace("    leak p@;\n", &format!("    {body}\n    leak p@;\n"));
    let d = run(RES, &proof);
    assert!(d.is_empty(), "{body}: {d:?}");
}

/// クレートの `spec fn` に資源を渡す形は、名前が本当にその `spec fn` を指すと決められるときだけ通す（B7）。
/// 証明の文の中の項目と `use` は書けず、本体の `use` や cfg で消える項目を通した名前は E0520。
#[test]
fn proofs_pass_resources_only_to_crate_spec_fns() {
    let spec = "pub struct S { pub v: u64 }\nimpl S { pub spec fn remove(a: Region<Ptr<Node>>, p: Ptr<Node>) -> bool = true; }\n";
    let src = format!("{spec}{RES}");
    let insert = |src: &str, body: &str| {
        let out = src.replace("    leak p@;\n", &format!("    {body}\n    leak p@;\n"));
        assert_ne!(out, src);
        out
    };
    // 証明の中の `use`。
    let proof = insert(
        &src,
        "proof { use ebykn_prelude::Region as S; S::remove(r, p); }",
    );
    let d = run(&src, &proof);
    assert!(
        d.iter()
            .any(|l| l.contains("cannot be written in a proof statement")),
        "{d:?}"
    );
    // `.eyk` の本体の `use` が名前を覆う。
    let local = src.replace(
        "    leak p@;\n    leak c@;\n",
        "    use ebykn_prelude::Region as S;\n    leak p@;\n    leak c@;\n",
    );
    assert_ne!(local, src);
    let d = run(&local, &insert(&local, "proof { S::remove(r, p); }"));
    assert!(d.iter().any(|l| l.contains("E0520")), "{d:?}");
    // cfg で消える項目。
    let gated = format!(
        "#[cfg(test)]\npub struct T {{ pub v: u64 }}\n#[cfg(test)]\nimpl T {{ pub spec fn remove(a: Region<Ptr<Node>>, p: Ptr<Node>) -> bool = true; }}\n#[cfg(not(test))]\nuse ebykn_prelude::Region as T;\n{RES}"
    );
    let d = run(&gated, &insert(&gated, "proof { T::remove(r, p); }"));
    assert!(d.iter().any(|l| l.contains("E0520")), "{d:?}");
    // 名前が決まるクレートの `spec fn` には渡せる。
    let d = run(
        &src,
        &insert(
            &src,
            "proof { assert S::remove(*r, p) == S::remove(*r, p); }",
        ),
    );
    assert!(d.is_empty(), "{d:?}");
}

/// パターンで領域の名前を覆ったあとも、ガードはその名前を領域とみなす（B8）。
#[test]
fn proofs_remember_shadowed_region_names() {
    let src = format!(
        "{NODE}pub fn g1(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>>\n    requires old(r).contains(p),\n    ensures !final(r).contains(p),\n{{\n    if let Some(r) = Some(0u64) {{ }}\n}}\n"
    );
    let proof = src.replace(
        "    if let Some(r) = Some(0u64) { }\n",
        "    if let Some(r) = Some(0u64) { }\n    proof { r.remove(p); }\n",
    );
    assert_ne!(proof, src);
    let d = run(&src, &proof);
    assert!(d.iter().any(|l| l.contains("E0520")), "{d:?}");
}

/// prelude の関数を、ソースのすべてのファイル（トレイトとトレイトの impl のメソッドを含む）から集め、一つずつ
/// 証明の中で呼んで確かめる（B4・B5 の再発防止）。資源の型の spec の関数は許すリストにあり、許すリストの名前は
/// どの資源の型でも spec でない関数の名前でない。spec でない関数は、包んだ形でもすべて E0520。prelude に
/// 関数・型・ファイルを足すと、分類を求めてこのテストが落ちる。
/// prelude の関数の宣言で `fn` の前に置く語。
const FN_QUALIFIERS: &[&str] = &[
    "pub",
    "pub(crate)",
    "pub(super)",
    "open",
    "closed",
    "spec",
    "proof",
    "exec",
    "broadcast",
    "uninterp",
    "const",
    "unsafe",
];

/// 行の先頭の属性（`#[..]`。入れ子の括弧を数える）を外す。
fn strip_attrs(mut t: &str) -> &str {
    while let Some(rest) = t.strip_prefix("#[") {
        let mut depth = 1;
        let Some(end) = rest.char_indices().find_map(|(i, c)| {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(i)
        }) else {
            return t;
        };
        t = rest[end + 1..].trim_start();
    }
    t
}

#[test]
fn every_prelude_fn_is_classified() {
    let spec_list = ebykn_check::body::PRELUDE_SPEC_FNS;
    // 資源を受け手にする型と、その受け手の書き方。
    let resource_types = [
        ("Region", "r"),
        ("PointsTo", "p@"),
        ("CellPointsTo", "c@"),
        ("Ptr", "p"),
        ("Cell", "c"),
        ("Loc", "p"),
    ];
    // 資源を受け手にしない型（`SeqExt` は `Seq` の拡張、`NonZst` は中だけで使う型）。
    let other_types = ["SeqExt", "Seq", "NonZst"];
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../ebykn_prelude/src");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{dir} を読めません: {e}"))
        .map(|e| e.expect("ディレクトリの項目").path())
        .collect();
    files.sort();
    assert!(files.len() >= 5, "{files:?}");
    let mut fns: Vec<(String, String, bool)> = Vec::new(); // (型, 名前, spec か)
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("{f:?} を読めません: {e}"));
        let file = f.file_name().unwrap().to_string_lossy().to_string();
        let known = ["lib.rs", "region.rs", "ptr.rs", "cell.rs", "seq.rs"];
        assert!(
            known.contains(&file.as_str()),
            "prelude の新しいファイル `{file}` を分類する"
        );
        // (見出しの字下げ、型、トレイトかトレイトの impl か)
        let mut head: Option<(usize, String, bool)> = None;
        for line in text.lines() {
            let indent = line.len() - line.trim_start().len();
            // 行の先頭の属性（`#[..]`）は外して読む（`#[verifier::external_body] pub proof fn ..`）。
            let t = strip_attrs(line.trim_start());
            if let Some((h, _, _)) = &head
                && indent <= *h
                && !t.is_empty()
                && !t.starts_with('}')
                && !t.starts_with("//")
            {
                head = None;
            }
            let decl = t.trim_start_matches("pub ").trim_start_matches("unsafe ");
            if decl.starts_with("impl") || decl.starts_with("trait ") {
                let is_trait = decl.starts_with("trait ");
                let rest = decl.trim_start_matches("impl").trim_start_matches("trait ");
                let mut depth = 0;
                let mut k = rest.len();
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
                let h = &rest[k..];
                let for_impl = h.contains(" for ");
                let h = h.rsplit(" for ").next().unwrap_or(h);
                let ty: String = h
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                head = Some((indent, ty, is_trait || for_impl));
                continue;
            }
            let Some(pos) = t.find("fn ") else { continue };
            if !(pos == 0 || t[..pos].ends_with(' ')) || t.starts_with("//") {
                continue;
            }
            // `fn` の前は修飾子だけ。読めない行は分類できないので失敗にする（読めない入力に黙らない）。
            assert!(
                t[..pos]
                    .split_whitespace()
                    .all(|w| FN_QUALIFIERS.contains(&w)),
                "{file}: 関数の行を読めません: `{line}`"
            );
            let name: String = t[pos + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let spec = t[..pos].contains("spec");
            match &head {
                // トレイトとトレイトの impl のメソッドは公開。ほかの impl は `pub` だけ。
                Some((_, ty, public)) if *public || t.starts_with("pub ") => {
                    fns.push((ty.clone(), name, spec));
                }
                Some(_) => {}
                None => assert!(
                    !t.starts_with("pub "),
                    "{file}: 公開の自由な関数 `{name}` を分類する"
                ),
            }
        }
    }
    assert!(fns.len() >= 30, "{fns:?}");
    for (ty, name, spec) in &fns {
        if other_types.contains(&ty.as_str()) {
            continue;
        }
        let Some((_, recv)) = resource_types.iter().find(|(t, _)| t == ty) else {
            panic!(
                "prelude の型 `{ty}` の `{name}` を分類する（資源の型か、資源を受け手にしない型か）"
            );
        };
        if *spec {
            assert!(
                spec_list.contains(&name.as_str()),
                "{ty} の spec fn `{name}` が PRELUDE_SPEC_FNS にない"
            );
            continue;
        }
        assert!(
            !spec_list.contains(&name.as_str()),
            "PRELUDE_SPEC_FNS の `{name}` は {ty} で spec でない関数の名前"
        );
        for call in [
            format!("{ty}::{name}({recv})"),
            format!("{recv}.{name}()"),
            format!("(*{recv}).{name}()"),
            format!("{{ {recv} }}.{name}()"),
            format!("matches!({recv}.{name}(), _)"),
        ] {
            let proof = RES.replace(
                "    leak p@;\n",
                &format!("    proof {{ {call}; }}\n    leak p@;\n"),
            );
            let d = run(RES, &proof);
            assert!(
                d.iter().any(|l| l.contains("E0520")),
                "`{call}` が E0520 にならない: {d:?}"
            );
        }
    }
    for s in spec_list {
        assert!(
            fns.iter()
                .any(|(t, n, sp)| n == s && *sp && !other_types.contains(&t.as_str())),
            "PRELUDE_SPEC_FNS の `{s}` が prelude の資源の型の spec fn にない"
        );
    }
}
