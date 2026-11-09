//! 呼び出しで領域を渡す（#19 の既定 P1）：呼ばれる側の宣言との照合（E0518）と、渡した領域の
//! 状態（E0519）。

mod support;
use support::*;

/// 領域を受け取る補助関数（移動・共有・可変）。
const HELPERS: &str = "\
fn take_all(n: u64) uses r: Region<Ptr<Node>> { }
fn read(n: u64) uses r: &Region<Ptr<Node>> { }
fn write(n: u64) uses r: &mut Region<Ptr<Node>> { }
fn two(n: u64) uses a: &mut Region<Ptr<Node>>, b: &Region<Ptr<Node>> { }
fn mixed(p: Ptr<Node>) uses &mut p@, r: &mut Region<Ptr<Node>> { }
";

fn with_helpers(body: &str) -> String {
    format!("{PRELUDE}{HELPERS}{body}\n")
}

const LOCAL: &str = "let mut r: Region<Ptr<Node>> = Region::empty();";

#[test]
fn regions_are_passed_by_move_shared_or_mut() {
    accepts(&with_helpers(&format!(
        "fn f() {{ {LOCAL} read(1, uses &r); write(1, uses &mut r); take_all(1, uses r); }}"
    )));
    accepts(&with_helpers(&format!(
        "fn f() {{ {LOCAL} let mut s: Region<Ptr<Node>> = Region::empty(); two(1, uses &mut r, &s); }}"
    )));
    // 権限と領域が混ざった宣言：権限は実行時の引数に付き、領域は `uses` の後に並べる。
    accepts(&with_helpers(&format!(
        "fn f() {{ {LOCAL} let p = Ptr::new(Node {{ val: 1 }}); mixed(p, uses &mut r); let _ = p.free(); }}"
    )));
    // `uses` で受けた参照の領域を、そのまま貸す（再帰）。
    accepts(&with_helpers(
        "fn g(n: u64) uses r: &mut Region<Ptr<Node>> decreases n { if n > 0 { read(n, uses &r); g(n - 1, uses &mut r); } }",
    ));
    // `owns` フィールドを貸す。
    accepts(&with_helpers(
        "impl Pool { fn f(&mut self) { write(1, uses &mut self@nodes); read(1, uses &self@nodes); } }",
    ));
}

#[test]
fn e0518_counts_and_modes_must_match_the_declaration() {
    // 宣言が領域を受け取るのに渡していない（今までの E0003 の位置）。
    rejects(
        &with_helpers("fn f() { write(1); }"),
        "E0518",
        "`write` takes regions; pass them at the end of the arguments as `uses &mut r`",
    );
    rejects(
        &with_helpers(&format!("fn f() {{ {LOCAL} two(1, uses &mut r); }}")),
        "E0518",
        "`two` takes 2 regions, `uses &mut a, &b` in this order, but this call passes 1",
    );
    // 受け取らない関数に渡した。
    rejects(
        &with_helpers(&format!(
            "fn plain(n: u64) {{ }}\nfn f() {{ {LOCAL} plain(1, uses &r); }}"
        )),
        "E0518",
        "`plain` takes no regions; remove `uses ..` from this call",
    );
    rejects(
        &with_helpers(&format!("fn f() {{ {LOCAL} write(1, uses &r); }}")),
        "E0518",
        "pass it as a mutable borrow (`&mut r`) (this call passes it as a shared borrow",
    );
    rejects(
        &with_helpers(&format!("fn f() {{ {LOCAL} read(1, uses &mut r); }}")),
        "E0518",
        "`uses r: &Region<..>`",
    );
    rejects(
        &with_helpers(&format!("fn f() {{ {LOCAL} take_all(1, uses &mut r); }}")),
        "E0518",
        "pass it as a move (`r`)",
    );
    // 順序の取り違え（`&mut` と `&` の位置）は、両方の位置で渡し方が食い違う。
    assert_eq!(
        codes(&with_helpers(&format!(
            "fn f() {{ {LOCAL} let mut s: Region<Ptr<Node>> = Region::empty(); two(1, uses &s, &mut r); }}"
        ))),
        ["E0518", "E0518"]
    );
    // `unchecked fn` の中でも、宣言との照合は止めない（型の誤り）。
    rejects(
        &with_helpers("unchecked fn f() { write(1); }"),
        "E0518",
        "`write` takes regions",
    );
}

#[test]
fn e0519_moved_regions_cannot_be_used() {
    rejects(
        &with_helpers(&format!(
            "fn f() {{ {LOCAL} take_all(1, uses r); read(1, uses &r); }}"
        )),
        "E0519",
        "the region `r` was moved",
    );
    // `take` は取り出せないので、続く `leak` も権限がないとして拒む。
    assert_eq!(
        codes(&with_helpers(&format!(
            "fn f(p: Ptr<Node>) {{ {LOCAL} take_all(1, uses r); take r[p]; leak p@; }}"
        ))),
        ["E0519", "E0504"]
    );
    // 束縛し直せば使える。
    accepts(&with_helpers(&format!(
        "fn f() {{ {LOCAL} take_all(1, uses r); let mut r: Region<Ptr<Node>> = Region::empty(); read(1, uses &r); }}"
    )));
    // 借用は呼び出しの間だけ。
    accepts(&with_helpers(&format!(
        "fn f() {{ {LOCAL} write(1, uses &mut r); write(2, uses &mut r); take_all(1, uses r); }}"
    )));
}

#[test]
fn unchecked_fns_do_not_get_e0519() {
    // `unchecked fn` の本体は §6.2 の検査をしない（E0519 も出さない）。宣言との照合（E0518）は出す。
    accepts(&with_helpers(&format!(
        "unchecked fn f() {{ {LOCAL} take_all(1, uses r); read(1, uses &r); }}"
    )));
    accepts(&with_helpers(
        "unchecked fn f() uses r: &Region<Ptr<Node>> { write(1, uses &mut r); take_all(1, uses r); }",
    ));
    rejects(
        &with_helpers(&format!(
            "unchecked fn f() {{ {LOCAL} read(1, uses &mut r); }}"
        )),
        "E0518",
        "`uses r: &Region<..>`",
    );
}

#[test]
fn e0519_borrowed_regions_cannot_be_moved_or_upgraded() {
    rejects(
        &with_helpers("fn f() uses r: &mut Region<Ptr<Node>> { take_all(1, uses r); }"),
        "E0519",
        "`r` is borrowed, not owned",
    );
    rejects(
        &with_helpers("fn f() uses r: &Region<Ptr<Node>> { write(1, uses &mut r); }"),
        "E0519",
        "borrowed as shared",
    );
    rejects(
        &with_helpers("impl Pool { fn f(&mut self) { take_all(1, uses self@nodes); } }"),
        "E0519",
        "`self@nodes` is borrowed, not owned",
    );
    // 値で受けた領域は持っているので、移動できる。
    accepts(&with_helpers(
        "fn f() uses r: Region<Ptr<Node>> { take_all(1, uses r); }",
    ));
}

#[test]
fn e0519_a_region_is_lent_mutably_once_per_call() {
    rejects(
        &with_helpers(&format!("fn f() {{ {LOCAL} two(1, uses &mut r, &r); }}")),
        "E0519",
        "`r` is passed twice",
    );
    rejects(
        &with_helpers(
            "impl Pool { fn f(&mut self) { two(1, uses &mut self@nodes, &self@nodes); } }",
        ),
        "E0519",
        "`self@nodes` is passed twice",
    );
}

#[test]
fn only_regions_can_be_passed() {
    rejects(
        &with_helpers("fn f(x: u64) { read(1, uses &x); }"),
        "E0001",
        "`x` is not a region, so it cannot be passed after `uses`",
    );
    rejects(
        &with_helpers("impl Pool { fn f(&self) { read(1, uses &self@order); } }"),
        "E0513",
        "`self@order` is a `spec` field",
    );
    rejects(
        &with_helpers("fn f() { read(1, uses &self@nodes); }"),
        "E0515",
        "`self@..`",
    );
    rejects(
        &with_helpers(&format!(
            "fn f() {{ {LOCAL} let c = || read(1, uses &r); }}"
        )),
        "E0512",
        "`r`",
    );
}

#[test]
fn unresolved_callees_keep_the_regions() {
    // 呼ばれる側を引けない呼び出し（クロージャ、外の関数）にも、書いた領域を渡す。受け取らなければ
    // Verus が型の誤りとして拒む（黙って落とさない）。
    let (c, lines) = run(&with_helpers(&format!(
        "fn f(g: fn(u64)) {{ {LOCAL} g(1, uses &mut r); std::mem::drop(1, uses r); }}"
    )));
    assert!(lines.is_empty(), "{lines:?}");
    let calls: Vec<_> = c.facts.calls.values().cloned().collect();
    assert_eq!(
        calls,
        [
            vec![ebykn_check::facts::ExtraArg::Region(0)],
            vec![ebykn_check::facts::ExtraArg::Region(0)],
        ]
    );
}

#[test]
fn regions_in_macro_arguments_are_not_read() {
    rejects(
        &with_helpers(&format!(
            "fn f() {{ {LOCAL} println!(\"{{:?}}\", read(1, uses &r)); }}"
        )),
        "E0003",
        "permissions cannot be used in the arguments of this macro",
    );
}
