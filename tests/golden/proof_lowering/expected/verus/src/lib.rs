#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

#[verifier::opaque] pub open spec fn double(x: int) -> int { x + x }

pub open spec fn sq(x: int) -> int { x *x }

#[doc = " 平方は単調に増える（非線形の算術。`by(nonlinear_arith)` で証明する）。"] proof fn sq_monotonic(a: int, b: int) by(nonlinear_arith)
requires 0 <= a, a <= b,
ensures sq(a) <= sq(b), {}

proof fn double_is_twice(x: int)
ensures double(x) == 2 *x, {
    reveal(double);
}

#[doc = " 証明ファイルだけの補題（剥がす）。"] proof fn sum_below(n: int)
requires 0 <= n,
ensures (forall | i: int | #![trigger sq(i)] 0 <= i && i <= n ==> 0 <= sq(i)), {
    assert forall | i: int | 0 <= i < (n + 1) implies 0 <= sq(i) by {
        sq_monotonic(0, i);
    };
}

proof fn unproven(x: int)
ensures x == x + 0, { vstd::prelude::admit() }

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] #[doc = " 0 から n までを数える。"] pub fn count(n: u64) -> (r: u64)
ensures r == n, {
    let mut i: u64 = 0;
    let ghost start = i;
    while i < n
    invariant i <= n, start == 0,
    decreases n - i, {
        i = i + 1;
        proof {
            assert(i <= n);
        }
    }
    proof {
        assert((i == n));
    }
    i
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn twice(x: u64) -> (r: u64)
requires x < 1000,
ensures r == double(x as int), {
    proof {
        double_is_twice(x as int);
    }
    x + x
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] #[doc = " 本体のない補題に頼る。"] pub fn leans(x: u64) -> u64 {
    proof {
        unproven(x as int);
    }
    x
}

} // verus!
