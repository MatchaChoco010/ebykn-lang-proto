// probe: 実行関数のループに decreases がないと、Verus は検証の前に拒む（#81。属性を付ける理由）
// expect: rejected
// expect-output: loop must have a `decreases` clause
use vstd::prelude::*;

verus! {

fn halve_to_zero(x: u64) -> (r: u64)
    ensures
        r == 0,
{
    let mut y = x;
    while y > 0 {
        y = y / 2;
    }
    y
}

} // verus!
