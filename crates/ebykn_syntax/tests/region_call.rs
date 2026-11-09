//! 呼び出しで領域を渡す構文（#19 の既定 P1）：引数の末尾に `uses` を置き、続けて領域を `,` で並べる。

mod support;
use support::*;

#[test]
fn region_arguments_follow_uses_at_the_end_of_a_call() {
    assert_eq!(
        e("build_chain(n, uses &mut r)"),
        "(call build_chain n (uses &mut r))"
    );
    assert_eq!(e("depth(x, uses &r)"), "(call depth x (uses &r))");
    assert_eq!(
        e("merge(x, y, uses &mut ra, &mut rb)"),
        "(call merge x y (uses &mut ra, &mut rb))"
    );
    assert_eq!(
        e("unlink(h, uses &mut self@nodes)"),
        "(call unlink h (uses &mut self@nodes))"
    );
    // 移動は `r`。実行時の引数がなくてもよい。末尾の `,` も書ける。
    assert_eq!(e("wrap(uses r)"), "(call wrap (uses r))");
    assert_eq!(e("wrap(x, uses r,)"), "(call wrap x (uses r))");
    // メソッドの呼び出しにも書ける。
    assert_eq!(
        e("self.helper(x, uses &self@nodes)"),
        "(.helper self x (uses &self@nodes))"
    );
}

#[test]
fn region_calls_nest_and_chain_like_rust() {
    assert_eq!(
        e("1 + max(depth(l, uses &r), depth(q, uses &r))"),
        "(+ 1 (call max (call depth l (uses &r)) (call depth q (uses &r))))"
    );
    assert_eq!(
        e("try_build(n, uses &mut r)?"),
        "(? (call try_build n (uses &mut r)))"
    );
    assert_eq!(e("f(x, uses &r).g()"), "(.g (call f x (uses &r)))");
}

#[test]
fn uses_outside_the_arguments_of_a_call_is_rejected() {
    // 呼び出しの外で `uses` の後が `;` で終わるものは、並びの終わりを読めないとして拒む。
    rejects(
        "fn f() { let t = uses &r; }",
        "closed by the `)` of the call",
    );
    rejects("fn f() { uses &mut r; }", "closed by the `)` of the call");
    for src in [
        "fn f() { let t = (a, uses &r); }",
        "fn f() { let t = [uses &r]; }",
        "fn f() { let t = (uses &mut r); }",
    ] {
        let d = diags(src);
        assert!(
            d.iter()
                .any(|d| d.contains("E0001") && d.contains("only at the end of the arguments")),
            "{d:?}\n---\n{src}"
        );
    }
}

#[test]
fn only_region_expressions_can_follow_uses() {
    for src in [
        "fn f() { g(uses &mut r.x); }",
        "fn f() { g(uses &mut r[0]); }",
        "fn f() { g(uses &self); }",
        "fn f() { g(uses &1); }",
        "fn f() { g(uses r s); }",
    ] {
        rejects(src, "a region after `uses`");
    }
    // `uses` の後は全部が領域なので、実行時の引数を後ろに書くと領域として読む（検査が拒む）。
    assert_eq!(e("g(uses &r, x)"), "(call g (uses &r, x))");
}

#[test]
fn uses_is_a_reserved_word() {
    for src in [
        "fn f() { let uses = 1; }",
        "fn f(uses: u64) { }",
        "fn uses() { }",
        "struct S { uses: u64 }",
        "fn f(s: S) -> u64 { s.uses }",
        "fn f() { uses::g(); }",
        "fn f() { g(uses); }",
        "fn f() { uses(1); }",
    ] {
        rejects(src, "`uses` is a keyword in Ebykn");
    }
    // `r#uses` は名前に使える。
    ok("fn f() { let r#uses = 1; }");
    // 契約の句の `uses` は今までどおり。
    ok("fn f(p: Ptr<u64>) uses &mut p@, r: &mut Region<Ptr<u64>> { }");
}
