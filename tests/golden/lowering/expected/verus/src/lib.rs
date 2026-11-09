#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

pub struct Node { pub val: u64, pub next: Option < Ptr < Node >>, }

pub fn set_val(p: Ptr < Node >, v: u64, Tracked(__eyk_perm_p): Tracked <&mut PointsTo < Node >>)
requires __eyk_perm_p.pptr() == p, __eyk_perm_p.is_init(),
ensures final(__eyk_perm_p).value().val == v, final(__eyk_perm_p).is_init(), final(__eyk_perm_p).pptr() == p, {
    p.borrow_mut(Tracked(&mut *__eyk_perm_p)).val = v;
}

pub fn get_val(p: Ptr < Node >, Tracked(__eyk_perm_p): Tracked <&PointsTo < Node >>) -> u64
requires __eyk_perm_p.pptr() == p, __eyk_perm_p.is_init(), { (*p.read(Tracked(&*__eyk_perm_p))).val }

pub fn mk(v: u64) -> (__eyk_ret:(Ptr < Node >, Tracked < PointsTo < Node >>))
ensures __eyk_ret.1@.is_init(), (__eyk_ret.1@.value()).val == v, __eyk_ret.1@.pptr() == __eyk_ret.0, {
    let (q, Tracked(__eyk_perm_q)) = Ptr::new(Node { val: v, next: None });
    (q, Tracked(__eyk_perm_q))
}

pub fn use_it() -> u64 {
    let (q, Tracked(__eyk_perm_q)) = mk(3);
    set_val(q, 4, Tracked(&mut __eyk_perm_q));
    let x = get_val(q, Tracked(&__eyk_perm_q));
    let n = q.free(Tracked(__eyk_perm_q));
    x
}

#[verifier::when_used_as_spec(__eyk_spec_double)] pub fn double(x: u32) -> (__eyk_r: u32)
requires x < 100,
ensures __eyk_r == __eyk_spec_double(x), { x *2 } pub open spec fn __eyk_spec_double(x: u32) -> u32 { (x *2) as u32 }

proof fn helper(x: u64)
ensures x + 0 == x, { vstd::prelude::admit() }

#[verifier::external_body] fn ffi(x: u64) -> u64 { x }

pub struct Pool {
    len: u64, nodes: Tracked < Region < Ptr < Node >>>, order: Ghost < Seq < Ptr < Node >>>,
}

impl View for Pool {
    type V = Seq < Ptr < Node >>;

    closed spec fn view(&self) -> Seq < Ptr < Node >> { self.order@ }

}

impl Pool {
    pub fn new() -> Self {
        Pool {
            len: 0, nodes: Tracked(Region::empty()), order: Ghost(Seq::empty())
        }
    }

    fn add(&mut self, v: u64) -> (p: Ptr < Node >)
    ensures final(self).nodes@.contains(p), final(self).nodes@.index(p).val == v, final(self)@ == old(self)@.push(p), final(self).order@.len() == old(self).order@.len() + 1, {
        let (p, Tracked(__eyk_perm_p)) = Ptr::new(Node { val: v, next: None });
        proof {
            self.nodes.borrow_mut().insert(__eyk_perm_p);
        }
        proof {
            self.order = Ghost(self.order@.push(p));
        }
        p
    }

    fn bump(&mut self, p: Ptr < Node >)
    requires old(self).nodes@.contains(p), {
        {
            let tracked __eyk_perm_p = self.nodes.borrow_mut().remove(p);
            let __eyk_res = {
                p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 0;
            };
            #[allow(unreachable_code)] let __eyk_res = {
                proof {
                    self.nodes.borrow_mut().insert(__eyk_perm_p);
                }
                __eyk_res
            };
            __eyk_res
        }
    }

    fn peek(&self, p: Ptr < Node >) -> u64
    requires self.nodes@.contains(p), {
        {
            let tracked __eyk_perm_p = self.nodes.borrow().borrow(p);
            { (*p.read(Tracked(&*__eyk_perm_p))).val }
        }
    }

}

pub fn with_region(p: Ptr < Node >, Tracked(r): Tracked <&mut Region < Ptr < Node >>>)
requires old(r).contains(p),
ensures final(r).contains(p), final(r).index(p) == old(r).index(p), {
    let tracked __eyk_perm_p = r.remove(p);
    proof {
        r.insert(__eyk_perm_p);
    }
}

pub fn bump(x: &mut u64) -> u64
requires *old(x) < 100,
returns *old(x), {
    let y = *x;
    *x = y + 1;
    y
}

pub open spec fn same(y: u64) -> u64 { y }

pub fn set3(x: &mut u64)
requires *x <= 9,
ensures *final(x) == 3, same(*final(x)) == 3, matches!(*old(x), 0..= 9), {
    *x = 3;
}

pub fn total(v: Vec < u64 >) -> u64 {
    let mut s: u64 = 0;
    let mut i = 0;
    while i < v.len()
    invariant i <= v.len(),
    decreases v.len() - i, {
        s = if s < 1000 { s + 1 } else { s };
        i = i + 1;
    }
    s
}

#[verifier::when_used_as_spec(__eyk_spec_quant)] pub fn quant(s: Seq < u64 >) -> (__eyk_r: bool)
ensures __eyk_r == __eyk_spec_quant(s), { true } pub open spec fn __eyk_spec_quant(s: Seq < u64 >) -> bool { true }

pub open spec fn all_pos(s: Seq < int >) -> bool { (forall | i: int | 0 <= i < s.len() ==> s[i] > 0) }

pub open spec fn some_pos(s: Seq < int >) -> bool { (exists | x | s.contains(x) && ((x == 1) || (x == 2))) }

pub struct Bag { items: Tracked < Region < Ptr < Node >>>, }

impl Bag {
    fn adopt(p: Ptr < Node >, Tracked(r): Tracked < Region < Ptr < Node >>>) -> (b: Bag)
    requires r.contains(p),
    ensures b.items@.contains(p), b.items@[p] == r.index(p), {
        let tracked mut r = r;
        Bag { items: Tracked(r) }
    }

    fn with_node(v: u64, Tracked(r): Tracked < Region < Ptr < Node >>>) -> (b: Bag)
    ensures (forall | q | r.contains(q) ==> b.items@.contains(q)), {
        let tracked mut r = r;
        let (p, Tracked(__eyk_perm_p)) = Ptr::new(Node { val: v, next: None });
        proof {
            r.insert(__eyk_perm_p);
        }
        Bag { items: Tracked(r) }
    }

}

pub fn wrap(n: u64, Tracked(items): Tracked < Region < Ptr < Node >>>) -> Bag {
    let tracked mut items = items;
    Bag { items: Tracked(items) }
}

fn set_owned(p: Ptr < Node >, Tracked(__eyk_perm_p): Tracked < PointsTo < Node >>) -> (v: u64)
requires __eyk_perm_p.pptr() == p, __eyk_perm_p.is_init(),
ensures v == 1, {
    let tracked mut __eyk_perm_p = __eyk_perm_p;
    p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 1;
    let n = p.free(Tracked(__eyk_perm_p));
    n.val
}

fn set_and_put(p: Ptr < Node >, Tracked(__eyk_perm_p): Tracked < PointsTo < Node >>, Tracked(r): Tracked <&mut Region < Ptr < Node >>>)
requires __eyk_perm_p.pptr() == p, __eyk_perm_p.is_init(), __eyk_perm_p.pptr() == p,
ensures final(r).contains(p), final(r).index(p).val == 2, {
    let tracked mut __eyk_perm_p = __eyk_perm_p;
    p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = 2;
    proof {
        r.insert(__eyk_perm_p);
    }
}

} // verus!
