// probe: ジェネリックな構造体の size_of が 0 でないことを Verus は知らない（prelude の Ptr::new がゼロサイズ型を自分で除く理由）
// expect: errors 0 1
// expect-output: postcondition not satisfied
use vstd::prelude::*;

verus! {

pub struct Node<T> {
    pub val: T,
    pub next: Option<usize>,
}

proof fn node_not_zst<T>()
    ensures
        vstd::layout::size_of::<Node<T>>() != 0,
{
}

} // verus!
