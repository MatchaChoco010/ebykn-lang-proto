// probe: exec_allows_no_decreases_clause の下でも、利用者が書いた decreases は確かめる（#81）
// expect: errors 3 2
// expect-output: decreases not satisfied at end of loop
// expect-output: could not prove termination
use vstd::prelude::*;

verus! {

// 正しい decreases は通る。
#[verifier::exec_allows_no_decreases_clause]
fn halve_to_zero(x: u64) -> (r: u64)
    ensures
        r == 0,
{
    let mut y = x;
    while y > 0
        decreases y,
    {
        y = y / 2;
    }
    y
}

// 減らない decreases は失敗する。
#[verifier::exec_allows_no_decreases_clause]
fn stuck(x: u64) -> u64 {
    let mut y = x;
    while y > 0
        decreases y,
    {
        y = y;
    }
    y
}

#[verifier::exec_allows_no_decreases_clause]
fn wrong_rec(n: u64) -> u64
    decreases n,
{
    if n == 0 {
        0
    } else {
        wrong_rec(n)
    }
}

} // verus!
