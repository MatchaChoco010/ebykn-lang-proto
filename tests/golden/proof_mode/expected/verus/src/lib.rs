#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

fn bump(c: u64) -> u64
requires c < 100, { c + 1 }

pub fn keep(x: u64) -> (r: u64)
requires x < 100,
ensures r == x, {
    let mut y = x;
    proof {
        y = 0;
    }
    y
}

} // verus!
