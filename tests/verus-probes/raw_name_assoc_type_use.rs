// probe: trait の関連型の名前を Rust の予約語の r# で書き（type r#type;）、X::r#type と使うと、Verus が関連型を認識できずに拒否する（#147）
// verus-args: --edition=2024
// expect: rejected
// expect-output: Verus does not recognize associated type `type`
use vstd::prelude::*;

verus! {

pub trait T {
    type r#type;
}

fn g<X: T>(x: X::r#type) -> X::r#type {
    x
}

} // verus!
