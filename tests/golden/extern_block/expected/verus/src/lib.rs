#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

mod ffi;

} // verus!

#[cfg_attr(verus_keep_ghost, verifier::external)] unsafe extern "C" {
    fn abs(x: i32) -> i32;
    pub safe fn labs(x: i64) -> i64;
}

verus! {

#[verifier::external_body] pub fn magnitude(x: i32) -> (r: i32)
requires x > i32::MIN,
ensures r >= 0, r == x || r == - x, { unsafe { abs(x) } }

#[verifier::external_body] pub fn magnitude64(x: i64) -> (r: i64)
requires x > i64::MIN,
ensures r >= 0, { if labs(x) == unsafe { ffi::llabs(x) } { labs(x) } else { 0 } }

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn distance(a: i32, b: i32) -> (r: i32)
requires a >= 0, b >= 0,
ensures r >= 0, { magnitude(a - b) }

} // verus!
