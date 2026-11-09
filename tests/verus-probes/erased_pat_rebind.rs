// probe: 書き下しの消去フィールドのパターン（#152）。実行文脈の let の文と let-else は Tracked(x) / Ghost(x) のまま通る。ほかの位置（match の腕・| の選択肢・if let・while let・for・クロージャの引数・fn の引数）は、欄を別の名前に束縛して本体の先頭で let Tracked(x) = .. / let Ghost(x) = .. とすれば通り、論理文脈（spec fn の let・match・if let・引数）は let x = ..@ とすれば通る
// expect: verified 12
use vstd::prelude::*;

verus! {

pub struct B {
    pub n: u64,
    pub items: Tracked<Map<u64, u64>>,
    pub g: Ghost<Seq<u64>>,
}

fn exec_let(b: B) -> (r: B)
    ensures r.items@ == b.items@, r.g@ == b.g@, r.n == b.n,
{
    let B { items: Tracked(items), g: Ghost(g), n } = b;
    B { n, items: Tracked(items), g: Ghost(g) }
}

fn exec_let_else(o: Option<B>) -> (r: Option<B>)
    ensures o is None ==> r is None,
{
    let Some(B { items: Tracked(items), g: Ghost(g), n }) = o else {
        return None;
    };
    Some(B { n, items: Tracked(items), g: Ghost(g) })
}

fn in_match(b: B) -> (r: B)
    ensures r.items@ == b.items@, r.g@ == b.g@, r.n == b.n,
{
    match b {
        B { items: __eyk_pat_items, g: __eyk_pat_g, n } => {
            let Tracked(items) = __eyk_pat_items;
            let Ghost(g) = __eyk_pat_g;
            B { n, items: Tracked(items), g: Ghost(g) }
        },
    }
}

fn in_or(o: Option<B>) -> u64 {
    match o {
        Some(B { items: __eyk_pat_items, n: 0, .. }) | Some(B { items: __eyk_pat_items, n: 1, .. }) => {
            let Tracked(items) = __eyk_pat_items;
            0
        },
        _ => 1,
    }
}

fn in_if_let(o: Option<B>) -> (r: Option<B>)
    ensures o is Some ==> r is Some && r->Some_0.g@ == o->Some_0.g@,
{
    if let Some(B { items: __eyk_pat_items, g: __eyk_pat_g, n }) = o {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(g) = __eyk_pat_g;
        Some(B { n, items: Tracked(items), g: Ghost(g) })
    } else {
        None
    }
}

fn in_while_let(v: Vec<B>) {
    let mut v = v;
    while let Some(B { items: __eyk_pat_items, g: __eyk_pat_g, n }) = v.pop()
        decreases v.len(),
    {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(g) = __eyk_pat_g;
    }
}

fn in_for(v: Vec<B>) {
    for B { items: __eyk_pat_items, g: __eyk_pat_g, n } in v {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(g) = __eyk_pat_g;
    }
}

fn in_closure(b: B) -> B {
    let f = |B { items: __eyk_pat_items, g: __eyk_pat_g, n }: B| -> B {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(g) = __eyk_pat_g;
        B { n, items: Tracked(items), g: Ghost(g) }
    };
    f(b)
}

fn in_param(B { items: __eyk_pat_items, g: __eyk_pat_g, n }: B) -> (r: B)
    requires n > 0,
    ensures r.n == n,
{
    let Tracked(items) = __eyk_pat_items;
    let Ghost(g) = __eyk_pat_g;
    B { n, items: Tracked(items), g: Ghost(g) }
}

spec fn spec_let(b: B) -> (Map<u64, u64>, Seq<u64>) {
    let B { items: __eyk_pat_items, g: __eyk_pat_g, .. } = b;
    let items = __eyk_pat_items@;
    let g = __eyk_pat_g@;
    (items, g)
}

spec fn spec_match(o: Option<B>) -> Map<u64, u64> {
    match o {
        Some(B { items: __eyk_pat_items, .. }) => {
            let items = __eyk_pat_items@;
            items
        },
        None => Map::empty(),
    }
}

spec fn spec_if_let(o: Option<B>) -> bool {
    if let Some(B { g: __eyk_pat_g, .. }) = o {
        let g = __eyk_pat_g@;
        g.len() > 0
    } else {
        false
    }
}

spec fn spec_param(B { items: __eyk_pat_items, g: __eyk_pat_g, .. }: B) -> Seq<u64> {
    let items = __eyk_pat_items@;
    let g = __eyk_pat_g@;
    g
}

fn uses_spec(b: B) -> (r: B)
    ensures
        spec_let(r).0 == b.items@,
        spec_let(r).1 == b.g@,
        spec_match(Some(r)) == b.items@,
        spec_if_let(Some(r)) == (b.g@.len() > 0),
        spec_param(r) == b.g@,
{
    b
}

} // verus!
