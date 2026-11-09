#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

fn side(x: &mut u32)
ensures *final(x) == 7, {
    *x = 7;
}

pub fn f() -> (r: u32)
ensures r == 1, { if cfg!(test) { 0 } else { 1 } }

pub fn g() -> (r: u32)
ensures r == 0, {
    let mut x = 0u32;
    #[cfg(test)] side(&mut x);
    x
}

pub fn g2() -> (r: u32)
ensures r == 7, {
    let mut x = 0u32;
    #[cfg(not(test))] side(&mut x);
    x
}

pub fn g3() -> (r: u32)
ensures r == 0, {
    let mut x = 0u32;
    #[cfg(test)] {
        x = 1;
    }
    x
}

pub fn g4(v: u32) -> (r: u32)
ensures r == 0, {
    #[cfg(test)] if v > 0 {
        return 1;
    }
    0
}

pub fn h(x: u32) -> (r: u32)
ensures r == 2, { match x { #[cfg(test)] 0 => 1, _ => 2, } }

pub fn p() -> (r: usize)
ensures r == 1, {
    let v: Vec < u32 > = vec![#[cfg(test)] 1u32, 0];
    v.len()
}

pub struct P { pub a: u32, #[cfg(test)] pub b: u32, }

pub fn m() -> (r: P)
ensures r.a == 1, { P { a: 1, #[cfg(test)] b: 2 } }

pub fn n() -> (r: u32)
ensures r == 3, {
    #[cfg(feature = "never")] let x = 1u32;
    #[cfg(not(feature = "never"))] let x = 3u32;
    x
}

#[cfg_attr(test, inline)] pub fn k() -> (r: u32)
ensures r == 4, { 4 }

pub fn u() -> (r: u32)
ensures r == 0, { if cfg!(my_flag) { 1 } else { 0 } }

pub fn t64() -> (r: u32)
ensures r == 0, { if cfg!(target_pointer_width = "64") { 0 } else { 1 } }

} // verus!
