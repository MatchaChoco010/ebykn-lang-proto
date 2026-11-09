//! PR #15 のレビューで見つかった穴の再発防止。入力はレビューで示されたもの。

mod support;
use support::*;

/// B1：属性の検査は、`#[..]` の最初の識別子だけでなく、`r#`、先頭の `::`、`cfg_attr` の中、
/// `verus::` の名前空間も見る。
#[test]
fn hidden_verifier_and_proof_attributes_are_rejected() {
    let e0102 = "E0102: `#[verifier::..]` attributes cannot be written in Ebykn";
    for src in [
        "#[cfg_attr(verus_keep_ghost, verifier::external_body)] fn f() ensures false {}",
        "#![cfg_attr(verus_keep_ghost, verifier::exec_allows_no_decreases_clause)]\nfn f() {}",
        "#[r#verifier::external_body] fn f() {}",
        "#[::verifier::external_body] fn f() {}",
        "#[cfg_attr(all(), cfg_attr(any(), verifier::opaque))] fn f() {}",
    ] {
        rejects(src, e0102);
    }
    rejects("fn f() { #[r#trigger] x }", "`#[trigger]` is proof syntax");
    rejects(
        "fn f() { #[cfg_attr(all(), trigger)] x }",
        "`#[trigger]` is proof syntax",
    );
    rejects(
        "#[verus::internal(spec)] fn f() {}",
        "`#[verus::..]` is Verus syntax and cannot be written in `.eyk`",
    );
    // 条件の中の語は属性ではない。
    ok("#[cfg_attr(feature = \"verifier\", derive(Clone))] struct S { a: u64 }");
}

/// B2-1：`verus!` と Verus のマクロは、項目・文・式のどの位置でも拒否する。
#[test]
fn verus_macros_are_rejected() {
    let want = "`verus!` is Verus syntax and cannot be written in `.eyk`";
    rejects(
        "verus! { proof fn bad() ensures false { assume(false); } }",
        want,
    );
    rejects(
        "fn f() { verus! { proof fn bad() ensures false { assume(false); } } }",
        want,
    );
    rejects("fn f() { let x = verus!(1); }", want);
    rejects(
        "fn f() { proof_decl! { } }",
        "`proof_decl!` is Verus syntax",
    );
    rejects(
        "fn f() { calc! { (==) a; { } b } }",
        "`calc!` is Verus syntax",
    );
}

/// B2-2：vstd の仕様用のマクロの引数は Ebykn の式として読む。
#[test]
fn spec_macro_arguments_are_ebykn_expressions() {
    assert_eq!(e("seq![n] + self@order"), "(+ seq![n] (@ self order))");
    assert_eq!(e("set![a, b@x]"), "set![a, (@ b x)]");
    assert_eq!(e("map![1 => a, 2 => b]"), "map![1 => a, 2 => b]");
    rejects(
        "fn f() { seq![forall|i: int| i > 0] }",
        "`forall|x| ..` is Verus syntax; in Ebykn write `forall x: T, P`",
    );
    rejects("fn f() { set![a &&& b] }", "E0001");
}

/// B2-3：ほかのマクロの引数は読まないが、証明の構文と Verus の構文を字句的に探す。
#[test]
fn proof_syntax_inside_other_macros_is_rejected() {
    for (src, want) in [
        (
            "vec![{ proof { assume(false); } 1u64 }]",
            "`proof { .. }` is proof syntax",
        ),
        (
            "println!(\"{}\", assume(false))",
            "`assume(..)` is Verus syntax",
        ),
        ("vec![admit()]", "`admit(..)` is Verus syntax"),
        ("vec![assert(x)]", "`assert(..)` is proof syntax"),
        (
            "vec![forall|i: int| i > 0]",
            "`forall|x| ..` is Verus syntax",
        ),
        (
            "vec![exists|i: int| i > 0]",
            "`exists|x| ..` is Verus syntax",
        ),
        (
            "vec![choose|i: int| i > 0]",
            "`choose|x| ..` is Verus syntax",
        ),
        (
            "vec![{ let ghost g = 1; g }]",
            "`let ghost / let tracked` is Verus syntax",
        ),
        ("vec![#[verifier::truncate] (x as u8)]", "E0102"),
        ("vec![#[trigger] x]", "`#[trigger]` is proof syntax"),
        ("vec![verus!{}]", "`verus!` is Verus syntax"),
    ] {
        rejects(&format!("fn f() {{ {src}; }}"), want);
    }

    // Rust のマクロはそのまま。
    ok("fn f() { println!(\"{} {}\", a, b); assert!(x); assert_eq!(a, b); let v = vec![1, 2]; }");
}

/// S1：`assert` などの呼び出しは、式のどの位置でも拒否する。
#[test]
fn proof_calls_are_rejected_in_any_expression_position() {
    for src in [
        "fn f() { foo(assert(x)) }",
        "fn f() { x + assert(y) }",
        "fn f() ensures assert(false) { }",
        "fn f() { let y = reveal(g); }",
        "fn f() { let y = hide(g); }",
        "fn f() { let y = reveal_with_fuel(g, 2); }",
    ] {
        rejects(src, "is proof syntax");
    }
    for src in [
        "fn f() { let y = assume(false); }",
        "fn f() -> bool = assume(false);",
        "fn f() { let y = admit(); }",
    ] {
        rejects(src, "is Verus syntax and cannot be written in `.eyk`");
    }
    // パス付きの呼び出しやメソッドは利用者の関数。
    ok("fn f() { m::assume(x); s.assert(y); }");
}

/// S2：差し込み口は `parse_eyk` の外では何もしない（上流と同じに動く）。
#[test]
fn hook_is_inert_outside_parse_eyk() {
    ok("fn f() { forall x: int, x > 0 }");
    let f: verus_syn::File = verus_syn::parse_str("fn f() { fn g() {} }").expect("parse");
    assert_eq!(f.items.len(), 1);
    let f: verus_syn::File =
        verus_syn::parse_str("fn f() { while x invariant y ensures z { } let a = x@; }")
            .expect("parse");
    assert_eq!(f.items.len(), 1);
    // ほかのスレッドでも。
    std::thread::spawn(|| {
        let f: verus_syn::File = verus_syn::parse_str("fn f() { fn g() {} }").expect("parse");
        assert_eq!(f.items.len(), 1);
    })
    .join()
    .unwrap();
}

/// S3：`self@` などは権限ではない（文法は `Ident "@"`）。
#[test]
fn keyword_paths_are_not_permissions() {
    // `crate@` と `super@` はフォークがパスとして読まない（構文エラー）。
    rejects("fn f() { crate@ == x }", "E0001");
    for src in ["self@", "Self@"] {
        rejects(
            &format!("fn f() {{ {src} == x }}"),
            "`e@` is Verus syntax; in Ebykn write `e.view()`",
        );
    }
    assert_eq!(e("p@"), "p@");
    assert_eq!(e("self@x"), "(@ self x)");
}

/// S4：`impl spec::Foo for T` は Rust の trait の impl（`impl spec` ではない）。
#[test]
fn impl_of_a_path_starting_with_spec_is_rust() {
    assert_eq!(
        heads("impl spec::Foo for T { spec fn f() -> int = 1; }"),
        ["impl spec::Foo for T (1 items)"]
    );
}

/// S5：`broadcast` などは、後ろに Verus の形が続くときだけ Verus の構文とする。
#[test]
fn verus_item_words_are_identifiers_otherwise() {
    ok("fn f() { broadcast(tx, 1); uninterp.run(); assume_specification(); }");
    rejects("broadcast use foo;", "`broadcast` is Verus syntax");
    rejects("broadcast group g { a }", "`broadcast` is Verus syntax");
    rejects("broadcast proof fn p() {}", "`broadcast` is Verus syntax");
    rejects(
        "uninterp spec fn u() -> int;",
        "`uninterp spec fn` is Verus syntax",
    );
    rejects(
        "assume_specification [ std::mem::swap ] (a: u64);",
        "`assume_specification` is Verus syntax",
    );
}

/// 入れ子の項目は深さについて線形に読む（指数的に読み直すと終わらない深さ）。
#[test]
fn nested_items_parse_in_linear_time() {
    let depth = 40;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&format!("fn f{i}() {{ "));
    }
    src.push_str("let __eyk_x = 1;");
    for _ in 0..depth {
        src.push_str(" }");
    }
    // 深い入れ子は debug ビルドの既定のスタックに収まらないので、大きいスタックで読む。
    // 指数的に読み直すと終わらないので、時間切れで失敗させる（深さ 16 で約 1 秒だった）。
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let _ = tx.send(diags(&src));
        })
        .unwrap();
    let d = rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("入れ子の項目を 30 秒で読み終えない（指数的に読み直している）");
    // 一番内側の診断は一度だけ出る。
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].contains("E0101"));
}

/// パターンの `@name` / `@..` に付いた属性は残す。
#[test]
fn attributes_on_erased_field_patterns_are_kept() {
    assert_eq!(
        e("let S { a, #[cfg(x)] @r, #[cfg(y)] @.. } = s;"),
        "(let S { a, #[cfg] @r, #[cfg] @.. } s)"
    );
}

/// `where` の後の `for<'a>` も Rust の where 句。
#[test]
fn higher_ranked_where_is_rust() {
    assert_eq!(
        heads("type T = u64 where for<'a> &'a u64: Copy;"),
        ["type T = u64 where for <'a > &'a u64 : Copy"]
    );
}

/// `async` などの中も、証明の構文を字句的に探す。
#[test]
fn proof_syntax_inside_opaque_rust_expressions_is_rejected() {
    rejects(
        "fn f() { foo(forall|x: int| x > 0).await }",
        "`forall|x| ..` is Verus syntax",
    );
    rejects(
        "fn f() { async { assume(false) } }",
        "`assume(..)` is Verus syntax",
    );
}

// ---------------------------------------------------------------------------
// 再レビュー（#16 の既定）

const NOT_ALLOWED: &str = "cannot be called in `.eyk`; only these macros can:";

/// N1：`.eyk` で呼べるマクロは固定の一覧だけ。別名・自作のマクロ・別のファイルの取り込みを拒否する。
#[test]
fn only_listed_macros_can_be_called() {
    // 別名を付ける。
    rejects(
        "use vstd::prelude::verus as v;\nv! { pub assume_specification [core::mem::drop::<u64>] (x: u64) ensures false; }",
        "`use vstd::prelude::verus as v` is Verus syntax",
    );
    rejects(
        "use vstd::prelude::verus as v;\nv! { pub assume_specification [core::mem::drop::<u64>] (x: u64) ensures false; }",
        &format!("macro `v!` {NOT_ALLOWED}"),
    );
    // 自作の macro_rules!（定義も呼び出しも）。
    let src = "macro_rules! m { ($v:ident, $a:ident) => { $v! { fn bad() ensures false { $a(false); } } } }\nfn g() { m!(verus, assume); }";
    rejects(
        src,
        &format!("1:1: E0001: syntax error: macro `macro_rules!` {NOT_ALLOWED}"),
    );
    rejects(
        src,
        &format!("2:10: E0001: syntax error: macro `m!` {NOT_ALLOWED}"),
    );
    rejects(
        "fn f() { paste::paste! { [<ver us>]! {} } }",
        &format!("macro `paste::paste!` {NOT_ALLOWED}"),
    );
    rejects(
        "fn f() { verus_builtin_macros::x! {} }",
        "cannot refer to the crate `verus_builtin_macros` directly",
    );
    // 別のファイルを読み込む。
    rejects(
        "include!(\"evil.rs\");",
        &format!("macro `include!` {NOT_ALLOWED}"),
    );
    rejects(
        "fn f() { include!(\"evil.rs\"); }",
        &format!("macro `include!` {NOT_ALLOWED}"),
    );
    rejects(
        "fn f() { let v = vec![include!(\"x\")]; }",
        &format!("macro `include!` {NOT_ALLOWED}"),
    );
    let unread = "cannot be written in `.eyk`, because it brings in code that Ebykn does not read";
    rejects(
        "#[path = \"evil.rs\"] mod evil;",
        &format!("`#[path]` {unread}"),
    );
    rejects(
        "#[cfg_attr(all(), path = \"evil.rs\")] mod evil;",
        &format!("`#[path]` {unread}"),
    );
    rejects(
        "#[macro_use] extern crate foo;",
        &format!("`#[macro_use]` {unread}"),
    );
    // 呼べるマクロの名前を外のクレートから取り込む・改名する。
    rejects("use foo::vec;", &format!("`use foo::vec` {unread}"));
    rejects(
        "use foo::bar as seq;",
        &format!("`use foo::bar as seq` {unread}"),
    );
    // Verus の属性とパス。
    rejects(
        "#[verus_verify(external_body)] fn f() {}",
        "`#[verus_verify]` is Verus syntax",
    );
    rejects(
        "#[verus_spec(ensures false)] fn f() {}",
        "`#[verus_spec]` is Verus syntax",
    );
    rejects(
        "fn f() { ::verus_builtin::assume_(false) }",
        "cannot refer to the crate `verus_builtin` directly",
    );
    rejects(
        "fn f() { vstd::prelude::admit() }",
        "cannot refer to the crate `vstd` directly",
    );
    rejects(
        "fn f() { vec![::verus_builtin::assume_(false)] }",
        "cannot refer to the crate `verus_builtin` directly",
    );
    // どの位置のマクロも同じ。
    for src in [
        "fn f(x: m!()) {}",
        "trait T { m!(); }",
        "impl S { m!(); }",
        "fn f() { let m!() = 1; }",
    ] {
        rejects(src, &format!("macro `m!` {NOT_ALLOWED}"));
    }
    // 一覧のマクロは呼べる（裸の名前、std / core / alloc、vstd のパス）。
    ok(
        "fn f() { seq![1, 2]; std::println!(\"x\"); core::assert!(x); let v = vec![1]; \
        let s = format!(\"{}\", line!()); todo!() }\nuse std::vec;\nuse crate::m::map;",
    );
}

/// 診断に一覧を出し、自作のマクロは `unchecked fn` で書けることを伝える（英語と日本語）。
#[test]
fn not_allowed_macro_message_lists_the_allowed_macros() {
    let d = diags("fn f() { dbg!(x); }");
    assert_eq!(d.len(), 1);
    assert!(d[0].starts_with("src/lib.eyk:1:10: E0001: syntax error: macro `dbg!` cannot be called in `.eyk`; only these macros can: vec, println,"), "{}", d[0]);
    assert!(d[0].ends_with("seq, set, map. To use your own macro, write the function that needs it as an `unchecked fn`"), "{}", d[0]);
    let j = diags_in("fn f() { dbg!(x); }", ebykn_syntax::diag::Lang::Ja);
    assert!(
        j[0].contains(
            "`.eyk` ではマクロ `dbg!` を呼べません。呼べるのは次のマクロだけです: vec、println、"
        ),
        "{}",
        j[0]
    );
    assert!(
        j[0].ends_with("自作のマクロが要るなら、それを使う関数を `unchecked fn` にします"),
        "{}",
        j[0]
    );
}

/// Verus のマクロの名前は、呼び出しでも `use` でも Verus の構文として拒否する（一覧は
/// `VERUS_MACROS` と独立に書く。一覧から名前を消すとこのテストが落ちる）。
#[test]
fn every_verus_macro_name_is_rejected() {
    for name in [
        "verus",
        "verus_",
        "proof",
        "proof_decl",
        "proof_with",
        "calc",
        "assert_by_contradiction",
        "assert_seqs_equal",
        "assert_sets_equal",
        "assert_maps_equal",
        "open_atomic_invariant",
        "open_local_invariant",
        "open_atomic_invariant_in_proof",
        "open_local_invariant_in_proof",
        "struct_with_invariants",
        "atomic_with_ghost",
        "state_machine",
        "tokenized_state_machine",
        "case_on_next",
        "case_on_init",
        "builtin_macros",
        "verus_proof_macro_exprs",
        "verus_exec_macro_exprs",
        "verus_proof_expr",
    ] {
        let want = format!("`{name}!` is Verus syntax and cannot be written in `.eyk`");
        rejects(&format!("fn f() {{ {name}! {{ }} }}"), &want);
        rejects(&format!("fn f() {{ vec![{name}!()] }}"), &want);
        rejects(
            &format!("use vstd::prelude::{name} as m;"),
            &format!("`use vstd::prelude::{name} as m` is Verus syntax"),
        );
    }
}

/// N2：`&&` と `&` の並び、参照の参照、クロージャの並びを、マクロの引数でも拒否しない。
#[test]
fn ampersands_and_bars_in_macro_arguments_are_rust() {
    ok(
        "fn f() { assert!(a && &b == &c); println!(\"{:?}\", &&&x); let v = vec![|| || 1]; \
        assert!(a || |x: u64| x > 0 == b); }",
    );
}

/// N3：宣言する名前は呼び出しではない。`extern` ブロックは中を項目として読む。
#[test]
fn declarations_are_not_calls() {
    ok("extern \"C\" { fn hide(w: u64); fn assert(c: i32); static reveal: u64; }");
    // 中の型は読む。
    rejects(
        "extern \"C\" { fn f(x: *const [u8; { assume(false); 1 }]); }",
        "`assume(..)` is Verus syntax",
    );
    rejects(
        "extern \"C\" { m!(); }",
        &format!("macro `m!` {NOT_ALLOWED}"),
    );
    // 項目として読むので、引数の `tracked` も見つける（字句の検査では見つけない）。
    rejects(
        "extern \"C\" { fn f(tracked x: u64); }",
        "`tracked` is Verus syntax; in Ebykn write `uses`",
    );
    // 中を読まないトークン列でも、`fn` の直後の名前は呼び出しではない。
    ok("fn f() { let v = vec![{ fn hide() {} fn assert() {} 1 }]; }");
}

/// N4：字句の検査の各形を、一つずつ固定する（外すと落ちる）。
#[test]
fn each_opaque_scan_form_is_rejected() {
    rejects(
        "fn f() { vec![{ proof fn p() {} 1 }] }",
        "`proof fn` is Verus syntax",
    );
    rejects(
        "fn f() { vec![{ exec fn p() {} 1 }] }",
        "`exec fn` is Verus syntax",
    );
    rejects("fn f() { vec![reveal(g)] }", "`reveal(..)` is proof syntax");
    rejects(
        "fn f() { vec![reveal_with_fuel(g, 1)] }",
        "`reveal_with_fuel(..)` is proof syntax",
    );
    rejects("fn f() { vec![hide(g)] }", "`hide(..)` is proof syntax");
    rejects(
        "fn f() { vec![assume_(x)] }",
        "`assume_(..)` is Verus syntax",
    );
}

/// N4：中を読まない型と項目の中も検査する。
#[test]
fn opaque_types_and_items_are_scanned() {
    // 文の先頭でない `assume(..)` は、構文解析器が読むだけで拒否しない。中を読まない所では
    // 字句の検査だけが見つける。
    let a = "{ let a = assume(false); 1 }";
    // 生ポインタの型（中を読まない型）。
    rejects(
        &format!("fn f(x: *const [u8; {a}]) {{}}"),
        "`assume(..)` is Verus syntax",
    );
    // `union` のフィールド。
    rejects(
        &format!("union U {{ a: [u8; {a}] }}"),
        "`assume(..)` is Verus syntax",
    );
    // where 句の述語とトレイト境界。
    rejects(
        &format!("fn f<T>() where T: Into<*const [u8; {a}]> {{}}"),
        "`assume(..)` is Verus syntax",
    );
    rejects(
        &format!("fn f<T: Into<*const [u8; {a}]>>() {{}}"),
        "`assume(..)` is Verus syntax",
    );
}

/// N4：vstd の仕様用のマクロの引数を Ebykn の式として読む。
#[test]
fn spec_macro_arguments_are_parsed() {
    use ebykn_syntax::ast::*;
    let f = ok("fn f() { seq![forall x: int, x > 0, self@x] }");
    let ItemKind::Fn(func) = &f.items[0].kind else {
        panic!()
    };
    let Some(FnBody::Block(b)) = &func.body else {
        panic!()
    };
    let StmtKind::Expr { expr: e, .. } = &b.stmts[0].kind else {
        panic!()
    };
    let ExprKind::Macro(m) = &e.kind else {
        panic!()
    };
    let MacroArgs::Exprs(v) = &m.args else {
        panic!("引数を読んでいない")
    };
    assert!(matches!(v[0].kind, ExprKind::Quant { .. }));
    assert!(matches!(v[1].kind, ExprKind::Erased { .. }));
    // 式として読めない引数は拒否する（トークンのまま通さない）。
    rejects("fn f() { seq![a b] }", "E0001");
    rejects("fn f() { map![a b] }", "E0001");
}

/// N5：`seq` / `set` / `map` は、裸の名前か vstd のパスのときだけ vstd のマクロ。
#[test]
fn third_party_seq_is_not_vstd() {
    rejects(
        "fn f() { my::seq!(a b c) }",
        &format!("macro `my::seq!` {NOT_ALLOWED}"),
    );
    rejects(
        "fn f() { std::seq![1] }",
        &format!("macro `std::seq!` {NOT_ALLOWED}"),
    );
}

/// N6：`r#` を付けた呼び出しも拒否する。
#[test]
fn raw_identifier_proof_calls_are_rejected() {
    rejects("fn f() { r#assume(false) }", "`assume(..)` is Verus syntax");
    rejects(
        "fn f() { let x = r#assert(y); }",
        "`assert(..)` is proof syntax",
    );
}

// ---------------------------------------------------------------------------
// 3 回目のレビュー（#16 の既定を広げたもの）

const DIRECT: &str = "directly; write the bare names instead";

/// 規則 1（F1）：vstd・verus_builtin・verus_builtin_macros・builtin は、use、extern crate、
/// パス、属性、マクロの引数のどこでも参照しない。
#[test]
fn forbidden_crates_are_never_referred_to() {
    for (src, krate) in [
        ("use vstd::prelude::admit as ok;\nfn f() { ok(); }", "vstd"),
        ("use vstd as v;\nfn f() { v::prelude::admit(); }", "vstd"),
        (
            "extern crate vstd as v;\nfn f() { v::pervasive::admit(); }",
            "vstd",
        ),
        (
            "mod m { pub use vstd::prelude::admit; }\nfn f() { m::admit(); }",
            "vstd",
        ),
        (
            "use verus_builtin as b;\nfn f() ensures false { b::assume_(false); }",
            "verus_builtin",
        ),
        (
            "use vstd::prelude::{self as p};\nfn f() { p::assume_(false); }",
            "vstd",
        ),
        (
            "fn f() { println!(\"{}\", { use vstd::prelude::admit as ok; ok(); 1 }) }",
            "vstd",
        ),
        ("use ::vstd::seq::Seq;", "vstd"),
        ("use a::{b, vstd::x};", "vstd"),
        ("use a::vstd;", "vstd"),
        ("fn f(x: vstd::seq::Seq<int>) {}", "vstd"),
        ("fn f() { let vstd::X { a } = y; }", "vstd"),
        ("fn f() { vstd::seq![1] }", "vstd"),
        ("use builtin::*;", "builtin"),
        ("fn f() { builtin::admit() }", "builtin"),
        (
            "use verus_builtin_macros::verus_verify as vv;",
            "verus_builtin_macros",
        ),
    ] {
        rejects(
            src,
            &format!("cannot refer to the crate `{krate}` {DIRECT}"),
        );
    }
    // 日本語の文言。
    let j = diags_in("use vstd::prelude::*;", ebykn_syntax::diag::Lang::Ja);
    assert_eq!(
        j,
        [
            "src/lib.eyk:1:5: E0001: 構文エラー: `.eyk` はクレート `vstd` を直接参照できません。`vstd::` を書かずに、裸の名前（`Seq`、`Set`、`Map`、`seq!`、`set!`、`map!` など）を使います"
        ]
    );
    // `builtin` はよくある語なので、パスの途中と変数はよい。名前の `vstd` も変数ならよい。
    ok(
        "mod builtin { pub fn f() {} }\nfn g() { crate::builtin::f(); let builtin = 1; let vstd = builtin; a.builtin::<u8>(); }",
    );
}

/// 規則 1：付録 A・B の `.eyk` は vstd を参照せずに読める（`Seq`、`seq!` などは prelude の名前）。
#[test]
fn appendix_does_not_refer_to_vstd() {
    ok(include_str!("../../../tests/golden/stack/src/lib.eyk"));
    ok(include_str!("fixtures/dlist.eyk"));
}

/// 規則 2：std・core・alloc・vstd・ebykn_prelude という名前への改名を拒否する。
#[test]
fn reserved_names_cannot_be_given_to_something_else() {
    let want = "the names `std`, `core`, `alloc`, `vstd` and `ebykn_prelude` cannot be given to something else";
    for src in [
        "extern crate foo as std;",
        "extern crate foo as ebykn_prelude;",
        "use foo as vstd;",
        "use foo as core;",
        "use foo::bar as alloc;",
        "use foo::std;",
        "use foo::{x as ebykn_prelude};",
    ] {
        rejects(src, want);
    }
    ok("use std;\nuse core;\nextern crate alloc;\nuse std::collections::HashMap;");
}

/// 規則 3（F2）：属性の verus_verify / verus_spec は、パスの最後の名前で見る。
#[test]
fn verus_attributes_are_found_by_their_last_name() {
    rejects(
        "#[vstd::prelude::verus_verify(external_body)] fn g() -> (r: bool) ensures r == false { true }",
        "`#[verus_verify]` is Verus syntax",
    );
    rejects(
        "#[cfg_attr(all(), vstd::prelude::verus_spec(ensures false))] fn f() {}",
        "`#[verus_spec]` is Verus syntax",
    );
    rejects(
        "#[a::b::verus_spec] fn f() {}",
        "`#[verus_spec]` is Verus syntax",
    );
    // 改名した属性は、改名の use で拒否する（vstd などを参照するため）。
    rejects(
        "use vstd::prelude::verus_verify as vv;\n#[vv(external_body)] fn g() ensures false {}",
        &format!("cannot refer to the crate `vstd` {DIRECT}"),
    );
}

/// F3：許可リストは最後の名前だけでは判定しない（パスの先頭も見る）。
#[test]
fn allowed_macro_names_under_other_paths_are_rejected() {
    rejects(
        "fn f() { evil::vec![1] }",
        &format!("macro `evil::vec!` {NOT_ALLOWED}"),
    );
    rejects(
        "fn f() { evil::println!(\"x\") }",
        &format!("macro `evil::println!` {NOT_ALLOWED}"),
    );
    rejects(
        "fn f() { vec![evil::vec![1]] }",
        &format!("macro `evil::vec!` {NOT_ALLOWED}"),
    );
}

/// F4：属性の値の中のマクロにも許可リストをかける。
#[test]
fn macros_in_attribute_values_are_checked() {
    rejects(
        "#[doc = evil!()] fn f() {}",
        &format!("macro `evil!` {NOT_ALLOWED}"),
    );
    rejects("#[doc = verus!{}] fn f() {}", "`verus!` is Verus syntax");
    rejects(
        "#[cfg_attr(all(), doc = evil!())] fn f() {}",
        &format!("macro `evil!` {NOT_ALLOWED}"),
    );
    ok("#[doc = \"text\"] #[doc = concat!(\"a\", \"b\")] fn f() {}");
}

/// 4 回目のレビュー：`use` の `{ }` のグループの中の改名も、参照しないクレートとして拒否する。
#[test]
fn forbidden_crates_in_use_groups_are_rejected() {
    for (src, krate) in [
        ("use {vstd as v};\nfn f() { v::prelude::admit(); }", "vstd"),
        ("use {self::m, vstd as v};", "vstd"),
        (
            "use {builtin as b};\nfn f() { b::assume_(false); }",
            "builtin",
        ),
        (
            "use {verus_builtin as vb, verus_builtin_macros as vm};",
            "verus_builtin",
        ),
        (
            "use {verus_builtin as vb, verus_builtin_macros as vm};",
            "verus_builtin_macros",
        ),
        ("use {vstd};", "vstd"),
    ] {
        rejects(
            src,
            &format!("cannot refer to the crate `{krate}` {DIRECT}"),
        );
    }
    // 同じ位置の同じ診断は一つにまとめる。
    assert_eq!(diags("use vstd as v;").len(), 1);
    // グループの中の自分のモジュールはよい。
    ok("use {self::m, crate::n as k};");
}
