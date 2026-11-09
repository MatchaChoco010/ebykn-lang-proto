//! 権限スコープと線形性（仕様 §6.2、E0503〜E0512）。各規則に受理する例と拒否する例。

mod support;
use support::*;

#[test]
fn e0503_take_while_held() {
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { take self@nodes[p]; put self@nodes[p]; }",
    ));
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { take self@nodes[p]; take self@nodes[p]; put self@nodes[p]; }",
        ),
        "E0503",
        "already held",
    );
    // borrow で借りる権限をすでに持っている。
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { take self@nodes[p]; borrow self@nodes[p] { } put self@nodes[p]; }",
        ),
        "E0503",
        "`p@`",
    );
}

#[test]
fn e0504_consume_requires_owned() {
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) -> u64 { take self@nodes[p]; let n = p.free(); n.val }",
    ));
    // 解放した後にもう一度解放する。
    rejects(
        &in_pool(
            "pub fn f() { let p = Ptr::new(Node { val: 1 }); let a = p.free(); let b = p.free(); }",
        ),
        "E0504",
        "not held",
    );
    // 取り出していない権限を戻す。
    rejects(
        &in_pool("pub fn f(&mut self, p: Ptr<Node>) { put self@nodes[p]; }"),
        "E0504",
        "put it back",
    );
    // 共有の借用は解放できない。
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) uses &p@ { let n = p.free(); }"),
        "E0504",
        "borrowed (shared)",
    );
    // 移動した後に leak する。
    rejects(
        &format!(
            "{PRELUDE}fn eat(p: Ptr<Node>) uses p@ {{ leak p@; }}\nfn f() {{ let p = Ptr::new(Node {{ val: 1 }}); eat(p); leak p@; }}"
        ),
        "E0504",
        "leak",
    );
}

#[test]
fn e0505_owned_left_at_exit() {
    accepts(&in_pool(
        "pub fn f() { let p = Ptr::new(Node { val: 1 }); leak p@; }",
    ));
    // gives の戻り値は残してよい。
    accepts(&in_pool(
        "pub fn f() -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: 1 }); q }",
    ));
    rejects(
        &in_pool("pub fn f() { let p = Ptr::new(Node { val: 1 }); }"),
        "E0505",
        "`p@`",
    );
    // `uses p@`（移動）の引数は使い切る。
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) uses p@ { }"),
        "E0505",
        "`p@`",
    );
    // `return` と `?` でも検査する。
    rejects(
        &in_pool(
            "pub fn f(c: bool) { let p = Ptr::new(Node { val: 1 }); if c { return; } leak p@; }",
        ),
        "E0505",
        "`p@`",
    );
    rejects(
        &in_pool(
            "pub fn f(x: Option<u64>) -> Option<u64> { let p = Ptr::new(Node { val: 1 }); let y = x?; leak p@; Some(y) }",
        ),
        "E0505",
        "`p@`",
    );
    // gives の戻り値を持たずに戻る。
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) -> (q: Ptr<Node>) gives q@ { p }"),
        "E0504",
        "return it with `gives`",
    );
}

#[test]
fn e0506_assign_while_held() {
    accepts(&in_pool("pub fn f(q: Ptr<Node>) { let mut p = q; p = q; }"));
    rejects(
        &in_pool(
            "pub fn f(q: Ptr<Node>) { let mut p = Ptr::new(Node { val: 1 }); p = q; leak p@; }",
        ),
        "E0506",
        "assign to `p`",
    );
}

#[test]
fn e0507_rebind_while_held() {
    accepts(&in_pool(
        "pub fn f() { let p = Ptr::new(Node { val: 1 }); leak p@; let p = 5u64; }",
    ));
    rejects(
        &in_pool("pub fn f() { let p = Ptr::new(Node { val: 1 }); let p = 5u64; leak p@; }"),
        "E0507",
        "bind `p` again",
    );
    // match の腕のパターンでも。
    rejects(
        &in_pool(
            "pub fn f(x: Option<u64>) { let p = Ptr::new(Node { val: 1 }); match x { Some(p) => {}, None => {} } leak p@; }",
        ),
        "E0507",
        "`p`",
    );
}

#[test]
fn e0508_write_through_shared() {
    accepts(&in_pool(
        "pub fn f(p: Ptr<Node>) uses &mut p@ { (*p).val = 2; }",
    ));
    accepts(&in_pool(
        "pub fn f(p: Ptr<Node>) -> u64 uses &p@ { (*p).val }",
    ));
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) uses &p@ { (*p).val = 2; }"),
        "E0508",
        "only borrowed (shared)",
    );
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) uses &p@ { *p = Node { val: 2 }; }"),
        "E0508",
        "`p`",
    );
    // 共有の借用を `uses &mut` の引数に貸す。
    rejects(
        &format!(
            "{PRELUDE}fn set(p: Ptr<Node>) uses &mut p@ {{ (*p).val = 1; }}\nfn f(p: Ptr<Node>) uses &p@ {{ set(p); }}"
        ),
        "E0508",
        "`p`",
    );
}

#[test]
fn e0509_consume_inside_borrow() {
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) -> u64 { borrow self@nodes[p] { (*p).val } }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { borrow mut self@nodes[p] { (*p).val = 3; } }",
    ));
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { borrow self@nodes[p] { let n = p.free(); } }",
        ),
        "E0509",
        "inside `borrow` of `p`",
    );
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>) { borrow mut self@nodes[p] { put self@nodes[p]; } }",
        ),
        "E0509",
        "put it back",
    );
    rejects(
        &format!(
            "{PRELUDE}fn eat(p: Ptr<Node>) uses p@ {{ leak p@; }}\nimpl Pool {{ pub fn f(&mut self, p: Ptr<Node>) {{ borrow mut self@nodes[p] {{ eat(p); }} }} }}"
        ),
        "E0509",
        "move it",
    );
}

#[test]
fn e0510_branches_must_agree() {
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, c: bool) { take self@nodes[p]; if c { put self@nodes[p]; } else { put self@nodes[p]; } }",
    ));
    // 発散する分岐は比べない。
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, c: bool) { take self@nodes[p]; if c { put self@nodes[p]; } else { panic!(\"no\"); } }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, x: Option<u64>) { take self@nodes[p]; match x { Some(_) => { put self@nodes[p]; } None => { put self@nodes[p]; return; } } }",
    ));
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, c: bool) { take self@nodes[p]; if c { put self@nodes[p]; } }",
        ),
        "E0510",
        "`p@` (not held / owned)",
    );
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, x: Option<u64>) { match x { Some(_) => { take self@nodes[p]; } None => {} } }",
        ),
        "E0510",
        "`p@`",
    );
}

#[test]
fn e0511_loops_must_agree() {
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { take self@nodes[p]; put self@nodes[p]; i = i + 1; } }",
    ));
    accepts(&in_pool(
        "pub fn f(&mut self, p: Ptr<Node>) { take self@nodes[p]; loop { put self@nodes[p]; break; } }",
    ));
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { take self@nodes[p]; i = i + 1; } }",
        ),
        "E0511",
        "end of the loop body",
    );
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { take self@nodes[p]; if i == 0 { continue; } put self@nodes[p]; i = i + 1; } }",
        ),
        "E0511",
        "`continue`",
    );
    rejects(
        &in_pool(
            "pub fn f(&mut self, p: Ptr<Node>, n: u64) { let mut i = 0; while i < n { take self@nodes[p]; if i == 0 { break; } put self@nodes[p]; i = i + 1; } }",
        ),
        "E0511",
        "`break`",
    );
}

#[test]
fn e0512_no_permissions_in_closures() {
    accepts(&in_pool(
        "pub fn f(x: u64) -> u64 { let g = |y: u64| y + x; g(1) }",
    ));
    // ポインタの値（`Copy`）を使うだけなら権限は使わない。
    accepts(&in_pool(
        "pub fn f(p: Ptr<Node>) -> usize uses &p@ { let g = || p.addr(); g() }",
    ));
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) uses &p@ { let g = || (*p).val; }"),
        "E0512",
        "uses `p`",
    );
    rejects(
        &in_pool("pub fn f(&mut self, p: Ptr<Node>) { let g = || { take self@nodes[p]; }; }"),
        "E0512",
        "self@nodes",
    );
}

#[test]
fn unchecked_fn_bodies_are_not_checked() {
    accepts(&in_pool(
        "unchecked fn f() { let p = Ptr::new(Node { val: 1 }); }",
    ));
}

#[test]
fn calls_through_use_and_modules_are_resolved() {
    // `use` で取り込んだ関数とモジュールのパスの呼び出しにも `uses` を当てる。
    rejects(
        &format!(
            "{PRELUDE}mod m {{ pub fn eat(p: Ptr<super::Node>) uses p@ {{ leak p@; }} }}\nuse m::eat as gulp;\nfn f() {{ let p = Ptr::new(Node {{ val: 1 }}); gulp(p); gulp(p); }}"
        ),
        "E0504",
        "move it",
    );
    rejects(
        &format!(
            "{PRELUDE}mod m {{ pub fn eat(p: Ptr<super::Node>) uses p@ {{ leak p@; }} }}\nfn f() {{ let p = Ptr::new(Node {{ val: 1 }}); m::eat(p); crate::m::eat(p); }}"
        ),
        "E0504",
        "move it",
    );
}
