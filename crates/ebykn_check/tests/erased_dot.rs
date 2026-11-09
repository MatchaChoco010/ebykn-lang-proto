//! 消去フィールドを `.` で書いたものの拒否（E0404。#99）。
//!
//! `self.nodes` などは、書き下しの後の `Tracked` / `Ghost` のフィールドに届き、領域の権限を捨てたり
//! 差し替えたりできた（`self.nodes = Tracked(Region::empty());` が verified になった）。受け手によらず、
//! 構造体リテラルとパターンのフィールドの名前でも拒否する。

mod support;
use ebykn_syntax::diag::Lang;
use support::*;

/// 診断が一つ以上あり、どれも `code` で、文言が `needle` を含む。
#[track_caller]
fn rejects_all(src: &str, code: &str, needle: &str) {
    let (_, lines) = run(src);
    assert!(!lines.is_empty(), "拒否されない:\n{src}");
    for l in &lines {
        assert!(
            l.contains(&format!(": {code}: ")) && l.contains(needle),
            "{code} / {needle:?} でない診断:\n{}\n---\n{src}",
            lines.join("\n")
        );
    }
}

#[test]
fn the_reported_program_is_rejected() {
    // #99 の再現。`self.nodes` は E0404、`Tracked(..)` は Verus の ghost の包み（E0001）。
    let src = "pub struct Node { val: u64 }
pub struct S { } owns { nodes: Region<Ptr<Node>> }
impl S {
    pub fn add(&mut self, v: u64) { let n = Ptr::new(Node { val: v }); put self@nodes[n]; }
    pub fn wipe(&mut self) { self.nodes = Tracked(Region::empty()); }
}
";
    let c = codes(src);
    assert!(c.iter().any(|c| c == "E0404"), "{c:?}");
    assert!(c.iter().any(|c| c == "E0001"), "{c:?}");
}

#[test]
fn e0404_for_every_receiver() {
    for body in [
        // 受け手が self。
        "pub fn f(&mut self) { let x = self.len; let r = &self.nodes; }",
        "pub fn f(&mut self) { let r = &mut self.nodes; }",
        "pub fn f(&self) -> bool { self.order.len() == 0 }",
        "pub fn f(&mut self) { let r = (*self).nodes; }",
        "pub fn f(&mut self) { let r = (self).nodes; }",
        // 型を書いた引数と変数。
        "pub fn f(other: &mut Pool) { let r = &mut other.nodes; }",
        "pub fn f(other: Pool) { let Pool { len, @.. } = other; let o: &Pool = &other; o.order; }",
        // 権限付きの `*p`。
        "pub fn f(p: Ptr<Pool>) uses &mut p@ { let r = &mut (*p).nodes; }",
        "pub fn f(p: Ptr<Pool>) uses &mut p@ { (*p).order = (*p).order; }",
        // `std::mem::swap` / `replace` に渡す。
        "pub fn f(&mut self, o: &mut Pool) { std::mem::swap(&mut self.nodes, &mut o.nodes); }",
        "pub fn f(&mut self, o: &mut Pool) { std::mem::swap(&mut self.order, &mut o.order); }",
        // 関数に `&mut` で渡す。
        "pub fn f(&mut self) { g(&mut self.nodes); }",
        // 型が分からない受け手。
        "pub fn f() { let r = Pool::new().nodes; }",
        "pub fn f(v: Vec<Pool>) { let r = &v[0].order; }",
        "pub fn f() { let b: _ = Pool::new(); let r = b.order; }",
        // 中を読まないマクロの引数。
        "pub fn f(&self) { println!(\"{:?}\", self.order); }",
        // 論理文脈。
        "pub fn f(&mut self) ensures final(self).order == old(self).order, { }",
        "pub fn f(&self) -> bool requires self.nodes.len() == 0, { true }",
    ] {
        let src = in_pool(body);
        rejects_all(&src, "E0404", "cannot be written with `.`");
    }
}

#[test]
fn e0404_through_types_that_may_reach_the_struct() {
    // `Box<Pool>`、型の別名、フィールドを持たない構造体（`Deref` で届きうる）、フィールドの連鎖。
    for src in [
        format!("{PRELUDE}pub fn f(b: Box<Pool>) {{ let r = &b.order; }}"),
        format!("{PRELUDE}type P2 = Pool;\npub fn f(b: &P2) {{ let r = &b.order; }}"),
        format!("{PRELUDE}pub struct W {{ p: Pool }}\npub fn f(w: &W) {{ let r = &w.order; }}"),
        format!("{PRELUDE}pub struct W {{ p: Pool }}\npub fn f(w: &W) {{ let r = &w.p.order; }}"),
    ] {
        rejects_all(&src, "E0404", "`order`");
    }
}

#[test]
fn e0404_in_literals_and_patterns() {
    // `..base` があっても、消去フィールドを名前で与えるのは `@` の形だけ。E0402 の欠落とは数えない。
    rejects(
        &in_pool("pub fn f(b: Pool) -> Pool { Pool { len: 0, nodes: b.len, ..b } }"),
        "E0404",
        "write `@nodes: ..`",
    );
    rejects(
        &in_pool(
            "pub fn new() -> Self { Pool { len: 0, nodes: Region::empty(), @order: Seq::empty() } }",
        ),
        "E0404",
        "write `@nodes: ..`",
    );
    rejects(
        &in_pool("pub fn f(x: Pool) { let Pool { len, nodes, @.. } = x; }"),
        "E0404",
        "write `@nodes`",
    );
    rejects(
        &in_pool("pub fn f(x: Pool) { let Pool { len, nodes: n, @order } = x; }"),
        "E0404",
        "write `@nodes`",
    );
    rejects(
        &in_pool("pub fn f(x: &mut Pool) { match x { Pool { order, @.. } => {} } }"),
        "E0404",
        "write `@order`",
    );
    // 分解代入の左辺はリテラルの形。
    rejects(
        &in_pool("pub fn f(x: Pool) { let mut n = 0; Pool { len: n, order: _, .. } = x; }"),
        "E0404",
        "write `@order: ..`",
    );
}

#[test]
fn other_fields_and_erased_forms_are_accepted() {
    accepts(&in_pool(
        "pub fn f(&self) -> u64 requires self@nodes.len() == 0, { self.len }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self) -> u64 ensures final(self)@order == old(self)@order, { self.len }",
    ));
    // 同じ名前の、ほかの構造体の通常のフィールド（受け手の型が分かる）。
    accepts(&format!(
        "{PRELUDE}pub struct A {{ nodes: u64 }}\npub fn f(a: A, b: &A) -> u64 {{ a.nodes + b.nodes }}"
    ));
    accepts(&format!(
        "{PRELUDE}pub struct A {{ nodes: u64 }}\npub fn f(a: A) -> u64 {{ let b: A = a; b.nodes }}"
    ));
    // クレートの構造体でない型の同じ名前のフィールド。
    accepts(
        "pub struct S { } spec { start: nat }\npub fn f(r: std::ops::Range<u64>) -> u64 { r.start }",
    );
    // 同じ名前のメソッドの呼び出しと、範囲の `..`。
    accepts(&in_pool(
        "pub fn f(&self, v: Vec<u64>, order: usize) { self.nodes(); println!(\"{}\", self.order()); let w = &v[0..order]; }\npub fn nodes(&self) {}\npub fn order(&self) -> u64 { 0 }",
    ));
}

#[test]
fn the_message_is_in_both_languages() {
    let src = in_pool("pub fn f(&mut self) { let r = &mut self.nodes; }");
    let (p, d) = ebykn_check::project::single(&src);
    assert!(d.is_empty(), "{d:?}");
    let c = ebykn_check::check(&p);
    assert_eq!(c.diags.len(), 1, "{:?}", c.diags);
    let f = p.file(c.diags[0].span.file);
    let en = c.diags[0].render(f, Lang::En);
    let ja = c.diags[0].render(f, Lang::Ja);
    assert!(en.contains("E0404") && en.contains("`self@nodes`"), "{en}");
    assert!(
        ja.contains("E0404") && ja.contains("`.` では書けません") && ja.contains("`self@nodes`"),
        "{ja}"
    );
}
