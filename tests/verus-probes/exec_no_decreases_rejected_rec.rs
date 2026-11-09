// probe: 実行関数の再帰に decreases がないと、Verus は検証の前に拒む（#81。属性を付ける理由）
// expect: rejected
// expect-output: recursive function must have a `decreases` clause
use vstd::prelude::*;

verus! {

fn count_down(n: u64) -> (r: u64)
    ensures
        r == 0,
{
    if n == 0 {
        0
    } else {
        count_down(n - 1)
    }
}

} // verus!
