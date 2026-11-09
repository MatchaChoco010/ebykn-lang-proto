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
        &in_pool("pub fn f() -> Ptr<Node> gives q@ { unimplemented!() }"),
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
    // 型の不変条件を持つ型の `impl Drop` は、理由を言う文言にする（PR #76 のレビューの S4）。
    rejects(
        "pub struct S { a: u64 }\nimpl Invariant for S { spec fn invariant(&self) -> bool = self.a > 0; }\nimpl Drop for S { fn drop(&mut self) {} }\n",
        "E0003",
        "`impl Drop` for a type with an invariant",
    );
    // `drop` の契約（`no_panic` を含む）は trait の impl の契約として E0003。
    rejects(
        "pub struct S { a: u64 }\nimpl Drop for S { fn drop(&mut self) no_panic {} }\n",
        "E0003",
        "traits with contracts",
    );
    // std の `Drop` でない同じ名前のトレイトは、これまでどおり trait の契約として扱う。
    rejects(
        "pub trait Drop { fn drop(&mut self); }\npub struct S { a: u64 }\nimpl Invariant for S { spec fn invariant(&self) -> bool = self.a > 0; }\nimpl Drop for S { fn drop(&mut self) {} }\n",
        "E0003",
        "traits with contracts",
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

#[test]
fn e0004_bare_mut_parameters_in_ensures() {
    // #10 の決定（案 B）：`ensures` の `&mut` 引数は `final(..)` か `old(..)` で書く。
    let mut_self = "`self` is a `&mut` argument, so a postcondition (`ensures` / `returns`) must say which value it means: write `final(self)` for the value after the call or `old(self)` for the value at entry";
    rejects(
        &in_pool("fn f(&mut self) ensures self.len == old(self).len, { }"),
        "E0004",
        mut_self,
    );
    rejects(
        &in_pool("fn f(self: &mut Self) ensures self.len == 0, { }"),
        "E0004",
        "write `final(self)`",
    );
    rejects(
        &in_pool("fn f(&mut self) ensures self@order.len() == 0, { }"),
        "E0004",
        "write `final(self)`",
    );
    rejects(
        &in_pool("fn f(&mut self, p: Ptr<Node>) ensures self@nodes[p].val == 0, { }"),
        "E0004",
        "write `final(self)`",
    );
    rejects(
        &in_pool("fn f(&mut self) ensures self.view() == old(self).view(), { }"),
        "E0004",
        "write `final(self)`",
    );
    rejects(
        &format!("{PRELUDE}fn f(x: &mut u64) ensures *x == 1, {{ }}"),
        "E0004",
        "write `final(x)` for the value after the call or `old(x)`",
    );
    rejects(
        &format!("{PRELUDE}fn f(p: Ptr<Node>) uses &mut p@ ensures p@.is_init(), {{ }}"),
        "E0004",
        "`p@` refers to the permission borrowed with `uses &mut p@`, so a postcondition (`ensures` / `returns`) must say which value it means: write `final(p@)`",
    );
    rejects(
        &format!("{PRELUDE}fn f(p: Ptr<Node>) uses &mut p@ ensures (*p).val == 1, {{ }}"),
        "E0004",
        "write `final(*p)` for the value after the call or `old(*p)`",
    );
    rejects(
        &format!(
            "{PRELUDE}fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> ensures r.contains(p), {{ }}"
        ),
        "E0004",
        "write `final(r)`",
    );
    rejects(
        &format!("{PRELUDE}lemma fn l(x: &mut u64) ensures *x == 1;"),
        "E0004",
        "write `final(x)`",
    );
    // 一つの `ensures` に二つあれば二つ。
    assert_eq!(
        codes(&in_pool(
            "fn f(&mut self) ensures self.len == 0, self@order.len() == 0, { }"
        )),
        ["E0004", "E0004"]
    );
}

#[test]
fn e0004_accepts_old_final_requires_and_rebinding() {
    accepts(&in_pool(
        "fn f(&mut self, p: Ptr<Node>) requires self.len == 0, old(self)@nodes.contains(p), ensures final(self).len == old(self).len, final(self)@order.len() == 0, final(self)@nodes[p].val == 0, old(self@nodes)[p].val == 0, { }",
    ));
    accepts(&format!(
        "{PRELUDE}fn f(x: &mut u64, p: Ptr<Node>) uses &mut p@ requires *x == 0, p@.is_init(), ensures *final(x) == 1, final(p@).is_init(), final(*p).val == old(*p).val, {{ }}"
    ));
    accepts(&format!(
        "{PRELUDE}fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> ensures final(r).contains(p), final(r)[p].val == old(r)[p].val, {{ }}"
    ));
    // 束縛し直した同じ名前は外の引数を隠す。
    accepts(&format!(
        "{PRELUDE}fn f(x: &mut u64, s: Seq<u64>, o: Option<u64>) ensures forall x: u64, x >= 0, s.map(|i: int, x: u64| x) == s, match o {{ Some(x) => x > 0, None => true }}, {{ }}"
    ));
    // `&self` と値の `self`、`&T` の引数は対象外。
    accepts(&in_pool(
        "fn f(&self, x: &u64) ensures self.len == 0, *x == 0, { }",
    ));
}

#[test]
fn e0507_permissions_of_rebound_names_in_contracts() {
    // #10 のレビュー：書き下しは `p@` / `*p` をいつも外側の引数の権限にするので、`p` を束縛し直した
    // 中では、`old` / `final` の下でも拒む（束縛に従って検査をやめると、自明に真の条件が通る）。
    let rebound =
        "`p@` refers to the permission of the parameter `p`, but `p` is bound again around it";
    assert_eq!(
        codes(&format!(
            "{PRELUDE}pub fn f(p: Ptr<Node>, s: Seq<Ptr<Node>>) uses &mut p@ requires p@.is_init(), s.contains(p), (*p).val == 0, ensures final(p@).is_init(), forall p in s, p@ == old(p@), {{ (*p).val = 5; }}"
        )),
        ["E0507", "E0507"]
    );
    rejects(
        &format!(
            "{PRELUDE}pub fn f(p: Ptr<Node>, s: Seq<Ptr<Node>>) uses &mut p@ ensures final(p@).is_init(), forall p in s, p@.is_init(), {{ }}"
        ),
        "E0507",
        rebound,
    );
    // `requires` でも、共有の借用でも、`*p` でも同じ。
    rejects(
        &format!(
            "{PRELUDE}pub fn f(p: Ptr<Node>, s: Seq<Ptr<Node>>) uses &p@ requires forall p in s, (*p).val == 0, {{ }}"
        ),
        "E0507",
        "`*p` refers to the permission of the parameter `p`",
    );
    // クロージャの引数、`match` の腕、`let` の文は、本体と同じ検査も束縛そのものを E0507 にする
    // （位置は束縛と `p@` の二つ）。量化子の束縛子はこの検査だけが見る。
    let lines = run(&format!(
        "{PRELUDE}pub fn f(p: Ptr<Node>, s: Seq<Ptr<Node>>) uses &p@ requires s.map(|i: int, p: Ptr<Node>| p@.is_init()) == s, {{ }}"
    ))
    .1;
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[1].contains(rebound), "{lines:?}");
    let lines = run(&format!(
        "{PRELUDE}pub fn f(p: Ptr<Node>, o: Option<Ptr<Node>>) uses &p@ requires match o {{ Some(p) => p@.is_init(), None => true }}, {{ }}"
    ))
    .1;
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[1].contains(rebound), "{lines:?}");
    let lines = run(&format!(
        "{PRELUDE}pub fn f(p: Ptr<Node>, q: Ptr<Node>) uses &p@ requires {{ let p = q; p@.is_init() }}, {{ }}"
    ))
    .1;
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[1].contains(rebound), "{lines:?}");
    // `gives` の権限も、事後条件では外側の名前（戻り値の権限）を指す。
    rejects(
        &format!(
            "{PRELUDE}pub fn f(s: Seq<Ptr<Node>>) -> (q: Ptr<Node>) gives q@ ensures q@.is_init(), forall q in s, q@.is_init(), {{ let q = Ptr::new(Node {{ val: 1 }}); q }}"
        ),
        "E0507",
        "`q@` refers to the permission of the parameter `q`",
    );
    // 束縛し直しても `p@` / `*p` を使わなければ通す。
    accepts(&format!(
        "{PRELUDE}pub fn f(p: Ptr<Node>, s: Seq<Ptr<Node>>) uses &mut p@ ensures final(p@).is_init(), forall p in s, s.contains(p), {{ }}"
    ));
}

#[test]
fn e0004_returns_and_macro_tokens() {
    // `returns e` は事後条件（`ensures r == e`）。
    rejects(
        &format!(
            "{PRELUDE}spec fn g(y: &mut u64) -> u64 = *y;\nfn ret4(x: &mut u64) -> u64 requires *x == 3, returns g(x) {{ *x = 7; 3 }}"
        ),
        "E0004",
        "write `final(x)`",
    );
    accepts(&format!(
        "{PRELUDE}fn ret4(x: &mut u64) -> u64 requires *x == 3, returns *old(x) {{ *x = 7; 3 }}"
    ));
    // 中を読まないマクロの引数に参照を丸ごと渡す形。`old(..)` の括弧の中と、フィールドの名前、
    // パスの途中は通す。Verus はマクロの引数の中の `final` を受け付けないので、事後の値は条件を
    // マクロの外で書くよう案内する。
    let g = "spec fn g(y: &mut u64) -> u64 = *y;\n";
    let in_macro = "inside the arguments of this macro write `old(x)` for the value at entry, but `final(..)` cannot be written there, so for the value after the call write the condition outside the macro with `final(x)` (for example `*final(x) == 3` instead of `matches!(*x, 3)`)";
    rejects(
        &format!("{PRELUDE}{g}fn f(x: &mut u64) ensures matches!(g(x), 3), {{ *x = 7; }}"),
        "E0004",
        in_macro,
    );
    accepts(&format!(
        "{PRELUDE}{g}fn f(x: &mut u64) ensures *final(x) == 3, matches!(g(old(x)), 3), matches!(*old(x), 3), {{ }}"
    ));
    rejects(
        &format!("{PRELUDE}{g}fn f(x: &mut u64) ensures matches!(g(final(x)), 3), {{ }}"),
        "E0004",
        "`final(..)` cannot be written inside the arguments of this macro",
    );
    rejects(
        &in_pool("fn f(&mut self) ensures matches!(self.len, 0), { }"),
        "E0004",
        "write `old(self)` for the value at entry",
    );
    accepts(&in_pool(
        "fn f(&mut self, o: Option<u64>) ensures matches!(old(self).len, 0), matches!(o, Some(len)), { }",
    ));
    // `.` / `::` の直後の名前（フィールドとパスの途中）は引数ではない。
    let t = "pub struct T { x: u64 }\nmod m { pub spec fn x() -> u64 = 0; }\n";
    accepts(&format!(
        "{PRELUDE}{t}fn f(x: &mut u64, t: T) ensures matches!(t.x, 0), matches!(m::x(), 0), {{ }}"
    ));
    // 構造体リテラルのフィールドの値（`S {{ f: x }}`）は引数。フィールドの名前（`x: ..`）は見ない。
    let r = "pub struct R<'a> { f: &'a mut u64, x: u64 }\n";
    rejects(
        &format!(
            "{PRELUDE}{r}fn f(x: &mut u64) ensures matches!(R {{ f: x, x: 0 }}, R {{ .. }}), {{ }}"
        ),
        "E0004",
        in_macro,
    );
    // 権限の `p@` は、トークン列の中にあれば本体の検査が E0003 にする（ここでは足さない）。
    rejects(
        &format!("{PRELUDE}fn f(p: Ptr<Node>) uses &mut p@ ensures matches!(p@.value(), _), {{ }}"),
        "E0003",
        "permissions cannot be used in the arguments of this macro",
    );
}

#[test]
fn e0004_old_and_final_take_mut_arguments() {
    // `old(e)` / `final(e)` の `e` は `&mut` の引数、`uses &mut p@` の `p@` / `*p`、それらの消去
    // フィールドだけ（`old(self.len)` は生成コードの Verus の型エラーになるので、Ebykn が拒む）。
    let msg = "`old(..)` / `final(..)` can only be applied to a `&mut` argument";
    rejects(
        &in_pool("fn f(&mut self) ensures final(self.len) == 0, { }"),
        "E0004",
        msg,
    );
    rejects(
        &in_pool("fn f(&mut self) requires old(self.len) == 0, { }"),
        "E0004",
        msg,
    );
    rejects(
        &in_pool("fn f(&self) ensures old(self).len == 0, { }"),
        "E0004",
        msg,
    );
    rejects(
        &format!("{PRELUDE}fn f(p: Ptr<Node>) uses &p@ ensures old(p@).is_init(), {{ }}"),
        "E0004",
        msg,
    );
    rejects(
        &format!(
            "{PRELUDE}fn f(x: &mut u64, s: Seq<u64>) ensures forall x in s, old(x) == 0, {{ }}"
        ),
        "E0004",
        msg,
    );
    accepts(&in_pool(
        "fn f(&mut self, p: Ptr<Node>) requires old(self).len == 0, ensures final(self).len == old(self).len, old(self@order).len() == final((self@order)).len(), { }",
    ));
    accepts(&format!(
        "{PRELUDE}fn f(p: Ptr<Node>, x: &mut u64) uses &mut p@ ensures final(p@).is_init(), final(*p).val == old(*p).val, *final(x) == *old(x), {{ }}"
    ));
}

#[test]
fn e0004_shadowing_by_let_statements_and_borrow_regions() {
    // `let` の文で束縛し直した名前は、その後では束縛した値を指す。
    accepts(&format!(
        "{PRELUDE}fn f(x: &mut u64) ensures {{ let x = 1u64; x == 1 }}, {{ }}"
    ));
    rejects(
        &format!("{PRELUDE}fn f(x: &mut u64) ensures {{ let y = 1u64; *x == y }}, {{ }}"),
        "E0004",
        "write `final(x)`",
    );
    // `borrow` の領域の `self@nodes` / `r` も `&mut` 引数の参照。
    let bare = |src: String, want: &str| {
        let lines = run(&src).1;
        assert!(
            lines
                .iter()
                .any(|l| l.contains(": E0004: ") && l.contains(want)),
            "{lines:?}"
        );
    };
    bare(
        in_pool("fn f(&mut self, p: Ptr<Node>) ensures borrow self@nodes[p] { true }, { }"),
        "write `final(self)`",
    );
    bare(
        format!(
            "{PRELUDE}fn f(p: Ptr<Node>) uses r: &mut Region<Ptr<Node>> ensures borrow r[p] {{ true }}, {{ }}"
        ),
        "write `final(r)`",
    );
}
