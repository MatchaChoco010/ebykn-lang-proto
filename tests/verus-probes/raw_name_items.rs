// probe: const・static・const fn・本体のない trait の関数・列挙子・列挙子のフィールド・予約語の関連型のほかの項目の名前は、予約語の r# でも Verus が通し、検証条件に出ても検証できる（#147）。関数・spec fn・proof fn・external_body・when_used_as_spec の組・構造体とそのフィールド（Ghost / Tracked を含む）・列挙型・trait・本体のある trait の関数・trait の関連定数と trait の impl の関連定数・予約語でない関連型・inherent なメソッド・モジュール・型の別名・型の引数・union・extern の static・型の不変条件
// verus-args: --edition=2024
// expect: verified 13
use vstd::prelude::*;

verus! {

fn r#match(x: u64) -> (r: u64)
    requires
        x < 5,
    ensures
        r == x + 1,
{
    x + 1
}

fn call_match() {
    let y = r#match(1);
    assert(y == 2);
}

spec fn r#loop(x: int) -> int {
    x + 1
}

proof fn r#while()
    ensures
        r#loop(1) == 2,
{
}

#[verifier::external_body]
fn r#unsafe() -> (r: u64)
    ensures
        r == 1,
{
    1
}

pub open spec fn __spec_type(x: u64) -> u64 {
    x
}

#[verifier::when_used_as_spec(__spec_type)]
pub fn r#type(x: u64) -> (r: u64)
    ensures
        r == __spec_type(x),
{
    x
}

proof fn use_type() {
    assert(r#type(1) == 1);
}

pub struct r#struct {
    pub r#type: u64,
    pub r#ref: Ghost<int>,
}

fn field(s: r#struct) -> (r: u64)
    requires
        s.r#type < 5,
        s.r#ref@ == 1,
    ensures
        r == s.r#type + 1,
{
    s.r#type + 1
}

pub struct Holder {
    pub r#match: Tracked<vstd::simple_pptr::PointsTo<u64>>,
}

pub enum r#enum {
    A(u64),
    B,
}

fn enum_name(e: r#enum) -> (r: u64)
    ensures
        e is A ==> r == 1,
{
    match e {
        r#enum::A(_) => 1,
        r#enum::B => 0,
    }
}

pub trait r#trait {
    type r#foo;

    const r#bar: u64;

    fn r#dyn(&self) -> (r: u64)
        ensures
            r == 1,
    {
        1
    }
}

pub struct S {
    pub x: u64,
}

impl r#trait for S {
    type r#foo = u64;

    const r#bar: u64 = 1;
}

impl S {
    fn r#impl(&self) -> (r: u64)
        requires
            self.x < 5,
        ensures
            r == self.x + 1,
    {
        self.x + 1
    }
}

mod r#mod {
    use vstd::prelude::*;

    pub open spec fn f() -> int {
        1
    }
}

proof fn use_mod()
    ensures
        r#mod::f() == 1,
{
}

type r#yield = u64;

fn generic<r#in>(x: r#in, y: r#yield) -> (r: r#in)
    ensures
        r == x,
{
    x
}

pub union r#union {
    pub r#foo: u64,
    pub b: u32,
}

unsafe extern "C" {
    pub static r#extern: u64;
}

pub struct Inv {
    r#type: u64,
}

impl Inv {
    #[verifier::type_invariant]
    spec fn inv(&self) -> bool {
        self.r#type < 5
    }

    fn get(&self) -> (r: u64)
        ensures
            r < 5,
    {
        proof {
            use_type_invariant(self);
        }
        self.r#type
    }
}

} // verus!
