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

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn caller(x: u64) -> (r: u64)
ensures r == 7, { liar(x) }

proof fn helper(x: u64)
ensures x + 0 == x, { vstd::prelude::admit() }

#[verifier::external] #[test] fn t() {}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn a() -> (r: u32)
ensures r == 1, { 1 }

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn b() -> (r: u32)
ensures r == 1, { 1 }

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn c() -> (r: u32)
ensures r == 1, { 1 }

} // verus!
