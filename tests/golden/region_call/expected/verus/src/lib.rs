#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

pub struct Node { pub val: u64, }

fn alloc(v: u64, Tracked(r): Tracked <&mut Region < Ptr < Node >>>) -> (p: Ptr < Node >)
ensures final(r).contains(p), final(r).index(p).val == v, (forall | q | old(r).contains(q) ==> final(r).contains(q) && final(r).index(q) == old(r).index(q)), {
    let (p, Tracked(__eyk_perm_p)) = Ptr::new(Node { val: v });
    proof {
        r.insert(__eyk_perm_p);
    }
    p
}

fn set(p: Ptr < Node >, v: u64, Tracked(r): Tracked <&mut Region < Ptr < Node >>>)
requires old(r).contains(p),
ensures final(r).index(p).val == v, (forall | q | old(r).contains(q) ==> final(r).contains(q)), {
    {
        let tracked __eyk_perm_p = r.remove(p);
        let __eyk_res = {
            p.borrow_mut(Tracked(&mut __eyk_perm_p)).val = v;
        };
        #[allow(unreachable_code)] let __eyk_res = {
            proof {
                r.insert(__eyk_perm_p);
            }
            __eyk_res
        };
        __eyk_res
    }
}

fn get(p: Ptr < Node >, Tracked(r): Tracked <&Region < Ptr < Node >>>) -> (v: u64)
requires r.contains(p),
ensures v == r.index(p).val, {
    {
        let tracked __eyk_perm_p = r.borrow(p);
        { (*p.read(Tracked(&*__eyk_perm_p))).val }
    }
}

pub struct Pair {
    a: Ptr < Node >, b: Ptr < Node >, nodes: Tracked < Region < Ptr < Node >>>,
}

impl Pair {
    pub closed spec fn invariant(&self) -> bool {
        self.nodes@.contains(self.a) && self.nodes@.contains(self.b)
    }

}

impl Pair {
    pub fn new(x: u64, y: u64) -> (__eyk_r: Self)
    ensures __eyk_r.invariant(), {
        let tracked mut r: Region < Ptr < Node >> = Region::empty();
        let a = alloc(x, Tracked(&mut r));
        let b = alloc(y, Tracked(&mut r));
        Pair { a, b, nodes: Tracked(r) }
    }

    pub fn set_a(&mut self, v: u64)
    requires old(self).invariant(),
    ensures final(self).invariant(), {
        set(self.a, v, Tracked(self.nodes.borrow_mut()));
    }

    pub fn get_b(&self) -> u64
    requires self.invariant(), { get(self.b, Tracked(self.nodes.borrow())) }

    pub fn set_a_wrong(&mut self, v: u64)
    requires old(self).invariant(),
    ensures final(self).invariant(), {
        let tracked mut r: Region < Ptr < Node >> = Region::empty();
        set(self.a, v, Tracked(&mut r));
    }

}

pub struct Single { p: Ptr < Node >, items: Tracked < Region < Ptr < Node >>>, }

impl Single {
    pub closed spec fn invariant(&self) -> bool { self.items@.contains(self.p) }

}

impl Single {
    pub fn new(x: u64) -> (__eyk_r: Self)
    ensures __eyk_r.invariant(), {
        let tracked mut items: Region < Ptr < Node >> = Region::empty();
        let p = alloc(x, Tracked(&mut items));
        Single { p, items: Tracked(items) }
    }

    pub fn set(&mut self, v: u64)
    requires old(self).invariant(),
    ensures final(self).invariant(), {
        set(self.p, v, Tracked(self.items.borrow_mut()));
    }

}

fn fill(n: u64, Tracked(r): Tracked <&mut Region < Ptr < Node >>>)
ensures (forall | q | old(r).contains(q) ==> final(r).contains(q) && final(r).index(q) == old(r).index(q)),
decreases n, {
    if n > 0 {
        alloc(n, Tracked(&mut *r));
        fill(n - 1, Tracked(&mut *r));
    }
}

fn sum(p: Ptr < Node >, k: u64, Tracked(r): Tracked <&Region < Ptr < Node >>>) -> (s: u64)
requires r.contains(p), r.index(p).val < 100, k < 100,
ensures s <= k *100,
decreases k, {
    if k == 0 { 0 } else {
        let v = get(p, Tracked(&*r));
        v + sum(p, k - 1, Tracked(&*r))
    }
}

pub fn filled(n: u64) -> u64
requires n < 10, {
    let tracked mut r: Region < Ptr < Node >> = Region::empty();
    let p = alloc(7, Tracked(&mut r));
    fill(n, Tracked(&mut r));
    sum(p, 3, Tracked(&r))
}

fn transfer(p: Ptr < Node >, Tracked(to): Tracked <&mut Region < Ptr < Node >>>, Tracked(from): Tracked <&mut Region < Ptr < Node >>>)
requires old(from).contains(p),
ensures final(to).contains(p), final(to).index(p) == old(from).index(p), {
    let tracked __eyk_perm_p = from.remove(p);
    proof {
        to.insert(__eyk_perm_p);
    }
}

pub struct Vertex { pub out: u64, }

pub struct Graph {
    a: Ptr < Vertex >, b: Ptr < Vertex >, verts: Tracked < Region < Ptr < Vertex >>>,
}

impl Graph {
    pub closed spec fn invariant(&self) -> bool {
        self.verts@.contains(self.a) && self.verts@.contains(self.b)
    }

}

fn vertex(Tracked(g): Tracked <&mut Region < Ptr < Vertex >>>) -> (p: Ptr < Vertex >)
ensures final(g).contains(p), final(g).index(p).out == 0, (forall | q | old(g).contains(q) ==> final(g).contains(q) && final(g).index(q) == old(g).index(q)), {
    let (p, Tracked(__eyk_perm_p)) = Ptr::new(Vertex { out: 0 });
    proof {
        g.insert(__eyk_perm_p);
    }
    p
}

fn connect(a: Ptr < Vertex >, b: Ptr < Vertex >, Tracked(g): Tracked <&mut Region < Ptr < Vertex >>>)
requires old(g).contains(a), old(g).contains(b), old(g).index(a).out < 100,
ensures final(g).index(a).out == old(g).index(a).out + 1, (forall | q | old(g).contains(q) ==> final(g).contains(q) && (q != a ==> final(g).index(q) == old(g).index(q))), {
    {
        let tracked __eyk_perm_a = g.remove(a);
        let __eyk_res = {
            a.borrow_mut(Tracked(&mut __eyk_perm_a)).out = (*a.read(Tracked(&__eyk_perm_a))).out + 1;
        };
        #[allow(unreachable_code)] let __eyk_res = {
            proof {
                g.insert(__eyk_perm_a);
            }
            __eyk_res
        };
        __eyk_res
    }
}

fn out_degree(a: Ptr < Vertex >, Tracked(g): Tracked <&Region < Ptr < Vertex >>>) -> (n: u64)
requires g.contains(a),
ensures n == g.index(a).out, {
    {
        let tracked __eyk_perm_a = g.borrow(a);
        { (*a.read(Tracked(&*__eyk_perm_a))).out }
    }
}

fn into_graph(a: Ptr < Vertex >, b: Ptr < Vertex >, Tracked(g): Tracked < Region < Ptr < Vertex >>>) -> (__eyk_r: Graph)
requires g.contains(a), g.contains(b),
ensures __eyk_r.invariant(), {
    #[allow(unused_mut)] let tracked mut g = g;
    Graph { a, b, verts: Tracked(g) }
}

pub fn triangle() -> (__eyk_r: Graph)
ensures __eyk_r.invariant(), {
    let tracked mut g: Region < Ptr < Vertex >> = Region::empty();
    let a = vertex(Tracked(&mut g));
    let b = vertex(Tracked(&mut g));
    connect(a, b, Tracked(&mut g));
    let n = out_degree(a, Tracked(&g));
    if n == 1 {
        connect(b, a, Tracked(&mut g));
    }
    into_graph(a, b, Tracked(g))
}

pub fn moved_between() -> u64 {
    let tracked mut x: Region < Ptr < Node >> = Region::empty();
    let tracked mut y: Region < Ptr < Node >> = Region::empty();
    let p = alloc(5, Tracked(&mut x));
    transfer(p, Tracked(&mut y), Tracked(&mut x));
    get(p, Tracked(&y))
}

} // verus!
