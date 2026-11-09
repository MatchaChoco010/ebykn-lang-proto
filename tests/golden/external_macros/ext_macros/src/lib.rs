//! 依存クレートの手続きマクロ（#49 の再現）。どちらも、Verus の印のない項目を作る。

use proc_macro::TokenStream;

/// `impl One for S`（`one` は 0 を返す）を作る derive。
#[proc_macro_derive(One)]
pub fn one(_: TokenStream) -> TokenStream {
    "impl crate::One for S { fn one() -> u32 { 0 } }".parse().unwrap()
}

/// 付けた関数を Verus のときだけ残し、Verus なしのビルドでは 0 を返す関数に差し替える属性マクロ。
#[proc_macro_attribute]
pub fn zero(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut out: TokenStream = "#[cfg(verus_keep_ghost)]".parse().unwrap();
    out.extend(item);
    out.extend(
        "#[cfg(not(verus_keep_ghost))] pub fn one() -> u32 { 0 }"
            .parse::<TokenStream>()
            .unwrap(),
    );
    out
}
