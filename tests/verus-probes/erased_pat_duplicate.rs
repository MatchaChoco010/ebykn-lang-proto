// probe: 書き下しは @x を同じ名前で束縛するので、同じ引数の並びやパターンで同じ名前を二度束縛すると、rustc が E0415 / E0416 で止める（PR #168 のレビュー。別の名前に変えると重複が見えなくなり、片方を黙って選んで verified になった）
// expect: rejected
// expect-output-count: 1 error[E0415]
// expect-output-count: 1 error[E0416]
use vstd::prelude::*;

verus! {

pub struct S {
    pub n: u64,
    pub k: Ghost<nat>,
}

spec fn pick(S { k: k, .. }: S, k: nat) -> nat {
    let k = k@;
    k
}

spec fn pick2(s: S, k: nat) -> nat {
    match (s, k) {
        (S { k: k, .. }, k) => {
            let k = k@;
            k
        },
    }
}

} // verus!
