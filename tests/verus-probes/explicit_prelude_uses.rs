// probe: 書き下しの先頭の明示の use（ebykn_lower の HEADER_USES。ebykn_prelude を除く）で、本体の requires(..) は名前が見つからない
// expect: rejected
// expect-output: cannot find function `requires` in this scope
#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;

verus! {

fn f(x: u64) -> (r: u64)
    ensures
        r == 0,
{
    requires(x == 0);
    x
}

} // verus!
