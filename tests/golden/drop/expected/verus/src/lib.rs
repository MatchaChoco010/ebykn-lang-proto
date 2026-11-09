#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

pub struct Guard { pub a: u64, }

impl Drop for Guard {
    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn drop(&mut self) opens_invariants none
    no_unwind {
        self.a = 0;
    }

}

pub struct Full { pub a: u64, }

impl core::ops::Drop for Full {
    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn drop(&mut self) opens_invariants none
    no_unwind {}

}

pub struct Owner { p: Ptr < u64 >, __eyk_erased_cells: Tracked < Region < Ptr < u64 >>>, }

impl Drop for Owner {
    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn drop(&mut self) opens_invariants none
    no_unwind {}

}

pub struct Overflow { pub a: u64, }

impl Drop for Overflow {
    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn drop(&mut self) opens_invariants none
    no_unwind {
        self.a = self.a + 1;
    }

}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] pub fn helper(x: u64) -> u64
no_unwind { x }

pub struct Calls { pub a: u64, }

impl Drop for Calls {
    #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn drop(&mut self) opens_invariants none
    no_unwind {
        self.a = helper(self.a);
    }

}

mod m {
    #[allow(unused_imports)]
    use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
    #[cfg(verus_keep_ghost)]
    #[allow(unused_imports)]
    use vstd::prelude::old;
    #[allow(unused_imports)]
    use ebykn_prelude::*;
    use std::ops::Drop as Finalize;

    pub struct Aliased { pub a: u8, }

    impl Finalize for Aliased {
        #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn drop(&mut self) opens_invariants none
        no_unwind {
            self.a = 1;
        }

    }

    pub trait Drop {
        fn drop(&mut self);

    }

    pub struct Local { pub a: u8, }

    impl Drop for Local {
        #[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn drop(&mut self) {}

    }

}
#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] #[doc = " スコープの終わりで `drop` が暗黙に呼ばれる。"] pub fn scope_end() -> (r: u64)
ensures r == 1, {
    let g = Guard { a: 1 };
    g.a
}

} // verus!
