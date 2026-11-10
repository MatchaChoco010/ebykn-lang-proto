// probe: exec static は spec・proof の文脈（契約、proof fn）から読めない（#163。Verus のモードの検査に任せる）
// expect: rejected
// expect-output-count: 2 cannot read static with mode exec
use vstd::prelude::*;

verus! {

exec static A: u64 = 1;

fn f() -> u64
    requires
        A == 1,
{
    0
}

proof fn p() {
    let x = A;
}

} // verus!
