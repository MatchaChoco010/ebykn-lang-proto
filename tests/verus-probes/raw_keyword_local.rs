// probe: Rust の予約語を r# で名前にした局所の名前（#119）。検証条件に出ると、Verus が SMT の記号に r#type@ の形のまま書き、z3 が読めずに function body check で失敗する。spec fn の引数なら定義が効かず、使う側が postcondition not satisfied になる。予約語でない r#foo・弱いキーワードの r#union・2024 の r#gen の関数の名前・フィールド・型の引数は通る
// verus-args: --edition=2024
// expect: errors 7 8
// expect-output-count: 7 error: function body check
// expect-output-count: 1 error: postcondition not satisfied
use vstd::prelude::*;

verus! {

// 失敗する（function body check）
fn let_kw() -> u64 {
    let r#type = 2u64;
    r#type + 1
}

fn let_gen() -> u64 {
    let r#gen = 2u64;
    r#gen + 1
}

fn param_kw(r#type: u64) -> u64
    requires
        r#type < 5,
{
    r#type + 1
}

fn ret_kw() -> (r#type: u64)
    ensures
        r#type == 2,
{
    2
}

fn match_kw(x: Option<u64>) -> u64 {
    match x {
        Some(r#type) => if r#type < 5 { r#type + 1 } else { 0 },
        None => 0,
    }
}

fn closure_kw() {
    let f = |r#type: u64| -> (r: u64)
        requires
            r#type < 5,
        ensures
            r == r#type + 1,
        { r#type + 1 };
}

spec fn g(i: int) -> int {
    i
}

fn forall_kw() {
    assert(forall|r#type: int| #[trigger] g(r#type) == r#type);
}

// 失敗する（定義が効かず postcondition not satisfied）
spec fn spec_kw(r#type: int) -> int {
    r#type + 1
}

proof fn use_spec_kw(x: int)
    ensures
        spec_kw(x) == x + 1,
{
}

// 通る
fn let_kw_unused_in_vc() -> u64 {
    let r#type = 2u64;
    r#type
}

fn let_foo() -> u64 {
    let r#foo = 2u64;
    r#foo + 1
}

fn let_union() -> u64 {
    let r#union = 2u64;
    r#union + 1
}

fn r#gen(x: u64) -> (r: u64)
    requires
        x < 10,
    ensures
        r == x + 1,
{
    x + 1
}

fn call_gen() -> (r: u64)
    ensures
        r == 3,
{
    r#gen(2)
}

pub struct S {
    pub r#type: u64,
}

fn field_kw(s: S) -> (r: u64)
    requires
        s.r#type < 5,
    ensures
        r == s.r#type + 1,
{
    s.r#type + 1
}

fn type_param_kw<r#type>(x: r#type) -> (r: r#type)
    ensures
        r == x,
{
    x
}

} // verus!
