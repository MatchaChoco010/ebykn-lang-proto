// probe: vstd の Seq::first() / last() は Option ではなく要素を返す（仕様 §7.1 の first() -> Option<T> とは型が違う）
// expect: rejected
// expect-output: the trait bound `std::option::Option<u64>: vstd::prelude::SpecEq<u64>` is not satisfied
use vstd::prelude::*;

verus! {

spec fn head_is(s: Seq<u64>, h: Option<u64>) -> bool {
    h == s.first()
}

} // verus!
