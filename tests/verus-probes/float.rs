// probe: 浮動小数点。is_finite_spec などのビット述語は spec としてあるが、演算は未解釈で 1.0 + 1.0 == 2.0 も示せない
// expect: errors 1 1
// expect-output: assertion failed
use vstd::prelude::*;
use vstd::float::*;

verus! {

proof fn has_predicates(x: f64) {
    assert(x.is_finite_spec() || x.is_infinite_spec() || x.is_nan_spec());
}

fn arithmetic_uninterpreted() {
    let x: f64 = 1.0;
    let y = x + x;
    assert(y == 2.0f64);
}

} // verus!
