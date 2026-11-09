#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

pub struct Node { pub val: u64, }

pub struct Bag {
    pub n: u64, __eyk_erased_items: Tracked < Region < Ptr < Node >>>, __eyk_erased_hist: Ghost < Seq < u64 >>,
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_match(b: Bag) -> (r: Bag)
ensures r.n == b.n, r.__eyk_erased_items@ == b.__eyk_erased_items@, r.__eyk_erased_hist@ == b.__eyk_erased_hist@, {
    match b {
        Bag {
            __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
        }
        => {
            let Tracked(items) = __eyk_pat_items;
            let Ghost(hist) = __eyk_pat_hist;
            Bag {
                n, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
            }
        },
    }
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_guard(b: Bag) -> (r: Bag)
ensures r.__eyk_erased_items@ == b.__eyk_erased_items@, {
    match b {
        Bag {
            __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
        }
        if n > 0 => {
            let Tracked(items) = __eyk_pat_items;
            let Ghost(hist) = __eyk_pat_hist;
            Bag {
                n: n - 1, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
            }
        }, Bag {
            __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
        }
        => {
            let Tracked(items) = __eyk_pat_items;
            let Ghost(hist) = __eyk_pat_hist;
            Bag {
                n, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
            }
        },
    }
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_or(o: Option < Bag >) -> (r: u64)
ensures matches!(o, None) ==> r == 0, {
    match o {
        Some(Bag {
            __eyk_erased_items: __eyk_pat_items, n: 0, __eyk_erased_hist: __eyk_pat_hist
        }) | Some(Bag {
            __eyk_erased_items: __eyk_pat_items, n: 1, __eyk_erased_hist: __eyk_pat_hist
        }) => {
            let Tracked(items) = __eyk_pat_items;
            let Ghost(hist) = __eyk_pat_hist;
            {
                let b = Bag {
                    n: 7, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
                };
                b.n
            }
        }, Some(Bag {
            __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
        }) => {
            let Tracked(items) = __eyk_pat_items;
            let Ghost(hist) = __eyk_pat_hist;
            n
        }, None => 0,
    }
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_if_let(o: Option < Bag >) -> (r: Option < Bag >)
ensures matches!(o, None) ==> matches!(r, None), {
    if let Some(Bag {
        __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
    }) = o {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(hist) = __eyk_pat_hist;
        Some(Bag {
            n, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
        })
    } else { None }
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_while_let(v: Vec < Bag >) -> (r: u64) {
    let mut v = v;
    let mut total: u64 = 0;
    while let Some(Bag {
        __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
    }) = v.pop()
    decreases v.len(), {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(hist) = __eyk_pat_hist;
        let b = Bag {
            n, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
        };
        if total < 1000 && b.n < 1000 {
            total = total + b.n;
        }
    }
    total
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_for(v: Vec < Bag >) -> (r: u64) {
    let mut count: u64 = 0;
    for Bag {
        __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
    }
    in v {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(hist) = __eyk_pat_hist;
        let b = Bag {
            n, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
        };
        if count < 1000 {
            count = count + 1;
        }
    }
    count
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_closure(b: Bag) -> (r: u64) {
    let f = | Bag {
        __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
    }
    : Bag | -> u64 {
        let Tracked(items) = __eyk_pat_items;
        let Ghost(hist) = __eyk_pat_hist;
        {
            let c = Bag {
                n, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
            };
            c.n
        }
    };
    f(b)
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn by_param(Bag {
    __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
}: Bag) -> (r: Bag)
requires n > 0,
ensures r.n == n, {
    let Tracked(items) = __eyk_pat_items;
    let Ghost(hist) = __eyk_pat_hist;
    Bag {
        n, __eyk_erased_items: Tracked(items), __eyk_erased_hist: Ghost(hist)
    }
}

spec fn hist_of(b: Bag) -> Seq < u64 > {
    {
        let Bag { __eyk_erased_hist: __eyk_pat_hist,.. } = b;
        let hist = __eyk_pat_hist@;
        hist
    }
}

spec fn has(o: Option < Bag >, p: Ptr < Node >) -> bool {
    match o {
        Some(Bag { __eyk_erased_items: __eyk_pat_items,.. }) => {
            let items = __eyk_pat_items@;
            items.contains(p)
        }, None => false,
    }
}

spec fn hist_len(Bag { __eyk_erased_hist: __eyk_pat_hist,.. }: Bag) -> nat {
    let hist = __eyk_pat_hist@;
    hist.len()
}

#[verifier::exec_allows_no_decreases_clause] #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] fn uses_spec(b: Bag, p: Ptr < Node >) -> (r: Bag)
ensures hist_of(r) == b.__eyk_erased_hist@, has(Some(r), p) == b.__eyk_erased_items@.contains(p), hist_len(r) == b.__eyk_erased_hist@.len(), { b }

#[verifier::when_used_as_spec(__eyk_spec_count_of)] fn count_of(Bag {
    __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
}: Bag) -> (__eyk_r: u64)
ensures __eyk_r == __eyk_spec_count_of(Bag {
    __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
}), {
    let Tracked(items) = __eyk_pat_items;
    let Ghost(hist) = __eyk_pat_hist;
    n
}
spec fn __eyk_spec_count_of(Bag {
    __eyk_erased_items: __eyk_pat_items, n, __eyk_erased_hist: __eyk_pat_hist
}: Bag) -> u64 {
    let items = __eyk_pat_items@;
    let hist = __eyk_pat_hist@;
    (n) as u64
}

} // verus!
