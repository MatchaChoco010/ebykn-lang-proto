// probe: ? 演算子（OPEN-13）。Option と Result の早期 return を検証できる
// expect: verified 2
use vstd::prelude::*;

verus! {

fn first_plus_one(x: Option<u64>) -> (r: Option<u64>)
    ensures
        x is None ==> r is None,
        x is Some && x->0 < 100 ==> r == Some((x->0 + 1) as u64),
{
    let v = x?;
    if v < 100 {
        Some(v + 1)
    } else {
        None
    }
}

fn result_q(x: Result<u64, bool>) -> (r: Result<u64, bool>)
    ensures
        x is Err ==> r == x,
{
    let v = x?;
    Ok(v / 2)
}

} // verus!
