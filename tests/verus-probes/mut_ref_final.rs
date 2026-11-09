// probe: &mut 引数の事後の値は final(x)。requires では裸の x と old(x) はどちらも入口の値
// expect: verified 2
use vstd::prelude::*;

verus! {

pub struct S {
    pub a: u64,
}

impl S {
    pub fn bump(&mut self)
        requires
            self.a < 100,
        ensures
            final(self).a == old(self).a + 1,
    {
        self.a = self.a + 1;
    }
}

fn caller() {
    let mut s = S { a: 1 };
    s.bump();
    assert(s.a == 2);
}

} // verus!
