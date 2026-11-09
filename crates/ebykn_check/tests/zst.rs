//! E0517：`Ptr<T>` の `T` に確実にゼロサイズ型と分かる型を使った（#12）。

mod support;
use support::*;

const UNIT: &str = "pub struct Unit;\npub struct Empty {}\npub struct Tup();\n";

#[test]
fn e0517_unit_type() {
    rejects(
        "pub fn f() { let p = Ptr::<()>::new(()); let _ = p.free(); }",
        "E0517",
        "`()` is a zero-sized type",
    );
    rejects(
        "pub fn f() { let p = Ptr::new(()); let _ = p.free(); }",
        "E0517",
        "use `Cell<()>`",
    );
    rejects("pub fn f(p: Ptr<()>) uses p@ { leak p@; }", "E0517", "`()`");
    rejects(
        "pub fn f() -> (q: Ptr<( )>) gives q@ { let q = Ptr::new(1u8); q }",
        "E0517",
        "`( )`",
    );
}

#[test]
fn e0517_structs_without_runtime_fields() {
    for ty in ["Unit", "Empty", "Tup"] {
        rejects(
            &format!("{UNIT}pub fn f(p: Ptr<{ty}>) uses p@ {{ leak p@; }}"),
            "E0517",
            &format!("`{ty}` is a zero-sized type"),
        );
    }
    // owns / spec だけの構造体（消去されるフィールドは実行時に 0 バイト）。
    rejects(
        "pub struct Node { pub val: u64 }\n\
         pub struct Ghost {} owns { nodes: Region<Ptr<Node>> } spec { n: nat }\n\
         pub fn f(p: Ptr<Ghost>) uses p@ { leak p@; }",
        "E0517",
        "`Ghost`",
    );
    // ゼロサイズ型のフィールドしかない構造体。
    rejects(
        &format!(
            "{UNIT}pub struct Pair {{ a: Unit, b: (), c: [u64; 0], d: [Empty; 3], e: std::marker::PhantomData<u64> }}\n\
             pub fn f(p: Ptr<Pair>) uses p@ {{ leak p@; }}"
        ),
        "E0517",
        "`Pair`",
    );
}

#[test]
fn e0517_enums_with_at_most_one_zero_sized_variant() {
    for def in [
        "pub enum M { A }",
        "pub enum M { A = 5 }",
        "pub enum M {}",
        "pub enum M { A((), [u8; 0]) }",
        "pub enum M { A { x: () } }",
    ] {
        rejects(
            &format!("{def}\npub fn f(p: Ptr<M>) uses p@ {{ leak p@; }}"),
            "E0517",
            "`M` is a zero-sized type",
        );
    }
    rejects(
        "pub enum W<T> { A(T) }\npub fn f(p: Ptr<W<()>>) uses p@ { leak p@; }",
        "E0517",
        "`W<()>`",
    );
    // 列挙子の値（`M::A`）。
    rejects(
        "pub enum M { A }\npub fn f() { let p = Ptr::new(M::A); let _ = p.free(); }",
        "E0517",
        "`M` is a zero-sized type",
    );
}

#[test]
fn accepts_enums_that_are_not_zero_sized() {
    for def in [
        "pub enum M { A, B }",
        "pub enum M { A(u8) }",
        "#[repr(u8)]\npub enum M { A }",
        "#[repr(C)]\npub enum M { A }",
        "#[cfg_attr(feature = \"x\", repr(u8))]\npub enum M { A }",
        "pub enum M { #[cfg(test)] A, B }",
    ] {
        accepts(&format!(
            "{def}\npub fn f(p: Ptr<M>) uses p@ {{ leak p@; }}"
        ));
    }
    accepts("pub enum M { A, B }\npub fn f() { let p = Ptr::new(M::A); let _ = p.free(); }");
    accepts("pub enum W<T> { A(T) }\npub fn f<T>(p: Ptr<W<T>>) uses p@ { leak p@; }");
}

#[test]
fn e0517_values_of_zero_sized_types() {
    rejects(
        &format!("{UNIT}pub fn f() {{ let p = Ptr::new(Unit); let _ = p.free(); }}"),
        "E0517",
        "`Unit`",
    );
    rejects(
        &format!("{UNIT}pub fn f() {{ let p = Ptr::new(Empty {{}}); let _ = p.free(); }}"),
        "E0517",
        "`Empty`",
    );
    rejects(
        "pub fn f() { let p = Ptr::new(((), ())); let _ = p.free(); }",
        "E0517",
        "`((), ())`",
    );
}

#[test]
fn e0517_type_arguments_are_substituted() {
    rejects(
        "pub struct Wrap<T> { x: T }\npub fn f(p: Ptr<Wrap<()>>) uses p@ { leak p@; }",
        "E0517",
        "`Wrap<()>`",
    );
    // 別名の右辺、Region の中、ターボフィッシュ、構造体のフィールド。
    rejects("pub type P = Ptr<()>;", "E0517", "`()`");
    rejects(
        "pub type U = ();\npub fn f(p: Ptr<U>) uses p@ { leak p@; }",
        "E0517",
        "`U`",
    );
    rejects(
        "pub struct S { len: u64 } owns { r: Region<Ptr<()>> }",
        "E0517",
        "`()`",
    );
    rejects("pub struct S { next: Option<Ptr<()>> }", "E0517", "`()`");
    rejects(
        "pub fn f() { let p = ebykn_prelude::Ptr::<()>::empty(); leak p@; }",
        "E0517",
        "`()`",
    );
}

#[test]
fn e0517_self_in_an_impl() {
    rejects(
        &format!(
            "{UNIT}impl Unit {{ pub fn f() {{ let p = Ptr::<Self>::new(Unit); let _ = p.free(); }} }}"
        ),
        "E0517",
        "`Self`",
    );
}

#[test]
fn phantom_data_only_from_std() {
    for import in [
        "use std::marker::PhantomData;",
        "use core::marker::PhantomData;",
        "use std::marker::*;",
    ] {
        rejects(
            &format!("{import}\npub fn f(p: Ptr<PhantomData<u64>>) uses p@ {{ leak p@; }}"),
            "E0517",
            "`PhantomData<u64>`",
        );
    }
    // std 以外から取り込んだ `PhantomData` は、std のものとみなさない。
    accepts(
        "pub mod other { pub struct PhantomData<T>(pub T); }\n\
         use other::PhantomData;\n\
         pub fn f(p: Ptr<PhantomData<u64>>) uses p@ { leak p@; }",
    );
    accepts(
        "use std::marker::*;\nuse extern_crate::PhantomData;\n\
         pub fn f(p: Ptr<PhantomData<u64>>) uses p@ { leak p@; }",
    );
    accepts("pub fn f(p: Ptr<PhantomData<u64>>) uses p@ { leak p@; }");
}

#[test]
fn accepts_types_that_are_not_zero_sized() {
    accepts("pub fn f() { let p = Ptr::new(1u64); let _ = p.free(); }");
    accepts("pub struct S { x: u8 }\npub fn f(p: Ptr<S>) uses p@ { leak p@; }");
    accepts("pub struct S(u8);\npub fn f(p: Ptr<S>) uses p@ { leak p@; }");
    accepts("pub fn f(p: Ptr<[u8; 1]>) uses p@ { leak p@; }");
    accepts("pub fn f(p: Ptr<((), u8)>) uses p@ { leak p@; }");
    // ノードはリンクのフィールドを持つので、`Node<()>` はゼロサイズ型でない（`Stack<()>` が確保するもの）。
    accepts(
        "pub struct Node<T> { pub val: T, next: Option<Ptr<Node<T>>> }\n\
         pub fn f() { let p = Ptr::new(Node { val: (), next: None }); let _ = p.free(); }\n\
         pub fn g(p: Ptr<Node<()>>) uses p@ { leak p@; }",
    );
    // `Cell<T>` はゼロサイズ型でもよい（識別子を持つ）。
    accepts("pub fn f() { let c = Cell::new(()); leak c@; }");
    accepts("pub fn f(c: Cell<()>) uses c@ { leak c@; }");
    // 実行時のフィールドを持つ構造体は、消去フィールドがあってもゼロサイズ型でない。
    accepts(
        "pub struct Node { pub val: u64 }\n\
         pub struct S { len: u64 } owns { nodes: Region<Ptr<Node>> }\n\
         pub fn f(p: Ptr<S>) uses p@ { leak p@; }",
    );
}

#[test]
fn does_not_judge_generic_parameters() {
    // ジェネリックな `T` は判定しない（ビルドで止まる）。
    accepts("pub fn f<T>(v: T) { let p = Ptr::new(v); let _ = p.free(); }");
    accepts("pub fn f<T>(p: Ptr<T>) uses p@ { leak p@; }");
    accepts("pub struct Wrap<T> { x: T }\npub fn f<T>(p: Ptr<Wrap<T>>) uses p@ { leak p@; }");
    // 型の引数がクレートの型と同じ名前でも、型の引数として読む。
    accepts("pub struct T;\npub fn f<T>(p: Ptr<T>) uses p@ { leak p@; }");
    accepts(
        "pub struct T;\npub struct W<T> { x: T }\npub fn f(p: Ptr<W<u8>>) uses p@ { leak p@; }",
    );
    // 型の引数の数が合わなければ判定しない。
    accepts("pub struct Wrap<T> { x: T }\npub fn f(p: Ptr<Wrap>) uses p@ { leak p@; }");
}

#[test]
fn does_not_judge_names_shadowed_in_a_body() {
    let src = format!(
        "{UNIT}pub fn f() {{ struct Unit(u64); let p = Ptr::<Unit>::new(Unit(1)); let _ = p.free(); }}"
    );
    accepts(&src);
    let src = format!(
        "{UNIT}pub mod m {{ pub struct Big(pub u64); }}\n\
         pub fn f() {{ use crate::m::Big as Unit; let p = Ptr::<Unit>::new(Unit(1)); let _ = p.free(); }}"
    );
    accepts(&src);
    let src = format!(
        "{UNIT}pub mod m {{ pub struct Unit(pub u64); }}\n\
         pub fn f() {{ use crate::m::*; let p = Ptr::<Unit>::new(Unit(1)); let _ = p.free(); }}"
    );
    accepts(&src);
}

#[test]
fn does_not_judge_a_crate_type_named_ptr() {
    accepts("pub struct Ptr<T> { x: T }\npub fn f(p: Ptr<()>) { let _ = p; }");
}

#[test]
fn stops_on_recursive_definitions() {
    // 大きさが無限の定義（rustc が拒否する）でも止まり、ゼロサイズ型としない。
    let mut src = String::new();
    for i in 0..20 {
        src.push_str(&format!("pub struct A{i}(A{n}, A{n});\n", n = (i + 1) % 20));
    }
    src.push_str("pub fn f(p: Ptr<A0>) uses p@ { leak p@; }");
    let lines = codes(&src);
    assert!(!lines.iter().any(|c| c == "E0517"), "{lines:?}");
}

#[test]
fn reports_in_japanese_too() {
    let (checked, _) = run("pub fn f(p: Ptr<()>) uses p@ { leak p@; }");
    let d = checked
        .diags
        .iter()
        .find(|d| d.code().as_str() == "E0517")
        .expect("E0517");
    let ja = d.message.text(ebykn_syntax::diag::Lang::Ja);
    assert!(
        ja.contains("ゼロサイズ型") && ja.contains("`Cell<()>`"),
        "{ja}"
    );
}
