// probe: クロージャ。契約のないクロージャはそのまま書けて、呼び出しの結果は ensures で縛らないと分からない。契約付きクロージャ（OPEN-12）は Verus の構文で書ける
// expect: verified 2
use vstd::prelude::*;

verus! {

fn plain_closure() {
    let f = |x: u64| -> u64 { x };
    let _ = f(1);
}

fn contracted_closure() {
    let f = |x: u64| -> (r: u64)
        requires
            x < 100,
        ensures
            r == x + 1,
        { x + 1 };
    let y = f(1);
    assert(y == 2);
}

} // verus!
