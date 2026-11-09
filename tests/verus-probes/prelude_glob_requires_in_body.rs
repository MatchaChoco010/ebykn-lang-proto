// probe: glob の use vstd::prelude::* の下では、本体の文 requires(..) が黙って事前条件になる（#16。書き下しは明示の use にする）
// expect: verified 1
use vstd::prelude::*;

verus! {

// ensures r == 0 は、本体の requires(x == 0) が事前条件として効くので通る。
fn f(x: u64) -> (r: u64)
    ensures
        r == 0,
{
    requires(x == 0);
    x
}

} // verus!
