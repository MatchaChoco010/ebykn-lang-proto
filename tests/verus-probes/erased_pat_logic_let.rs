// probe: 論理文脈（spec fn・proof fn の本体）では、let の文でも Tracked(x) のパターンを受けない（#152。Ghost(x) は通る）
// expect: rejected
// expect-output-count: 1 pattern cannot have mode proof
// expect-output-count: 1 expression has mode spec, expected mode proof
use vstd::prelude::*;

verus! {

pub struct B {
    pub n: u64,
    pub items: Tracked<Map<u64, u64>>,
    pub g: Ghost<Seq<u64>>,
}

pub open spec fn spec_ghost(b: B) -> Seq<u64> {
    let B { g: Ghost(g), .. } = b;
    g
}

pub open spec fn spec_tracked(b: B) -> Map<u64, u64> {
    let B { items: Tracked(items), .. } = b;
    items
}

pub proof fn proof_ghost(b: B) -> (r: Seq<u64>)
    ensures r == b.g@,
{
    let B { g: Ghost(g), .. } = b;
    g
}

pub proof fn proof_tracked(b: B) -> (r: Map<u64, u64>)
    ensures r == b.items@,
{
    let B { items: Tracked(items), .. } = b;
    items
}

} // verus!
