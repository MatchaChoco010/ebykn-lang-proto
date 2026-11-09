// probe: 列挙子の名前を r# で書くと、予約語でない r#foo でも、使ったところで Verus が no variant で拒否する（#147）
// verus-args: --edition=2024
// expect: rejected
// expect-output: no variant `r#foo` for this datatype
use vstd::prelude::*;

verus! {

pub enum E { r#foo(u64), B }

fn f(e: E) -> u64 {
    match e {
        E::r#foo(x) => x,
        E::B => 0,
    }
}

} // verus!
