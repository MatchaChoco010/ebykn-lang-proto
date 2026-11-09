//! 文（仕様 §5.4）、構造体リテラルとパターン、クロージャ（仕様 §5.6）。

mod support;
use support::*;

#[test]
fn take_put_leak() {
    assert_eq!(e("take self@nodes[h];"), "(take self@nodes[h])");
    assert_eq!(e("put r[p];"), "(put r[p])");
    assert_eq!(e("leak p@;"), "(leak p)");
    // 添字が識別子でないもの（E0502）は M2 で検査する。構文としては読む。
    assert_eq!(e("take r[f(x)];"), "(take r[(call f x)])");
    // 文の終わりの `;` は要る。
    rejects(
        "fn f() { take r[p] }",
        "1:20: E0001: syntax error: expected `;`",
    );
    rejects("fn f() { let x = take r[p]; }", "E0001");
}

#[test]
fn borrow_blocks() {
    assert_eq!(
        e("borrow mut self@nodes[t] { (*t).next = Some(n); } x"),
        "(borrow mut self@nodes[t] {(= (. (paren (* t)) next) (call Some n));}) x"
    );
    assert_eq!(
        e("borrow r[h] { Some(&(*h).val) }"),
        "(borrow r[h] {(call Some (& (. (paren (* h)) val)))})"
    );
    // 付録 A：match の腕とブロックの末尾の式（`OPEN.md`）。
    assert_eq!(
        e("match x { Some(t) => borrow r[t] { 1 }, None => 2 }"),
        "(match x (Some(t) => (borrow r[t] {1})) (None => 2))"
    );
    // 領域添字の後にはブロックが要る。
    rejects("fn f() { borrow r[p]; }", "E0001");
}

#[test]
fn spec_field_assignment() {
    assert_eq!(
        e("self@order = self@order.push(n);"),
        "(spec= (@ self order) (.push (@ self order) n))"
    );
    // ふつうの代入とフィールドへの代入は Rust のまま。
    assert_eq!(
        e("self.head = Some(n);"),
        "(= (. self head) (call Some n));"
    );
}

#[test]
fn loop_clauses_in_any_order() {
    assert_eq!(
        e("while i < n invariant i <= n, decreases n - i invariant j { i += 1; }"),
        "(while (< i n) (invariant (<= i n)) (decreases (- n i)) (invariant j) {(+= i 1);})"
    );
    assert_eq!(
        e("for x in it invariant x > 0 { }"),
        "(for x it (invariant (> x 0)) {})"
    );
    assert_eq!(e("loop decreases n { }"), "(loop (decreases n) {})");
    assert_eq!(
        e("'outer: while a { break 'outer; }"),
        "(while a {(break);})"
    );
    assert_eq!(
        e("let x = loop { break 5; };"),
        "(let x (loop {(break 5);}))"
    );
    // match の腕のループ。
    assert_eq!(
        e("match x { A => while c invariant d { }, B => for i in 0..n invariant i < n { } }"),
        "(match x (A => (while c (invariant d) {})) (B => (for i (.. 0 n) (invariant (< i n)) {})))"
    );
    for (src, want) in [
        (
            "while x invariant_except_break y { }",
            "`invariant_except_break` is Verus syntax",
        ),
        ("while x ensures y { }", "`ensures` is Verus syntax"),
        (
            "loop invariant_ensures y { }",
            "`invariant_ensures` is Verus syntax",
        ),
    ] {
        rejects(&format!("fn f() {{ {src} }}"), want);
    }
}

#[test]
fn rust_statements() {
    assert_eq!(e("let h = self.head?;"), "(let h (? (. self head)))");
    assert_eq!(
        e("let Some(x) = y else { return; };"),
        "(let Some(x) y else {(return);})"
    );
    assert_eq!(
        e("if let Some(x) = y { x } else { 0 }"),
        "(if (let Some(x) y) {x} {0})"
    );
    assert_eq!(e("let x: u64 = 1; x"), "(let x: u64 1) x");
    assert_eq!(
        e("let n = Ptr::new(Node { val: v, next: None });"),
        "(let n (call Ptr::new (struct Node val:v next:None)))"
    );
    for (src, want) in [
        (
            "let ghost x = 1;",
            "`let ghost / let tracked` is Verus syntax",
        ),
        (
            "let tracked x = 1;",
            "`let ghost / let tracked` is Verus syntax",
        ),
    ] {
        rejects(&format!("fn f() {{ {src} }}"), want);
    }
}

#[test]
fn struct_literals_with_erased_fields() {
    assert_eq!(
        e("Stack { head: None, @nodes: Region::empty(), @order: Seq::empty() }"),
        "(struct Stack head:None @nodes:(call Region::empty) @order:(call Seq::empty))"
    );
    assert_eq!(e("Foo::<T> { a: 1, @b: 2 }"), "(struct Foo<T> a:1 @b:2)");
    assert_eq!(e("S { a, @r: x, ..base }"), "(struct S a:a @r:x ..base)");
    assert_eq!(e("S { a: 1, ..base }"), "(struct S a:1 ..base)");
    // `@name` だけの略記は文法にない（`@name: e`）。
    rejects("fn f() { S { @r, a } }", "E0001");
}

#[test]
fn struct_patterns_with_erased_fields() {
    assert_eq!(e("let S { a, @r, @q } = s;"), "(let S { a, @r, @q } s)");
    assert_eq!(
        e("let S { a: b, @r, @.. } = s;"),
        "(let S { a: b, @r, @.. } s)"
    );
    assert_eq!(e("let S { @r, .. } = s;"), "(let S { @r, .. } s)");
    assert_eq!(
        e("match s { S { a, @r } => 1, T { x, .. } => 2 }"),
        "(match s (S { a, @r } => 1) (T { x, .. } => 2))"
    );
    assert_eq!(
        e("let S { ref mut a, @r } = s;"),
        "(let S { ref mut a, @r } s)"
    );
}

#[test]
fn closures() {
    assert_eq!(
        e("let f = |x: u64| x + 1;"),
        "(let f (closure [x: u64] (+ x 1)))"
    );
    assert_eq!(e("let f = move || 1;"), "(let f (closure [] 1))");
    // 契約付きクロージャは Verus の構文をそのまま通す（OPEN-12）。
    assert_eq!(
        e("let f = |x: int| requires x > 0 ensures |r: int| r > 0 { x + 1 };"),
        "(let f (closure [x: int] (requires (> x 0)) (ensures (closure [r: int] (> r 0))) {(+ x 1)}))"
    );
    rejects(
        "fn f() { let g = |tracked x: u64| x; }",
        "`tracked` is Verus syntax; in Ebykn write `uses`",
    );
    rejects(
        "fn f() { let g = proof_fn|x: u64| x; }",
        "`proof_fn` is Verus syntax",
    );
}
