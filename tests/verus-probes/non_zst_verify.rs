// probe: verus! の外の定数でゼロサイズ型を拒む検査（prelude の NonZst）は、cfg(verus_keep_ghost_body) で外すので検証では効かない。Verus が検証でこの名前の cfg を立てなくなれば、外の定数を参照して拒否される
// expect: verified 3
use core::marker::PhantomData;

use vstd::prelude::*;

pub struct NonZst<T>(PhantomData<T>);

impl<T> NonZst<T> {
    pub const OK: () = assert!(core::mem::size_of::<T>() != 0, "zero-sized T");
}

verus! {

#[verifier::exec_allows_no_decreases_clause]
fn assert_non_zst<T>()
    ensures
        vstd::layout::size_of::<T>() != 0,
{
    #[cfg(not(verus_keep_ghost_body))]
    let _ = NonZst::<T>::OK;
    if core::mem::size_of::<T>() == 0 {
        loop {}
    }
}

pub fn unit() -> (r: u32)
    ensures
        r == 42,
{
    assert_non_zst::<()>();
    42
}

} // verus!
