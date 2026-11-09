// probe: trait の関連型の名前を Rust の予約語の r# で書き（type r#type;）、その trait に関数があると、Verus が AIR の内部エラー（use of undeclared function）で落ちる（#147）
// verus-args: --edition=2024
// expect: rejected
// expect-output: ill-typed AIR code
// expect-output: use of undeclared function
use vstd::prelude::*;

verus! {

pub trait T {
    type r#type;
    fn f(&self) -> u64 { 1 }
}

} // verus!
