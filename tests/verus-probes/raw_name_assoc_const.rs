// probe: trait の関連定数の名前を Rust の予約語の r# で書き（const r#type: u64;）、impl で与えると、Verus が生成コードの関数のモードで拒否する（#147）。予約語でない r#foo は通る（raw_name_items.rs）
// verus-args: --edition=2024
// expect: rejected
// expect-output: function for external trait must have mode `exec`
use vstd::prelude::*;

verus! {

pub trait T {
    const r#type: u64;
}

pub struct S;

impl T for S {
    const r#type: u64 = 1;
}

} // verus!
