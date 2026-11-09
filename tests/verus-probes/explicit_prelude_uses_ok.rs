// probe: 書き下しの先頭の明示の use だけで、Seq・seq!・int・Tracked・Ghost・old・final・仕様の算術と連鎖した比較が使える
// expect: verified 3
#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;

verus! {

pub struct S {
    pub a: u64,
    pub q: Ghost<Seq<int>>,
}

spec fn sum(s: Seq<int>, i: int) -> int
    decreases i,
{
    if 0 < i <= s.len() { s[i - 1] + sum(s, i - 1) } else { 0 }
}

fn bump(x: &mut S)
    requires
        old(x).a < 100,
    ensures
        final(x).a == old(x).a + 1,
        final(x).q@ == seq![1int] + old(x).q@,
{
    x.a = x.a + 1;
    proof {
        x.q = Ghost(seq![1int] + x.q@);
    }
}

fn mk() -> (r: (u64, Tracked<Map<int, int>>))
    ensures
        r.0 == 0,
{
    (0, Tracked(Map::tracked_empty()))
}

} // verus!
