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
            let inner_cx = match g.delimiter() {
                Delimiter::Brace => AttrCx::Brace,
                // 入れ子の `vec![..]` / `vec!(..)`。
                Delimiter::Bracket | Delimiter::Parenthesis
                    if punct(before(i, 1), '!') && ident(before(i, 2), &["vec"]) =>
                {
                    AttrCx::Elements
                }
                _ => AttrCx::Other,
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

/// 文と項目の頭のキーワード（属性は文か項目に付く）。`const { .. }` と `unsafe { .. }` はブロックの式として
/// [`block_like`] が見る。
const STMT_KEYWORDS: &[&str] = &[
    "let", "fn", "struct", "enum", "use", "const", "static", "impl", "mod", "trait", "type", "pub",
    "extern",
];

/// 前置のキーワード（属性はその式に付き、続く式はその被演算子になる）。
const PREFIX_KEYWORDS: &[&str] = &["return", "break", "continue"];

/// 許す位置の属性に続くトークン `t`（属性の並びの後から、引数の終わりまで）が、属性をそのまま受ける形か。
///
/// 構文解析器は、式の外側の属性を、代入・二項演算・型変換・範囲の左の項に移す（`#[a] x = 1` の属性は `x`
/// に付く）。左の項は安定版の Rust が属性を許す位置でないので、E0658 になる。`..` で始まる範囲の属性は、
/// 構文解析器が拒否する。そこで、続く式を区切り（`,`・`;`）まで読み、二項演算・代入・型変換・範囲の記号が
/// 最上位（グループの外）に一つでもあれば拒否する。前置の演算子（`-`、`!`、`*`、`&`）、パス（`a::b`、
/// `<T as Tr>::f`、`f::<T>`）、メソッド呼び出しと欄、`?`、マクロの呼び出しは読む。読めない記号は拒否する。
///
/// 次は、続く式を読まずに許す：文と項目（[`STMT_KEYWORDS`]）、`return` などの前置のキーワード、
/// クロージャ、`{ .. }` の中の match の腕（区切りまでに `=>`）と構造体の欄（`名前: 式`）、`{ .. }` の中で
/// ブロックのような式（[`block_like`]）で終わる文（後ろは次の文になる。`.` か `?` が続けば読み続ける）。
fn target_ok(t: &[TokenTree], cx: AttrCx) -> bool {
    let first = t.first();
    if ident(first, &["const", "unsafe"]) && group(t.get(1), Delimiter::Brace) {
        // `const { .. }` / `unsafe { .. }` はブロックの式（下の `block_like`）。
    } else if ident(first, STMT_KEYWORDS)
        || (ident(first, &["unsafe"]) && matches!(t.get(1), Some(TokenTree::Ident(_))))
        || ident(first, PREFIX_KEYWORDS)
        || punct(first, '|')
        || ident(first, &["move"])
    {
        return true;
    }
    if cx == AttrCx::Brace {
        let field = matches!(first, Some(TokenTree::Ident(_) | TokenTree::Literal(_)))
            && punct(t.get(1), ':')
            && !joint(t.get(1), ':');
        if field || fat_arrow(t) {
            return true;
        }
    }
    let (start, operand) = match block_like(t) {
        Some(end) => {
            if cx == AttrCx::Brace && !punct(t.get(end), '.') && !punct(t.get(end), '?') {
                return true;
            }
            (end, true)
        }
        None => (0, false),
    };
    operators_absent(t, start, operand)
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
/// 付く）。
fn block_like(t: &[TokenTree]) -> Option<usize> {
    let k = if punct(t.first(), '\'')
        && matches!(t.get(1), Some(TokenTree::Ident(_)))
        && punct(t.get(2), ':')
    {
        3
    } else {
        0
    };
    match t.get(k) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => Some(k + 1),
        Some(TokenTree::Ident(n)) if n == "unsafe" || n == "const" || n == "loop" => {
            group(t.get(k + 1), Delimiter::Brace).then_some(k + 2)
        }
        Some(TokenTree::Ident(n)) if n == "while" || n == "for" || n == "match" => {
            first_brace(t, k + 1).map(|b| b + 1)
        }
        Some(TokenTree::Ident(n)) if n == "if" => if_chain(t, k),
        _ => None,
    }
}

/// `t[from..]` の最上位で最初の `{ .. }` の位置（`;`・`,` より前）。条件の中の構造体リテラルは Rust が
/// 許さないので、最初の `{ .. }` が本体になる。
fn first_brace(t: &[TokenTree], from: usize) -> Option<usize> {
    for (k, tok) in t.iter().enumerate().skip(from) {
        if group(Some(tok), Delimiter::Brace) {
            return Some(k);
        }
        if punct(Some(tok), ';') || punct(Some(tok), ',') {
            return None;
        }
    }
    None
}

/// `t[k]` の `if` から始まる `if .. { .. } else if .. { .. } else { .. }` の終わりの位置。
fn if_chain(t: &[TokenTree], k: usize) -> Option<usize> {
    let end = first_brace(t, k + 1)? + 1;
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
