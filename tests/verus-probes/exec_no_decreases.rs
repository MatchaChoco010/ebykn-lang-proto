// probe: exec_allows_no_decreases_clause を付けた実行関数は、decreases のないループと再帰の停止を示さずに通し、ensures とあふれは確かめる（#81）
// expect: errors 4 1
// expect-output: possible arithmetic underflow/overflow
use vstd::prelude::*;

verus! {

#[verifier::exec_allows_no_decreases_clause]
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

#[verifier::exec_allows_no_decreases_clause]
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

// 停止を示さなくても、あふれは確かめる。
#[verifier::exec_allows_no_decreases_clause]
fn overflow(x: u64) -> u64 {
    let mut y = x;
    loop {
        y = y + 1;
    }
}

} // verus!
