// probe: const generics の引数を予約語の r# で名前にすると（const r#type: u64）、Verus が検証の前に拒否する（#119）
// verus-args: --edition=2024
// expect: rejected
// expect-output: type parameter r#type used before being declared
use vstd::prelude::*;

verus! {

fn const_kw<const r#type: u64>() -> (r: u64)
    ensures
        r == r#type,
{
    r#type
}

} // verus!
