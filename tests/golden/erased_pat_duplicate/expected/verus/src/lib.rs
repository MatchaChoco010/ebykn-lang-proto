#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

pub struct S { pub n: u64, __eyk_erased_k: Ghost < nat >, }

spec fn pick(S { __eyk_erased_k: k,.. }: S, k: nat) -> nat {
    let k = k@;
    k
}

spec fn pick2(s: S, k: nat) -> nat {
    match (s, k) {
        (S { __eyk_erased_k: k,.. }, k) => {
            let k = k@;
            k
        },
    }
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn caller(s: S) -> (r: u64)
ensures pick(s, 5) == s.__eyk_erased_k@, pick2(s, 5) == s.__eyk_erased_k@, { 0 }

} // verus!
