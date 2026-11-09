// probe: vstd::simple_pptr の PPtr::new / empty / put / borrow / write / borrow_mut（final 付き）/ take / into_inner / free の名前と契約
// expect: verified 3
use vstd::prelude::*;
use vstd::simple_pptr::*;

verus! {

fn new_borrow_into_inner() {
    let (p, Tracked(perm)) = PPtr::<u64>::new(5);
    assert(perm.pptr() == p);
    assert(perm.is_init() && perm.value() == 5);
    let r = p.borrow(Tracked(&perm));
    assert(*r == 5);
    let v = p.into_inner(Tracked(perm));
    assert(v == 5);
}

fn empty_put_write_take_free() {
    let (p, Tracked(mut perm)) = PPtr::<u64>::empty();
    assert(perm.is_uninit());
    p.put(Tracked(&mut perm), 1);
    // write は V: Copy を要求し、初期化済みでも未初期化でもよい。
    p.write(Tracked(&mut perm), 2);
    assert(perm.value() == 2);
    let v = p.take(Tracked(&mut perm));
    assert(v == 2 && perm.is_uninit());
    p.free(Tracked(perm));
}

fn borrow_mut_final() {
    let (p, Tracked(mut perm)) = PPtr::<u64>::new(1);
    let r = p.borrow_mut(Tracked(&mut perm));
    *r = 7;
    assert(perm.value() == 7);
    let _ = p.into_inner(Tracked(perm));
}

} // verus!
