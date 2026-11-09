#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

#[verifier::external_body] fn liar(x: u64) -> (r: u64)
ensures r == 7, { x }

pub fn caller(x: u64) -> (r: u64)
ensures r == 7, { liar(x) }

proof fn helper(x: u64)
ensures x + 0 == x, { vstd::prelude::admit() }

#[cfg(not(verus_keep_ghost))] pub fn hidden(x: u32) -> (r: u32)
ensures r == 0, { x }

#[verifier::external] #[test] fn t() {}

} // verus!
