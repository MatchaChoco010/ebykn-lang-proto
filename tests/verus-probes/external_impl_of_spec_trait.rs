// probe: verus! の外の impl（依存クレートの derive や属性マクロが作る形）は検証されず、契約付きの trait を実装していても、それを前提に呼び出し側が verified になる（#49）
// expect: verified 2
use vstd::prelude::*;

verus! {

pub trait One {
    fn one() -> (r: u32)
        ensures
            r == 1;
}

pub struct S {}

pub fn get<T: One>() -> (r: u32)
    ensures
        r == 1,
{
    T::one()
}

pub fn s_one() -> (r: u32)
    ensures
        r == 1,
{
    get::<S>()
}

} // verus!

// 実行すると 0 を返す。Verus はこの impl を見ない。
impl One for S {
    fn one() -> u32 {
        0
    }
}
