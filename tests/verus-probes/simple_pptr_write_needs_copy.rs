// probe: simple_pptr の write と read は V: Copy を要求する（Copy でない型には使えない）
// expect: rejected
// expect-output: the trait bound `S: std::marker::Copy` is not satisfied
use vstd::prelude::*;
use vstd::simple_pptr::*;

verus! {

pub struct S {
    pub a: u64,
}

fn write_non_copy() {
    let (p, Tracked(mut perm)) = PPtr::<S>::new(S { a: 1 });
    p.write(Tracked(&mut perm), S { a: 2 });
    let _ = p.into_inner(Tracked(perm));
}

} // verus!
