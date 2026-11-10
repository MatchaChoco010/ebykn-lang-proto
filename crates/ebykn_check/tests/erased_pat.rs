//! 消去フィールドのパターン `@x` の位置（#152）。
//!
//! 書き下しは、実行文脈の `let` の文のほかの位置の `@x` を包みの値のまま束縛し、腕・本体の先頭で束縛し
//! 直す。その前に評価される位置（match のガード、`&&` で続く条件、`for` のループ句、関数とクロージャの
//! 契約）で `@x` の名前を使うものは E0003。使わないものは通す。生成したものが Verus を通ることは
//! golden の `tests/golden/erased_pat` が確かめる。

mod support;
use support::*;

const F: &str = "pub struct Node { pub val: u64 }
pub struct B { pub n: u64 } owns { items: Region<Ptr<Node>> } spec { hist: Seq<u64> }
";

fn src(body: &str) -> String {
    format!("{F}{body}\n")
}

#[test]
fn the_reported_positions_are_accepted() {
    for body in [
        // #152 の再現（match の腕・クロージャの引数・fn の引数）。
        "fn f(b: B) -> B { match b { B { @items, n, @hist } => B { n, @items: items, @hist: hist } } }",
        "fn f(b: B) -> B { let g = |B { @items, n, @hist }: B| -> B { B { n, @items: items, @hist: hist } }; g(b) }",
        "fn g(B { @items, n, @hist }: B) -> B { B { n, @items: items, @hist: hist } }",
        // `if let` / `while let` / `for`。
        "fn f(o: Option<B>) -> Option<B> { if let Some(B { @items, n, @hist }) = o { Some(B { n, @items: items, @hist: hist }) } else { None } }",
        "fn f(v: Vec<B>) { let mut v = v; while let Some(B { @items, n, @hist }) = v.pop() decreases v.len(), { let b = B { n, @items: items, @hist: hist }; } }",
        "fn f(v: Vec<B>) { for B { @items, n, @hist } in v { let b = B { n, @items: items, @hist: hist }; } }",
        // 束縛し直す前の位置でも、`@x` でない名前は使える。
        "fn f(b: B) -> B { match b { B { @items, n, @hist } if n > 0 => B { n, @items: items, @hist: hist }, B { @items, n, @hist } => B { n, @items: items, @hist: hist } } }",
        "fn g(B { @items, n, @hist }: B) -> (r: B) requires n > 0, ensures r.n == n, { B { n, @items: items, @hist: hist } }",
        // 検査だけを通る形（let chain は #166 のとおり、固定した Verus が受けない）。
        "fn f(o: Option<B>) -> u64 { if let Some(B { @items, n, @hist }) = o && n > 0 { let b = B { n, @items: items, @hist: hist }; b.n } else { 0 } }",
        // 論理文脈（`spec fn` の本体と引数）。
        "spec fn h(b: B) -> Seq<u64> = { let B { @hist, @.. } = b; hist };",
        "spec fn h(B { @hist, @.. }: B) -> nat = hist.len();",
    ] {
        accepts(&src(body));
    }
}

#[test]
fn e0003_when_the_guard_uses_the_binding() {
    rejects(
        &src(
            "fn f(b: B) -> B { match b { B { @items, n, @hist } if hist.len() > 0 => B { n, @items: items, @hist: hist }, B { @items, n, @hist } => B { n, @items: items, @hist: hist } } }",
        ),
        "E0003",
        "match guard",
    );
}

#[test]
fn e0003_when_a_later_condition_uses_the_binding() {
    rejects(
        &src(
            "fn f(o: Option<B>) -> u64 { if let Some(B { @items, n, @hist }) = o && items.contains(Ptr::null()) { let b = B { n, @items: items, @hist: hist }; b.n } else { 0 } }",
        ),
        "E0003",
        "later condition",
    );
}

#[test]
fn e0003_when_a_for_clause_uses_the_binding() {
    rejects(
        &src(
            "fn f(v: Vec<B>) { for B { @items, n, @hist } in v invariant hist.len() >= 0, { let b = B { n, @items: items, @hist: hist }; } }",
        ),
        "E0003",
        "`invariant` / `decreases`",
    );
}

/// `while let` の句も同じ（PR #168 のレビュー）。外に同じ名前があっても、句は外の名前を指さない：Verus は
/// `while let` のパターンの名前を句の中でも見えるものとして扱うので、書き下しの後では句が別のものを指す。
#[test]
fn e0003_when_a_while_let_clause_uses_the_binding() {
    rejects(
        &src(
            "fn f(b0: B, v: Vec<B>) -> (r: u64) requires b0@hist.len() == 3, { let B { @items, @hist, n: _n0 } = b0; let c0 = B { n: 0, @items: items, @hist: hist }; let mut v = v; while let Some(B { @items, @hist, n }) = v.pop() invariant hist.len() == 3, decreases v.len(), { let c = B { n, @items: items, @hist: hist }; } 0 }",
        ),
        "E0003",
        "`invariant` / `decreases`",
    );
}

#[test]
fn e0003_when_the_contract_uses_the_binding() {
    for body in [
        "fn g(B { @items, n, @hist }: B) -> (r: B) ensures r@hist == hist, { B { n, @items: items, @hist: hist } }",
        "fn g(B { @items, n, @hist }: B) -> B requires items.len() > 0, { B { n, @items: items, @hist: hist } }",
        "spec fn h(B { @hist, @.. }: B) -> nat decreases hist.len(), = 0;",
        "fn f(b: B) -> B { let g = |B { @items, n, @hist }: B| -> (r: B) ensures r@hist == hist, { B { n, @items: items, @hist: hist } }; g(b) }",
    ] {
        rejects(&src(body), "E0003", "contract");
    }
}

/// 名前はマクロの引数の中でも数える（トークンを読む。多めに拒否する）。
#[test]
fn e0003_when_a_macro_in_the_guard_mentions_the_binding() {
    rejects(
        &src(
            "fn f(b: B) -> B { match b { B { @items, n, @hist } if matches!(hist, _) => B { n, @items: items, @hist: hist }, B { @items, n, @hist } => B { n, @items: items, @hist: hist } } }",
        ),
        "E0003",
        "match guard",
    );
}

/// 関数の引数のパターンも、本体の中のパターンと同じに読む：`owns` の欄を黙って省けず（E0402）、
/// 消去フィールドでない名前は `@` で書けない。
#[test]
fn parameter_patterns_are_checked_like_body_patterns() {
    rejects(
        &src("fn g(B { n, @hist }: B) -> u64 { n }"),
        "E0402",
        "omits the `owns` field `items`",
    );
    rejects(
        &src("fn g(B { @n, @.. }: B) -> u64 { 0 }"),
        "E0402",
        "`n` is not an `owns` or `spec` field of `B`",
    );
}
