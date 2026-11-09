// probe: broadcast group / broadcast use（仕様 §7.2 の lemma group / use lemmas の書き下し）
// expect: verified 2
use vstd::prelude::*;

verus! {

pub open spec fn f(x: int) -> int {
    x + 1
}

pub broadcast proof fn lemma_f_pos(x: int)
    requires
        x >= 0,
    ensures
        #[trigger] f(x) > 0,
{
}

pub broadcast group group_f {
    lemma_f_pos,
}

proof fn with_group(y: int)
    requires
        y >= 0,
{
    broadcast use group_f;

    assert(f(y) > 0);
}

} // verus!
