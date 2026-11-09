//! 権限以外の §5・§6 の規則（E0402、E0501、E0502、E0513〜E0516、領域の認識）と未実装の機能。

mod support;
use support::*;

#[test]
fn e0501_uses_and_gives_targets() {
    accepts(&in_pool("pub fn f(p: Ptr<Node>) uses p@ { leak p@; }"));
    accepts(&in_pool("pub fn f(p: &Ptr<Node>) uses &p@ { }"));
    accepts(&format!("{PRELUDE}fn f(c: Cell<u64>) uses &mut c@ {{ }}"));
    accepts(&in_pool(
        "pub fn f() -> (q: Ptr<Node>) gives q@ { let q = Ptr::new(Node { val: 1 }); q }",
    ));
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) uses &q@ { }"),
        "E0501",
        "`q` is not a parameter",
    );
    rejects(
        &in_pool("pub fn f(p: Option<Ptr<Node>>) uses &p@ { }"),
        "E0501",
        "directly",
    );
    rejects(
        &in_pool("pub fn f(p: &mut Ptr<Node>) uses &mut p@ { }"),
        "E0501",
        "directly",
    );
    rejects(
        &in_pool("pub fn f(r: u64) uses x: Seq<u64> { }"),
        "E0501",
        "Region",
    );
    rejects(
        &in_pool("pub fn f() -> Ptr<Node> gives q@ { Ptr::empty() }"),
        "E0501",
        "named return value",
    );
    rejects(
        &in_pool("pub fn f() -> (q: Option<Ptr<Node>>) gives q@ { None }"),
        "E0501",
        "the return value `q`",
    );
}

#[test]
fn e0502_region_index_is_an_identifier() {
    rejects(
        &in_pool("pub fn f(&mut self, p: (Ptr<Node>, u64)) { take self@nodes[p.0]; }"),
        "E0502",
        "variable name",
    );
}

#[test]
fn regions_are_owns_fields_or_region_variables() {
    accepts(&format!(
        "{PRELUDE}fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> {{ take r[p]; put r[p]; }}"
    ));
    rejects(
        &format!("{PRELUDE}fn f(p: Ptr<Node>, r: u64) {{ take r[p]; }}"),
        "E0001",
        "`r` is not a region",
    );
    rejects(
        &in_pool("pub fn f(&mut self, p: Ptr<Node>) { take self@len[p]; }"),
        "E0001",
        "`len` is not a region",
    );
}

#[test]
fn e0513_spec_field_as_region() {
    rejects(
        &in_pool("pub fn f(&mut self, p: Ptr<Node>) { take self@order[p]; }"),
        "E0513",
        "`self@order` is a `spec` field",
    );
    // 論理式の添字は通常の添字。
    accepts(&in_pool(
        "pub closed spec fn first(&self) -> Ptr<Node> = self@order[0];",
    ));
}

#[test]
fn e0514_method_call_of_a_permission_function() {
    let src = |call: &str| {
        format!(
            "{PRELUDE}impl Pool {{ pub fn touch(&self, p: Ptr<Node>) uses &p@ {{ }} }}\nfn f(pool: &Pool, p: Ptr<Node>) uses &p@ {{ {call} }}"
        )
    };
    accepts(&src("Pool::touch(pool, p);"));
    rejects(&src("pool.touch(p);"), "E0514", "`x.touch(..)`");
}

#[test]
fn e0515_self_erased_without_self() {
    accepts(&in_pool("pub fn f(&self) -> nat { 0 }"));
    rejects(
        &in_pool("pub fn f() { self@order = Seq::empty(); }"),
        "E0515",
        "takes `self`",
    );
    rejects(
        &in_pool("pub fn f(p: Ptr<Node>) { take self@nodes[p]; }"),
        "E0515",
        "takes `self`",
    );
}

#[test]
fn e0516_assign_to_owns_field() {
    accepts(&in_pool(
        "pub fn f(&mut self) { self@order = Seq::empty(); }",
    ));
    rejects(
        &in_pool("pub fn f(&mut self) { self@nodes = Region::empty(); }"),
        "E0516",
        "`owns` field `nodes`",
    );
    rejects(
        &in_pool("pub fn f(x: &mut Pool) { x@nodes = Region::empty(); }"),
        "E0516",
        "`nodes`",
    );
}

#[test]
fn e0402_erased_fields_in_literals_and_patterns() {
    accepts(&in_pool(
        "pub fn new() -> Self { Pool { len: 0, @nodes: Region::empty(), @order: Seq::empty() } }",
    ));
    accepts(&in_pool(
        "pub fn f(&self, base: Pool) -> Pool { Pool { len: 1, ..base } }",
    ));
    accepts(&in_pool(
        "pub fn f(p: Pool) -> u64 { let Pool { len, @nodes, .. } = p; len }",
    ));
    accepts(&in_pool(
        "pub fn f(p: Pool) -> u64 { let Pool { len, @.. } = p; len }",
    ));
    rejects(
        &in_pool("pub fn new() -> Self { Pool { len: 0, @nodes: Region::empty() } }"),
        "E0402",
        "erased field `order`",
    );
    rejects(
        &in_pool("pub fn f(p: Pool) -> u64 { let Pool { len, .. } = p; len }"),
        "E0402",
        "omits the `owns` field `nodes`",
    );
    rejects(
        &in_pool(
            "pub fn new() -> Self { Pool { @len: 0, @nodes: Region::empty(), @order: Seq::empty() } }",
        ),
        "E0402",
        "`len` is not an `owns` or `spec` field",
    );
}

#[test]
fn unimplemented_features_fail_loudly() {
    rejects(
        "pub struct M { a: u64 }\npub fn finite(m: &M) -> bool { true }\ntype F = M where finite(&self);\n",
        "E0003",
        "subset types",
    );
    rejects(
        "trait T { fn f(&self) -> (r: u64) ensures r > 0; }\n",
        "E0003",
        "traits with contracts",
    );
    rejects(
        "trait T { spec fn f(&self) -> u64; }\n",
        "E0003",
        "traits with contracts",
    );
    rejects(
        &format!(
            "{PRELUDE}fn g(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> {{ }}\nfn f(p: Ptr<Node>) {{ g(p); }}"
        ),
        "E0003",
        "named erased parameter",
    );
    // 契約のない trait は Rust のまま通す。
    accepts("trait T { fn f(&self) -> u64; }\n");
}

#[test]
fn invariant_impl_shape() {
    accepts(&format!(
        "{PRELUDE}impl Invariant for Pool {{ closed spec fn invariant(&self) -> bool = self.len > 0; }}"
    ));
    rejects(
        &format!("{PRELUDE}impl Invariant for Pool {{ spec fn inv(&self) -> bool = true; }}"),
        "E0001",
        "impl Invariant for T",
    );
}
