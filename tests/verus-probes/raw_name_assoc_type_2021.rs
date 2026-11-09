// probe: edition 2021 では gen は予約語でないので、trait の関連型の r#gen は、関数を持つ trait でも通る（#147）。Ebykn が Ebykn.toml の edition が 2024 より前なら関連型の r#gen を E0104 にしない根拠
// verus-args: --edition=2021
// expect: verified 1
use vstd::prelude::*;

verus! {

pub trait T {
    type r#gen;
    fn f(&self) -> u64 { 1 }
}

} // verus!
