// probe: vstd::cell::pcell_maybe_uninit::PCell の new / empty / put / borrow / write / borrow_mut / take / into_inner と、PointsTo の is_exclusive
// expect: verified 3
use vstd::prelude::*;
use vstd::cell::pcell_maybe_uninit::*;

verus! {

pub struct S {
    pub a: u64,
}

fn new_borrow_into_inner() {
    let (c, Tracked(perm)) = PCell::new(S { a: 5 });
    assert(perm.id() == c.id() && perm.is_init() && perm.value().a == 5);
    let r = c.borrow(Tracked(&perm));
    assert(r.a == 5);
    let v = c.into_inner(Tracked(perm));
    assert(v.a == 5);
}

fn empty_put_write_take() {
    let (c, Tracked(mut perm)) = PCell::<S>::empty();
    assert(perm.is_uninit());
    c.put(Tracked(&mut perm), S { a: 1 });
    // pcell_maybe_uninit の write は Copy を要求しない。
    c.write(Tracked(&mut perm), S { a: 2 });
    let r = c.borrow_mut(Tracked(&mut perm));
    r.a = 3;
    let v = c.take(Tracked(&mut perm));
    assert(v.a == 3 && perm.is_uninit());
}

proof fn exclusive(tracked a: &mut PointsTo<u64>, tracked b: &PointsTo<u64>) {
    a.is_exclusive(b);
    assert(a.id() != b.id());
}

} // verus!
