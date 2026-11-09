// probe: edition 2021 では gen は予約語でないので、rustc が r#gen の r# を外して Verus に渡し、局所の名前の r#gen が検証条件に出ても通る（#119）。Ebykn が Ebykn.toml の edition が 2024 より前なら E0103 を捨てる根拠
// verus-args: --edition=2021
// expect: verified 2
use vstd::prelude::*;

verus! {

fn let_gen() -> u64 {
    let r#gen = 2u64;
    r#gen + 1
}

fn param_gen(r#gen: u64) -> (r: u64)
    requires
        r#gen < 5,
    ensures
        r == r#gen + 1,
{
    r#gen + 1
}

} // verus!
