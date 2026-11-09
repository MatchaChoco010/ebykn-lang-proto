// probe: 同じ関数への assume_specification を二度書くと Verus が拒む（仕様 §8.2 で合成して一つにする前提）
// expect: rejected
// expect-output: duplicate specification for `assume_specification_duplicate::f`
use vstd::prelude::*;

#[verifier::external]
fn f(x: u64) -> u64 {
    x
}

verus! {

pub assume_specification[ f ](x: u64) -> (r: u64)
    ensures
        r == x,
;

pub assume_specification[ f ](x: u64) -> (r: u64)
    ensures
        r >= x,
;

} // verus!
