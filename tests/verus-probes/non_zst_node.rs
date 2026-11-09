// probe: ゼロサイズ型の要素を持つノード（Node<()>）は、同じ検査を --compile のコンパイルでも通る（Stack<()> / DList<()> が確保するもの）
// expect: verified 3
// verus-args: --compile
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

pub struct Node<T> {
    pub val: T,
    pub next: Option<usize>,
}

pub fn unit_node() -> (r: u32)
    ensures
        r == 42,
{
    assert_non_zst::<Node<()>>();
    42
}

} // verus!
