// probe: 構造体パターンの中の Tracked(x) / Ghost(x) を、match の腕・if let・クロージャの引数・fn の引数の中に書くと E0532 になる（#152。let の文と let-else では通る。erased_pat_rebind.rs）
// expect: rejected
// expect-output-count: 4 cannot find tuple struct or tuple variant `Tracked` in this scope
// expect-output-count: 4 cannot find tuple struct or tuple variant `Ghost` in this scope
use vstd::prelude::*;

verus! {

pub struct B {
    pub n: u64,
    pub items: Tracked<Map<u64, u64>>,
    pub g: Ghost<Seq<u64>>,
}

pub fn in_match(b: B) -> B {
    match b {
        B { items: Tracked(items), g: Ghost(g), n } => B { n, items: Tracked(items), g: Ghost(g) },
    }
}

pub fn in_if_let(o: Option<B>) -> Option<B> {
    if let Some(B { items: Tracked(items), g: Ghost(g), n }) = o {
        Some(B { n, items: Tracked(items), g: Ghost(g) })
    } else {
        None
    }
}

pub fn in_param(B { items: Tracked(items), g: Ghost(g), n }: B) -> B {
    B { n, items: Tracked(items), g: Ghost(g) }
}

pub fn in_closure(b: B) -> B {
    let f = |B { items: Tracked(items), g: Ghost(g), n }: B| -> B {
        B { n, items: Tracked(items), g: Ghost(g) }
    };
    f(b)
}

} // verus!
