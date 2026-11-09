// probe: #[automatically_derived] を付けた impl の本体は Verus が検証しない（#49 のレビューの S1。Ebykn はこの項目を unknown にして verify を失敗させる）
// expect: errors 1 1
// expect-output: precondition not satisfied
use vstd::prelude::*;

verus! {

pub struct S {
    pub x: u32,
}

pub fn never()
    requires
        false,
{
}

// 本体の `never()` の事前条件を、Verus は確かめない。
#[automatically_derived]
impl Default for S {
    fn default() -> S {
        never();
        S { x: 0 }
    }
}

pub struct T {
    pub x: u32,
}

// 印のない同じ impl は確かめるので、ここだけがエラーになる。
impl Default for T {
    fn default() -> T {
        never();
        T { x: 0 }
    }
}

} // verus!
