//! std のマクロの引数の中の属性の位置（PR #59 の再確認、#130）。引数はトークンのまま持ち、Ebykn が式として
//! 読まないので、安定版の Rust が属性を許す位置かをトークンで決める。トークンで見分けられない形は、許さない
//! 側に倒す（`Unimpl::AttrPosInMacro` の E0003）。安定版の Rust と食い違わないことは、
//! `crates/ebykn_check/tests/attr_rustc.rs` が rustc と突き合わせて確かめる。

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

/// マクロの引数の中の属性の位置の文脈。
#[derive(Clone, Copy, PartialEq, Eq)]
enum AttrCx {
    /// `vec![..]` の要素の並び（要素の頭は、安定版の Rust が式の属性を許す）。
    Elements,
    /// `{ .. }` の中（文の頭、match の腕と構造体の欄の頭）。
    Brace,
    /// ほか（式の中。`assert!(..)` の引数、括弧、添字など）。
    Other,
}

/// トークン列に、安定版の Rust が許さない位置の属性があるか（グループの中を含む）。許すのは、`vec!` の要素と
/// 繰り返しの数の頭（先頭と `,`・`;` の後）と、`{ .. }` の中の文・腕・欄の頭（先頭、`;`・`,`・`{ .. }` の
/// 後）の外側の属性で、続く式が属性を左の項に移す形でないもの（[`target_ok`]）だけ。内側の属性 `#![..]` は
/// どこでも許さない（`{ #![..] .. }` はブロック式の属性になる。入れ子の `fn` の本体のように許される場所も
/// あるが、トークンでは見分けない）。ほかの位置の式の属性は、安定版の Rust では `attributes on expressions
/// are experimental`（E0658）になり、Verus の toolchain では通るので、verified なのに Verus なしのビルドが
/// 落ちる。
pub(super) fn has_misplaced_attribute(ts: &TokenStream, vec: bool) -> bool {
    misplaced(ts, if vec { AttrCx::Elements } else { AttrCx::Other })
}

fn punct(t: Option<&TokenTree>, c: char) -> bool {
    matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == c)
}

/// `c` で、直後の記号とつながっている（`::`、`..`、`=>` の一つ目）。
fn joint(t: Option<&TokenTree>, c: char) -> bool {
    matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == c && p.spacing() == Spacing::Joint)
}

fn ident(t: Option<&TokenTree>, names: &[&str]) -> bool {
    matches!(t, Some(TokenTree::Ident(n)) if names.iter().any(|m| n == m))
}

fn group(t: Option<&TokenTree>, d: Delimiter) -> bool {
    matches!(t, Some(TokenTree::Group(g)) if g.delimiter() == d)
}

fn misplaced(ts: &TokenStream, cx: AttrCx) -> bool {
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    let before = |k: usize, back: usize| k.checked_sub(back).and_then(|j| tts.get(j));
    // `tts[k]` が属性の `[..]` か（`#[..]` / `#![..]` の `[..]`）。
    let attr_body = |k: usize| {
        group(tts.get(k), Delimiter::Bracket)
            && (punct(before(k, 1), '#') || (punct(before(k, 1), '!') && punct(before(k, 2), '#')))
    };
    // `i` から始まる外側の属性の、直前のトークン（`#` の前）が属性を許す位置か。属性の並びの二つ目からは、
    // 一つ目が許されていれば許す（一つ目が許されなければ、そこで拒否している）。
    let allowed = |i: usize| {
        let Some(prev) = i.checked_sub(1) else {
            return cx != AttrCx::Other;
        };
        if attr_body(prev) {
            return true;
        }
        match cx {
            AttrCx::Elements => punct(tts.get(prev), ',') || punct(tts.get(prev), ';'),
            AttrCx::Brace => {
                punct(tts.get(prev), ';')
                    || punct(tts.get(prev), ',')
                    || group(tts.get(prev), Delimiter::Brace)
            }
            AttrCx::Other => false,
        }
    };
    tts.iter().enumerate().any(|(i, t)| match t {
        TokenTree::Group(g) => {
            if attr_body(i) {
                return false;
            }
            // 入れ子のマクロの呼び出し `名前!(..)` / `名前![..]` / `名前!{..}` の引数は、区切りの種類によらず、
            // `vec!` なら要素の並び、ほかは式の中として見る（`println!{..}` の `{ .. }` はブロックでない）。
            // `if !{ .. }` のような予約語の後の `!` もマクロとして見て、中の属性を拒否する側に倒す。
            let inner_cx =
                if punct(before(i, 1), '!') && matches!(before(i, 2), Some(TokenTree::Ident(_))) {
                    if ident(before(i, 2), &["vec"]) {
                        AttrCx::Elements
                    } else {
                        AttrCx::Other
                    }
                } else if g.delimiter() == Delimiter::Brace {
                    AttrCx::Brace
                } else {
                    AttrCx::Other
                };
            misplaced(&g.stream(), inner_cx)
        }
        TokenTree::Punct(p) if p.as_char() == '#' => {
            let inner = punct(tts.get(i + 1), '!');
            let body = if inner { i + 2 } else { i + 1 };
            if !group(tts.get(body), Delimiter::Bracket) {
                return false;
            }
            if inner || !allowed(i) {
                return true;
            }
            // 続く式は、属性の並びの一つ目で見る。
            if i.checked_sub(1).is_some_and(attr_body) {
                return false;
            }
            let mut s = body + 1;
            while punct(tts.get(s), '#') && group(tts.get(s + 1), Delimiter::Bracket) {
                s += 2;
            }
            !target_ok(&tts[s..], cx)
        }
        _ => false,
    })
}

/// `{ .. }` の中の文と項目の頭のキーワード（属性は文か項目に付く）。`const { .. }` と `unsafe { .. }` は
/// ブロックの式として [`block_like`] が見る。
const STMT_KEYWORDS: &[&str] = &[
    "let", "fn", "struct", "enum", "use", "const", "static", "impl", "mod", "trait", "type", "pub",
    "extern",
];

/// ブロックのような式の頭のキーワード。
const BLOCK_KEYWORDS: &[&str] = &["if", "match", "while", "for", "loop", "unsafe", "const"];

/// 許す位置の属性に続くトークン `t`（属性の並びの後から、引数の終わりまで）が、安定版の Rust が属性を
/// そのまま受けると分かっている形か。分からない形は、すべて拒否する（綴りを一つずつ塞ぐのでなく、
/// 分かっている形だけを通す。PR #140 のレビュー）。
///
/// 構文解析器は、式の外側の属性を、代入・二項演算・型変換・範囲の左の項に移す（`#[a] x = 1` の属性は `x`
/// に付く）。左の項は安定版の Rust が属性を許す位置でないので、E0658 になる。`..` で始まる範囲の属性は、
/// 構文解析器が拒否する。そこで、続く式を区切り（`,`・`;`）まで読み（[`operators_absent`]）、すべての
/// トークンを読めて、二項演算・代入・型変換・範囲の記号が最上位（グループの外）にないときだけ許す。
///
/// 次の形は、続く式をそこまで読まずに許す：
/// - `{ .. }` の中の文と項目（[`STMT_KEYWORDS`]、`unsafe fn` など）、match の腕（区切りまでに `=>`）、
///   構造体の欄（`名前: 式`）。
/// - 戻り値の型のないクロージャ（`|x| 本体`、`move || 本体`）。本体が式の終わりまで続くので、属性は
///   クロージャ全体に付く。戻り値の型を書いたクロージャ（`|| -> T { .. }`）は本体がブロックで終わり、後ろに
///   `as` や二項演算が続きうるので許さない。
/// - ブロックのような式（[`block_like`]）。`{ .. }` の中では、その後ろに `.` も `?` も続かなければ、文は
///   そこで終わる（後ろは次の文になる）。`.` か `?` が続くときと、`vec!` の要素では、後ろを読み続ける。
///   ブロックのような式の終わりが分からなければ許さない。
fn target_ok(t: &[TokenTree], cx: AttrCx) -> bool {
    let first = t.first();
    if cx == AttrCx::Brace {
        let item = (ident(first, STMT_KEYWORDS)
            && !(ident(first, &["const"]) && group(t.get(1), Delimiter::Brace)))
            || (ident(first, &["unsafe"]) && matches!(t.get(1), Some(TokenTree::Ident(_))));
        let field = matches!(first, Some(TokenTree::Ident(_) | TokenTree::Literal(_)))
            && punct(t.get(1), ':')
            && !joint(t.get(1), ':');
        if item || field || fat_arrow(t) {
            return true;
        }
    }
    if let Some(ok) = closure_ok(t) {
        return ok;
    }
    let label = punct(first, '\'');
    if label || ident(first, BLOCK_KEYWORDS) || group(first, Delimiter::Brace) {
        let Some(end) = block_like(t) else {
            return false;
        };
        if cx == AttrCx::Brace && !punct(t.get(end), '.') && !punct(t.get(end), '?') {
            return true;
        }
        return operators_absent(t, end, true);
    }
    operators_absent(t, 0, false)
}

/// `t` がクロージャ（`|..|`、`||`、`move` の付いたもの）で始まるなら、許すか。戻り値の型のないものだけを
/// 許す。クロージャでなければ `None`。
fn closure_ok(t: &[TokenTree]) -> Option<bool> {
    let k = usize::from(ident(t.first(), &["move"]));
    if !punct(t.get(k), '|') {
        return (k == 1).then_some(false);
    }
    // 引数の並びの終わり（`||` なら二つ目の `|` の次）。
    let end = if joint(t.get(k), '|') && punct(t.get(k + 1), '|') {
        k + 2
    } else {
        let close = t
            .iter()
            .enumerate()
            .skip(k + 1)
            .find(|(_, x)| punct(Some(x), '|'))
            .map(|(j, _)| j)?;
        close + 1
    };
    Some(!(joint(t.get(end), '-') && punct(t.get(end + 1), '>')) && t.get(end).is_some())
}

/// 区切り（`,`・`;`）までの最上位に `=>` があるか（match の腕）。
fn fat_arrow(t: &[TokenTree]) -> bool {
    for (k, tok) in t.iter().enumerate() {
        if punct(Some(tok), ',') || punct(Some(tok), ';') {
            return false;
        }
        if joint(Some(tok), '=') && punct(t.get(k + 1), '>') {
            return true;
        }
    }
    false
}

/// `t` の先頭のブロックのような式（`{ .. }`、`if`、`match`、`while`、`for`、`loop`、`unsafe { .. }`、
/// `const { .. }`、ラベル `'l:` の付いたもの）の終わりの位置。条件と本体の中は読まない（属性はこの式全体に
/// 付く）。本体の `{ .. }` を確かに見分けられないときは `None`（[`body_brace`]）。
fn block_like(t: &[TokenTree]) -> Option<usize> {
    let k = if punct(t.first(), '\'') {
        (matches!(t.get(1), Some(TokenTree::Ident(_))) && punct(t.get(2), ':')).then_some(3)?
    } else {
        0
    };
    match t.get(k) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => Some(k + 1),
        Some(TokenTree::Ident(n)) if n == "unsafe" || n == "const" || n == "loop" => {
            group(t.get(k + 1), Delimiter::Brace).then_some(k + 2)
        }
        Some(TokenTree::Ident(n)) if n == "while" || n == "match" => {
            body_brace(t, k + 1).map(|b| b + 1)
        }
        Some(TokenTree::Ident(n)) if n == "for" => {
            // パターンは `in` まで。パターンの中の `{ .. }`（構造体のパターン）は本体と見分けにくいので、
            // パターンに `{ .. }` があれば許さない。
            let r#in = t
                .iter()
                .enumerate()
                .skip(k + 1)
                .take_while(|(_, x)| !group(Some(x), Delimiter::Brace))
                .find(|(_, x)| ident(Some(x), &["in"]))
                .map(|(j, _)| j)?;
            body_brace(t, r#in + 1).map(|b| b + 1)
        }
        Some(TokenTree::Ident(n)) if n == "if" => if_chain(t, k),
        _ => None,
    }
}

/// `t[from..]` の条件（`if` / `while` の条件、`match` の対象、`for` の `in` の後）に続く本体の `{ .. }` の
/// 位置。条件の中の構造体リテラルは Rust が許さないが、`{ .. }` は条件の中にも書ける（`if let S { v } = s`
/// の構造体のパターン、`if { b }` のブロック、`match m!{..}` のマクロ、`if || { .. }` のクロージャ、入れ子の
/// `if` / `match`）。そこで、最上位で最初の `{ .. }` を本体とみなすのは、条件が次をすべて満たすときだけにする：
/// 空でない、`let` とブロックのような式の頭のキーワード（[`BLOCK_KEYWORDS`]、`move`・`async`・`return`・
/// `break`）と `|` を含まない、`{ .. }` の直前が被演算子の終わり（名前、リテラル、`( .. )` / `[ .. ]`、`?`）。
/// どれかを満たさなければ `None`。
fn body_brace(t: &[TokenTree], from: usize) -> Option<usize> {
    const COND_KEYWORDS: &[&str] = &["let", "move", "async", "return", "break", "continue"];
    for (k, tok) in t.iter().enumerate().skip(from) {
        match tok {
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                let end_of_operand = k > from
                    && match &t[k - 1] {
                        TokenTree::Ident(n) => !BLOCK_KEYWORDS.iter().any(|w| n == w),
                        TokenTree::Literal(_) => true,
                        TokenTree::Group(g) => g.delimiter() != Delimiter::Brace,
                        TokenTree::Punct(p) => p.as_char() == '?',
                    };
                return end_of_operand.then_some(k);
            }
            TokenTree::Ident(n) if BLOCK_KEYWORDS.iter().chain(COND_KEYWORDS).any(|w| n == w) => {
                return None;
            }
            TokenTree::Punct(p) if matches!(p.as_char(), '|' | ';' | ',') => return None,
            _ => {}
        }
    }
    None
}

/// `t[k]` の `if` から始まる `if .. { .. } else if .. { .. } else { .. }` の終わりの位置。
fn if_chain(t: &[TokenTree], k: usize) -> Option<usize> {
    let end = body_brace(t, k + 1)? + 1;
    if !ident(t.get(end), &["else"]) {
        return Some(end);
    }
    if ident(t.get(end + 1), &["if"]) {
        if_chain(t, end + 1)
    } else {
        group(t.get(end + 1), Delimiter::Brace).then_some(end + 2)
    }
}

/// `t[from..]` を区切り（`,`・`;`）まで読み、二項演算・代入・型変換・範囲の記号が最上位にないか。`operand`
/// は、`from` の直前までが被演算子として閉じているか（そこに続く記号は二項演算になる）。
fn operators_absent(t: &[TokenTree], from: usize, mut operand: bool) -> bool {
    let mut i = from;
    while let Some(tok) = t.get(i) {
        match tok {
            TokenTree::Ident(n) => {
                if operand && n == "as" {
                    return false;
                }
                operand = true;
                i += 1;
            }
            TokenTree::Literal(_) | TokenTree::Group(_) => {
                operand = true;
                i += 1;
            }
            TokenTree::Punct(p) => match p.as_char() {
                ',' | ';' => return true,
                // パスの `::`。`::<..>` の型引数は読み飛ばす。
                ':' if joint(Some(tok), ':') && punct(t.get(i + 1), ':') => {
                    i += 2;
                    operand = false;
                    if punct(t.get(i), '<') {
                        let Some(j) = skip_angle(t, i) else {
                            return false;
                        };
                        i = j;
                        operand = true;
                    }
                }
                // `..` は範囲。
                '.' if joint(Some(tok), '.') => return false,
                '.' => {
                    i += 1;
                    operand = false;
                }
                '?' => i += 1,
                // 式の中の属性（位置は `misplaced` が見る）。
                '#' if group(t.get(i + 1), Delimiter::Bracket) => i += 2,
                // マクロの呼び出し `m!(..)`。
                '!' if operand
                    && !joint(Some(tok), '!')
                    && matches!(
                        i.checked_sub(1).and_then(|j| t.get(j)),
                        Some(TokenTree::Ident(_))
                    ) =>
                {
                    i += 1;
                    operand = false;
                }
                // 修飾されたパス `<T as Tr>::f`。
                '<' if !operand => {
                    let Some(j) = skip_angle(t, i) else {
                        return false;
                    };
                    i = j;
                    operand = true;
                }
                '-' | '!' | '*' | '&' if !operand => i += 1,
                // 二項演算・代入・`=>`、読めない記号。
                _ => return false,
            },
        }
    }
    true
}

/// `t[i]` の `<` に対応する `>` の次の位置。`->` の `>` は数えない。閉じなければ `None`。
fn skip_angle(t: &[TokenTree], i: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, tok) in t.iter().enumerate().skip(i) {
        match tok {
            TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
            TokenTree::Punct(p) if p.as_char() == '>' => {
                if k > 0 && joint(t.get(k - 1), '-') {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(k + 1);
                }
            }
            TokenTree::Punct(p) if p.as_char() == ';' => return None,
            _ => {}
        }
    }
    None
}
