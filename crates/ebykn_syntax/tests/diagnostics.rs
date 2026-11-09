//! 診断の形式（仕様 §10.1）：`ファイル:行:列: コード: メッセージ`、英語と日本語、JSON、まとめての報告。

mod support;
use ebykn_syntax::diag::{Code, Lang};
use support::*;

#[test]
fn format_is_file_line_col_code_message() {
    let (f, p) = parse("fn f() {}\n\nfn __eyk_g() {}\n");
    assert_eq!(p.diags.len(), 1);
    let d = &p.diags[0];
    assert_eq!(d.code(), Code::E0101);
    assert_eq!((d.span.start.line, d.span.start.col), (3, 4));
    assert_eq!((d.span.lo, d.span.hi, d.span.len()), (14, 21, 7));
    assert_eq!(
        d.render(&f, Lang::En),
        "src/lib.eyk:3:4: E0101: identifier `__eyk_g` uses the prefix `__eyk_`, which is reserved for generated names"
    );
    // 既定の言語は英語。
    assert_eq!(Lang::default(), Lang::En);
}

/// 列は `char` で数える（`OPEN.md`）。位置のバイトは別に持つ。
#[test]
fn columns_count_chars_not_bytes() {
    let src = "fn f() { let s = \"あいう\"; let __eyk_x = 1; }";
    let (_, p) = parse(src);
    let d = &p.diags[0];
    assert_eq!(d.span.start.col, 29);
    assert_eq!(d.span.lo, src.find("__eyk_x").unwrap());
    // 「あいう」は 3 文字で 9 バイト。
    assert_eq!(d.span.lo - (d.span.start.col - 1), 6);
}

#[test]
fn json_is_rustc_compatible() {
    let (f, p) = parse("fn __eyk_f() {}");
    let j = p.diags[0].to_json(&f, Lang::En);
    assert_eq!(
        j,
        "{\"$message_type\":\"diagnostic\",\"message\":\"identifier `__eyk_f` uses the prefix `__eyk_`, which is reserved for generated names\",\
         \"code\":{\"code\":\"E0101\",\"explanation\":null},\"level\":\"error\",\
         \"spans\":[{\"file_name\":\"src/lib.eyk\",\"byte_start\":3,\"byte_end\":10,\"line_start\":1,\"line_end\":1,\"column_start\":4,\"column_end\":11,\
         \"is_primary\":true,\"text\":[],\"label\":null,\"suggested_replacement\":null,\"suggestion_applicability\":null,\"expansion\":null}],\
         \"children\":[],\"rendered\":\"src/lib.eyk:1:4: E0101: identifier `__eyk_f` uses the prefix `__eyk_`, which is reserved for generated names\\n\"}"
    );
    // 日本語でも同じ形で、文字列は JSON の規則で書く。
    let j = p.diags[0].to_json(&f, Lang::Ja);
    assert!(j.contains("\"message\":\"識別子 `__eyk_f` は、"), "{j}");
}

#[test]
fn syntax_errors_have_code_e0001_in_both_languages() {
    let src = "fn f() { a < b < c }";
    assert_eq!(
        diags(src),
        [
            "src/lib.eyk:1:16: E0001: syntax error: comparison operators cannot be chained; write `a < b && b < c`"
        ]
    );
    assert_eq!(
        diags_in(src, Lang::Ja),
        [
            "src/lib.eyk:1:16: E0001: 構文エラー: 比較演算子は連ねて書けません。`a < b && b < c` と書きます"
        ]
    );
    // 構文解析器（verus_syn）の文言は英語だけで、日本語の文言にもそのまま埋め込む（`OPEN.md`）。
    assert_eq!(
        diags_in("fn f() { let x = ; }", Lang::Ja),
        ["src/lib.eyk:1:18: E0001: 構文エラー: expected an expression"]
    );
}

#[test]
fn lexical_and_eof_errors_have_positions() {
    assert_eq!(
        diags("fn f() {\n    let s = \"abc;\n}\n"),
        [
            "src/lib.eyk:2:13: E0001: syntax error: cannot read tokens here (unbalanced delimiter or unterminated literal)"
        ]
    );
    assert_eq!(
        diags_in("fn f() {\n    let s = \"abc;\n}\n", Lang::Ja),
        [
            "src/lib.eyk:2:13: E0001: 構文エラー: ここからトークンを読めません（括弧の対応か、閉じていないリテラル）"
        ]
    );
    // 入力の途中で終わったら、ファイルの末尾を指す。
    assert_eq!(
        diags("fn f()"),
        [
            "src/lib.eyk:1:7: E0001: syntax error: expected a contract clause (`requires`, `ensures`, ..) or a function body"
        ]
    );
    assert_eq!(
        diags("type T ="),
        ["src/lib.eyk:1:9: E0001: syntax error: unexpected end of file"]
    );
    assert_eq!(
        diags_in("type T =", Lang::Ja),
        ["src/lib.eyk:1:9: E0001: 構文エラー: ファイルが途中で終わっています"]
    );
}

/// 複数の誤りはまとめて、位置の順に報告する。
#[test]
fn errors_are_reported_together_in_order() {
    let src = "fn a() { a <== b }\n\
               fn b() no_unwind {}\n\
               struct S(u64); owns { x: Region<Ptr<u64>> }\n\
               fn ok() {}\n\
               proof fn p() { }\n\
               fn c() { x is Y }\n\
               impl S { fn view(&self) {} }\n\
               #[verifier::external] fn __eyk_d() {}\n";
    assert_eq!(
        diags(src),
        [
            "src/lib.eyk:1:10: E0001: syntax error: `a <== b` is Verus syntax; in Ebykn write `b ==> a`",
            "src/lib.eyk:2:8: E0001: syntax error: `no_unwind` is Verus syntax; in Ebykn write `no_panic`",
            "src/lib.eyk:3:16: E0401: `owns { .. }` can only follow a struct with named fields, but `S` is a tuple struct",
            "src/lib.eyk:5:1: E0001: syntax error: `proof fn` is Verus syntax; in Ebykn write `lemma fn`",
            "src/lib.eyk:6:10: E0001: syntax error: `e is V` is Verus syntax; in Ebykn write `matches!(e, V { .. })`",
            "src/lib.eyk:7:13: E0403: `view` cannot be defined directly in `impl T`; write `impl View for T` instead",
            "src/lib.eyk:8:1: E0102: `#[verifier::..]` attributes cannot be written in Ebykn",
            "src/lib.eyk:8:26: E0101: identifier `__eyk_d` uses the prefix `__eyk_`, which is reserved for generated names",
        ]
    );
    // 読めた項目は AST に残る。
    let (_, p) = parse(src);
    let names: Vec<String> = p.file.items.iter().map(item_head).collect();
    assert!(names.contains(&"fn ok() {..}".to_string()), "{names:?}");
}

/// 一つの項目の誤りは、次の項目を巻き込まない。
#[test]
fn recovery_resumes_at_the_next_item() {
    let src = "open spec fn o() -> int { 1 }\nfn z() { forall|x: int| x > 0 }\n";
    assert_eq!(
        diags(src),
        [
            "src/lib.eyk:1:1: E0001: syntax error: `open spec fn` is Verus syntax; in Ebykn write `spec fn`",
            "src/lib.eyk:2:10: E0001: syntax error: `forall|x| ..` is Verus syntax; in Ebykn write `forall x: T, P`",
        ]
    );
}
