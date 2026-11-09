// probe: let-else（OPEN-13）
// expect: verified 1
use vstd::prelude::*;

verus! {

fn unwrap_or_zero(x: Option<u64>) -> (r: u64)
    ensures
        x is None ==> r == 0,
        x matches Some(v) ==> r == v,
{
    let Some(v) = x else {
        return 0;
    };
    v
}

} // verus!
