// probe: &mut 引数を ensures で裸のまま参照するとエラー。old(x) か final(x) を明示する（仕様 §6.4・付録 A.3 の形は通らない）
// expect: rejected
// expect-output: to dereference a mutable reference parameter in a postcondition, disambiguate by wrapping it in either `old` or `final`
use vstd::prelude::*;

verus! {

pub struct S {
    pub a: u64,
}

impl S {
    pub fn bump(&mut self)
        requires
            old(self).a < 100,
        ensures
            self.a == old(self).a + 1,
    {
        self.a = self.a + 1;
    }
}

} // verus!
