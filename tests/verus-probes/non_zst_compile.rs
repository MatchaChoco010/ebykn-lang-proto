// probe: 同じ検査は --compile のコンパイルでは効き、ゼロサイズ型の実体を、ジェネリックな関数を経由しても E0080 で止める。Verus がコンパイルでも cfg(verus_keep_ghost_body) を立てるようになれば、E0080 が出ない
// expect: compile-error 4
// expect-output: error[E0080]
// expect-output: zero-sized T
// expect-output: while instantiating `fn assert_non_zst::<()>`
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

fn via_generic<T>() {
    assert_non_zst::<T>();
}

pub fn unit_via_generic() {
    via_generic::<()>();
}

} // verus!
