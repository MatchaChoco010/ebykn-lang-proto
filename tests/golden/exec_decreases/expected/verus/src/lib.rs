#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn halve_to_zero(x: u64) -> (r: u64)
ensures r == 0, {
    let mut y = x;
    while y > 0 {
        y = y / 2;
    }
    y
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn count_down(n: u64) -> (r: u64)
ensures r == 0, { if n == 0 { 0 } else { count_down(n - 1) } }

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn count(n: u64) -> (r: u64)
ensures r == n,
decreases n, { if n == 0 { 0 } else { count(n - 1) + 1 } }

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn halve_checked(x: u64) -> (r: u64)
ensures r == 0, {
    let mut y = x;
    while y > 0
    decreases y, {
        y = y / 2;
    }
    y
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn stuck(x: u64) -> u64 {
    let mut y = x;
    while y > 0
    decreases y, {
        y = y + 0;
    }
    y
}

pub open spec fn tri(n: nat) -> nat
decreases n, { if n == 0 { 0 } else { n + tri((n - 1) as nat) } }

pub struct Counter { pub n: u64, }

impl Counter {
    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn drain(&mut self)
    ensures final(self).n == 0, {
        while self.n > 0 {
            self.n = self.n - 1;
        }
    }

}

#[verifier::external_body] fn spin() { loop {} }

pub trait Drain {
    fn left(&self) -> u64;

    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn spin_down(&self) -> u64 {
        let mut k = self.left();
        while k > 0 {
            k = k - 1;
        }
        k
    }

}

} // verus!
