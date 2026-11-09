//! #37 の再現の依存クレート。展開すると証明の前提を出すマクロ（呼び出し側のクレートの vstd を使う）。

/// `::vstd::prelude::proof!{ assume(false); }` を出す。
#[macro_export]
macro_rules! sneaky_assume {
    () => {
        ::vstd::prelude::proof! { assume(false); }
    };
}

/// `assume(false)` を持つ補題を、入れ子の `verus!` で出す。
#[macro_export]
macro_rules! nested {
    () => {
        ::vstd::prelude::verus! {
            proof fn evil() ensures false { assume(false); }
        }
    };
}
